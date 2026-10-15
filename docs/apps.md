# Phase 5 design: apps

Status: **design v1; Phase 5a (apps in the core) and 5b (apps in the shell) built** (2026-10-16).
Builds on D03, D04, D24, D29 and architecture.md §7. §8 says what 5a built and how it differs, §9
what 5b built: app windows, the message protocol, `@faber-os/shell`, the role picker and Apps. The open questions in
§7 were answered with their recommendations while Mikkel was away: both wasm logic and UI-only
apps; SENNEP's app in the SENNEP V2 repo, with a small example app here.

An **app** uses capabilities to do a job (drive SENNEP, log a temperature, follow a wall). It
never talks to hardware or connectors: it asks for **roles** ("a laser-scan", "a planar-base I
may drive"), and the user picks which parts fill them when installing it (D04).

## 1. What a user does

1. Installs `sennep-0.1.0.faberpkg` (the same Install window as connectors, Phase 3).
2. The window asks for each role: "LiDAR: which part?" offering only parts with `laser-scan@1`
   (and connectors from the store when nothing fits, Phase 7). Roles with `command = true` say so:
   "SENNEP will be able to drive base".
3. The app opens as a window (its UI) and runs its logic in the background.
4. Taking a lease, the D46 confirm, the e-stop and the lease's dead man's switch all work exactly
   as for the Drive window. An app is just another lease holder; operators can always take over.

## 2. The parts of an app

| Part | Runs as | Can do |
|---|---|---|
| **Logic** (optional) | a wasm component, `faber:app` world | read its roles' signals, command leased roles, publish derived signals (D24), store small state, log |
| **UI** (optional) | static web files in a shell window (an iframe with a strict sandbox) | the same, through `@faber-os/shell` (a small JS SDK over a message channel; no direct API access) |

An app needs at least one of the two. A UI-only app (a dashboard) is the simplest kind.

## 3. Manifest

As in architecture.md §7, with `faber-manifest = 1`, `type = "app"`, `[[requires]]` roles
(`role`, `capability`, `command`, `count`), `[[suggests]]`, `[[publishes]]`, plus:

- `ui = "ui/"` and `module = "app.wasm"`, either or both.
- `[window]`: title, icon, default size, whether it is pinned on install.

## 4. The sandbox

- **Logic:** the same Wasmtime host as connectors (Phase 3: 32 MB, epoch limit per call),
  ticked at a rate the manifest asks for (max 100 Hz). It sees only its roles, by role name; it
  never learns part names or ports. No filesystem, network or clock imports: time is passed in.
- **UI:** an iframe with `sandbox="allow-scripts"` (no same-origin), served from the core under
  `/apps/<id>/`, with a CSP that forbids network access. It talks only through `postMessage` to
  the shell, which checks every request against the app's roles. So a UI can't call the API
  directly or reach the internet.
- **Leases:** an app asks for a lease on a role; the core grants it as `holder = "app:<id>"`.
  When the app's logic traps, its UI window closes, or the app is stopped, every lease ends.

## 5. In the core and the shell

| Piece | Where |
|---|---|
| WIT `faber:app` | `wit/app/` |
| App host (logic) | `core/src/apps/` (layer 3, reuses `wasm`) |
| Role bindings | `config/apps.toml`: `[apps.sennep.roles] lidar = "lidar"`, … (TOML, D13) |
| Derived signals | addresses `app:<id>/<name>` in the router, read-only (D24) |
| API | `GET/POST/DELETE /api/v0/apps`, `PUT /api/v0/apps/{id}/roles` |
| Shell | app windows (iframe host + message channel), the role picker in the Install window, an "app" badge on derived signals |
| JS SDK | `sdk/js/shell` → npm `@faber-os/shell` (later published) |

## 6. The SENNEP app v0 (roadmap)

LiDAR view, manual drive (on-screen joystick), battery, proximity ring, and `sennep:pose`
published as a derived signal. It runs first in the apartment world (Phase 2), then on the real
SENNEP through FaberLink (Phase 4).

## 7. Open questions for Mikkel

1. **Apps in Rust only for logic**, or also JavaScript logic (in the UI, running only while the
   window is open)? Recommendation: both; logic that must keep running (a safety stop, a
   controller) belongs in wasm, a dashboard can be UI-only.
2. **Where does SENNEP's app live:** in the SENNEP V2 repo (it is SENNEP's) or in Faber's
   examples? Recommendation: SENNEP V2 repo, with a small example app in Faber.
3. **Machine-config export/import** (roadmap): parts.toml + apps.toml + settings as one file.
   The backup zip already holds all of it; is a separate "share my setup" file still wanted?
   Recommendation: a "Share setup" that makes a backup without your files.

## 8. As built (Phase 5a: apps in the core)

**The app world** (`wit/app/app.wit`, `faber:app@0.1.0`). An app's logic exports
`start(now-us)`, `tick(now-us)` and `stop()`, and imports only:

| Interface | Functions |
|---|---|
| `roles` | `all()` (name, capability, command, how many parts), `latest(role, index)`, `poll(role, index)`: the newest reading only if this app hasn't polled it before ("subscribe by polling") |
| `drive` | `acquire`, `renew`, `release`, `send(role, index, command) -> result<command, drive-error>`. The core holds the lease for the app as `app:<id>`; the app never sees a lease id. Errors: `no-such-role`, `held(who)`, `lease-required`, `lease-ended(reason)`, `estop-engaged`, `bad-command`, `part-not-ready` |
| `publish` | `publish(name, reading)`: only `[[publishes]]` names, with their capability; anything else stops the app as a bug |
| `state` | `get`, `set` (fails past 64 KB of keys and values), `remove`, `keys`; saved in `<data>/apps/<id>/state.toml` at most once a second and when a run ends, kept across updates, removed on uninstall |
| `log` | `log(level, message)`, as `app:<id>` in the log, 50 lines a second |

Readings and commands are the connector world's own types (`faber:connector/types`), so an app
reads exactly what a connector published. `start` gets the time too, unlike the first sketch.

**Guest SDK** (`sdk/rust/app-wasm`, `faber-app-wasm`): an app is a type implementing `App`
(`start`, `tick`, `stop`) against a `Host` trait, plus `faber_app_wasm::export!(MyApp);`.
`testing::TestHost` plays the core, so the logic is tested on the PC (`cargo test`).

**Manifest** (`core/src/apps/manifest.rs`): strict like the declarative reader, every mistake with
its line. `[package]` has `type = "app"`, `module` (a `.wasm` next to `faber.toml`), `ui`
(`"ui/"`), `tick-hz` (1–100, default 10, only with a module); at least one of `module` and `ui`.
`[window]` (`title`, `icon`, `width`, `height` 100–4000 px, `pinned`) only with a UI.
`[[requires]]`: `role`, `capability`, `command` (only for a capability that takes commands),
`count` (`"1"` or `"1.."`). `[[suggests]]`: `connector`, `for-role` (a role above).
`[[publishes]]`: `name`, `capability` (not one that takes commands: derived signals are
read-only). `accept = ["part", "derived"]` (a role filled by another app's derived signal) is
**not built**: roles take parts only.

**Packages**: the same `.faberpkg` and installer (api.md, "Packages"). An app's package holds its
component and `ui/**` (limits and allowed types in api.md), installs into `<data>/apps/<id>/`, and
is refused (`409 app-enabled`) to uninstall while the app is on: the simplest safe rule, so
nothing it drives stops by surprise.

**Roles**: `config/apps.toml` (`core/src/roles.rs`, layer 2), edited keeping comments.
`PUT /api/v0/apps/{id}/roles` checks that each part exists and provides the role's capability.

**The host** (`core/src/apps/`, layer 3): one component instance per run, under the connectors'
limits (32 MB, 20 ms per call) and import check, on its own thread `app:<id>` ticked at
`tick-hz`. A trap, a limit or a broken rule ends the run, its leases end (the parts get their safe
state), and it restarts after 1, 2, 4, … s; after 5 crashes in a row it stays stopped. The
machine (`core/src/machine/apps.rs`) runs an app when it is on and every role has a part
providing the role's capability; it checks again whenever something changes and every 200 ms,
so an app stops when a part leaves its role (`conflict`, naming the part) and starts again when
the part is back. Statuses use the parts' states (api.md, "Apps"). The shared Wasmtime engine,
limits, import check and WIT bindings moved from `wasm` into `sandbox` (layer 2), since `apps`
and `wasm` are both layer 3 and may not import each other.

**Derived signals**: `app:<id>/<name>` in the router while the app runs, read-only (no leases or
commands: those are by part name, and a part's name never holds a `:`). They are read like a
part's: `GET /api/v0/signals/app:<id>/<name>` and the `signal` topic.

**The example** (`examples/apps/wall-follower`): roles `lidar` (`laser-scan@1`) and `base`
(`planar-base@1`, `command = true`). It keeps the base's right side 0.3 m from a wall (sliding
sideways, so it needs a holonomic base like SENNEP), turns left at corners, and publishes the gap
as `app:wall-follower/wall-distance`, a **`distance@1`** (apps.md's sketch said `distance@1` too:
it is the capability that fits a single measured distance, with `no-target` when there is no
wall). `core/tests/apps.rs` proves it in the apartment world.

**The `faber` command**: `faber new app <name>` (template `templates/app-rust`: a nearest-distance
app over a `count = "1.."` role, with a UI), `faber check` (manifest, UI files, build, load in the
sandbox, then start, 20 ticks and stop with no parts picked) and `faber pack`.

**Not built in 5a**: the shell side and apps in backups (both built in 5b, §9), and roles filled
by derived signals (`accept`).

## 9. As built (Phase 5b: apps in the shell)

**Installing.** Opening an app's `.faberpkg` shows the Install window's app pane
(`shell/src/windows/InstallApp.svelte`): name, version, publisher, how it runs, its README (safe
minimal Markdown, as for connectors), the "Not checked by the Faber registry" warning, and **what
it needs**: a picker per role offering only the parts that provide the role's capability (a
select for `count = "1"`, checkboxes for `"1.."`; the only part that fits is suggested). A role
with `command = true` says it in words: "Wall follower will be able to drive "base"." Its derived
signals are listed with an **app** badge (D24). Install, then `PUT …/roles` with the picks, then,
if "Turn it on now" is ticked (only possible once every role has a part), `POST …/enable`.

**Apps** is a tool of its own (Start → Apps, `tool:apps`), not a Settings section: apps are what a
beginner installs first (principles.md, step 0), and it is work with live state, like Parts, not a
preference. Settings → Connectors lists connector packages only. Each app shows its state in words
(the part states, with the core's message and hint), an on/off switch that shows what the core
says (it flips when the `apps` topic does), its parts (the same picker; Save restarts an app that
is on), Open (its window), its derived signals with the badge and a viewer, and Uninstall,
disabled while it is on with the reason ("Turn it off to uninstall it, so nothing it drives stops
by surprise"). The list comes only from the `apps` topic: an answer to "turn it on" can be older
than what the topic already said.

**Icons.** An app's own `icon.svg` (at the root of its package) is served at
`/apps/<id>/icon.svg` as an image only (api.md, "Apps"); `AppInfo.icon` names the icon to show
(its `[window]` icon, else that one). The shell shows it with `<img>` in the window's title, the
taskbar and Apps, never inline, so an SVG can't run script or load anything.

**App windows** (`shell/src/windows/AppWindow.svelte`). An app with a UI opens as a window titled
and sized by its `[window]`: an `<iframe sandbox="allow-scripts" referrerpolicy="no-referrer">`
on `/apps/<id>/ui/index.html`. The page talks only to the shell, through `postMessage`, answered by
the **host** (`shell/src/lib/state/appHost.ts`, plain TypeScript, tested against hostile
messages).

**The protocol, v1** (`sdk/js/shell/src/protocol.ts`, shared by the SDK and the host). Every
message carries `faber: 1`. A request is `{faber, id, type, …}`; each gets one reply,
`{faber, re: id, ok: true, value}` or `{faber, re: id, ok: false, error: {code, message, hint}}`.

| `type` | Fields | Answers |
|---|---|---|
| `hello` | | the app (`id`, `name`, `version`), `roles`, `published`, `status`, `theme` |
| `roles` | | `[{role, capability, command, count, parts}]`: `parts` is how many, never which |
| `status` | | `{enabled, state, message?}` |
| `theme` | | `{scheme: 'light' \| 'dark', tokens}`: a fixed list of the shell's CSS tokens |
| `latest` | `source` | the newest reading or `null` |
| `subscribe` | `source`, `maxHz?` | `{subscription}`; then `signal` events. At most 20 Hz (default 10) and 16 at once |
| `unsubscribe` | `subscription` | `null` |
| `drive` | `role`, `index?` | `null` once the lease is held |
| `renew` | `role`, `index?` | `null`; keeps the lease alive without a command |
| `command` | `role`, `index?`, `command` | `{applied, clamped}` |
| `release` | `role`, `index?` | `null`; the part gets its safe state |

A `source` is `{role, index?}` (a role's part, `index` for a `1..` role) or `{published: name}`
(the app's own derived signal). Events, unasked: `signal`, `signal-ended`, `status`, `theme`,
`lease-ended` (`role`, `index`, `reason`, `error`). Error codes: `bad-request`, `unknown-request`,
`unsupported-version`, `unknown-role`, `unknown-signal`, `no-part`, `not-commandable`, `app-off`,
`not-confirmed`, `lease-required`, `bad-command`, `too-many-requests` (over 200 a second),
`too-many-subscriptions`, `app-gone`, and the core's own (`lease-held`, `estop-engaged`, …).

**The sandbox, as built.**

- The iframe has `allow-scripts` only: no `allow-same-origin`, so the page's origin is opaque. Its
  files are served with `sandbox allow-scripts` in their CSP too, so a UI page opened on its own is
  just as sandboxed and never runs as the shell's origin. `connect-src 'none'` blocks fetch,
  XHR and WebSockets; `frame-src 'none'` stops it framing the shell; `form-action 'none'`.
  Its own files answer the opaque origin (`Access-Control-Allow-Origin: null`) so module scripts
  load.
- The shell's page has `frame-src 'self'`, so the frame can't be navigated to another site to
  carry what it read away.
- The host reads only messages whose `source` is this window's own frame **and** whose origin is
  `null`; anything else is dropped unanswered. Every field is checked; a role the app doesn't
  have, a part index it doesn't have, a signal it doesn't publish, a command on a role without
  `command = true` or of another capability are refused before anything reaches the core.
- An app sees roles only: never part names, ports or other parts.
- Nothing an app sends is ever shown as HTML by the shell: its title and errors are text, its
  icon an `<img>`.
- A new page in the frame (a reload), the app turned off, a role given another part, Stop, and
  closing the window end what the page had (leases, subscriptions); answers for an old page are
  dropped.

**Driving from a window.** Only a `command = true` role, only while the app is on (the switch is
the user's "this app may act"), and only after the **D46 confirm**, asked by the shell, outside
the frame, once per session like the Drive window ("Wall follower wants to drive "base"…").
Background logic needs no confirm (5a). The lease is held as **`app:<id>`**, the same holder as the
app's logic: the core's lease rules key on lease ids, not holder names, so this gives the window
no extra right, and every lease list and "… is driving" message shows the user the app they
installed. Two holders of one app can't drive one part at once (leases are exclusive). The host
never renews on its own: the SDK's `Drive` renews every 100 ms from the page's own timer, so a page
that hangs or is closed lets the lease lapse within 250 ms (the dead man's switch). While a window
drives, the shell shows a bar above the frame, "Wall follower is driving base.", with a **Stop**
the app can't hide.

**`@faber-os/shell`** (`sdk/js/shell`, README there): one dependency-free ES module,
`faber.hello/roles/latest/subscribe/published/subscribePublished/drive/status/onStatus/theme/
onTheme`, `applyTheme`, `connect` for tests, errors as `FaberError` (`code`, `message`, `hint`).
Tested with vitest against a fake parent window. Not published to npm (it is `private` until
Mikkel decides).

**The wall follower's window** (`examples/apps/wall-follower/ui`): on/off, and the wall distance
live from its own derived signal, in the shell's look. It carries the built SDK (the SDK's test
rewrites the copy when the SDK changes). No speed slider: see "Not built".

**Backups** hold `config/apps.toml` and every app's folder with its `state.toml`; a restore checks
each app like an install (api.md, "Storage and backups").

**Tests.** `shell/src/lib/state/appHost.test.ts` sends the host hostile messages (other origins,
other windows, junk, unknown types, roles the app lacks, commands on read-only roles, floods);
component tests cover the Install pane, Apps and the app window (the sandbox attribute, the D46
sheet, Stop, closing); `shell/e2e/apps.spec.ts` installs the packed example from the desktop, runs
it in the apartment and watches its window; the layout test opens an app window.

**Not built in 5b:** a window changing its app's saved state (no `state` messages: the logic owns
`state.toml` while it runs, so writing it from the API needs a shared store first); apps in the
start menu and pinning an app's window to the taskbar (`[window] pinned` is read but not used);
the template (`faber new app`) doesn't carry the SDK yet; roles filled by derived signals
(`accept`, from 5a).
