# Phase 5 design: apps

Status: **design v1, proposed** (2026-10-15). Builds on D03, D04, D24, D29 and architecture.md
§7. Nothing here is built yet.

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
