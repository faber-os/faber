# Working notes

## State
- All design decisions D01–D28 are accepted and there are no open questions (`docs/decisions.md`). The next design work is the **starter capability field lists** (WIT), then Phase 1 in `docs/roadmap.md`.
- `private/` (git-ignored) holds Mikkel's explainers: WASM, A/B updates, licences + the Apache patent clause, wire formats, package signing, frontend options, the name brainstorm. Never link to it from public docs.

## Things to keep in mind
- **Derived signals (D24)** are allowed, but they stay read-only and visibly separate from parts (`app:name` addresses, an "app" badge). SENNEP's `sennep:pose` is the test case. Watch that this doesn't grow into apps acting as connectors.
- **Signing keys (D27):** the Faber maintainer (Mikkel) holds the offline OS-image key and the registry-index key. Plan key rotation before the first signed release. No trust is based on GitHub accounts.
- **The first-chat idea "start with an install script" was replaced by D17** (develop on a PC, ship a real A/B image at the first public release).
- **The name:** only `Faber` / `faber` / `FABER` (CLAUDE.md rule 10). The GitHub org and npm scope are `faber-os`, because the bare names are taken.

## Context
- Viam is the closest prior art (typed component APIs + a module registry), but it is cloud-first, AGPL, and has no OS image, sandbox or maker tools. Faber's niche is the gap list in `docs/research/prior-art-appliance-os.md`.
- Research facts are as of 2026-10-15 and partly unverified (marked in the research files).
