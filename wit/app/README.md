# The app world

`faber:app@0.1.0` is what an app's logic is (docs/apps.md §4): it exports `start`, `tick` and
`stop`, and imports only `roles` (its roles' readings), `drive` (leases and commands on the roles
it may drive), `publish` (its derived signals), `state` (a small store that outlives restarts)
and `log`. Readings and commands are the connector world's types, so an app reads exactly what a
connector publishes.

There is no WASI in this world: no files, network, environment or clock (time is passed in). A
component that imports anything outside these interfaces is refused when the core loads it.

Write apps in Rust with `faber-app-wasm` (`sdk/rust/app-wasm`): a type that implements `App`, plus
`faber_app_wasm::export!(MyApp);`.

Check it (CI does the same):

```bash
mkdir -p /tmp/app/deps/connector && cp -r wit/capabilities /tmp/app/deps/
cp wit/connector/*.wit /tmp/app/deps/connector/ && cp wit/app/*.wit /tmp/app/
wasm-tools component wit /tmp/app
```
