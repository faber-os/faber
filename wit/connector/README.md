# The connector world

`faber:connector@0.1.0` is what a wasm connector is (docs/packages.md §3): it exports `create`,
`on-bytes`, `on-tick` and `command`, and imports only `context` (publish, set-health, log). It
uses the capability types from [`../capabilities`](../capabilities), so the readings a wasm
connector publishes are the same values a built-in one publishes.

There is no WASI in this world: no files, network, environment or clock. A component that
imports anything outside `faber:` is refused when the core loads it.

Write connectors in Rust with `faber-connector-wasm` (`sdk/rust/connector-wasm`): the same code as
a built-in connector, plus `faber_connector_wasm::export!(definition);`.

Check it (CI does the same):

```bash
mkdir -p /tmp/wit/deps && cp -r wit/capabilities /tmp/wit/deps/ && cp wit/connector/*.wit /tmp/wit/
wasm-tools component wit /tmp/wit
```
