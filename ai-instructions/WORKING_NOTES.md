# Working notes

## State
- All design decisions D01–D30 are accepted and there are no open questions (`docs/decisions.md`). Capabilities are specified (`docs/capabilities.md`). Phase 1 is built and passes CI locally; the only open Phase 1 item is Mikkel testing the real LD19. Next: Phase 2 (WIT host API, Wasmtime, `.faberpkg`).
- WIT sketches share one package version (`faber:capabilities@0.1.0`). Per-capability versioning (`distance@1`) still needs a concrete scheme before the API is frozen (Phase 2).
- `private/` (git-ignored) holds Mikkel's explainers: WASM, A/B updates, licences + the Apache patent clause, wire formats, package signing, frontend options, the name brainstorm. Never link to it from public docs.

## Things to keep in mind
- **Derived signals (D24)** are allowed, but they stay read-only and visibly separate from parts (`app:name` addresses, an "app" badge). SENNEP's `sennep:pose` is the test case. Watch that this doesn't grow into apps acting as connectors.
- **Signing keys (D27):** the Faber maintainer (Mikkel) holds the offline OS-image key and the registry-index key. Plan key rotation before the first signed release. No trust is based on GitHub accounts.
- **The first-chat idea "start with an install script" was replaced by D17** (develop on a PC, ship a real A/B image at the first public release).
- **The name:** only `Faber` / `faber` / `FABER` (CLAUDE.md rule 10). The GitHub org and npm scope are `faber-os`, because the bare names are taken.

## Context
- Viam is the closest prior art (typed component APIs + a module registry), but it is cloud-first, AGPL, and has no OS image, sandbox or maker tools. Faber's niche is the gap list in `docs/research/prior-art-appliance-os.md`.
- Research facts are as of 2026-10-15 and partly unverified (marked in the research files).

## Phase 1 notes (2026-10-15)
- **Serial protocol was ambiguous** (found by the shell agent): no "opened" reply, unclear which errors end a session. Fixed at the protocol level: `serial-opened` is sent before any data (the machine opens the port, the WebSocket announces it, then reading starts), and only `serial-closed` ends an open session. Rules in docs/api.md.
- **The shell's list of server message types is keyed by the generated union**, so a new message type in the core is a TypeScript compile error until the shell handles it (it had silently dropped `serial-opened`).
- **Comments in parts.toml:** a comment block directly above a part goes with it on removal; anything above a blank line stays. New parts go after trailing comments (a comment-only file keeps its header on top).
- **Open:** capability `info` (e.g. LiDAR range min/max) has no place in the API yet, so the scan viewer fits the farthest point. Add `info` to `ProvideInfo`/`SignalInfo` when connector manifests carry it (Phase 2).
- **Open:** the LD19 connector doesn't judge rotation speed (the datasheet range wasn't verified), only silence and corrupted packets.
- The shell agent ran a broad `taskkill` on `node.exe` while testing; Mikkel's other node processes appeared to survive, but that's unverified.
