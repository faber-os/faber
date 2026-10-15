# Wall follower: an app that drives a base along a wall

An **app** (docs/apps.md): it drives a mobile base along the wall on its right, keeping the
base's side 0.3 m from it, using a LiDAR. It doesn't know which LiDAR or which base: it asks for
two **roles**, and you pick the parts that fill them.

| Role | Capability | What it does with it |
|---|---|---|
| `lidar` | `laser-scan@1` | Looks at the wall: to the right, 30° ahead and 30° behind, and straight ahead. |
| `base` | `planar-base@1`, `command = true` | Drives it. The app takes a lease on it while it runs. |

It publishes how far the base's side is from the wall as the derived signal
`app:wall-follower/wall-distance` (`distance@1`), so you can watch it like any sensor.

## What you need

- A Faber core. No hardware at all? Use the simulated apartment (below).
- A planar base that can move sideways (holonomic, like SENNEP's mecanum wheels) and a LiDAR on
  its centre line, or the simulated ones.

## Steps

1. **Build and pack it:** in this folder, run `faber pack`. It builds `src/lib.rs` for
   WebAssembly and writes `wall-follower-0.1.0.faberpkg`.
2. **Install it:** double-click the package in Faber's Files, or
   `curl --data-binary @wall-follower-0.1.0.faberpkg "http://faber.local:7878/api/v0/packages?name=wall-follower-0.1.0.faberpkg"`.
3. **Try it in the simulated apartment:** pick the apartment in the World window, and add a
   simulated base (`generic-planar-base`, 0.8 × 0.65 m) and a simulated LD19 called `lidar`.
   Put the base in the living room near the south wall:
   `PUT /api/v0/world` with `{"name": "apartment", "start": {"x": 7.0, "y": 0.8, "yaw": 0}}`.
4. **Pick its parts:** the Install window offers the parts that fit each role (or, in Apps, change
   them later). Without the shell: `PUT /api/v0/apps/wall-follower/roles` with
   `{"roles": {"lidar": "lidar", "base": "base"}}`.
5. **Turn it on:** tick "Turn it on now" when installing, or switch it on in Apps
   (`POST /api/v0/apps/wall-follower/enable`).
6. **Open its window:** Apps → Wall follower → Open. It shows whether it is on and its distance
   to the wall, live.

## What you should see

The base slides right until its side is 0.3 m from the wall, then drives east along it. At the
corner it turns left and follows the next wall. `app:wall-follower/wall-distance` settles at
about 0.3 m. The Drive window shows `app:wall-follower` driving the base.

## If it doesn't work

- **The app says a role has no part:** pick the parts (step 4). `GET /api/v0/apps` says which.
- **It runs, but the base doesn't move:** someone else holds the base's lease (the Drive window?),
  or the e-stop is engaged. The app asks again every tick and drives as soon as it can.
- **It drives into the wall or away from it:** the LiDAR must sit on the base's centre line,
  looking forward. Change the tuning in the app's state (`<data>/apps/wall-follower/state.toml`):
  `target-m`, `half-width-m` (from the LiDAR to the base's side), `front-m`, `speed`.

## The code, explained

`src/lib.rs` has two parts:

- `steer` turns one scan into a velocity. It looks for the wall at -90° (the right), slides
  sideways to keep the gap (`vy`), turns so the rays at -60° and -120° are equally long (then
  the base is parallel to the wall), and stops and turns left when something is ahead. It is a
  plain function, so `cargo test` checks it against made-up scans, without wasm or a core.
- `WallFollower` is the app. On every tick it polls the LiDAR role for a new scan (`poll` hands
  each scan out once), steers by it, publishes the gap, and sends the velocity to the base role.
  It sends between scans too: every command renews the lease, and a lease that isn't renewed
  ends and stops the base (the dead man's switch). If the lease ended (an operator took over,
  the e-stop), it asks for it again, and waits while someone else drives.

`ui/` is the app's window: plain HTML and JavaScript, no build step.

- `index.html` loads `app.js` as a module. Faber shows the page in a sandboxed frame that can't
  reach the network or Faber's API, so everything goes through `faber-shell.js`: the
  [`@faber-os/shell`](../../../sdk/js/shell/README.md) SDK, copied in because an app's page can
  only load its own files. Don't edit it: the SDK's tests rewrite it when the SDK changes.
- `app.js` takes the shell's look (`faber.theme()`, `applyTheme`), shows whether the app is on
  (`faber.status()`, `faber.onStatus`), and follows the app's own derived signal
  (`faber.subscribePublished('wall-distance', …)`, at most 5 times a second). The window only
  watches; the logic drives the base whether the window is open or not.

A speed slider would need the window to change the app's saved state (`state.toml`), which the
logic reads. Faber can't do that from a window yet (the app protocol has no state messages), so
the speed is still tuned in `state.toml` (above).
