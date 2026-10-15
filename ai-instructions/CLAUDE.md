# Faber — project rules for AI agents

## What this is
Faber (Latin "maker"; the image is "Faber OS") is Mikkel's open-source "Home Assistant OS
for maker and robotics hardware": a Raspberry Pi appliance image with a web desktop (remote
browser + attached screen), a typed hardware model (parts → capabilities), sandboxed third-party
connectors and apps from a GitHub-based store, built-in bench tools, and safety primitives for
things that move. SENNEP V2 (`../SENNEP V2`) is the first app and the reference machine.

Status (2026-10-15): **design phase, docs only.** No repo yet (Mikkel asked to wait with
`git init`). Start with `README.md`, then `docs/architecture.md`, `docs/glossary.md`,
`docs/decisions.md`, `docs/roadmap.md`.

## Never violate
1. **Glossary is law.** Use only the terms in `docs/glossary.md`, with exactly one meaning each.
   New concept → add it to the glossary first. Four concepts only: core, connector, tool, app.
2. **Apps depend on capabilities, never on connectors.** Connectors never talk to apps directly.
3. **No direct hardware access** outside the port manager.
4. **Real-time and last-line safety live on microcontrollers** (bridges), never only on the Pi.
5. **Local-first.** No feature may require an account or the internet.
6. **No kernel, no drivers.** Faber sits on Debian + the Raspberry Pi kernel.
7. **Decisions are recorded** in `docs/decisions.md` (proposed vs accepted). Don't re-debate an
   accepted decision without new information; don't treat a proposed one as settled.
8. **Public project.** No secrets, personal data, invoices or chat exports. Personal explainers and notes for Mikkel go in `private/` (git-ignored); public docs never link there.
9. **No time estimates** anywhere (global rule).
10. **Write the name only as `Faber`, `faber` or `FABER`** (see "How the name is written" in
    `docs/research/naming.md`), so a rename stays a case-sensitive search and replace.

## Working style
Same as Mikkel's global rules (`GLOBAL_CLAUDE.md`): status → research → plan → ask → build,
honest pushback, direct answers. Docs in English. Log every session in `SESSIONS.md`.
