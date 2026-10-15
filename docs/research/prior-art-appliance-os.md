# Prior art: Pi appliance operating systems

Researched 2026-10-15 from official repos and docs (linked). Points marked *(unverified)* were
not confirmed against a live source. Companion file:
[prior-art-hubs-and-robotics.md](prior-art-hubs-and-robotics.md).

## Summary table

| | Base | Extension unit | Store | Updates | Licence | Health (Oct 2026) |
|---|---|---|---|---|---|---|
| **CasaOS** | install script on Debian / Raspberry Pi OS | docker-compose + `x-casaos` block | GitHub repo → built `dist/` index; 3rd-party stores by URL | script | Apache-2.0 | stalled (last stable Dec 2024) |
| **ZimaOS** | Buildroot image | same as CasaOS | same | OTA, likely RAUC | no licence; freemium since 1.5 | active |
| **Umbrel** | Debian-based image (since 1.0) | `umbrel-app.yml` + compose | GitHub repo, PR-reviewed; community stores = template repo, namespaced IDs | A/B via Rugix Ctrl | PolyForm **Noncommercial** | very active, VC-backed |
| **balena** | Yocto image | compose + feature labels | balenaHub (catalog) | container deltas + A/B host | Apache-2.0 / openBalena AGPL | healthy, cloud-first |
| **DietPi** | minimal Debian image | bash installer in core repo (~200 titles) | none (PR to core) | in place, no A/B | GPLv2 *(unverified)* | active |
| **HA OS** | Buildroot image | Docker "apps" (were add-ons) | git repos by URL | RAUC A/B | Apache-2.0 | very healthy |
| **OctoPi / MainsailOS** | stock Pi OS + CustomPiOS modules | — | — | git/apt via Moonraker, no rollback | GPLv3 | active |

## CasaOS and ZimaOS (IceWhale)

- CasaOS is a set of Go microservices (gateway, app management, storage, users, **message bus**)
  with a Vue dashboard. It is installed by a `curl | bash` script on top of an existing OS.
- Apps are compose files in [CasaOS-AppStore](https://github.com/IceWhaleTech/CasaOS-AppStore).
  CI builds a `store.json` index, and third-party stores publish the same layout at a URL.
- Isolation is Docker only, and users routinely paste in arbitrary compose files with `privileged: true`.
- **Lesson:** CasaOS stagnated once the company built ZimaOS (no licence, then paywalled in 1.5,
  [Notebookcheck](https://www.notebookcheck.net/ZimaOS-1-5-0-goes-paid-community-reacts-with-dissapointment.1122850.0.html)).
  Open-core splits can kill the open half.

## Umbrel

- A TypeScript daemon plus a React UI. It has the **most polished web desktop** of the group:
  wallpaper, dock, app windows, search, widgets.
  2.0 (Sep 2026) added multi-user and an MCP server ([announcement](https://community.umbrel.com/t/introducing-umbrelos-2-0/25444)).
- **Store mechanics worth copying** ([umbrel-apps](https://github.com/getumbrel/umbrel-apps)):
  - one folder per app with `umbrel-app.yml` + `docker-compose.yml`;
  - PR review, with the rule that an app must open to a usable UI after install;
  - apps can **depend on other apps**;
  - community stores are a template repo with a store id, and every app id is **prefixed with that id**.
- Rebuilt as a real image in 1.0, because an install script **could not update the kernel or
  drivers**. Uses A/B updates via **Rugix Ctrl** on Pi `tryboot`.
- The licence is source-available, noncommercial. Nobody can sell a kit built on it.

## balena

- Yocto host with a container engine and a **Supervisor that reconciles the device to a target
  state**. Hardware access is granted by compose labels (`io.balena.features.*`) or `privileged`.
- It has the best-engineered updates of the group (binary deltas, A/B host, update locks).
- It is cloud-first. The self-hosted openBalena is still beta, CLI-only, and has no host OS
  updates ([forum](https://forums.balena.io/t/enable-balenaos-host-updates-in-openbalena/373321)).

## DietPi

- Bash tools on a minimal Debian image per board. `dietpi-software` is a curated catalogue
  maintained in the core repo.
- **The idea worth copying:** `dietpi.txt` on the FAT boot partition, for headless first-boot
  setup (Wi-Fi, software, script) ([docs](https://dietpi.com/docs/usage/)).

## Others

- **Raspberry Pi Connect:** browser remote shell and screen, plus Remote Updates (beta, Mar 2026)
  that support tryboot A/B ([post](https://www.raspberrypi.com/news/new-remote-updates-on-raspberry-pi-connect/)).
  It is proprietary and needs a Raspberry Pi ID, so it fits only as an optional channel.
- **OctoPi / MainsailOS:** a stock Pi OS image plus modules (CustomPiOS / CustoPiZer). These serve
  exactly the maker audience, and they live without rollback.

## Building a Pi 5 image

| Tool | Verdict for a solo dev |
|---|---|
| **rpi-image-gen** (official, BSD-3) | **Best fit.** YAML layers, binary Debian packages, rootless, Trixie, recommended by Raspberry Pi for A/B layouts. Needs an arm64 Debian host (a Pi 5 works). [repo](https://github.com/raspberrypi/rpi-image-gen) |
| **Rugix Bakery** (MIT/Apache) | Strong alternative: Dockerfile-like, OTA-ready with Rugix Ctrl, what Umbrel ships. [repo](https://github.com/rugix/rugix) |
| pi-gen | Proven but awkward to customise |
| CustomPiOS | Easiest start, but A/B is hard to add later |
| Buildroot / Yocto | Too much packaging work for one person |

## A/B updates

| Tool | Verdict |
|---|---|
| Pi firmware `tryboot` | The native A/B primitive on Pi 4/5; build on top of it |
| **Rugix Ctrl** | **Top pick:** tryboot support, deltas, signatures, permissive licence, proven at Umbrel |
| RAUC | Most mature (HAOS), but needs U-Boot or a custom backend on Pi 5 |
| Mender, SWUpdate | Avoid for this project |

## Lessons for Faber

1. **Ship a real image with A/B from the first public release.** Retrofitting A/B means asking users to re-flash. During development, the core just runs on a PC or plain Pi OS.
2. **Copy Umbrel's store:** one folder per package in a GitHub repo, PR review, CI that lints manifests, checks builds and checksums, namespaced third-party stores added by URL.
3. **Declarative hardware access, one level higher than HAOS:** apps declare capabilities, connectors declare port claims, and the core enforces and arbitrates both.
4. **One isolated unit per connector, restarted on crash,** plus balena-style reconciling to a target state.
5. **`faber.txt` on the boot partition** for headless setup (DietPi idea).
6. **An OSI licence, and keep the core whole.** No open-core, no noncommercial clause.
7. **Tailscale built in, not an app.** Add a hotspot fallback with a captive portal.
8. **Docker is not the extension unit.** It is too heavy for connectors and gives too-coarse hardware access.

## Gaps none of them fill (Faber's reason to exist)

1. A **typed hardware model** on a local, open appliance OS. Viam has one, but it is cloud-coupled and AGPL.
2. **Port brokering and conflict detection** (two users of one UART, I2C address clashes), plus declarative device-tree overlays.
3. **Built-in bench tools:** serial monitor, I2C scanner, GPIO panel.
4. **One UI for the remote browser and the attached screen.**
5. **Microcontroller companions as managed peers:** flash firmware from the store, a versioned protocol, health checks.
6. **Robot safety semantics:** e-stop, "don't update while armed", actuators made safe when a connector dies.
7. **A self-hostable update channel** (signed bundles on GitHub Releases plus a JSON index).
