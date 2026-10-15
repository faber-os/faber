# Features

Phase 1 is built (see `docs/roadmap.md`).

| Feature | Status | Description | Date | Depends on | Blocked by |
|---|---|---|---|---|---|
| Design docs | Implemented | README, architecture, glossary, decisions (D01–D28), roadmap, research | 2026-10-15 | — | — |
| Name | Implemented | **Faber** (was the working name AlmaOS); identifier rules in `docs/research/naming.md` | 2026-10-15 | — | — |
| Licence | Implemented | Apache-2.0 + NOTICE | 2026-10-15 | — | — |
| Repo | Implemented | https://github.com/faber-os/faber (public); `faber-registry` comes in Phase 6 | 2026-10-15 | — | — |
| `faber-core` crate name | Implemented | Placeholder 0.0.1 published on crates.io to reserve the name | 2026-10-15 | — | — |
| Capability spec | Implemented (design) | 41 capabilities, common rules and the lease in `docs/capabilities.md`; WIT sketches in `wit/capabilities/` | 2026-10-15 | — | — |
| Design principles | Implemented (design) | Ease of use first, `docs/principles.md` (D30) | 2026-10-15 | — | — |
| Core (Phase 1) | Implemented | `faber-core`: parts.toml config, port manager (serial + replay, claims), supervised runtime, router, REST + WebSocket API v0 | 2026-10-15 | — | — |
| LD19 connector | Implemented | `connectors/ldrobot-ld19`, built-in Rust, tested on a real capture | 2026-10-15 | Core | — |
| Demo recording | Implemented | `replay:demo-ld19` built into the core, so Faber works with no hardware | 2026-10-15 | Core | — |
| Shell v0 | Implemented | Svelte desktop: taskbar, start menu, windows, Parts + Add part, laser-scan viewer, serial monitor, logs, about | 2026-10-15 | API v0 | — |
| CI | Implemented | fmt, clippy, tests (Linux + Windows), arch rules, WIT, shell checks, browser e2e against the release build | 2026-10-15 | — | — |
| Real LD19 on the PC | Planned | Mikkel plugs in the LD19 and adds it via its USB port | — | Phase 1 | Mikkel |
