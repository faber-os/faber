# My app

A Faber app: it watches one or more distance sensors and publishes the nearest distance any of
them measures, as `app:my-app/nearest`, so a viewer or another app can use it.

This text is shown in Faber's install dialog. Describe your app here.

## What you need

- A Faber machine with at least one part that provides `distance@1` (a real sensor, or a
  simulated one: Add part → Simulated).

## Steps

1. Install `my-app-0.1.0.faberpkg`: double-click it in Files.
2. Pick the sensors that fill its `sensors` role, and turn it on.

## What you should see

`app:my-app/nearest` shows the nearest distance, and changes as something comes closer.

## If it doesn't work

- **It says a role has no part:** pick at least one distance sensor for `sensors`.
- **It always says no target:** none of the sensors measures anything right now; check them in
  Parts.

## Making it yours

- `src/lib.rs` is the logic; `cargo test` runs it on your PC with a pretend machine.
- `faber.toml` says which roles it needs and what it publishes.
- `faber check` builds it for WebAssembly and runs it in Faber's sandbox; `faber pack` makes the
  package.
