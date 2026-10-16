# @faber-os/shell

The JavaScript SDK for a Faber **app's UI** ([docs/apps.md](../../../docs/apps.md)). It reads the
app's roles, follows their readings, drives the parts the user let the app drive, and gives the
page the shell's look. One small ES module with no dependencies.

> **Not on npm yet.** The package is `private` until Mikkel says it may be published. Until then,
> build it here and copy `dist/faber-shell.js` into your app's `ui/` folder (the template and the
> wall-follower example carry a copy).

## How an app's UI runs

An app's UI is static files in its package's `ui/` folder. The shell opens it as a window: an
`<iframe sandbox="allow-scripts">` without `allow-same-origin`, served with a content security
policy that forbids the network. So the page **can't** call Faber's API, fetch anything or open
other pages. It can only talk to the shell window around it, and the shell checks every request
against the app's roles (the parts the user picked when installing it).

This SDK is that conversation, written once so your app doesn't have to.

## Use it

```html
<!-- ui/index.html -->
<script type="module" src="app.js"></script>
```

```js
// ui/app.js
import { faber, applyTheme } from './faber-shell.js';

applyTheme(await faber.theme()); // look like the shell
faber.onTheme(applyTheme); // and follow it to dark mode

for (const role of await faber.roles()) console.log(role.role, role.capability, role.parts);

// Readings of a role's part, at most 10 a second.
const stop = faber.subscribe('lidar', (scan) => draw(scan.reading));

// This app's own derived signals ([[publishes]] in faber.toml).
faber.subscribePublished('wall-distance', (d) => show(d.reading));

// Driving a role with command = true. Faber asks the user first (once per session).
const base = await faber.drive('base');
base.onEnd((e) => console.log('stopped driving:', e.message));
await base.command({
  capability: 'planar-base@1',
  command: { kind: 'set-velocity', vx: 0.2, vy: 0, omega: 0 },
});
await base.release(); // the base stops
```

## API

| Function                                                              | What it does                                                                                                                                                                                                                                                                                                            |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `faber.hello()`                                                       | The app (`id`, `name`, `version`), its roles, its derived signals, its status and the theme, in one answer.                                                                                                                                                                                                             |
| `faber.roles()`                                                       | Each role: `role`, `capability`, `command` (may drive it), `count` (`1`, `1..`, or `0..1`/`0..` for an optional role), `parts` (how many parts fill it) and `mounts` (where each sits on the machine: x forward, y left, m and rad; a part's readings are in its own frame). Never which parts: an app sees roles only. |
| `faber.latest(role, {index})`                                         | The newest reading of the role's part (`index` picks one of a `1..` role's parts), or `null`.                                                                                                                                                                                                                           |
| `faber.subscribe(role, cb, {index, maxHz, onError})`                  | Calls `cb` with each new reading, at most `maxHz` a second (default 10, at most 20). Returns a function that stops it. `onError` hears why it couldn't start or ended.                                                                                                                                                  |
| `faber.published(name)`, `faber.subscribePublished(name, cb)`         | The same for the app's own derived signals.                                                                                                                                                                                                                                                                             |
| `faber.drive(role, {index})`                                          | Takes the lease on a `command = true` role's part, after the user's go-ahead. Answers a `Drive`: `command(c)` (answers `{applied, clamped}`), `release()`, `onEnd(cb)` and `ended`. The app must be turned on.                                                                                                          |
| `faber.status()`, `faber.onStatus(cb)`                                | Whether the app is on (`enabled`) and how it runs (`state`: `running`, `stopped`, `invalid`, …, with `message`).                                                                                                                                                                                                        |
| `faber.theme()`, `faber.onTheme(cb)`                                  | `{scheme: 'light' \| 'dark', tokens}`: the shell's CSS custom properties (`--surface`, `--text`, `--text-muted`, `--accent`, `--line`, `--ok`, `--warn`, `--danger`, `--font-ui`, `--radius-md`, `--space-3`, …).                                                                                                       |
| `faber.state.get(key)`, `.set(key, value)`, `.remove(key)`, `.keys()` | The app's saved state: strings by name, shared with the app's logic (its `state` interface) and every window of the app, kept by Faber across restarts and in backups. Keys are 1-256 bytes; keys and values together at most 64 KB. `get` answers `null` for a key that isn't set.                                     |
| `faber.state.onChange(cb)`                                            | Calls `cb(key, value)` for every change, whoever made it (the logic, this or another window); `value` is `null` when the key was removed. Returns a function that stops it.                                                                                                                                             |
| `applyTheme(theme)`                                                   | Sets those tokens on the page's root and its `color-scheme`, so `var(--surface)` works in your CSS.                                                                                                                                                                                                                     |
| `connect({parent, self, timeoutMs})`                                  | A connection of your own (tests use it with a fake shell); `faber` is one made on first use.                                                                                                                                                                                                                            |

**Driving safely.** The lease on a part lasts 250 ms unless something renews it. A `Drive` renews
it every 100 ms from your page's own timer, so if your page hangs or its window is closed the lease
lapses and the part stops. The e-stop, an operator taking over, the part going away or the app
being turned off end it too: `onEnd` says why, and `command` then fails with `lease-ended`. Call
`faber.drive()` again to drive on.

## Errors

Every failure is a `FaberError` in Faber's house style: `message` says what happened, `hint`
what to try next, and `code` is stable for programs.

| `code`                                        | When                                                                                                        |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `no-shell`                                    | Nothing answered within 5 s: the page isn't open in a Faber app window.                                     |
| `unknown-role`, `unknown-signal`              | The app has no such role, or doesn't publish that signal.                                                   |
| `no-part`                                     | No part fills the role (or not that many: `index`). Pick its parts in Faber's Apps window.                  |
| `not-commandable`                             | Driving a role without `command = true`.                                                                    |
| `app-off`                                     | Driving while the app is turned off.                                                                        |
| `not-confirmed`                               | The user didn't let the app drive.                                                                          |
| `lease-required`, `lease-ended`               | A command without driving first, or after the lease ended.                                                  |
| `bad-request`, `unknown-request`              | A message the shell doesn't understand (a bug in the app, or an SDK newer than the shell).                  |
| `bad-key`, `state-full`                       | A state key that is empty or longer than 256 bytes, or a value that would make the state larger than 64 KB. |
| `too-many-requests`, `too-many-subscriptions` | More than 200 messages a second, or 16 subscriptions at once.                                               |
| the core's codes                              | `lease-held` (someone else drives it), `estop-engaged`, `bad-command`, `part-not-ready` … as in the API.    |

## The protocol

Messages are plain objects posted to `window.parent`, each with `faber: 1`. A request has an `id`
and a `type`; the shell answers each with `{faber: 1, re: id, ok: true, value}` or
`{faber: 1, re: id, ok: false, error: {code, message, hint}}`, and sends events (`signal`,
`signal-ended`, `status`, `theme`, `lease-ended`, and `state` once asked with `state-watch`) on
its own. Every message type is in
[`src/protocol.ts`](src/protocol.ts), which the shell's host imports too.

## Develop it

```bash
npm ci
npm test              # vitest, against a fake shell window
npm run check         # types
npm run format:check
npm run build         # dist/faber-shell.js and its .d.ts
```

`npm test` also checks that the example apps carry exactly the built module: when it changes, the
test rewrites their copy and fails once, so you commit it.
