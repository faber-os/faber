# Design principles

**Faber should be as easy as possible.** Plug and play where it can be, very easy to set up where it
can't. It's meant as **the next step after Arduino, before bare Raspberry Pi**: anyone with a bit
of technical curiosity should be able to use it, without having to learn Linux, networking or
driver code first.

Arduino became popular because getting started is easy. Where it often fails is everything
*around* the code: examples that aren't explained, are out of date, or only work with one library
version. Faber treats docs and examples as part of the product, not an afterthought.

These principles apply to every feature, doc and package. When two designs are otherwise equal,
pick the one that is easier to understand.

## The ladder

Each step should feel like a small, natural step from the one before. Nobody should need a step
above where they are.

| Step | Who | What they do | What they need to know |
|---|---|---|---|
| 0 | Anyone | Flash the card, open `faber.local`, install an app | How to use a browser |
| 1 | Beginner | Plug in a sensor, add it as a part, watch it live | Which pin it's wired to (Faber helps) |
| 2 | Arduino-level maker | Write a **declarative connector** for a simple sensor | How to read a datasheet table |
| 3 | Hobby programmer | Write an **app** (UI and/or logic) from a template | Basic TypeScript or Rust |
| 4 | Developer | Write a **WASM connector**, a bridge firmware, or propose a capability | Rust/C, protocols |

## Using Faber (steps 0–1)

1. **Zero configuration to start.** Every setting has a sensible default. A fresh machine works
   before anything has been configured.
2. **Detect, then suggest.** Faber looks for hardware and proposes what to do: *"Found an I²C device
   at 0x76. It looks like a BME280. Install its connector?"* USB devices are recognised by their
   vendor/product id. The user confirms, and doesn't have to type anything.
3. **No terminal, ever, for normal use.** Everything is possible from the shell. The terminal tool
   exists for experts, but no guide may require it.
4. **See everything live.** Every part gets a viewer the moment it's added, so "is it working?" is
   always answered by looking.
5. **Errors say what to do.** Never "error -5". Always *what happened, why it probably happened, and
   the next thing to try*: *"No reply from the sensor at 0x76. Check that SDA is on pin 3 and SCL on
   pin 5, and that it has 3.3 V. [Open I²C scanner]"*
6. **Safe by default.** Things that move start stopped, need a lease, and stop on their own when
   something goes wrong. A beginner can't hurt themselves or their robot by misconfiguring a setting.
7. **Progressive disclosure.** Simple views first. Advanced settings live behind an "Advanced"
   toggle, never in the way.
8. **One obvious way.** If there are two ways to do something, one of them should go.

## Building for Faber (steps 2–4)

1. **Start from a template that already works.** `faber new connector` / `faber new app` produces a
   package that builds, runs against a mock port and shows up in the shell, before you change a line.
2. **The easiest kind first.** Docs always show the declarative connector first, then WASM, then
   native, and say why you'd move up.
3. **Fast feedback.** Rebuild and reload a package without restarting the core. Mock ports let you
   develop without the hardware attached.
4. **Clear errors from the tools too.** Manifest mistakes point to the exact line and suggest the fix.

## Docs and examples

These rules exist because of everything that goes wrong with Arduino examples.

1. **Every example is tested in CI** against the current release. An example that doesn't build or
   run fails the build. Examples are never older than the code they describe.
2. **Every example has the same parts:**
   - **what you need** (parts, with links to the connector);
   - **wiring** (a diagram and a pin table);
   - **steps**;
   - **what you should see**;
   - **if it doesn't work**;
   - and **the code, explained**, with comments that say *why*, not just *what*.
3. **Versioned with the release.** The docs for version X describe version X. Old pages are clearly
   marked as such.
4. **Plain language.** Short sentences, terms from the [glossary](glossary.md) only, and new terms
   explained the first time they appear. Write for someone who has finished a few Arduino projects.
5. **Packages ship their own example.** The registry won't accept a connector without a working
   example and a wiring diagram.

## How to use this document

- New features are checked against it during design. "Is this the easiest version we can build?"
  is a valid review comment.
- If a principle blocks something important, change the principle here deliberately, with a
  decision in [decisions.md](decisions.md). Never just quietly break it.
