# Users, roles and signing in

Faber has users, roles and a login from the start (D16: retrofitting auth is how insecure products
happen). This page is the design; [api.md](api.md#users-and-signing-in) lists the endpoints.

**Status:** accepted (D53, 2026-10-16; Phase 6a). Built as described here; Mikkel accepted every point marked
**confirm**.

## Roles

Every user, API token and the attached screen has exactly one role. Each role can do everything
the one below it can.

| Role | May | May not |
|---|---|---|
| **viewer** | See everything: parts, signals, ports, apps, files, logs, settings. Open app windows. **Press Stop.** | Change anything. |
| **operator** | Also: drive (leases, commands, an app window's drive), release the e-stop, set simulated values and the world, the serial monitor, change their own app state, files in the home folder, display settings (look, accessibility, units, taskbar, desktop). | Install or remove packages, add or remove parts, turn apps on or off, flash a board, back up or restore, change the machine's own settings (name, time, the attached screen's role, developer settings), manage users. |
| **admin** | Everything, including users, developer settings, installs, flashing, the e-stop release, backups and restore. | — |

**Engaging the e-stop is never gated.** Anyone who can reach the machine can press Stop
(`POST /api/v0/estop` needs no sign-in at all, so the sign-in screen has a Stop button too), and
so can every role and the attached screen in an app window (`app-window-stop`). Releasing it needs
an operator.

A role is checked by the core, never only by the shell. The shell shows what a role can't do as
disabled with the reason ("Needs the operator role"), never hidden, so a viewer can see what exists
and whom to ask.

### Where each rule lives

One table in the core (`core/src/api/access.rs`) gives every route its minimum role, or `Public`
for the few that need no sign-in: the session and setup routes, `GET /api/v0/system`, the
translations (the sign-in screen speaks the machine's language), engaging the e-stop, and the
app UI files. A route without an entry is refused for everyone (`500 no-access-rule`, logged), and
a test reads the API's source and fails if any route lacks an entry (or an entry has no route), so
a new endpoint can't be forgotten. Another test (`core/tests/auth.rs`) calls every route as nobody,
viewer, operator and admin and checks the boundary.

WebSocket messages have their own table in the same file (an exhaustive `match`, so a new message
doesn't compile without a rule): subscribing, and opening, beating, stopping, releasing and closing
an app window are for viewers; the serial monitor and an app window's messages and drives need an
operator. Each message is checked with the sender's role at that moment.

A few rules depend on more than the route, and are checked where the request is handled:

- **Files:** `machine:/` (whole-machine files, D34) and `home:/Backups` are for admins only.
  Backups hold the users file (password hashes), so they aren't for every viewer to download.
- **Settings:** `PUT /api/v0/settings` needs an operator, and an admin when it changes the
  machine's name, time, or the attached screen's role.
- **Users and tokens:** anyone signed in can change their own password (with the current one)
  and make, list and revoke their own API tokens; only admins manage other users and all tokens.

## First run: creating the admin

A fresh machine has no users. The **first browser to open Faber** gets "Create the admin": a name
and a password, then it is signed in as that admin. `POST /api/v0/setup` works only while no users
exist; after that it answers `409 already-set-up`. Creating the admin and checking that none exists
happen under one lock, so two browsers racing can't both win.

That first-come window is the one moment a stranger on the same network could claim the machine.
It is short (one form), and the image's `faber.txt` provisioning (Phase 6) will be able to create
the admin before the machine is ever on a network.

**The attached screen** is never shown the setup wall: it works as its own role from the first
boot (below), and its user menu offers "Create the admin" while no users exist (someone at the
screen is physically at the machine).

## The attached screen (kiosk)

The screen on the machine runs the same shell (D14). It is **signed in as a role, not as a user**:
Settings → This machine → "The attached screen is signed in as" **viewer** (default) or
**operator**. Only admins change it. It can always press Stop.

**How the core knows a request comes from the attached screen:** the kiosk browser talks to its
own listener, `--screen-listen 127.0.0.1:7879` (the image starts Chromium on it). The core accepts
only a loopback address for that listener, and also checks that each connection on it comes from a
loopback address. Requests on the normal listener get no trust for being local, and the core never
reads `X-Forwarded-For`, `X-Real-IP`, `Forwarded` or similar headers for anything.

Why a separate listener and not "everything from 127.0.0.1": on a PC the developer's own browser is
on loopback too, and on the Pi a local tunnel or proxy (Tailscale Serve, Cloudflare Tunnel)
connects from loopback on behalf of remote users. Either would silently become the attached screen.
A listener that only the kiosk is pointed at can't be confused that way. (Anyone who can open a
connection to it on the machine itself, e.g. over SSH, already has more access than the screen.)

## Developer mode: `--no-auth`

`faber-core --no-auth` treats every request as an admin (no user name), for development on a PC.
It refuses to start unless `--listen` is a loopback address, logs a warning, and the shell shows a
warning bar ("Sign-in is off") on every screen. Without `--no-auth`, `--listen` may now be any
address: remote access needs a login.

**The tests don't use it.** The Rust test harnesses sign in through the real path: they create an
admin and an API token and send it with every request (`core/tests/support`, `signed_in_api`), and
the end-to-end tests start the core without users, create the admin through the real "Create the
admin" screen once and reuse its cookie (`shell/e2e/global-setup.ts`). That way every test runs
the same code as a real machine, and `--no-auth` stays a convenience nobody depends on.

## Passwords

- **argon2id** (the `argon2` crate), **64 MiB memory, 3 passes, 1 lane** (RFC 9106's second
  recommendation with one lane). On a Pi 5 that should take a few hundred milliseconds (not yet
  measured on the Pi). The core hashes **one password at a time**, so a burst of logins can't use
  more than 64 MiB.
- At least 8 characters, at most 256. No other rules.
- Stored in `<data>/config/users.toml` as PHC strings (`$argon2id$v=19$m=65536,t=3,p=1$…`), never
  the password. A hash made with other parameters still verifies (they are in the string).
- A wrong name and a wrong password give the same answer (`401 sign-in-failed`) and take the same
  time (an unknown name is checked against a dummy hash).

```toml
# <data>/config/users.toml
[[users]]
name = "mikkel"
role = "admin"
password = "$argon2id$v=19$m=65536,t=3,p=1$…"

[[tokens]]
id = "3f9a1c0e"
name = "drive-square"
user = "mikkel"
role = "operator"
hash = "sha256:…"
created = "2026-10-16T21:04:11Z"
```

## Sessions

- Signing in makes a random **256-bit token**, sent as the cookie `faber-session`: `HttpOnly`,
  `SameSite=Strict`, `Path=/`. `Secure` is added once the core serves HTTPS itself (the networking
  part of Phase 6); it serves plain HTTP today.
- Stored **server-side, in memory**, keyed by the token's SHA-256 (the token itself is never kept
  or logged). Expiry is **sliding**: a session ends after **7 days without use**, and in any case
  **30 days** after signing in. Times are on the core's clock, so the tests move them by hand.
- Signing out ends the session. Changing a password ends every other session of that user (the
  one that changed it stays); an admin setting someone's password, changing their role or removing
  them ends all of theirs. A role change applies at once to everything that user has open.
- **Confirm:** sessions are not saved, so **restarting the core signs every browser out**. The
  attached screen and API tokens are unaffected. Saving sessions (hashed) to survive a reboot is
  easy to add if that is too annoying on a robot that is switched off often.
- **The WebSocket** authenticates with the same cookie (or a bearer token) when it is opened. When
  the session ends or its user's role changes, the core closes the socket (code 4401) or applies
  the new role to the next message.

### CSRF and DNS rebinding

- `SameSite=Strict` keeps the cookie off requests started by other sites.
- Every **state-changing request** (anything but `GET`/`HEAD`) and **every WebSocket upgrade**
  that carries an `Origin` header must carry this machine's, as the browser addressed it
  (`http://` or `https://` + its `Host`). Otherwise: `403 bad-origin`. That covers sign-in and
  setup too (login CSRF) and the public Stop. A request without `Origin` passes: every current
  browser sends it on these requests, so its absence means a program, which can't be tricked into
  sending someone's cookie. (A sandboxed app frame's `Origin` is `null`, which is refused: app UIs
  talk only through the shell.)
- **Host check:** the core answers routes only for host names that can be this machine: an IP
  address, a name without dots (the local network's own names, e.g. `sennep`), or one under
  `.local`, `.internal`, `.localhost`, `.lan`, `.home.arpa` or `.ts.net`, or the machine's name. A
  web page that points its own (dotted, public) domain at the machine's address (DNS rebinding)
  gets `421 unknown-host`, so it can't create the admin on a fresh machine or use a `--no-auth`
  core. **Confirm:** a public tunnel to your own domain (later) will need that domain added in
  Settings.

### Rate limiting

Failed sign-ins are counted per IP address (the TCP peer, never a header) and per user name. The
first 5 failures are free; after that each attempt must wait 1 s, 2 s, 4 s, … up to 15 minutes
after the previous failure, answered by `429 too-many-attempts` with a `Retry-After` header. A
successful sign-in clears that name's and address's count; counts are forgotten an hour after the
last failure. The limiter remembers at most 10 000 names and addresses (the oldest are dropped).

## API tokens (programs)

Programs such as `examples/drive-square.py` use **personal API tokens**: Settings → Users → API
tokens → "Make token", with a name and a role up to your own. Only someone who signed in with a
password makes tokens (not a token, and not the attached screen). The token (`faber_` + 43 characters)
is **shown once**; the core keeps only its SHA-256 and compares in constant time. A program sends it
as `Authorization: Bearer faber_…`.

A token acts as its user with **the lower of its own role and its user's current role**, so
demoting a user demotes their tokens too. Removing a user removes their tokens. Tokens never
expire; revoke them in Settings or with `DELETE /api/v0/tokens/{id}`.

## Backups

Backups include `config/users.toml` (hashes only). A **setup** (Share setup) doesn't.

Restoring keeps this machine's own users by default. "Also restore users and API tokens" replaces
them with the backup's, and then **ends every session**: the person restoring signs in again with an
account from the backup. A backup without a users file leaves the users as they are either way.

## In the shell

- **Sign-in screen:** "Sign in to *machine name*", a name and a password, and a **Stop** button
  (it works without signing in). The shell asks `GET /api/v0/session` before it connects to
  anything else, and shows it whenever nobody is signed in. It speaks the machine's language.
- **First run:** while there are no users, the same screen is "Create the admin": a name, a
  password and the password again.
- **User menu** on the taskbar (a person icon; with the large taskbar, the name too): who is
  signed in and their role, "Password and API tokens…" (Settings → Users) and "Sign out". On the
  attached screen it offers "Sign in as someone…" instead (and "Create the admin…" on a fresh
  machine); the page starts again as that person, and signing out returns it to the screen's role.
- **Settings → Users:** change your password; as an admin, the users (add, role, set password,
  remove); and your API tokens (make one, shown once with Copy; revoke). Settings → Name has the
  attached screen's role.
- **Disabled, never hidden:** what the role can't do stays visible and disabled, with the reason
  ("Needs the operator role") as its tooltip or beside it: Add part and removing parts, the Apps
  switches, roles and uninstall, installing, the flasher, Drive (no "Start driving?" question for a
  viewer), the serial monitor, manual simulated values, the world, Release on the taskbar, backups
  and restore, and the settings the role can't change. Stop is never disabled.
- **Signed out elsewhere** (another tab, a password change, the user removed, the session ran
  out): the next request or the closed WebSocket tells the shell, and the page starts again at the
  sign-in screen, with nothing of the last user's left in it.
- `--no-auth` shows a bar at the top: "Sign-in is off".

## App windows

App UIs keep talking only through the shell (docs/apps.md §9), so they act with the role of the
person using the shell: a viewer can open an app's window and see it, but its `drive` and
`drive-via` are refused by the shell with `not-allowed` ("Needs the operator role") before the
D46 question, and by the core too (its messages to the logic and its state changes need an
operator). An app's UI files (`/apps/{id}/ui/…`) are public like the shell's own files:
they are the package's code, not data, and a sandboxed frame may not send the cookie.

## What Mikkel must confirm

1. Sessions are in memory: a core restart signs browsers out (the screen and tokens are fine).
2. Stop needs **no** sign-in ("never gate stopping"): the sign-in screen has a Stop button, and
   anyone who can reach the machine can press it. The cost: someone on the network could keep it
   stopped (they still can't release it or drive). Gating it is one table entry.
3. Viewers can't send messages to an app's logic (only open and watch its window), because a message
   could make the logic move things. Apps that only show data still work.
4. Operators may change display settings (look, accessibility, taskbar), which are machine-wide
   (D40); only admins change the machine's name, time and screen role.
5. The host-name check (`421 unknown-host`) and its list of accepted names.
6. Removing yourself, or the last admin, and demoting the last admin, are refused.
7. A request without an `Origin` header is not checked for it (only programs leave it out; the
   cookie is `SameSite=Strict` anyway). Requiring it would break `curl` with a cookie, nothing else.
8. Adding, removing, pausing and resuming parts needs an **admin** (it changes the machine), not an operator. An
   operator drives, simulates and edits files, but can't change what the machine is made of.
9. Viewers may make API tokens for themselves (viewer tokens only), e.g. for a dashboard script.
