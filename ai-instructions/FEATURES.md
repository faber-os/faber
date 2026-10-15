# Features

Phase 1 is built (see `docs/roadmap.md`).

| Feature | Status | Description | Date | Depends on | Blocked by |
|---|---|---|---|---|---|
| Design docs | Implemented | README, architecture, glossary, decisions (D01–D28), roadmap, research | 2026-10-15 | — | — |
| Name | Implemented | **Faber** (was the working name AlmaOS); identifier rules in `docs/research/naming.md` | 2026-10-15 | — | — |
| Licence | Implemented | Apache-2.0 + NOTICE | 2026-10-15 | — | — |
| Repo | Implemented | https://github.com/faber-os/faber (public); `faber-registry` comes in Phase 7 | 2026-10-15 | — | — |
| `faber-core` crate name | Implemented | Placeholder 0.0.1 published on crates.io to reserve the name | 2026-10-15 | — | — |
| Capability spec | Implemented (design) | 41 capabilities, common rules and the lease in `docs/capabilities.md`; WIT sketches in `wit/capabilities/` | 2026-10-15 | — | — |
| Design principles | Implemented (design) | Ease of use first, `docs/principles.md` (D30) | 2026-10-15 | — | — |
| Core (Phase 1) | Implemented | `faber-core`: parts.toml config, port manager (serial + replay, claims), supervised runtime, router, REST + WebSocket API v0 | 2026-10-15 | — | — |
| LD19 connector | Implemented | `connectors/ldrobot-ld19`, built-in Rust, tested on a real capture | 2026-10-15 | Core | — |
| Demo recording | Implemented | `replay:demo-ld19` built into the core, so Faber works with no hardware | 2026-10-15 | Core | — |
| Shell v0 | Implemented | Svelte desktop: taskbar, start menu, windows, Parts + Add part, laser-scan viewer, serial monitor, logs, about | 2026-10-15 | API v0 | — |
| CI | Implemented | fmt, clippy, tests (Linux + Windows), arch rules, WIT, shell checks, browser e2e against the release build | 2026-10-15 | — | — |
| Files, Notes, Settings, recordings | Implemented | Home folder with Trash, System folder (validated parts.toml), developer full-machine access, serial monitor recordings as replay ports, optional sample (D32–D35) | 2026-10-15 | Core | — |
| Shell design picks | Implemented | D38: logo lockup, icon plates, menus, taskbar sizes + pin/unpin, tabbed windows, start menu with power options (disabled until the image), Files views + zoom, six styles × light/dark | 2026-10-15 | Shell v0 | — |
| Removable disks | Implemented | D39: USB sticks and SD cards as `disk-<name>:/` places in Files (Windows WMI, Linux /proc/mounts), no Trash | 2026-10-15 | Files | — |
| Uploads without overwrite | Implemented | `PUT files/content?mode=create`; the shell asks Replace / Keep both / Skip | 2026-10-15 | Files | — |
| Settings batch 1 | Implemented | D40–D43: accessibility (motion, contrast, text size), units (length), date and time in the machine's zone, backups to home:/Backups with restore, storage use, About and diagnostics, reset to defaults | 2026-10-15 | Settings | — |
| API fingerprint | Implemented | Shell and core detect a mismatched build and ask for a restart | 2026-10-15 | — | — |
| Translations | Implemented | D44: 621 English texts in one catalogue, translation files built in or in `<data>/config/translations/` (no rebuild), per-key English fallback, `npm run i18n:check`, a test that fails on new raw text; picker only with 2+ languages | 2026-10-15 | Shell | — |
| Machine name | Implemented | D45: `settings.machine.name`, browser tab, backups, `<name>.local` later | 2026-10-15 | Settings | — |
| World engine (Phase 2b) | Implemented | `core/src/sim`: geometry, raycast grid, kinematics, LD19 scans, distance cones, a walking person, six prebuilt worlds (empty, apartment, office, warehouse, maze, obstacle course) | 2026-10-15 | — | — |
| Driving core (Phase 2a) | Implemented | distance/motor/planar-base/estop/pose capabilities, generic parts (need a bridge, simulated for now), manual simulated parts, leases with safe states, commands with clamping, the e-stop, `examples/drive-square.py` | 2026-10-15 | — | — |
| World wiring (Phase 2c) | Implemented | world parts in `simparts`, world API and `world` topic, `GET /signals/{part}/{name}`, the Phase 2 done-when test (`core/tests/world.rs`) | 2026-10-15 | 2a, 2b | — |
| Drive window, Stop, Simulated switch (Phase 2d) | Implemented | Simulated switch in Add part, manual sensor sliders/patterns, Drive window (D46 confirm, pad, keys, 20 Hz, lease keeper), taskbar Stop + Pause/Break, distance and base viewers | 2026-10-15 | 2a | — |
| World window (Phase 2e) | Implemented | top view with machine, scan points, cones, movers, trail; world picker; reset; per-part limits; connector settings described by the core and the Add part form built from them | 2026-10-15 | 2c | — |
| Declarative connectors (Phase 3c) | Implemented | `faber.toml` only: serial line patterns with typed fields, NMEA (GGA/RMC/GSA) → `position@1`; loaded from `<data>/connectors/<id>/`; guide in `docs/connectors-declarative.md`; two tested examples | 2026-10-15 | — | — |
| Wasm connectors (Phase 3a) | Implemented | Wasmtime 49 sandbox (32 MB, 20 ms per call), `faber:connector` WIT, guest macro (`faber-connector-wasm`), LD19 as wasm gives identical readings (232/232); `GET /connectors/problems` | 2026-10-15 | — | — |
| Packages and faber CLI (Phase 3b) | Implemented | `.faberpkg` checked fully before install, live install/uninstall, Install window, Settings → Connectors, `position@1` viewer, `faber new/check/pack`, templates, first-connector guide and its end-to-end test | 2026-10-15 | 3a, 3c | — |
| Phase 3 polish | Implemented | backups include installed connectors (re-checked on restore, all or nothing), `packages` WS topic, install errors name the package file, reproducible package bytes | 2026-10-15 | 3b | — |
| FaberLink protocol (Phase 4a) | Implemented | `sdk/rust/faberlink` (COBS, CRC-16, every message, per-capability layouts, core session); `firmware/faberlink` C++11 Arduino library enforcing the board safety rules; interop tests compile the C++ via `cc` (MSVC/g++); CI compiles the examples for Uno and Mega | 2026-10-15 | — | — |
| faber-bridge connector (Phase 4b) | Implemented | sub-parts `mega.left`, leases gate commands, SAFE on lease end, e-stop both ways (board button latches), reconnect, opt-in "Check for a FaberLink board", Parts/Add part; proof in `core/tests/bridge.rs` against the real C++ core | 2026-10-15 | 4a | needs a real Mega |
| Arduino flasher (Phase 4c) | Implemented | `sdk/rust/flash` (Intel HEX, STK500v1/v2, signatures, verify), `POST /ports/{id}/flash` + `flash` topic, firmware in packages, Flasher window; CI exports the FaberLink example's hex as an artifact | 2026-10-15 | 4b | needs a real board |
| Apps in the core (Phase 5a) | Implemented | `faber:app` WIT (roles, drive, publish, state, log), `faber-app-wasm` guest SDK, app packages into `<data>/apps/<id>/` with UI, `config/apps.toml` roles, supervised app host holding leases as `app:<id>`, derived signals `app:<id>/<name>`, apps API + topic, strict-CSP UI serving, `faber new app`, wall-follower example proven in the apartment | 2026-10-15 | Phase 3 | — |
| App windows, JS SDK (Phase 5b) | In progress | iframe app windows, `@faber-os/shell`, role picker, apps in backups | — | 5a | — |
| Restart / shut down | Planned | Power items exist in the start menu but are disabled until the machine image (Phase 6) | — | Image | Phase 6 |
| Copy / paste, multi-select in Files | Planned | Also unlocks moving between disks and home | — | Files | — |
| Real LD19 on the PC | Planned | Mikkel plugs in the LD19 and adds it via its USB port | — | Phase 1 | Mikkel |
