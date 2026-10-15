# Name research

## Decision: Faber (2026-10-15)

**Faber** is Latin for "maker" (*homo faber*, "man the maker"). The Pi image is called **Faber OS**
in prose, so people can search for it. AlmaOS was the working name and was dropped (see below).

### How the name is written (keeps a rename a search and replace)

The name appears in exactly three spellings, and never as an abbreviation, a pun or a word fused
with something else:

| Form | Used for | Examples |
|---|---|---|
| `Faber` | prose, UI, display names | "Faber", "Faber OS", `FaberLink` |
| `faber` | every identifier | `faber.local`, `faber-core`, `faber.toml`, `faber.txt`, `.faberpkg`, `faber pack`, the `faber` registry id, repos `faber` and `faber-registry` |
| `FABER` | environment variables | `FABER_DATA_DIR` |

GitHub org and npm scope: **`faber-os`** (`@faber-os/shell`), because the bare `faber` names are
taken. To rename: case-sensitive replace `Faber` → `New`, `faber` → `new`, `FABER` → `NEW`, then
rename files and folders containing `faber`. **Exclude this file** (it talks about other "Faber"s).
Once crates, a domain or a GitHub org are published, renaming costs more than the text change.

### Collision check for Faber (web, 2026-10-15)

- **No OS, Pi, robotics or maker project** named Faber or Faber OS was found.
- Small unrelated software projects also use the name: a Laravel deployment CLI plus MCP server,
  `faberjs` (a Node framework, GitHub org `Faberhq`), and a new "Faber" MCP product (GitHub `getfaber`).
- **Trademarks** (not formally searched): Faber S.p.A. (cooker hoods) is the most plausible
  class-9 holder. Faber-Castell and Faber & Faber are a different market. Search EUIPO/WIPO classes
  9 and 42 before any commercial use.
- **Taken:** crates.io `faber` (a 2022 placeholder), npm scope `@faber`, and on GitHub `faber`,
  `faberos`, `faber-dev`, `faberhq` and `getfaber`.
- **Free:** crates.io `faber-core` and `faber-os`, GitHub `faber-os` and `usefaber`. The domains
  `faber.dev`, `faberos.dev`, `faber-os.org` and `faberos.org` had no DNS records (check them at a registrar).
- It is a generic word, so "Faber OS" should always be paired with "OS" or "robot" in
  descriptions to stay searchable.

## Earlier research (superseded)

Status: **open**. AlmaOS is dropped (Mikkel, 2026-10-15). It stays the folder name until a new
name is picked. The first alternatives below were rejected too, and a wider brainstorm is in progress. Researched 2026-10-15
with web searches only. No formal trademark (USPTO/EUIPO) or WHOIS checks were done yet.

## AlmaOS: technically free, practically confusing

- No existing product called "AlmaOS" was found. Searches return **AlmaLinux OS**, a major
  RHEL-compatible Linux distribution. People already shorten it to "Alma", and it ships official
  **Raspberry Pi images**. "AlmaOS for Raspberry Pi" would read as an AlmaLinux spin.
- The "AlmaLinux OS" mark is held by the Linux Foundation. Its usage policy warns against similar
  names on similar offerings: <https://almalinux.org/p/the-almalinux-os-trademark-usage-policy>.
- GitHub: `AlmaOS` is taken (an idle personal account), `alma-os` is free.
- crates.io / npm: `almaos` and `alma-os` are free. `alma` is taken.
- Domains: `almaos.com` is parked. `almaos.org` and `almaos.dev` did not resolve.

**Verdict:** usable, but it will be lost in AlmaLinux search results and carries some
trademark risk. Renaming is cheapest now, before a repo, crates or a domain exist.

## Alternatives checked

Most short English words are taken on crates.io (pinion, gantry, junction, sprocket, keel,
tether, jig, plinth, pinout …). The candidates below are compound or coined names.

| Name | Idea | Collisions found | crates.io | Verdict |
|---|---|---|---|---|
| **Rigdeck** | a *rig* (robot, test rig) plus a *deck* (the desktop) | none; GitHub handle is an idle account | free | strong |
| **Kitbay** | a bay you plug kits/connectors into | none; GitHub handle free | free | strong |
| **Tinkerdeck** | a deck for tinkerers | none | free | good, long |
| **Benchtop** | the maker's workbench, on a screen | none; GitHub handle taken | free | good, generic |
| **Workbay** | workshop plus app bay | none; GitHub handle taken | free | good |
| PinionOS | the small gear that drives the big one (and "Pi") | hobby kernel "Pinion"; a "PINION" software trademark exists | `pinionos` free | risky |
| GantryOS | a gantry carries the tools | Viam uses "gantry" as a component type, plus several tools | `gantryos` free | weak |
| Plinth | a base to build on | FreedomBox's admin UI used to be called Plinth | taken | avoid |

## Before committing to a name

1. Search EUIPO/USPTO for the name in class 9 (software).
2. Check the domain (`.dev` / `.org`) and the GitHub org.
3. Reserve the crate name with a placeholder publish once the repo exists.
