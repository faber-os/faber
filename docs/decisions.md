# Decisions

Short records of *why* things are the way they are, so they don't get re-debated. Status is
**accepted** (Mikkel approved), **proposed** (written by an AI agent, waiting on Mikkel), or
**superseded by Dnn**.

| # | Decision | Status | Why (short) |
|---|---|---|---|
| D01 | Faber is general purpose. SENNEP is its first *app* and its field test, not its purpose. | accepted | More useful to others, and it forces a clean core/app split. SENNEP keeps the scope honest: build only what a real project needs. |
| D02 | HA-OS-style appliance (Linux underneath, never visible) plus a web desktop. No custom kernel, no drivers. | accepted | A kernel or drivers would be years of work. The value is in the hardware model and the UX. |
| D03 | Exactly four concepts: **core, connector, tool, app**. Exactly two package types: connector and app. | accepted | HA's add-on / integration / platform confusion. See [glossary.md](glossary.md). |
| D04 | Apps depend on **capabilities**, never on connectors. | accepted | It makes apps hardware-independent, and it's how the ecosystem scales. |
| D05 | A fixed, versioned starter set of capabilities. New ones only through registry review. | accepted | Ad-hoc capabilities would fragment apps (HA entity domains, Node-RED's untyped payloads). |
| D06 | Capability data shapes follow ROS `sensor_msgs`/`geometry_msgs`, SI units, µs timestamps. | accepted | Battle-tested shapes, and a cheap ROS bridge later. |
| D07 | Third-party code runs as **WebAssembly** (Wasmtime, Component Model, WIT). Native connectors allowed, but marked unsandboxed. | accepted | None of HA, Viam, CasaOS or Node-RED sandboxes extensions. This is a real differentiator, and it's safer for a store. Native covers camera/ML. |
| D08 | Declarative (manifest-only) connectors as a first-class kind. | accepted | ESPHome shows most simple sensors need no code. Lowers the bar for contributors. |
| D09 | The core is one Rust process. WASM packages run inside it, in isolated instances. | accepted | Simple, small, debuggable, runs on a dev PC. Native connectors are the only extra processes. |
| D10 | The port manager owns all hardware access and arbitrates claims. | accepted | Enables permissions, conflict detection and the built-in tools. No other platform has it. |
| D11 | Real-time stays on microcontrollers via **bridges** (FaberLink). The Pi side is supervisory. | accepted | Linux and WASM aren't real-time. Matches SENNEP's Pi ↔ Mega split. |
| D12 | Safety primitives in the core: leases, safe states, e-stop, update lock, bridge heartbeat. | accepted | Things that move need it, and nobody else provides it. |
| D13 | Config is TOML files. The UI edits the same files. | accepted | Avoids HA's YAML-vs-UI split. Git-friendly. TOML over YAML: no indentation traps, native in Rust. |
| D14 | One shell for remote browsers and the attached screen (Cage + Chromium kiosk). | accepted | Mikkel wants the desktop on an attached screen. One codebase. |
| D15 | Network: mDNS `<name>.local` + hotspot fallback + built-in Tailscale. Public tunnel opt-in, with a warning. | accepted | Mikkel asked for local, Wi-Fi/online and Tailscale. Exposing something that moves to the internet must be a deliberate choice. |
| D16 | Auth and roles in the core from v0.1. | accepted | Retrofitting auth is how insecure products happen. |
| D17 | Ship a real image (rpi-image-gen, Debian Trixie) with A/B updates (Rugix Ctrl, tryboot) from the **first public release**. Develop on a PC before that. | accepted | Umbrel rebuilt from install script to image because a script couldn't update the kernel. Retrofitting A/B means re-flashing. Buildroot/Yocto are too heavy for one person. |
| D18 | Store = GitHub registry repo, PR-reviewed, CI-validated. Namespaced third-party registries by URL. | accepted | Umbrel/CasaOS model, no server to run, free. Namespacing prevents id collisions. |
| D19 | Licence: **Apache-2.0** for everything (core, SDK, first-party packages), with a `NOTICE` file ("Copyright 2026 Mikkel Ryborg Blom"). Store packages may carry their own licences. | accepted | Open source with no open-core and no noncommercial clause (CasaOS stalled, ZimaOS paywalled, Umbrel is noncommercial). Anyone may use it, including commercially, as long as the credit stays. Apache adds patent protection over MIT. |
| D20 | Not a home-server OS (no media/NAS/Docker-app focus). | accepted | CasaOS and Umbrel already do that well. The gap is hardware. |
| D21 | Apps never provide capabilities. | superseded by D24 | — |
| D22 | Faber is designed on its own terms first. SENNEP is then **retrofitted** onto it as the field test. FaberLink is designed fresh, not derived from SENNEP's protocol (answer to Q4). | accepted | SENNEP's Pi ↔ Mega protocol is only early pre-work, and SENNEP is still at the first-prototype print stage. |
| D23 | Personal notes and explainers live in `private/` (git-ignored), never in the public docs. | accepted | Explainers written for Mikkel (e.g. what WASM is) aren't project documentation. |

| D24 | Apps may publish **derived signals**: computed data (pose, map, nearest obstacle) under the app's own name, typed with a capability shape. They can never claim ports or accept commands. A role accepts a derived signal only if the app's manifest opts in. (Answer to Q7.) | accepted | Lets apps share computed data without a fifth concept. SENNEP tests it by publishing its pose. |
| D25 | Name: **Faber** (Latin "maker"). The image is called "Faber OS" in prose. Identifiers follow the rules in [research/naming.md](research/naming.md) so a rename stays a search and replace. | accepted | The working name AlmaOS collided with AlmaLinux OS. Faber has no OS/robotics collision, but the bare `faber` crate, npm scope and GitHub name are taken, hence `faber-os` for the org and npm scope. |
| D26 | Wire format (core ↔ shell, core ↔ native connectors, recordings): **MessagePack**, with **JSON** as a debug fallback (`?format=json`). Both come from the same Rust types, and WIT stays the only schema. (Answer to Q3.) | accepted | Compact and fast, mature in Rust and the browser, no second schema language. JSON keeps debugging readable. |
| D27 | Signing (answer to Q5): **phase 1:** HTTPS + sha256 pinned in the registry at review, plus **minisign**. The Faber maintainer holds two keys: the **OS-image key** (offline) and the **registry-index key**. **Later: TUF** (`tough`) directly, for expiry, rollback protection and key rotation. No GitHub-account-based trust (no SSH-key or attestation trust model). | accepted | A GitHub account is not a trust anchor. minisign is the simplest thing that protects the OS image and the index. TUF is the proper long-term design for a store. |
| D28 | Shell: **Svelte 5 + Vite**, built as a static single-page app served by the core (no Node.js on the machine). The shared widget kit (`<faber-gauge>`, `<faber-scan-view>` …) comes later as **Lit web components**, so app UIs in any framework can use it. Future docs website: Astro Starlight. (Answer to Q2.) | accepted | Fine-grained reactivity suits a desktop full of live values better than React's re-rendering. Small and readable for contributors. Web components keep app authors free to choose their own framework. |

## Open questions

None right now. Q1–Q7 are resolved (in D19 and D22–D28). New questions start at Q8.
