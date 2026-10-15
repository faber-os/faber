# Phase 5 design: apps

Status: **design v1; Phase 5a (apps in the core) built** (2026-10-15). Builds on D03, D04, D24,
D29 and architecture.md §7. §8 says what was built and how it differs; the shell side (app
windows, the role picker, `@faber-os/shell`) is Phase 5b and not built yet. The open questions in
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

**Not built in 5a**: the shell (app windows in a sandboxed iframe, the message channel and
`@faber-os/shell`, the role picker in the Install window, the "app" badge), roles filled by
derived signals (`accept`), and apps in backups: a backup holds neither `config/apps.toml` nor
`<data>/apps/` yet, though its `packages.toml` lists installed apps.

