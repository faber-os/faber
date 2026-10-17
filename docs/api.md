# Core API (v0)

How the shell (and later app UIs) talk to `faber-core`. **v0 is internal and may change.** The
types are defined once in Rust (`core/src/api/dto.rs`, `sdk/rust/capabilities`), and the TypeScript
versions in `shell/src/lib/api/generated/` are generated from them by `cargo test`. Never edit
the generated files by hand.

The core listens on `127.0.0.1:7878` by default (`--listen 0.0.0.0:7878` opens it to the
network). **Every request says who sent it** (D16): a browser signs in and sends the session
cookie, a program sends an API token. See [Users and signing in](#users-and-signing-in) and
[auth.md](auth.md).

## Conventions

- Field names are **camelCase**. Enum values are **kebab-case** (`usb-serial`, `port-claimed`).
- Times are µs on the core's monotonic clock (`tUs`). `GET /api/v0/system` gives the current value.
- **Units are SI, always** (D06, D43): metres, radians, seconds (µs for timestamps). The shell's
  units setting (metric/imperial, degrees/radians) only changes what the shell *shows*; it converts
  in one place (`shell/src/lib/viewers/units.ts`). The API, recordings and apps never change units,
  so capabilities stay comparable. Wall-clock times are milliseconds since 1970 (UTC); the shell
  shows them in the machine's time zone (D42).
- **Errors** (any non-2xx status) always have this body, so the shell can show *what happened* and
  *what to try* (docs/principles.md):

  ```json
  { "error": { "code": "port-claimed", "message": "COM3 is used by the part \"lidar\".", "hint": "Pause or remove \"lidar\" in Parts first, or pick another port." } }
  ```

  A refused guided-board plan (`422 invalid-plan`) also lists everything wrong with it in
  `problems` ([Guided boards](#guided-boards)).
- **Who may call what:** each route has a minimum role (`viewer` < `operator` < `admin`), listed
  in [auth.md](auth.md#roles) and enforced from one table in the core (`core/src/api/access.rs`).
  Every endpoint can answer `401 not-signed-in` (no or an ended session, or an invalid API token)
  and `403 not-allowed`, whose body names the role it needs in `needs` (`"operator"`). A request
  to a host name that can't be this machine gets `421 unknown-host`; a request whose `Origin` isn't
  this machine's address gets `403 bad-origin` (the [details](#users-and-signing-in)).

## REST (JSON)

| Method and path | Body / result |
|---|---|
| `GET /api/v0/system` | `SystemInfo`. Its `api` is a fingerprint of the API types the core was built with (`core/build.rs`); the shell computes its own the same way and, when they differ, asks for the core to be restarted instead of failing request by request. `timeZone` is the machine's own IANA time zone (e.g. `Europe/Copenhagen`), read from its OS each time; `UTC` (with a log line) if the OS doesn't say. `buses` says where the I²C, SPI and GPIO buses come from: `system` (the machine's own: Linux, e.g. a Raspberry Pi), `fake` (simulated buses with simulated parts on them: the core runs with `--fake-hardware`, for trying things out and for tests) or `none` (Windows and macOS give programs no I²C, SPI or GPIO; the shell says so). |
| `GET /api/v0/about` | `AboutInfo`: `version`, `api` (the fingerprint), `dataFolder` (where config and files live on this computer), `uptimeS` (seconds since the core started), `os`, `timeZone` (the machine's own). |
| `GET /api/v0/connectors` | `ConnectorInfo[]`: every connector the core can run: the built-in ones and those installed in `<data>/connectors/<id>/`. `kind` says how it runs: `built-in`, `declarative` (only a `faber.toml`, run by the core's engine; [connectors-declarative.md](connectors-declarative.md)) or `wasm`. Each `provides` entry carries its capability's `info` (limits, e.g. a distance sensor's range) with the default settings; a part's own limits are in its `signals` (below). `port` is `{"kind": "serial", "baud": …}`, `{"kind": "i2c", "addresses": [72, 73], "default": 72}` (one device on an I²C bus, at one of those 7-bit addresses), `{"kind": "spi"}`, `{"kind": "gpio"}` or `{"kind": "bridge"}` (the generic connectors: real hardware like this sits on a FaberLink board and appears as the board's own part, so a part on such a connector is only ever simulated). `settings` describes every setting a part of this kind takes, in form order: `key`, `label`, `description` (both in English for now), `unit?`, `advanced` (shown behind "Advanced"), and `kind` with its values: `number` (`default`, `min?`, `minExclusive?`, `max?`, `suggested?`: where a new part's form starts), `bool` (`default`), `choice` (`default`, `choices`: `value`, `label`), `string` (`default`) or `part-list` (other parts' names, empty by default). `default` is what a part gets when parts.toml leaves the setting out. |
| `GET /api/v0/connectors/problems` | `ConnectorProblem[]`: installed connectors (wasm or declarative) the core couldn't load, when it started or since a package was installed or removed, each with its `folder` in `<data>/connectors/`, a `message` and a `hint`. The core runs without them. |
| `GET /api/v0/packages`, `POST /api/v0/packages`, `DELETE /api/v0/packages/{id}` | Connector and app packages: see [Packages](#packages). |
| `GET /api/v0/apps`, `POST /api/v0/apps/{id}/enable` and `/disable`, `PUT /api/v0/apps/{id}/roles` | Apps: see [Apps](#apps). |
| `GET /api/v0/ports` | `PortInfo[]`: every port of every kind (see [Ports](#ports)), with what claims it and suggestions. `claimedBy?` is who holds the whole port; on an I²C bus, `addresses?` lists who holds which address (`address`, `claimedBy`), lowest first, and is left out when none is used. `reserved?` says, in words, why no part can have the port: something outside Faber uses it, e.g. `used by I²C bus 1 (i2c-1)` for GPIO2, whose pin belongs to the bus while the bus is on. |
| `GET /api/v0/ports/problems` | `PortProblem[]`: things plugged in that never became a port (see [Port problems](#port-problems)) |
| `POST /api/v0/ports/scan-i2c` | Needs an operator. `{"port": "i2c-1"}` → `200 I2cScan`: a safe scan of one I²C bus, only when someone asks (never by itself). See [I²C scan](#i²c-scan). Errors: `400 unknown-port`, `400 not-an-i2c-bus`, `409 port-claimed` (a tool holds the whole bus), `409 port-unavailable`. |
| `GET /api/v0/ports/header` | `HeaderPin[]`: the Raspberry Pi's 40-pin header, pins 1–40: `pin`, `name` (`GPIO17`, `3V3`, `5V`, `GND`), `line?` (its `gpio-<line>` port) and `function?` (`I²C 1 SDA`, `UART TX`, …). The same on every Pi with a 40-pin header, so it is answered on any computer. |
| `POST /api/v0/ports/check-board` | `{"port": "usb:2341:0042"}` → `200 BoardCheck`, after up to 2.5 s: is a FaberLink board on this port? See [FaberLink boards](#faberlink-boards-bridges). |
| `GET /api/v0/flasher/boards`, `POST /api/v0/ports/{id}/flash` | The Arduino flasher: puts a program on an Uno, Nano or Mega. See [The Arduino flasher](#the-arduino-flasher). |
| `GET /api/v0/arduino`, `POST /api/v0/arduino/…` | The Arduino editor: sketches, compiling, uploading and "what's on this board". See [The Arduino editor](#the-arduino-editor). |
| `GET /api/v0/arduino/pins/{board}`, `GET /api/v0/arduino/parts`, `/api/v0/arduino/plans…` | Guided boards: pin maps, part templates, the allocator and plans Faber writes a sketch from. See [Guided boards](#guided-boards). |
| `GET /api/v0/parts` | `PartInfo[]` |
| `POST /api/v0/parts` | `NewPart` → `201 PartInfo`. A real part has a `port`; a simulated one has `simulate` (`manual` or `world`) instead (see [Driving](#driving-leases-commands-and-the-e-stop)). A part on an I²C bus has an `address` (7-bit, e.g. `72` for 0x48; left out, the connector's `default`), written to parts.toml as `address = 0x48`. `settings` (optional) are the connector's settings, as in `[parts.<name>.settings]`. Errors: `400 invalid-name` (including `world`, the simulated world's own name), `400 unknown-connector`, `400 unknown-port`, `400 wrong-port` (the connector needs another kind of port, or the port is `reserved`), `400 unknown-address` (not one of the connector's `addresses`), `400 address-not-used` (an address for a part that isn't on an I²C bus), `400 invalid-settings`, `400 port-and-simulate` (both given), `400 needs-bridge` (a generic part without `simulate`: add the FaberLink board it sits on instead), `400 bridge-not-simulated` (a `faber-bridge` with `simulate`: a board is real hardware), `409 name-taken`, `409 port-claimed` (on an I²C bus: that address is taken; two parts on one bus need different addresses), `409 world-has-a-base` (a second `planar-base` with `simulate = "world"`: the world has one machine) |
| `DELETE /api/v0/parts/{name}` | `204`. Errors: `404 unknown-part`. Ends any lease on the part (`part-removed`); removing a bridge ends those on its sub-parts too. For a sub-part (`mega.arm`) it removes `[parts.mega.parts.arm]` from parts.toml, which works only while the board doesn't have that part: `409 part-on-board` otherwise. |
| `POST /api/v0/parts/{name}/pause` | Needs an admin. `200 PartInfo` with `state.kind` `paused`. Writes `paused = true` into `[parts.<name>]` (the rest of parts.toml, comments too, stays as it was), then stops the part: its leases end (`part-paused`), its motors get their safe state, and its port is free for another program, e.g. the Arduino IDE uploading a sketch to a FaberLink board. It stays paused when its port goes and comes back, and after a restart, until it is resumed. A bridge's sub-parts show `paused` with it; a lease on a paused part, or on a part of a paused board, is refused with `409 part-paused`. Pausing a paused part changes nothing. Errors: `404 unknown-part`, `409 pause-the-board` (a sub-part such as `mega.left`: it pauses with its board), `409 config-broken` (parts.toml has an error; nothing is changed until it is fixed). |
| `POST /api/v0/parts/{name}/resume` | Needs an admin. `200 PartInfo`. Takes `paused = true` out again and starts the part (`state.kind` `starting`, then as usual). Errors as for `pause`. |

`PartInfo` has `port` for a real part and `simulate` for a simulated one, never both; a part on an
I²C bus also has its `address`. Each of its
`signals` (`address`, `capability`, `info?`) carries the capability's `info` as **this part's own
settings** give it (a base with `max-vx = 0.6` says 0.6, not the connector's default 0.5). `info` is
absent when the capability has none or the part's settings can't be read (the part is `invalid`).
A `battery@1` signal also has `battery` (`empty`, `full`, `low`, `critical`, in V): the pack's
voltages for its chemistry and cells, so a screen shows the same thresholds the core uses for its
level (the Battery viewer, a manual battery's slider). A bridge's `PartInfo` has `board` and is followed by its sub-parts, which have `bridge`; see
[FaberLink boards](#faberlink-boards-bridges).

Adding or removing a part **writes `parts.toml`** (keeping comments and formatting) and then
reconciles. The file stays the source of truth (D13).

### Reading a signal once

`GET /api/v0/signals/{part}/{name}` → `200 SignalValue` `{address, capability, reading}`: the
newest reading (`null` until the part has published one), events included. `404 unknown-signal`
when no such signal exists right now. An app's derived signals read the same way, with
`app:<id>` for the part: `GET /api/v0/signals/app:wall-follower/wall-distance`. The WebSocket `signal` topic streams them; this is for
simple programs that poll over plain HTTP (e.g. `examples/drive-square.py` checks `bumped`).

## WebSocket: `GET /api/v0/ws?format=msgpack|json`

One connection carries everything live. The default `format` is `msgpack`, sent as binary frames.
`json` uses text frames, for debugging (D26).

It is signed in when it opens, like any request (the cookie, an API token, or the attached
screen), and needs a viewer; a browser's upgrade must carry this machine's `Origin`. Each message
is then checked against the sender's role **at that moment** (a role change applies to the next
one): `serial-*`, `app-window-send` and `app-window-drive` need an operator, the rest a viewer.
A refused message is answered with an `error` (`not-allowed`, `needs`) for its `id`, and a refused
`app-window-drive` with `app-window-drive-refused`. When the session ends (signed out, password
changed, user removed), the core closes the socket with code **4401**.

**Client → core** (`ClientMessage`, tagged by `type`):

| `type` | Fields | Meaning |
|---|---|---|
| `subscribe` | `id`, `topic` | Start receiving a topic. `id` is chosen by the client and unique per connection. |
| `unsubscribe` | `id` | Stop it. |
| `serial-open` | `id`, `port`, `baud` | Open a port for the serial monitor. Fails with `port-claimed` while a part uses it. |
| `serial-send` | `id`, `data` (bytes) | Write bytes. |
| `serial-close` | `id` | Close it. |
| `app-window-open` | `id`, `app` | Open an app window's live link to its app's logic (see [App windows](#app-windows)). |
| `app-window-send` | `id`, `kind`, `data` (`{"text": …}` or `{"bytes": …}`) | A message from the window for the logic. |
| `app-window-heartbeat` | `id` | The window's page is alive. |
| `app-window-drive` | `id`, `role`, `index?` | The window drives a `drive = "window"` role through the logic (after the D46 confirm). |
| `app-window-release` | `id`, `role`, `index?` | It lets go of it. |
| `app-window-stop` | `id` | The user pressed the shell's Stop for this window. |
| `app-window-close` | `id` | Close the link. |

**Topics** (`Topic`, tagged by `kind`):

| `kind` | Fields | You get |
|---|---|---|
| `signal` | `address`, `maxHz?` | `signal` messages with readings of `part/name` |
| `parts` | — | a `parts` message now and after every change |
| `ports` | — | a `ports` message now and after every change, including a change in `problems` |
| `settings` | — | a `settings` message now and after every change |
| `files` | — | `files-changed` with the folders whose contents changed (through the core). `home:/` means "a lot changed": refresh everything shown. |
| `logs` | — | the recent log (up to 1000 entries) as `log` messages, then each new entry. Every new subscription gets the recent log again, so a client that reconnects should start its log view afresh. |
| `leases` | — | a `leases` message with every live lease now, then `lease-granted` (`lease`) and `lease-ended` (`lease`, `reason`) as they happen. Lease ids are never sent here. If the client falls behind, it gets a fresh `leases` list. |
| `estop` | — | an `estop` message (`status`: `engaged`, `engagedBy?`) now and on every engage and release. Every-message: none is lost. |
| `packages` | — | a `packages` message (`packages`: `PackageInfo[]`, as `GET /api/v0/packages`) now and after every install, uninstall or reload of the connectors, so every browser tab sees an install at once. If `packages.toml` can't be read, an `error` (`io-error`) instead. |
| `world` | — | a `world` message 20 times a second for as long as the subscription lasts, and `world-changed` (`name`) whenever the world is switched. See [The simulated world](#the-simulated-world). |
| `flash` | — | a `flash` message (`port`, `stage`, `done`, `total`) after every page the Arduino flasher writes or reads back, on any port (the Arduino editor's uploads and code checks too). See [The Arduino flasher](#the-arduino-flasher). |
| `arduino-tools` | — | an `arduino-tools` message (`step`: `downloading`, `verifying`, `unpacking`, `installing-core`, `done`; `done?` and `total?` bytes while downloading; `line?`: what `arduino-cli` printed last while installing the core) at every step of "Install Arduino tools". See [The Arduino editor](#the-arduino-editor). |
| `apps` | — | an `apps` message (`apps`: `AppInfo[]`, as `GET /api/v0/apps`) now and whenever an app is installed, removed, turned on or off, gets other parts, or changes state. See [Apps](#apps). |
| `notices` | — | a `notice` message (`notice`: `Notice`) for each thing worth telling the user, as it happens. Nothing is replayed. See [Notices](#notices). |
| `app-state` | `app` | an `app-state` message (`app`, `values`) now, then `app-state-changed` (`app`, `key`, `value?`) for every change to that app's state. `error` `unknown-app` if there is no such app. See [Apps](#apps). |

**Core → client** (`ServerMessage`, tagged by `type`): `signal`, `parts`, `ports`, `log`,
`serial-opened`, `serial-data`, `serial-closed`, `leases`, `lease-granted`, `lease-ended`, `estop`,
`packages`, `apps`, `world`, `world-changed`, `flash`, `arduino-tools`, `notice`, `app-state`, `app-state-changed`,
`app-window-opened`, `app-window-message`, `app-window-driving`, `app-window-drive-refused`,
`app-window-drive-ended`, `app-window-closed`, `error`. Each carries the `id` of the subscription,
serial session or app window link it belongs to. `error` may come without an `id`.

**Serial sessions**, step by step:

1. The client sends `serial-open`.
2. The core answers either `serial-opened` (with the port id) or an `error` with the same `id`.
   An `error` before `serial-opened` means the port never opened.
3. `serial-data` only ever arrives after `serial-opened`.
4. After `serial-opened`, only `serial-closed` ends the session: after `serial-close`, when the port
   stops working (its `reason` says why), or when the WebSocket closes. An `error` for an open
   session (e.g. `serial-write-failed`) leaves it open.

While a session is open, the port shows `claimedBy: "tool:serial-monitor"`. A part's claim shows
the part's name.

### App windows

An app window's live link to its own logic ([apps.md](apps.md) §12). Only the shell's app host
opens these, for the app's page (which can't reach the API itself) after checking every request.

1. `app-window-open` (`app`) is answered by `app-window-opened` (`window`: the window's id as the
   app's logic sees it) or an `error` (`unknown-app`, `id-in-use`).
2. **Messages.** `app-window-send` (`kind`: 1–64 characters of `a-z`, `0-9`, `-`; `data`:
   `{"text": "…"}` or `{"bytes": <binary>}`, at most 4 KB with the kind) reaches the logic's next
   `messages.poll()`. Nothing is saved: it is dropped while the logic isn't running. At most 50 a
   second per window; past that, or past the size, an `error` (`too-many-messages`,
   `message-too-large`, `bad-kind`, `reserved-kind` for `drive-started` and `drive-ended`, which
   only the core sends). The logic's messages for the window arrive as `app-window-message`
   (`kind`, `data`), also at most 50 a second.
3. **Driving through the logic.** `app-window-drive` (`role`, `index?`) is answered by
   `app-window-driving` or `app-window-drive-refused` (`error`: `app-not-running`, `unknown-role`,
   `not-window-driven`, `no-part`). From then until the drive ends, the app's logic may hold the
   role's lease; before and after, the core refuses it. It ends with `app-window-release`,
   `app-window-stop` (the shell's Stop bar), `app-window-close`, the WebSocket closing, the app's
   logic stopping, or no `app-window-heartbeat` for 300 ms; the window hears
   `app-window-drive-ended` (`reason`: `released`, `stopped`, `heartbeat-lost`, `app-stopped`) and
   the logic `drive-ended`, and the core has already ended the logic's lease on the role (unless
   another window still drives it).
4. The link ends with `app-window-close` or the WebSocket. If the app is uninstalled, the core
   says `app-window-closed` (`reason`: `app-removed`).

**WebSocket error codes:**

| Code | When |
|---|---|
| `bad-message` | The core couldn't read a client message (no `id`) |
| `id-in-use` | `subscribe` or `serial-open` reused an id that is still active |
| `unknown-signal` | `subscribe` to a signal address that doesn't exist right now |
| `signal-ended` | A subscribed signal went away (its part was removed or restarted). Subscribe again when the part is back. |
| `unknown-port`, `port-claimed`, `port-unavailable` | `serial-open` failed (see step 2) |
| `serial-write-failed` | `serial-send` couldn't write. The session stays open. |
| `unknown-session` | `serial-send` for a session that isn't open |
| `unknown-app`, `unknown-window` | `app-window-open` for an app that isn't installed, or another `app-window-*` for a link that isn't open |
| `bad-kind`, `reserved-kind`, `message-too-large`, `too-many-messages` | `app-window-send` broke a limit (see [App windows](#app-windows)) |
| `internal` | A bug in the core. The log has details. |

Every error carries `message` and `hint`, like REST errors.

### Notices

Things the core tells every screen once, as they happen. The shell shows each as a toast, and the
core also writes each to the log (as the part or app whose signal it is), so nothing is lost while
no screen is open. A `Notice` is tagged by `kind`:

| `kind` | Fields | When |
|---|---|---|
| `battery-level` | `tUs`, `address` (the battery's signal), `level` (`ok` / `low` / `critical`), `previous` (left out for a battery seen for the first time), `volts` | a `battery@1` signal's level changes (docs/capabilities.md): to `low` or `critical`, or back to `ok`. Never for a first `ok`; a battery that is already low when it first reports is told about. Parts and apps' derived signals alike. |

The level carries the capability's hysteresis, so a voltage wobbling around a threshold doesn't
flood the topic. The shell keeps one toast per battery (low, then critical, replaces it): low and
critical stay until dismissed, "ok again" goes away by itself.

## Driving: leases, commands and the e-stop

The design is [simulation.md](simulation.md), sections 5 and 6; the rules come from
[capabilities.md](capabilities.md), "The lease". A working program is
[`examples/drive-square.py`](../examples/drive-square.py).

**Leases.** Anything that can move (`motor@1`, `planar-base@1`) is commanded only by the holder
of its **lease**: an exclusive right that lasts **250 ms** unless a command or a renewal keeps it
alive. When a lease ends for any reason, the part gets its **safe state** (stopped) and every
`leases` subscriber hears `lease-ended`. A lease on a part *built from* others (a base's
`builtFrom` motors) locks those too.

| Method and path | Body / result |
|---|---|
| `POST /api/v0/parts/{name}/lease` | `{"holder": "drive-square.py", "takeOver"?: true}` → `201 LeaseGrant` (`lease` id, `part`, `holder`, `deadlineUs`, `durationMs`, `locks`). `takeOver` ends the lease in the way (operators taking over; its holder gets `lease-ended` with `taken-over`). Errors: `404 unknown-part`, `400 not-leasable` (nothing on it moves), `400 bad-request` (empty `holder`), `409 lease-held` (the message says who drives it), `423 estop-engaged` |
| `POST /api/v0/parts/{name}/lease/renew` | `{"lease": …}` → `200 LeaseGrant` with the new deadline. Errors: `403 lease-required`, `403 lease-ended`, `404 unknown-part` |
| `DELETE /api/v0/parts/{name}/lease?lease=…` | `204`; the part gets its safe state. Errors: `403 lease-required`, `403 lease-ended`, `404 unknown-part` |
| `GET /api/v0/leases` | `LeaseInfo[]`: who drives what (`part`, `holder`, `deadlineUs`, `locks`), without ids |
| `POST /api/v0/parts/{name}/command` | `{"lease": …, "command": Command}` → `200 CommandResult` (`applied`, `clamped`, `deadlineUs`). Errors: `400 bad-command` (a capability or kind this part doesn't take, or a value that is not a number), `403 lease-required`, `403 lease-ended`, `404 unknown-part`, `409 part-not-ready` (not running now, or a second world base the world refused), `423 estop-engaged` |

A `Command` is tagged like a reading:
`{"capability": "planar-base@1", "command": {"kind": "set-velocity", "vx": 0.2, "vy": 0, "omega": 0}}`
(m/s and rad/s, machine frame: x forward, y left), or for a motor
`{"capability": "motor@1", "command": {"kind": "set-velocity" | "set-duty" | "set-position", "velocityRadS" | "duty" | "positionRad": …}}`.
Every accepted command **renews the lease**. Values beyond the part's info (its `maxVelocity`, a
motor's `maxVelocityRadS`, duty −1..1) are **clamped**, not refused: `applied` is what the part
got, and `clamped` says whether it differs from what was sent. A base that is not `holonomic` gets
`vy` 0.

`lease-ended` messages and `403 lease-ended` errors give the **reason** (the error in its
`reason` field, next to `code`, so a program doesn't have to read the message): `released`, `expired`,
`estop`, `taken-over`, `part-removed`, `part-restarted` (its config changed), `part-failed`,
`part-paused`, `shutdown`. Expired leases are ended within 50 ms of their deadline.

**The e-stop.**

| Method and path | Body / result |
|---|---|
| `GET /api/v0/estop` | `EstopStatus`: `engaged`, `engagedBy?` (an `estop@1` reading: `tUs`, `source`, `reason`), `history` (recent engages and releases, oldest first) |
| `POST /api/v0/estop` | `{"reason"?: …, "source"?: "app" \| "shell" \| "part"}` → `200 EstopStatus`. **Always allowed, from anyone (signed in or not), and never fails**: an empty body is fine, and a body the core can't read still engages it. Every lease ends (`estop`), every actuator gets its safe state, and new leases and commands get `423 estop-engaged` until it is released. Engaging it again keeps the first reason. |
| `POST /api/v0/estop/release` | `{"source"?: …}` (optional) → `200 EstopStatus`. Needs an operator; the shell asks to confirm. Releasing restarts nothing: a new command does. Errors: `400 bad-request` (an unreadable body), `409 estop-latched` (a FaberLink board's own stop button is still pressed: reset it on the board first). |

**FaberLink boards.** A board's motors are leased and commanded like any part, by their sub-part
names (`POST /api/v0/parts/mega.left/lease`). The core sends a `COMMAND` to the board only for a
command that passed the lease check, and `SAFE` whenever the lease ends. The e-stop reaches every
board at once and in every heartbeat; a board's own stop button engages it with `source: "part"`.
See [FaberLink boards](#faberlink-boards-bridges).

**Manual simulated sensors.** A part with `simulate = "manual"` publishes a value you set; an
actuator reports the commands it gets (a base: `commanded`, and `measured` following it within
its acceleration limit).

| Method and path | Body / result |
|---|---|
| `GET /api/v0/parts/{name}/simulation` | `{"pattern": Pattern}` |
| `PUT /api/v0/parts/{name}/simulation` | `{"pattern": Pattern}` → `200` with the same. `Pattern` is tagged by `kind`: `steady` (`value`), `sine` (`center`, `amplitude`, `periodS`), `noise` (`center`, `amplitude`), `step` (`low` for half of `periodS`, then `high`), `drain` (from `from` to `to` in a straight line over `periodS`, then `to`: a battery running down). Values are in the capability's unit (m for `distance` and `laser-scan`, V for `battery`, counts/s for `encoder`, rad/s about z for `imu`; for `obstacle` 0.5 and up is detected). Errors: `404 unknown-part`, `409 not-manual` (not `simulate = "manual"`), `400 not-a-sensor` (an actuator), `400 bad-pattern` |

A distance reading outside the sensor's range is reported as `too-close` / `too-far` at the edge
of the range; a manual `laser-scan` is a ring of points at the value. `simulate = "world"` parts
get their values from [the simulated world](#the-simulated-world).

## FaberLink boards (bridges)

A microcontroller running the FaberLink library ([faberlink.md](faberlink.md)) is added as one part
with `connector = "faber-bridge"` on its serial port. The board says which parts it hosts, and each
becomes a **sub-part** named `<bridge>.<name>`: `mega.left` for the board `mega`'s part `left`.
Its signal is `mega.left/motor` (the capability's name). A name the user picks can't contain a
`.` and a signal name never does, so `mega.left/motor` and `mega/estop` (the board's own stop
button, an `estop@1` signal) can't be confused. A board's name for a part that isn't a valid part
name is lowercased with other characters turned into `_` (`Left Motor` → `left_motor`), or becomes
`part<id>`.

In `GET /api/v0/parts`, a bridge is followed by its sub-parts.

- The bridge's `PartInfo` has `board` (`BoardLink`): `link` (`connecting`, `describing`, `ready`,
  `no-answer`, `lost`, `incompatible`), and once the board has answered `board` (its name),
  `firmware`, `firmwareVersion`, `protocol` (the FaberLink version) and `heartbeatTimeoutMs`; plus
  `hardwareEstop` (its stop button is latched) and `failsafe` (it stopped its actuators because the
  heartbeat didn't reach it). Its `state` says what is wrong and what to try. Once the board has
  answered, `code` (`BoardCodeLink`) says what code it runs as far as its build id tells:
  `buildId?` (the id it said in `HELLO_ACK`; missing for a sketch not compiled by Faber's editor)
  and `record?` (the `FlashRecord` of the upload with that build id). Parts shows "Code from
  Faber: <sketch>", or without a record "Code not flashed from Faber (unknown)". See
  [The Arduino editor](#the-arduino-editor).
- A sub-part's `PartInfo` has `bridge` (its bridge's name), the bridge's `port`, its `mount` from
  `[parts.<bridge>.parts.<name>]`, and its one signal with the info the board gave (limits).
  `pins?` (`BoardPinInfo[]`) is the board's pins it uses, when the board says (a FaberLink
  library from 2026-10-16 on, for its own part classes; [faberlink.md](faberlink.md) §3): `role`
  (`pwm`, `dir`, `en`, `in1`, `in2`, `stby`, `trig`, `echo`, `signal`, `sense`, or `other` for a
  role a newer library added), `pin` (the Arduino pin number, `A0` is 14 on an Uno and 54 on a
  Mega) and `label` (as printed on the board: `D7`, `A0`). Absent when the board doesn't say.
  While the board is out of reach, every sub-part is `failed` ("Stopped: …") and its leases end
  (`part-failed`). One that parts.toml names but the board doesn't have is `degraded` with
  "Not on the board".

| Method and path | Body / result |
|---|---|
| `POST /api/v0/ports/check-board` | `BoardCheckRequest` `{"port": …}` → `200 BoardCheck`, tagged by `result`: `found` (`board`, `firmware`, `firmwareVersion`, `parts`: how many it hosts), `other-version` (`message`: a FaberLink board that speaks another protocol version, and what to update), or `nothing`. It opens the port, says `HELLO` a few times and listens for up to 2.5 s; it sends nothing else, so a board it finds keeps everything stopped. A board found is suggested for the port in `GET /api/v0/ports` (connector `faber-bridge`) until the port goes away. Errors: `400 unknown-port`, `400 not-a-serial-port` (a recording), `409 port-claimed` (a part or a tool uses it: it is never checked), `409 port-unavailable` (it couldn't be opened). |

The core never checks a port by itself: opening a serial port restarts most Arduino-based devices
(a 3D printer mid-print, too) and they may not like the bytes, so it happens only when someone asks.

## The Arduino flasher

Puts a program (an Intel HEX file, as the Arduino IDE exports it) on an Arduino over its USB port,
through the bootloader the board ships with: **STK500v1** for the Uno and Nano (Optiboot, 115 200
baud; a Nano on the old bootloader at 57 600) and **STK500v2** for the Mega 2560 (115 200 baud). No
Arduino IDE and no other program is needed. The protocol code is `sdk/rust/flash` (`faber-flash`);
it has been tested against simulated bootloaders only, not yet against a real board.

**Flashing replaces the program on the board.** The core only ever does it when asked, never by
itself, and it writes nothing before the program fits, the bootloader has answered and the chip's
signature is the board's (a wrong board is refused with nothing written).

| Method and path | Body / result |
|---|---|
| `GET /api/v0/flasher/boards` | `FlashBoard[]`: `id` (`uno`, `nano`, `nano-old`, `mega2560`), `name`, `chip`, `room` (the bytes a program may use: the flash minus the bootloader). |
| `POST /api/v0/ports/{id}/flash` | `FlashRequest` → `200 FlashResult`, once it is done: seconds for an Uno, up to a minute for a whole Mega. `{id}` is the port id, URL-encoded (`usb%3A2341%3A0043`). `board?` is a board id; left out, it is the board the port's USB ids name. `firmware` is `{"kind": "file", "path": "home:/Documents/blink.hex"}` (a file in Files) or `{"kind": "package", "package": "<id>"}` (an installed package's `firmware/<board>.hex`). `stopPart?` (default `false`): stop the FaberLink board on the port, see below. `FlashResult`: `port`, `board`, `boardName`, `bytes` (where the program ends), `pages` (written and read back), `stoppedPart?`. |

**What it does:** checks the program (valid HEX, fits the board), claims the port (it shows
`claimedBy: "tool:arduino-flasher"`, so nothing else opens it), opens it at the bootloader's speed,
resets the board through DTR/RTS (released 250 ms, then asserted, then 50 ms for the bootloader to
start), syncs, reads the chip's signature, writes every page that holds part of the program, reads
each one back and compares, and leaves the bootloader, which starts the new program. Then it gives
the port back.

**A port in use:** a port a part uses is refused (`409 port-claimed`). The one exception is a
FaberLink board (a `faber-bridge` part): without `stopPart` it is refused with
`409 port-used-by-bridge`; with `stopPart: true` the bridge is stopped first (its leases end with
`part-restarted` and its motors get `SAFE`), waits while the flasher holds its port, and starts
again by itself afterwards, on the new firmware. A port open in a tool (the serial monitor) is
always refused.

**The guess:** `GET /api/v0/ports` gives each port a `board?` (`BoardGuess`): `{"board": "uno"}` or
`{"board": "mega2560"}` for an official Arduino's USB ids (vendor `2341` or `2a03`), or
`{"usbChip": "CH340"}` / `{"usbChip": "FT232"}` for the USB chips clone boards use, which say nothing
about the board: then the request must name one.

**Errors**, each with a hint:

| Status and code | When |
|---|---|
| `400 unknown-port`, `400 not-a-serial-port` | No such port right now, or a recording |
| `400 unknown-board` | `board` isn't one of `GET /api/v0/flasher/boards` |
| `400 board-needed` | No `board`, and the port's USB ids don't name one (a clone) |
| `404 not-found`, `404 not-installed` | The file isn't there; the package isn't installed or has no firmware for this board |
| `409 port-claimed`, `409 port-used-by-bridge`, `409 port-unavailable` | A part or tool uses the port; a FaberLink board does (ask again with `stopPart`); it couldn't be opened |
| `422 firmware-invalid` | Not valid Intel HEX: the line that is wrong (a bad checksum, a cut-off file, …) |
| `422 firmware-too-large` | The program doesn't fit next to the bootloader |
| `422 board-not-answering` | The bootloader never answered: "The board didn't answer the bootloader." Try pressing reset just as you press Flash; check the board is the one picked. Nothing was written. |
| `422 wrong-board` | The chip's signature isn't the board's (e.g. an old Nano's ATmega168 picked as an Uno). Nothing was written. |
| `422 board-stopped-answering` | It stopped in the middle; flash again |
| `422 verify-failed` | What was read back differs (the address and both bytes) |
| `422 damaged-answers` | The Mega's answers kept arriving with bad checksums: a cable or noise problem |
| `422 board-refused` | The bootloader said no to a command |
| `409 port-failed` | The port stopped working (unplugged) |

The `flash` topic on the WebSocket follows it: `stage` is `connecting` (reset and sync), `writing`,
`verifying`, then `done`; `done` and `total` count bytes (whole pages). The result, or the error,
comes as the `POST`'s answer.

**Firmware in packages:** a package may carry `firmware/<board>.hex`, one per board, each declared
in its `faber.toml` as `[[firmware]]` with `board = "uno"`. Installing checks that every declared
file is there and valid HEX that fits the board, and that no undeclared one is. `PackageInfo`
lists the boards in `firmware`.

## The Arduino editor

Write a sketch in Files, compile it with `arduino-cli` on the machine, and put it on the board with
Faber's own flasher ([arduino-editor.md](arduino-editor.md)). After every upload the core records
what it flashed on which board, so it can later prove which code a board still runs. Sketches are
ordinary files, `home:/Arduino/<name>/<name>.ino` plus any `.h`/`.cpp` tabs, read and saved with
the [Files](#files) API.

Who may: anyone signed in reads (`GET`s); an **operator** makes sketches, compiles and opens a
flashed copy; an **admin** uploads, checks a board (it resets it) and installs the tools.

| Method and path | Body / result |
|---|---|
| `GET /api/v0/arduino` | `ArduinoInfo`: `tools` (`ArduinoTools`: `state` `ready`, `no-avr-core`, `missing`, or `broken` with `problem`; `cli?` with `path`, `source` (`faber`, `path`, `arduino-ide` or `fake`) and `version?`; `installing`; `install?` with `cliVersion`, `url` (the release file on Arduino's GitHub), `core` (`arduino:avr`) and `approxMb`, missing where Arduino publishes no `arduino-cli` for the computer), `boards` (`ArduinoBoard[]`: `id`, `name`, `chip`, `fqbn`, `room`; the flasher's boards), `templates` (`SketchTemplate[]`: `id`, `name`, `description`, `faberlink`) and `sketchbook` (`home:/Arduino`). |
| `POST /api/v0/arduino/tools/install` | → `200 ArduinoTools` when done (minutes). Downloads `arduino-cli` into `<data>/tools/arduino-cli/` if none is found, checking its SHA-256 against the release's published checksums before unpacking anything, then runs `arduino-cli core install arduino:avr` if the AVR core is missing. Only ever on a request (the editor asks first, showing `install`). The `arduino-tools` topic follows it. Errors: `409 already-installing`, `409 download-failed` (offline; the hint says what to check), `422 checksum-missing`, `422 checksum-mismatch` (nothing was installed), `422 download-damaged`, `422 no-arduino-cli-for-this-computer`, `409 core-install-failed`, `500 io-error`. |
| `POST /api/v0/arduino/sketches` | `NewSketchRequest` `{"name": "Rover", "template": "faberlink-board"}` → `201 SketchRef` (`path`: `home:/Arduino/Rover/Rover.ino`, `copy`: false). Errors: `400 invalid-sketch-name` (Arduino's rule: a letter or digit first, then letters, digits, `_`, `-` and `.`, at most 63), `400 unknown-template`, `409 already-exists`. |
| `POST /api/v0/arduino/compile` | `CompileRequest` `{"sketch": …, "board": "uno"}` (`sketch`: the `.ino`, another file of the sketch, or its folder) → `200 CompileResult`: `ok`, `board`, `sketch` (its `.ino`), `sketchName`, `buildId` (16 hex digits, compiled in as `FABER_BUILD_ID`), `size?` (`flash`, `flashMax`, `ram`, `ramMax`, in bytes), `diagnostics` (`SketchDiagnostic[]`: `severity` `error` or `warning`, `file?` (in the sketch folder, or a library file's name), `inSketch`, `line?`, `column?`, `message`) and `message?` (what went wrong when no error names a file). With `"hex": true` in the request, a compile that worked also returns `hex?`: the program as Intel HEX (Circuits runs it, docs/circuits.md). A sketch with errors is a `200` with `ok` false. Errors: `400 unknown-board`, `400 not-a-sketch` (no `<folder>/<folder>.ino`), `400 invalid-sketch-name`, `400 not-in-home`, `409 arduino-tools-missing` and `409 arduino-core-missing` (both: press Install Arduino tools), `422 sketch-too-big`, `500 arduino-tools-broken`. |
| `POST /api/v0/arduino/upload` | `UploadRequest` (`sketch`, `board`, `port`, `stopPart?`) → `200 UploadResult`: `compile` (a `CompileResult`) and, when it compiled, `flashed` (a `FlashResult`, as the flasher's) and `record?` (a `FlashRecord`, below). It compiles, then flashes through [the Arduino flasher](#the-arduino-flasher), with its claims, refusals (`409 port-used-by-bridge` until `stopPart` is true) and errors; the `flash` topic follows it. A failed compile flashes nothing. |
| `GET /api/v0/arduino/boards/{port}/code` | → `200 BoardCodeInfo` (`port`, `identity`, `code`): what is on the board, from what can be known without touching it. It never resets anything. Errors: `400 unknown-port`. |
| `POST /api/v0/arduino/boards/{port}/check` | `CheckCodeRequest` `{}` or `{"stopPart": true}` → `200 BoardCodeInfo`: reads the recorded program back through the bootloader and compares it. **It resets the board** (the sketch starts again); `FlashRecord.checkMs` says about how long it takes. A board Faber never flashed is answered `never` without being touched. Errors as for the flasher: `409 port-used-by-bridge` (the hint says checking stops the board), `409 port-claimed`, `422 board-not-answering`, … |
| `POST /api/v0/arduino/flashed/{id}/open` | `OpenFlashedRequest` `{}` or `{"copy": true}` → `200 SketchRef`, a sketch to open for that record: if the sketch in Files is still exactly the flashed files (and no copy is asked for), its own path with `copy` false; otherwise the flashed files are copied to `home:/Arduino/<name>-on-board/` (`-2`, `-3`, … if taken), with `copy` true. Errors: `404 unknown-flash`. |

`BoardCodeInfo.identity` (`BoardIdentityInfo`) is `serial?` (the board's USB serial number), `port`
and `unique` (false without a serial number: an identical board on the same port can't be told
apart). `code` (`CodeCheck`) is tagged by `result`:

| `result` | Fields | Meaning |
|---|---|---|
| `same` | `record`, `proof` | The board runs exactly this flash. |
| `different` | `record`, `proof` | The board runs other code now; `record` is the last thing Faber flashed on it. |
| `needs-check` | `record` | Faber flashed this board, but only reading it back can tell what runs now. |
| `never` | — | Faber has never flashed this board. |

`proof` (`CodeProof`, tagged by `method`) is `build-id` (a connected FaberLink board said its build
id) or `read-back` (with `elapsedMs`). A `FlashRecord` has `id`, `board`, `boardName`, `sketch`,
`sketchName`, `buildId`, `sha256` (of the programmed bytes: every page written, as written),
`bytes`, `flashedAt` (RFC 3339, UTC), `identity` and `checkMs`.

The records are in `<data>/arduino/boards.toml`, with an exact copy of each board's last sketch and
program in `<data>/arduino/flashed/<id>/`.

## Guided boards

The core of guided mode ([wiring.md](wiring.md), D57): each board's pins, the part templates, the
allocator that suggests pins and checks a plan, and plans that Faber turns into a sketch. A guided
board is a sketch folder, `home:/Arduino/<name>/`, with the plan `board.toml`, the generated
`<name>.ino` and your own `my_code.h`; compiling and uploading it is the editor's
[compile and upload](#the-arduino-editor), as for any sketch. The shell's wizard is Parts → Add part → On a board's pins ([wiring.md](wiring.md) §14).

Who may: anyone signed in reads, and asks for a suggestion (it writes nothing); an **operator**
starts, saves and takes over a plan; uploading stays an **admin**'s.

| Method and path | Body / result |
|---|---|
| `GET /api/v0/arduino/pins/{board}` | `PinMap` for `uno`, `nano`, `nano-old`, `mega2560` or `pi-header`: `id`, `name`, `chip`, `kind` (`arduino` or `pi`), `logicVolts`, `analogReferenceVolts?` (none on a Pi), `pins` (`BoardPin[]`: `name` (`D7`, `A0`, `GPIO17`), `number?` (the Arduino pin number, or the GPIO line), `physical?` (a Pi header pin), `digital`, `analog?` (its channel), `pwm?` (`timer2b`; `pwm0-ch2` on a Pi), `interrupt?`, `pinChange`, `buses` (`PinBus[]`: `kind` `i2c`/`spi`/`serial`, `bus`, `line`), `led`, `volts`, `providedBy?`), `reserved` (`ReservedPins[]`: `pins`, `reason`), `notes` (`PinNote[]`: `pins`, `note`, `avoid`: the needs the allocator keeps them for last), `servoTimers` (in the order the Servo library takes them), `servosPerTimer`, `toneTimer?`, `power` (a Pi header's 3V3/5V/GND positions) and `headers` (`PinHeader[]`, for drawing the board with its USB connector on the left: `edge` `top`/`bottom` (positions left to right) or `right` (top to bottom), `rows` (2 for a double header, listed pair by pair, inner row first) and `positions`: a pin's name, or `GND`, `5V`, `3V3`, `VIN`, `AREF`, `IOREF`, `RESET`, `NC`, `SDA`, `SCL` for a position that isn't one of the map's pins; headers on one edge follow each other with a gap; only the order along each header, from the boards' published pinouts, not measured positions). Errors: `404 unknown-board`. |
| `GET /api/v0/arduino/parts` | `PartTemplate[]`, in catalogue order: `id`, `name`, `description`, `covers` (products), `capability?`, `class` (the FaberLink class), `include`, `defaultName`, `warnings`, `power` (`supply`: `5V`, `3V3`, `own` or `none`; `currentMa?`; `ownSupply`; `logicVolts?`; `maxInputVolts?`), `settings` (`PartSetting[]`: `id`, `name`, `unit?`, `type` `number`/`integer`/`choice`, `default`, `min?`, `max?`, `options` (`id`, `name`), `apply` (`live`: no upload; `build`: compiled in, docs/wiring.md §11), `help?`), `shapes` (`PartShape[]`: `id`, `name`, `description?`, `roles` (`PartRole[]`: `id`, `name`, `needs` (`digital-out`, `digital-in`, `analog-in`, `pwm`, `interrupt`), `help`, `provided`: it may go on a pin a part provides, `optional`: it may be left unconnected, and a suggestion leaves it out unless it is asked for). Role ids are the `PINS` message's role names (`pwm`, `dir`, `en`, `in1`, `in2`, `stby`, `trig`, `echo`, `signal`, `sense`), so a plan's pins and a code-mode board's reported pins mean the same, `wiring` (lines with `{pin:<role>}` and `{setting:<id>}`)), `uses` (`servo`, `tone`, `interrupts-off`), `bus?`, `i2cAddresses`, `addressSetting?`, `addressHint?` and `provides?` (`PinProvider`: `count`, `name`, `abilities`, `volts`: the pins such a part adds). Only shapes the FaberLink library can drive are listed. |
| `GET /api/v0/arduino/plans` | `GuidedBoardSummary[]`, by name: `name`, `board?`, `parts`, `takenOver`, `problem?` (the first thing wrong). A sketch folder without `board.toml` is a code-mode sketch and isn't listed. |
| `POST /api/v0/arduino/plans` | `NewPlanRequest` `{"name": "mega", "board": "mega2560"}` → `201 GuidedBoard`: makes the folder with an empty plan, its sketch and `my_code.h`. Errors: `400 invalid-board-name` (a sketch name), `404`/`422 unknown-board`, `422 board-not-guided` (`pi-header`: not built yet), `409 already-exists`. |
| `GET /api/v0/arduino/plans/{name}` | → `200 GuidedBoard`: `name`, `folder`, `sketch` (its `.ino`, for compile and upload), `plan?` (`BoardPlan`: `board`, `name`, `parts` by name, each a `PlannedPart`: `template`, `shape?`, `pins` (role → pin name), `settings` (id → number or choice id, every setting filled in), and `takenOver?`), `fileProblem?` (`board.toml` itself can't be read: a `PlanProblem` with its `line`), `problems` (`PlanProblem[]`, below, with the `line` in `board.toml` where one applies), `wiring` (`PartWiring[]`: `part`, `template`, `lines` (`Trig → D7`), `warnings`), `generated` (the sketch still has Faber's mark) and `edited` (the sketch was changed outside Faber since Faber wrote it). Errors: `404 unknown-plan`. |
| `POST /api/v0/arduino/plans/{name}/suggest` | `SuggestRequest` (`template`, `shape?`, `name?` (an existing part's name edits that part), `pins?` (roles already chosen), `settings?`) → `200 PartSuggestion`: `name` (the one asked for, or a free one: `front`, `front_2`), `template`, `shape`, `roles` (`RoleOptions[]`: `role`, `name`, `needs`, `suggested?`, `suitable` (best first), `unsuitable`, `inUse` (`pin`, `by`: `front (Trig)`, `the I²C bus, SDA (imu)`), `reserved` (`pin`, `reason`)), `part` (the `PlannedPart` with the suggested pins and every setting) and `problems` (what the part would add to the plan's problems, including those it causes for other parts, like a servo stopping a motor's PWM). Writes nothing. Errors: `404 unknown-plan`, `400 invalid-part` (an unknown template or shape, a shape not ready yet, or a bad name), `422 plan-file-invalid`, `422 board-not-guided`. |
| `PUT /api/v0/arduino/plans/{name}` | `SavePlanRequest` `{"plan": BoardPlan, "overwriteEdits"?: true}` → `200 GuidedBoard`: checks the plan, writes `board.toml` (keeping its comments; pins as numbers for plain digital pins and names otherwise, every setting written out), rewrites `<name>.ino` and creates `my_code.h` if it is missing (never overwritten). The plan's `name` is the path's. Errors: `422 invalid-plan` (the error body's `problems` lists every error; nothing is written), `409 sketch-edited` (the sketch was changed outside Faber: take it over, or send `overwriteEdits`), `409 plan-taken-over`, `422 board-not-guided`, `404 unknown-plan`. |
| `POST /api/v0/arduino/plans/{name}/take-over` | → `200 GuidedBoard`: "Take over the code" (docs/wiring.md §8). The sketch's generated header becomes a note that it is your code, the rest of it stays byte for byte, and `board.toml` gets `taken-over` (RFC 3339) and a comment: from then on it is only a note, and saving the plan is refused. One way; doing it again changes nothing. The sketch then compiles for any board, like any code-mode sketch. |

A `PlanProblem` is `severity` (`error`: the plan can't be saved like this; `warning`: worth
knowing), `code`, `part?`, `role?`, `setting?`, `pin?`, `message`, `hint` and `line?`. The codes:

| Code | Severity | When |
|---|---|---|
| `unknown-template`, `unknown-shape`, `shape-not-ready`, `invalid-part-name` | error | The part itself (a name the sketch uses, like `link` or `loop`, isn't a part name). |
| `unknown-setting`, `invalid-setting` | error | A setting the template doesn't have, or a value out of its range or not one of its choices. |
| `unknown-role`, `missing-pin`, `unknown-pin` | error | Pins that don't match the shape's roles, or a pin the board doesn't have. |
| `reserved-pin` | error | D0/D1 (the USB link to Faber), a Pi's GPIO0/1. |
| `unsuitable-pin` | error | The pin can't do the role's job (`Echo needs a digital pin: A6 on an Arduino Nano is analog only.`). |
| `pin-used-twice` | error | Two roles on one pin, or a pin something outside the plan uses. |
| `bus-pin-used` | error | A plain pin on SDA/SCL (or SPI) while a part uses that bus. |
| `servo-timer-conflict`, `tone-timer-conflict` | error | A PWM role on a pin whose timer the Servo library (Uno: Timer1, D9/D10; Mega: Timer5 first, D44–D46) or `tone()` (Timer2) takes. |
| `too-many-servos` | error | More servos than the Servo library drives on the board. |
| `i2c-address-clash` | error | Two I²C parts on one address (0x68 MPU6050 and DS3231, …); the hint is the template's way to move one. |
| `divider-too-high` | error | A battery divider that would put more than the analog reference on its pin with a full battery; the hint gives the smallest R1. |
| `interrupt-pins-used` | warning | Every free interrupt pin is used as a plain pin. |
| `interrupts-off` | warning | A part that turns interrupts off while it works (an LED strip). |
| `own-supply` | warning | A part that needs its own supply (motors). |
| `level-shift-needed` | warning | A part whose signals or inputs don't match the board's logic level. |
| `power-budget` | warning | The parts take more than about 400 mA from the board's own supply. |

The editor's compile and upload refuse a guided board's sketch for a board its plan isn't for
(`409 plan-for-another-board`; a Nano's two bootloaders count as one board), until it is taken
over.

## The simulated world

The design is [simulation.md](simulation.md), section 7. Every `simulate = "world"` part lives in
one 2D world, which moves on 50 times a second on the core's clock while any world part runs:

- The **world base** (the one `planar-base` with `simulate = "world"`) is the machine. Its
  footprint is its `length` × `width` settings. It drives with its latest command (lease-checked
  and clamped like any command), changing speed no faster than its `max-accel`, and publishes
  `commanded`, `measured` (the velocity it really has) and `bumped` (it stopped short because the
  next step would have hit something; it never passes through). Its safe state, after a lease
  ends or the e-stop, is a zero command, so it **ramps** to a stop within its acceleration limit.
- World **`laser-scan`** parts raycast one revolution from their `mount` at the connector's rate
  and range (the LD19: 450 points, 10 Hz, 0.02–12 m, about 1 cm of noise). World **`distance`**
  parts cast their cone (`min-m`, `max-m`, `fov-rad`) and report the nearest hit, at `rate-hz`
  (10 by default). Without a world base the machine stands at the start, so sensors alone still
  see the room. A world part with another capability (a motor) is `degraded` and says so.
- The machine's **ground truth** is the signal `world/pose` (`pose@1`, frame `world`), for as
  long as any world part runs. A part can't be called `world` for that reason.

The world comes from `[world]` in parts.toml: `name` (one of `GET /api/v0/worlds`) and an
optional `start = { x, y, yaw }` (m and rad, world frame: x east, y north); without `start` the
world's own start is used. Without `[world]`, world parts are in the `empty` world (the log says
so once). An unknown `name` is a config error with its line, like any other. Changing `[world]`,
by hand or with `PUT /api/v0/world`, rebuilds the world with the machine at rest at its start.

| Method and path | Body / result |
|---|---|
| `GET /api/v0/worlds` | `WorldInfo[]`: `name`, `title`, `description`, `size` (`[east–west, north–south]` in m), in menu order |
| `GET /api/v0/world` | `WorldView`: the current world's `name`, `title`, `description`, `start` (`Pose`: `x`, `y`, `yaw`), `geometry` and `footprint?` (`length`, `width`; absent until a world base exists). `geometry`: `bounds` (`min`, `max`), `open` (rays may escape), `walls` (`Segment[]`: `a`, `b`), `shapes` (solid polygons: `label?`, `points`), `overhead` (drawn only: table tops), `movers` (`label?`, `radius`, `speed`, `path`, `offset`: they loop their path). Points are `[x, y]` in m. |
| `PUT /api/v0/world` | `{"name": …, "start"?: {"x", "y", "yaw"}}` → `200 WorldView`. Writes `[world]` in parts.toml, keeping its comments; without `start`, any old one goes. Errors: `400 unknown-world` (the hint lists them), `400 start-outside-world`, `400 bad-request`, `409 config-broken` (parts.toml has a problem to fix first) |
| `POST /api/v0/world/reset` | `204`: the machine is back at its start, at rest. A lease keeps driving it from there. |

A `world` message: `pose` (`x`, `y`, `yaw`: world frame), `velocity` (`vx`, `vy`, `omega`:
machine frame), `bumped`, `movers` (`[{x, y, radius}]`, where each moving obstacle is now) and
`tUs` (when the world last moved on, core clock).

## Files

Paths are `<root>:/<path>` with `/` separators, e.g. `home:/Desktop/notes.txt`. Roots:

| Root | What | Available |
|---|---|---|
| `home` | Faber's home folder: `Desktop`, `Documents`, `Recordings`, `Backups`, `System`, `Trash` (fixed folders, `special` in listings) | always |
| `disk-<name>` | A connected USB stick or SD card, e.g. `disk-e:/` (Windows drive E:) or `disk-sennep-sd:/` (Linux `/media/<user>/SENNEP SD`). `<name>` is lowercase letters, digits and single dashes. | while it is plugged in; the core looks every port scan (2 s) |
| `machine` | The whole computer (`machine:/` lists drives on Windows, `/` on Linux) | only with the developer setting `fullMachineFiles` on |

| Method and path | Body / result |
|---|---|
| `GET /api/v0/files/roots` | `FileRoot[]`: home, then connected disks (their `label`, e.g. "SENNEP SD (E:)"), then `machine` if on |
| `GET /api/v0/files/list?path=…` | `FolderListing`. Folders first, then files, by name. |
| `GET /api/v0/files/content?path=…` | The raw bytes, up to 16 MB (for Notes and previews; bigger files answer `too-large`). With `&download=1` it streams the file of any size as an attachment, keeping non-ASCII names (`filename*`). |
| `PUT /api/v0/files/content?path=…` | Raw bytes as the body: creates or overwrites the file → `204`. Max 16 MB. `&mode=create` refuses with `409 already-exists` instead of overwriting (uploads). |
| `POST /api/v0/files/folder` | `PathRequest` → `201` |
| `POST /api/v0/files/move` | `MoveRequest`: rename or move → `204` |
| `POST /api/v0/files/trash` | `PathsRequest`: moves to `home:/Trash` → `204` |
| `POST /api/v0/files/restore` | `PathsRequest` with Trash paths: back to where they came from → `204` |
| `POST /api/v0/files/delete` | `PathsRequest`: deletes **for good**. Only for paths in the Trash, on a disk, or in `machine:` → `204` |
| `POST /api/v0/files/empty-trash` | → `204` |
| `POST /api/v0/files/samples` | Copies the sample LD19 recording into `home:/Recordings` → `201` |

Rules the core enforces (never the shell):
- A path can never leave its root: `..`, absolute paths inside a path and symbolic links pointing
  outside are refused (`bad-path`).
- The fixed folders can't be renamed, moved, deleted or trashed (`protected`).
- `home:/System` holds `parts.toml`, which is the machine config itself. Writing it is validated
  first: a broken config is refused with `422 config-invalid`, its message saying the line. A valid
  one is saved and applied at once. Nothing else can be created in System.
- Only the home folder has a Trash. Trashing anything on a disk or in `machine:` is refused with
  `409 no-trash`; the shell offers "Delete for good" instead. A whole disk (`disk-e:/`) can't be
  deleted (`protected`).
- A move stays within its root; moving between home and a disk is refused (`bad-path`) until copy
  lands.
- A disk that is plugged in or pulled out is announced as `files-changed` with its root
  (`["disk-e:/"]`): fetch `files/roots` again. A path on a disk that is gone answers
  `404 not-found`.
- Files in `home:/Recordings` ending in `.fbcap` are replay ports (`replay:<file name>`).

File error codes: `bad-path`, `not-found`, `already-exists`, `protected`, `not-a-folder`,
`not-a-file`, `too-large`, `config-invalid`, `machine-files-off`, `no-trash`, `io-error`.

## Storage and backups

| Method and path | Body / result |
|---|---|
| `GET /api/v0/storage` | `StorageInfo`: bytes used by `home:/Recordings`, `Documents`, `Desktop` and `Backups` (`recordingsBytes` …), and `freeBytes` / `totalBytes` of the disk that holds the data folder. |
| `POST /api/v0/backups` | `BackupRequest` `{includeRecordings}` → `201 BackupInfo` `{path, bytes}`. Errors: `507 disk-full` (checked before anything is written), `500 io-error`. |
| `POST /api/v0/backups/setup` | No body → `201 BackupInfo`: **Share setup**, a backup without the user's files (below). Errors as for a backup. |
| `POST /api/v0/backups/restore` | `RestoreRequest` `{path, users?}` with a `.zip` in the home folder → `204`. `users: true` also replaces this machine's users and API tokens with the backup's and then signs everyone out; without it the machine keeps its own users ([auth.md](auth.md#backups)). `422 backup-invalid` when the backup's users file has no admin. Errors: `400 bad-path` (not in home), `404 not-found`, `422 backup-invalid`, `422 config-invalid` (with the line), `507 disk-full`. |

A backup (D41) is `home:/Backups/faber-backup-YYYY-MM-DD-HHMM.zip`, named after the minute it was
made in the shown time zone (`time.timeZone`, else the machine's); a second one in the same minute
gets ` (2)`. Inside, it is laid out like the data folder:

```
config/parts.toml
config/settings.toml        (if the settings were ever changed)
config/packages.toml        (if a package was ever installed)
config/apps.toml            (if an app was ever turned on or given parts)
config/users.toml           (the users and API tokens, as hashes; never in a setup)
connectors/<id>/…           each installed package's connector, as installed
apps/<id>/…                 each installed app, as installed, with its saved state.toml
files/Desktop/…
files/Documents/…
files/Recordings/…          only with includeRecordings
```

Only packages listed in `packages.toml` are backed up; a folder copied into `connectors/` or
`apps/` by hand isn't a package and is left out. An app's `state.toml` is its data, so it is backed
up with the app (the `.tmp` and `.broken` files Faber leaves while saving it are not).

**Restore checks everything before anything changes:** the zip must be a Faber backup
(`config/parts.toml`, and nothing outside the folders above: `backup-invalid`), `parts.toml` must
be valid (`config-invalid`, its message saying the line, exactly like saving it in Files),
`settings.toml` must be readable, every name must be a safe path, and the files must fit on the
disk. **Connector code is never copied in blind:** every connector in the backup must be listed in
its `packages.toml` and goes through the same checks and staging load as
`POST /api/v0/packages`. One that fails fails the whole restore with `422 backup-invalid`, naming
the connector and why (e.g. `The connector "my-sensor" in this backup can't be restored:
connectors/my-sensor/faber.toml line 13: …`). **Apps are checked the same way:** each must be
listed as an app in the backup's `packages.toml` and passes the install checks and staging load
(its logic compiled in the sandbox, its UI's files checked); its `state.toml` must read as the
app reads it (text keys and values, at most 64 KB); and `apps.toml` must be valid
(`422 config-invalid`, with the line, like `parts.toml`). A backup made before connectors or apps
were backed up has none, and restores as before, leaving the installed ones (and `apps.toml`)
alone. Then:

- The connectors are put in place first, like an install: one installed already at the same
  version is left alone; another version replaces the installed one, and the parts on it restart.
  Its `source` is kept from the backup, `installedAt` is the time of the restore. Packages that
  aren't in the backup stay. If one doesn't load once in place, all are put back and the restore
  fails before anything else changes. The `packages` topic announces the change.
- The apps are put in place the same way, with their runs stopped meanwhile (an app saves its
  state when its run ends, so it can't overwrite the restored one). Each gets the state from the
  backup; an app whose backup has no `state.toml` keeps the one it has.
- `parts.toml` is saved and applied at once (parts start or stop), then `apps.toml` (if the backup
  has one): apps that were on start again once their parts are there.
- The settings are replaced, **except the developer settings**: a backup never turns
  whole-machine files on. A time zone this machine doesn't know falls back to its own.
- Files are put back where they were. A file that is already there with other contents is first
  moved to the **Trash** (so nothing is lost); one with the same contents is left alone. Files that
  aren't in the backup stay.
- `settings` and `files-changed` (`home:/`) are announced, and restored recordings become replay
  ports.

**Share setup** (Settings → Storage; apps.md §7, Q3) makes `home:/Backups/<machine>-setup-YYYY-MM-DD-HHMM.zip`:
a backup without the user's files, to set up another machine the same way. It holds
`config/parts.toml`, `config/settings.toml` **without** `machine` (the name) and `developer`,
`config/packages.toml`, `config/apps.toml` and the installed connectors and apps (with their saved
state), and `config/setup.toml` (`kind = "setup"`) to say what it is, but nothing from `files/`
(no Desktop, Documents or Recordings). It is restored with `POST /api/v0/backups/restore` like a
backup, through every check above; the receiving machine **keeps its own name**, its files and its
developer settings. A setup that holds files is refused (`422 backup-invalid`).

Backups of any size can be downloaded with `GET /api/v0/files/content?path=…&download=1`, which
streams from disk.

## Settings

Stored in `config/settings.toml`. One `Settings` object, shared by every browser and the attached
screen. Every field has a default, so a settings.toml from an older Faber still loads.

| Method and path | Body / result |
|---|---|
| `GET /api/v0/settings` | `Settings` |
| `PUT /api/v0/settings` | `Settings` → `Settings`. Needs an operator, and an **admin** when it changes `machine`, `time` or `screen` (`403 not-allowed`, `needs: "admin"`). `developer` must be unchanged (`use-developer-endpoint` otherwise). `time.timeZone` must be empty or a zone the core knows (`invalid-time-zone`). |
| `POST /api/v0/settings/developer` | `DeveloperChange` → `Settings`. Turning `fullMachineFiles` on needs `acknowledgedRisk: true` (`risk-not-acknowledged` otherwise). |
| `POST /api/v0/settings/reset` | → `Settings`: every setting back to its default, **including** `developer` (whole-machine files off). Parts and files are not settings and stay. |

| Field | Values (default first) | Notes |
|---|---|---|
| `machine.name` | `faber`, or any host name (`a-z`, `0-9`, `-`, 1–63, no dash at the ends) | browser tab, About, backup names (`<name>-backup-….zip`), and `<name>.local` on the Pi image. `400 invalid-machine-name` otherwise. Reset keeps it. |
| `appearance.theme` | `light`, `dark`, `system` | |
| `appearance.style`, `appearance.wallpaper` | `aluminium`, … | |
| `appearance.language` | `en`, or a language code (`da`, `pt-BR`) | the shell's language (D44). `400 invalid-language` if it isn't a code. A code without a usable translation shows English. |
| `accessibility.reduceMotion` | `system`, `on`, `off` | `system` follows `prefers-reduced-motion` |
| `accessibility.higherContrast` | `system`, `on`, `off` | `system` follows `prefers-contrast: more` |
| `accessibility.textSize` | `"100"`, `"115"`, `"130"`, `"150"` | percent of the normal text size |
| `units.length` | `metric`, `imperial` | display only (D43) |
| `time.timeZone` | `""`, or an IANA zone | `""` = the machine's own (`SystemInfo.timeZone`) |
| `time.clock` | `24h`, `12h` | |
| `taskbar.size`, `desktop.icons` | | |
| `desktop.removed` | program keys (`tool:<id>`, `accessory:<id>`) | programs the user took off the desktop ("Remove from desktop"). Every other program has a desktop icon, so a program added in a later version appears by itself; missing in older files = none removed |
| `taskbar.pinned` | program keys, in order: `tool:<id>`, `accessory:<id>`, `app:<id>` | an app whose `[window] pinned = true` is added at the end when it is first installed (not on an update, so unpinning it sticks); uninstalling an app takes it out |
| `screen.role` | `viewer`, `operator` | the role the attached screen is signed in as ([auth.md](auth.md#the-attached-screen-kiosk)); admins only |
| `developer.fullMachineFiles` | `false`, `true` | only through `/settings/developer` (admins only) |

Reset is its own endpoint rather than a `PUT` of the defaults because the defaults live in one
place (the core), and because reset must also turn whole-machine files off, which `PUT` refuses on
purpose.

The WebSocket topic `settings` sends the settings now and after every change.

## Users and signing in

The design, the roles and the reasons are in [auth.md](auth.md). A browser signs in and gets the
session cookie `faber-session` (`HttpOnly`, `SameSite=Strict`); a program sends an API token as
`Authorization: Bearer faber_…`. The attached screen, on its own listener (`--screen-listen`), is
signed in as its role without either.

| Method and path | Who | Body / result |
|---|---|---|
| `GET /api/v0/session` | anyone | `SessionInfo`: `machineName`, `language`, `needsSetup` (no users yet), `signInOff` (`--no-auth`), and `identity?` (`user?`, `role`, `via`: `session`, `token`, `screen` or `developer`) when the request is signed in. |
| `POST /api/v0/session` | anyone | `SignIn` `{name, password}` → `200 SessionInfo` and the cookie. Errors: `401 sign-in-failed` (the same for a wrong name and a wrong password), `429 too-many-attempts` with `Retry-After` (seconds) after 5 failures from the address or for the name, doubling from 1 s up to 15 min. |
| `DELETE /api/v0/session` | anyone | `204`, the session ended and the cookie cleared. |
| `POST /api/v0/setup` | anyone, once | `SignIn` → `201 SessionInfo` and the cookie: the first admin, signed in. Only while no users exist: `409 already-set-up` after. Errors: `400 invalid-name` (1–32 of `a-z 0-9 - _ .`), `400 password-too-short` (under 8 characters), `400 password-too-long` (over 256 bytes), `500 users-file-broken` (users.toml exists but can't be read: never treated as a fresh machine). |
| `GET /api/v0/users` | admin | `UserInfo[]` (`name`, `role`), never a password. |
| `POST /api/v0/users` | admin | `NewUser` `{name, password, role}` → `201 UserInfo`. Errors as for setup, and `409 name-taken`. |
| `DELETE /api/v0/users/{name}` | admin | `204`; their tokens go and their sessions end. `409 remove-yourself`, `409 last-admin`, `404 unknown-user`. |
| `PUT /api/v0/users/{name}/role` | admin | `RoleChange` `{role}` → `204`. It applies at once to their sessions and tokens. `409 last-admin` (demoting the only admin). |
| `PUT /api/v0/users/{name}/password` | viewer | `PasswordChange` `{password, current?}` → `204`. Your own needs `current` (`403 wrong-password`) and ends your other sessions; an admin may set anyone's (no `current`), which ends all of theirs. Someone else's as a non-admin: `403 not-allowed`. |
| `GET /api/v0/tokens` | viewer | `TokenInfo[]` (`id`, `name`, `user`, `role`, `created`): your own; an admin's lists everyone's. Never the token itself. |
| `POST /api/v0/tokens` | viewer | `NewToken` `{name, role}` → `201 TokenCreated` `{token, secret}`. The `secret` (`faber_` + 43 characters) is **shown this once**. `role` is at most your own (`403 role-too-high`). Only a browser that signed in with a password can make one (`403 not-allowed` with a token). |
| `DELETE /api/v0/tokens/{id}` | viewer | `204`: your own, or anyone's for an admin. `404 unknown-token`. |

A token acts with the lower of its own role and its user's current role. A revoked or unknown
token gets `401 not-signed-in` (never treated as "nobody").

**Checks on every request** (`core/src/api/auth.rs`):

- **Host:** only names that can be this machine are answered: an IP address, a name without dots,
  `localhost`, names under `.local`, `.internal`, `.localhost`, `.lan`, `.home.arpa`, `.ts.net`,
  and the machine's name. Anything else is `421 unknown-host`, so a web page that points its own
  domain at the machine (DNS rebinding) gets nowhere.
- **Origin:** a request other than `GET`/`HEAD`, and a WebSocket upgrade, that carries an `Origin`
  must carry this machine's (`http://` or `https://` + the `Host` it was sent to), or it gets
  `403 bad-origin`. Browsers always send `Origin` on these, so only programs leave it out.
- **Role:** the route's entry in `core/src/api/access.rs`. A route without one is refused for
  everyone (`500 no-access-rule`, logged); a test fails if any route lacks one.

The **files** routes also check each path: `machine:/…` and `home:/Backups/…` need an admin
(backups hold the users' password hashes), reading the rest of the home folder a viewer, and
changing it an operator.

## Translations

Translations of the shell installed on this machine: `<code>.json` files in
`<data>/config/translations/` (created on start). A file works as soon as it is there; no restart.
English is built into the shell and is never listed. How to write one: [translations.md](translations.md).

| Method and path | Body / result |
|---|---|
| `GET /api/v0/translations` | `TranslationInfo[]`, sorted by code: `code` (from the file name), `name` (the file's `_language`, else the code), and `problem` when the file can't be used (e.g. `da.json has a problem on line 3: …`). Files whose name isn't `<code>.json` are ignored. |
| `GET /api/v0/translations/{lang}` | The file, as one JSON object. Errors: `400 invalid-language` (not `[a-z]{2,3}(-[A-Z]{2})?`, so no path can reach outside the folder), `404 unknown-language`, `422 translation-invalid` (broken JSON, with the line; not one object; bigger than 1 MB). |

The core doesn't check the texts inside a file: the shell does, key by key, and shows English for
anything it can't use. How complete a translation is (the percentage in Settings) is worked out by
the shell too, since only the shell has the English catalogue.

## Packages

A connector package (`.faberpkg`, [packages.md](packages.md) §5) is a zip with `faber.toml` at its
root, `connector.wasm` (wasm connectors only), `README.md`, `LICENSE`, and optionally `icon.svg`,
`captures/*.fbcap` and `firmware/<board>.hex` (bridge firmware, see [the Arduino flasher](#the-arduino-flasher)). Installing one needs no restart: the connector is offered by
`GET /api/v0/connectors` at once, and parts already using that connector restart with the new
version. Until the store exists (Phase 7), nothing is checked by the Faber registry: every package
says `checked: false`, and the shell says so before installing.

| Method and path | Body / result |
|---|---|
| `GET /api/v0/packages` | `PackageInfo[]`, by id: `id`, `name`, `version`, `kind` (`wasm` or `declarative`), `publisher?`, `installedAt` (RFC 3339, UTC), `source` (the file it came from), `checked` (always `false` for now), `usedBy` (the parts using it), `problem?` (a `ConnectorProblem`, when its connector isn't loaded), and `firmware` (the boards it carries firmware for, for [the Arduino flasher](#the-arduino-flasher)). Read from `<data>/config/packages.toml`. Errors: `500 io-error` (that file can't be read; the hint says how to recover). |
| `POST /api/v0/packages?mode=preview&name=<file name>` | The package file as the body (any content type). Checks it in full, exactly as installing would, and installs nothing: `200 PackagePreview`: `connector` (a `ConnectorInfo`, as `GET /api/v0/connectors` will list it; its `port` is what it can touch), `publisher?`, `readme` (the README as text: show it as text or safe Markdown, never as HTML), `captures` (its recordings), `replaces?` (the version installed now) and `checked`. |
| `POST /api/v0/packages?name=<file name>` | The same, and installs it into `<data>/connectors/<id>/`: `201 PackageInfo`. `name` is remembered as `source` (default `upload.faberpkg`). The version it replaces is kept until the new one has loaded; if it doesn't load, the old one is put back. |
| `DELETE /api/v0/packages/{id}` | `204`. Removes the folder and its line in `packages.toml`. Errors: `404 not-installed`, `409 package-in-use` (a part uses its connector: the message names the parts; remove them first). |

The WebSocket topic `packages` sends the list now and after every install or uninstall, from any
browser tab or API client.

Errors from `POST`, each with a hint: `400 bad-request` (a `mode` other than `preview`),
`413 package-too-big` (over 50 MB, as a file or unpacked), `422 package-invalid` (not a zip, a path
outside the package such as `../x` or `/etc/x`, a missing `faber.toml`, `README.md`, `LICENSE` or
`.wasm`, a file a package may not hold, or a `[package]` header that can't be read),
`422 package-not-loadable` (the connector itself doesn't load: a `faber.toml` mistake with its line,
named inside the package file, e.g. `my-sensor-0.1.0.faberpkg: faber.toml line 7: …`,
a rule that doesn't read, or a component that isn't one or asks for more than the connector world,
such as WASI), `409 package-id-taken` (its id is a built-in connector's).

An **app's package** ([apps.md](apps.md)) has `type = "app"` in its `faber.toml` and holds
`faber.toml`, `README.md`, `LICENSE`, optionally `icon.svg`, its logic's component (one `.wasm`,
named by `module`) and its UI in `ui/` (at most 1000 files of 10 MB each, plain names of letters,
digits, `.`, `-` and `_`, none hidden, and only web file types: html, css, js, mjs, json, map, txt,
svg, png, jpg, jpeg, gif, webp, ico, woff, woff2, wasm). It installs into `<data>/apps/<id>/`; an
update keeps the app's saved `state.toml`. Its preview answers `200 AppPackagePreview`: `app` (an
`AppInfo` as `GET /api/v0/apps` will list it, off and with no parts picked), `readme`,
`replaces?` and `checked`. It is listed in `GET /api/v0/packages` with `kind: "app"` and an empty
`usedBy`. **Uninstalling an app is refused while it is turned on** (`409 app-enabled`: turn it off
first, so nothing it drives stops by surprise); uninstalling it also removes its `[apps.<id>]` from
`config/apps.toml`. Apps and connectors share ids: `409 package-id-taken` when an app's package
has the id of an installed connector, or the other way round. `422 package-not-loadable` covers an
app too: a manifest mistake with its line, a missing `ui/index.html`, or a component that isn't
one or asks for more than the app world.

## Apps

An app ([apps.md](apps.md)) uses capabilities through **roles** (`[[requires]]` in its manifest):
the user picks which parts fill them, and turns it on. `config/apps.toml` holds both (the file is
the source of truth, D13; edits keep its comments):

```toml
[apps.wall-follower]
enabled = true

[apps.wall-follower.roles]
base = "base"
lidar = "lidar"
proximity = ["front", "rear"]   # a role with count = "1.." or "0..": a list
```

An app **runs** when it is on and every role has a part that provides the role's capability. It
drives its `command = true` roles under leases held as `app:<id>`, exactly like any other holder:
every command renews the lease, a lease not renewed in time ends and stops the part, the e-stop
ends it, and an operator can take over (`{"takeOver": true}`): the app's next command answers
`lease-ended` (`taken-over`), and it may ask again (refused while the operator drives). The D46
confirm before driving is the shell's: for an app, turning it on is the user's go-ahead. Its leases
end when it is turned off, when it stops or traps, and when the core stops.

A command role declared **`drive = "window"`** is different: the logic may hold its lease only
while one of the app's windows drives it through the logic ([App windows](#app-windows)), after the
shell's D46 confirm and with the shell's Stop bar showing. Before that, its `acquire` is refused
(`part-not-ready`, "no window is driving …"); when the window lets go, the user presses Stop, the
window closes or its heartbeat stops, the core ends the lease itself (the logic's next command
answers `lease-ended`, `drive-ended`). `AppInfo.roles[].drive` says which (`logic` or `window`).

| Method and path | Body / result |
|---|---|
| `GET /api/v0/apps` | `AppInfo[]`: `id`, `name`, `version`, `publisher?`, `enabled`, `state` (a `PartState`: the same states as a part, below), `logic` (it has a component), `tickHz?`, `roles` (`role`, `capability`, `command`, `count`: `1`, `1..`, or for an optional role `0..1` or `0..`, `parts`: the parts picked, `drive`: `logic` or `window`), `publishes` (`name`, `capability`, `address`: `app:<id>/<name>`), `ui?` (`/apps/<id>/ui/index.html`), `icon?` (what the shell shows for it: its `[window]` icon, else the package's own `icon.svg` as `/apps/<id>/icon.svg`) and `window?` (`title?`, `icon?` (a URL, for an icon under `ui/`), `width?`, `height?`, `pinned`). |
| `GET /api/v0/apps/{id}` | One `AppInfo`. `404 unknown-app`. |
| `POST /api/v0/apps/{id}/enable` | Turns it on (`enabled = true` in apps.toml): `200 AppInfo`. It starts when its roles are filled. `404 unknown-app`, `422 config-broken` (apps.toml can't be read: the message has its line). |
| `POST /api/v0/apps/{id}/disable` | Turns it off: its logic stops, its leases end and its parts get their safe state before this answers. `200 AppInfo`. |
| `PUT /api/v0/apps/{id}/roles` | `{"roles": {"lidar": "lidar", "proximity": ["front", "rear"]}}` (a part's name, or a list): exactly these roles get these parts; roles left out get none. An app that is on restarts with its new parts. `200 AppInfo`. Errors: `400 unknown-role`, `400 too-many-parts` (more than one for a `count = "1"` or `"0..1"` role), `400 bad-request` (a part twice), `422 unknown-part`, `422 role-mismatch` (the part doesn't provide the role's capability: the message says what it provides). Sub-parts (`mega.left`) fill roles like any part. |
| `GET /api/v0/apps/{id}/state` | `AppState`: `values` (every key and value the app saved, both strings), `bytes` (keys and values together) and `maxBytes` (65536). The app's logic (the `state` interface) and its windows share this one store. `404 unknown-app`. |
| `PUT /api/v0/apps/{id}/state` | `{"key": "speed", "value": "0.3"}` sets one key; `"value": null` removes it. `200 AppState`. The running logic sees it at its next `get`, and the `app-state` topic tells every window. Errors: `400 bad-key` (a key of 0 or more than 256 bytes), `413 state-full` (the store would pass 64 KB), `404 unknown-app`. |
| `GET /apps/{id}/ui/{path}` | The app's UI files, for the shell's app window. Only files the installer accepts, with their type from their extension, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer` and `Content-Security-Policy: default-src 'self'; connect-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; media-src 'self'; object-src 'none'; frame-src 'none'; child-src 'none'; worker-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'; sandbox allow-scripts`: a UI can't reach the network or the API, only the shell, through `postMessage` ([apps.md](apps.md) §9). `sandbox` gives every page an opaque origin even when it is opened on its own, so it never runs as the shell's origin; its own files answer that origin with `Access-Control-Allow-Origin: null` (module scripts and fonts are fetched with CORS). `404 not-found` for anything else. |
| `GET /apps/{id}/icon.svg` | The icon at the root of an app's package, for `<img>` only: `image/svg+xml`, `nosniff` and `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; img-src data:; sandbox`, so opened on its own it runs and loads nothing. The shell never puts an app's SVG inline. `404 not-found` when the app has none. |

**States** (`state.kind`, with `message` and `hint` where there is a problem):

| Kind | When |
|---|---|
| `stopped` | Turned off ("The app is turned off."), or Faber stopped it after it crashed 5 times in a row (the message says the last crash; turn it off and on to retry). |
| `invalid` | On, but a role has no part ("The role "lidar" has no part …"), or apps.toml can't be read. |
| `conflict` | On, but a part filling a role was removed, or doesn't provide the role's capability right now. It starts by itself when the part is back. |
| `starting`, `running` | Its logic is starting, or runs at its `tickHz`. A UI-only app is `running` while on. |
| `failed` | Its logic crashed: a trap, a panic ("The app stopped because of a bug: it panicked (…)"), more than 32 MB of memory, more than 20 ms in one call, or a broken rule (publishing a signal its manifest doesn't declare). Its leases have ended; Faber restarts it after 1, 2, 4, … s. |

**State.** Each app has one small store (keys and values are strings, 64 KB together), kept in
the core while it runs and saved in `<data>/apps/<id>/state.toml`: at most once a second, when its
logic's run ends, before a backup and when the core stops. Its logic and its windows read and
write the same store, so a window's slider and the logic that reads it always agree. The
`app-state` topic (`{"kind": "app-state", "app": "<id>"}`) sends an `app-state` message (`app`,
`values`) at once, then an `app-state-changed` (`app`, `key`, `value?`; no `value` when removed)
for every change, whoever made it; after missing changes it sends `app-state` again. A window never
calls these itself: the shell's app host does, for the `faber.state` calls ([apps.md](apps.md)).
Editing `state.toml` by hand while the core runs has no effect (and is overwritten): use the API.

**Derived signals** (D24) are published as `app:<id>/<name>` while the app runs, read-only: they
appear in `GET /api/v0/signals/…` and the `signal` topic, but there are no leases or commands on
them (`404 unknown-part`). A part's name can never contain `:`, so no part clashes with them.

App logs are in the log with the source `app:<id>`.

## Recordings

While a serial session is open, `serial-record-start {id, name}` starts saving everything it
receives to `home:/Recordings/<name>.fbcap`. The core answers `serial-recording {id, path}`.
`serial-record-stop {id}` (or closing the session) ends it with `serial-recording-stopped {id,
path, bytes}`. The new recording becomes a replay port at once. Names follow the part-name rule
(`invalid-name`) and must not exist yet (`already-exists`).

## Format details

- **Readings** are tagged by capability: `{ "capability": "laser-scan@1", "reading": { … } }`.
- **No-return ranges** (`+inf` in `laser-scan`) are real infinity in MessagePack. JSON has no
  infinity, so they are `null` there. The shell's API layer turns `null` back into `Infinity`, so
  the rest of the shell only ever sees numbers.
- **Bytes** (`serial-send`, `serial-data`) are MessagePack `bin`, and arrive as `Uint8Array`. In
  JSON they are arrays of numbers. The API layer converts them to `Uint8Array`.
- **64-bit integers** (`tUs`) fit safely in a JavaScript number.

## Ports

| Id | What |
|---|---|
| `usb:<vid>:<pid>` | A USB serial adapter, e.g. `usb:10c4:ea60` (CP2102). If two identical adapters are plugged in, they become `usb:<vid>:<pid>:<serial number>`. |
| `serial:<name>` | A serial port without USB information, e.g. `serial:COM3` or `serial:/dev/ttyAMA0`. Its `kind` is `bluetooth` for Bluetooth serial links. |
| `replay:<name>` | A recording in `home:/Recordings` played back in a loop at real speed (D35). The LD19 sample is added with `POST /api/v0/files/samples` (D32). |
| `i2c-<bus>` | An I²C bus, `/dev/i2c-<bus>` (the Pi's header bus is `i2c-1`). Claimed **per address**: two parts at different addresses share it, two at the same address are a conflict. Its `kind` is `i2c`. |
| `spi-<bus>.<cs>` | One SPI device: a bus and a chip select, `/dev/spidev<bus>.<cs>`. Kind `spi`. |
| `gpio-<line>` | One GPIO line on the header, by its number: GPIO17 (pin 11) is `gpio-17`. Its label names the pin. GPIO0 and GPIO1 (the HAT EEPROM's) are never offered, and a line a bus uses (GPIO2 and 3 while `i2c-1` is on) is `reserved`. Kind `gpio`. |

I²C, SPI and GPIO exist only on Linux (the Pi), through the kernel's own interfaces: `I2C_RDWR` on
`/dev/i2c-*`, spidev, and the GPIO character device's v2 interface (never sysfs). On Windows and
macOS there are none (`SystemInfo.buses` is `none`). `faber-core --fake-hardware` attaches
simulated buses instead: `i2c-1` with a TMP102 at 0x48 and a BME280 at 0x76 (which only says who it
is), `spi-0.0` and `spi-0.1` with MOSI looped back to MISO, and every header GPIO.

## I²C scan

`POST /api/v0/ports/scan-i2c` looks for devices the way `i2cdetect` does, but it only ever
**reads**: each address from `first` (0x08) to `last` (0x77) gets a one-byte read, skipping the
ranges the I²C specification reserves (0x00–0x07, 0x78–0x7f) as `i2cdetect` does. `i2cdetect`'s
default "quick write" is never used, so the scan writes nothing to a device it doesn't know. An
address a part uses isn't probed at all.

`I2cScan`: `port`, `first`, `last`, and `found`: only the addresses that answered or are in use,
lowest first. Each has `address`, `usedBy?` (the part using it; not probed) and `suggestions`
(`chip`, `connector?`, `reason`): what it might be, from common default addresses (0x48–0x4B
TMP102, 0x76/0x77 BME280, 0x68 MPU-6050, 0x29 VL53L0X, 0x3C SSD1306) and from installed connectors
that can be at that address. `connector` is set when one is installed, so Add part can use it.
These are guesses: other chips use the same addresses, and each `reason` says the scan can't be
sure.

## Port problems

Something plugged in over USB that the computer can't talk to never becomes a port, so the port
list alone would just be empty. The core also looks for these (with every port scan, every 2 s)
and reports each one as a `PortProblem`:

```json
{
  "code": "usb-no-answer",
  "label": "Plugged in, but the computer can't talk to it",
  "detail": "Something is plugged in over USB, but it didn't answer when the computer tried to start talking to it (Windows: \"port reset failed\").",
  "hint": "Unplug it and plug it back in firmly. If that doesn't help: try another USB cable (some cables only carry power, not data), ..."
}
```

They come in two places: `GET /api/v0/ports/problems`, and the optional `problems` field of the
`ports` WebSocket message, which is **left out when there are none** (so `{ type, id, ports }` is
still a complete message). Two things failing the same way give two entries.

| `code` | What happened |
|---|---|
| `usb-no-answer` | It didn't answer at all (Windows: "port reset failed") |
| `usb-garbled` | Its answers didn't get through properly (Windows: "device descriptor request failed", "set address failed") |
| `usb-invalid-answer` | It answers, but not by the USB rules: bad connection, or crashed firmware |
| `usb-link-failed` | A USB 3 connection never came up |
| `usb-not-recognised` | Another failure where the computer couldn't find out what it is |
| `usb-no-driver` | It answers, but there is no driver for it (e.g. CH340, CP210x) |
| `usb-stopped` | Windows stopped it because it reported a problem (the code is in `detail`) |

Show `label` as the headline, `detail` under it and `hint` as the thing to try. Things the user
disabled on purpose are never reported. Today this works on **Windows** only; on Linux the list is
always empty (a failed USB connection there only shows up in the kernel log).

## Capture files (`.fbcap`)

The raw bytes from a serial port, with the time each chunk arrived. Used for replay ports, tests
and (later) recording from the serial monitor. Little endian:

```
"FABERCAP1\n"                          10-byte magic
repeat:  t_us u64 | len u32 | len bytes   one chunk as read from the port
```

A truncated last chunk is ignored. This is the same layout as SENNEP's `.ldr` recordings (only
the magic differs).
