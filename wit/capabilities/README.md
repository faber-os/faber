# Capability WIT sketches

**Sketch v0** (validated with `wasm-tools component wit`). These files mirror [docs/capabilities.md](../../docs/capabilities.md) in WIT, the
interface language of the WebAssembly Component Model. They are a design aid, not a stable API.

Conventions in these sketches:
- Every capability is one `interface` with up to four parts: `reading` (the signal), `info`
  (facts reported once), `settings`, and command functions.
- Leased commands take a `lease-id` as their first argument. Shared commands don't.
- Signals are published through the host's router (not defined here yet).
- **OPEN:** in the real API every capability gets its own version (`distance@1`). Here they all
  share one package version for simplicity.
