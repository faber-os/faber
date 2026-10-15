# Core API (v0)

How the shell (and later app UIs) talk to `faber-core`. **v0 is internal and may change.** The
types are defined once in Rust (`core/src/api/dto.rs`, `sdk/rust/capabilities`), and the TypeScript
versions in `shell/src/lib/api/generated/` are generated from them by `cargo test`. Never edit
the generated files by hand.

Phase 1 listens on **`127.0.0.1:7878` only** and has no login (D16: auth comes before any remote
access, in Phase 6).

## Conventions

- Field names are **camelCase**. Enum values are **kebab-case** (`usb-serial`, `port-claimed`).
- Times are µs on the core's monotonic clock (`tUs`). `GET /api/v0/system` gives the current value.
- **Errors** (any non-2xx status) always have this body, so the shell can show *what happened* and
  *what to try* (docs/principles.md):

  ```json
  { "error": { "code": "port-claimed", "message": "COM3 is used by the part \"lidar\".", "hint": "Remove or stop \"lidar\" first, or pick another port." } }
  ```

## REST (JSON)

| Method and path | Body / result |
|---|---|
| `GET /api/v0/system` | `SystemInfo` |
| `GET /api/v0/connectors` | `ConnectorInfo[]`: every connector the core can run |
| `GET /api/v0/ports` | `PortInfo[]`: every port, with what claims it and suggestions |
| `GET /api/v0/ports/problems` | `PortProblem[]`: things plugged in that never became a port (see [Port problems](#port-problems)) |
| `GET /api/v0/parts` | `PartInfo[]` |
| `POST /api/v0/parts` | `NewPart` → `201 PartInfo`. Errors: `400 invalid-name`, `400 unknown-connector`, `400 unknown-port`, `409 name-taken`, `409 port-claimed` |
| `DELETE /api/v0/parts/{name}` | `204`. Errors: `404 unknown-part` |

Adding or removing a part **writes `parts.toml`** (keeping comments and formatting) and then
reconciles. The file stays the source of truth (D13).

## WebSocket: `GET /api/v0/ws?format=msgpack|json`

One connection carries everything live. The default `format` is `msgpack`, sent as binary frames.
`json` uses text frames, for debugging (D26).

**Client → core** (`ClientMessage`, tagged by `type`):

| `type` | Fields | Meaning |
|---|---|---|
| `subscribe` | `id`, `topic` | Start receiving a topic. `id` is chosen by the client and unique per connection. |
| `unsubscribe` | `id` | Stop it. |
| `serial-open` | `id`, `port`, `baud` | Open a port for the serial monitor. Fails with `port-claimed` while a part uses it. |
| `serial-send` | `id`, `data` (bytes) | Write bytes. |
| `serial-close` | `id` | Close it. |

**Topics** (`Topic`, tagged by `kind`):

| `kind` | Fields | You get |
|---|---|---|
| `signal` | `address`, `maxHz?` | `signal` messages with readings of `part/name` |
| `parts` | — | a `parts` message now and after every change |
| `ports` | — | a `ports` message now and after every change, including a change in `problems` |
| `settings` | — | a `settings` message now and after every change |
| `files` | — | `files-changed` with the folders whose contents changed (through the core). `home:/` means "a lot changed": refresh everything shown. |
| `logs` | — | the recent log (up to 1000 entries) as `log` messages, then each new entry. Every new subscription gets the recent log again, so a client that reconnects should start its log view afresh. |

**Core → client** (`ServerMessage`, tagged by `type`): `signal`, `parts`, `ports`, `log`,
`serial-opened`, `serial-data`, `serial-closed`, `error`. Each carries the `id` of the subscription
or serial session it belongs to. `error` may come without an `id`.

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
| `internal` | A bug in the core. The log has details. |

Every error carries `message` and `hint`, like REST errors.

## Files

Paths are `<root>:/<path>` with `/` separators, e.g. `home:/Desktop/notes.txt`. Roots:

| Root | What | Available |
|---|---|---|
| `home` | Faber's home folder: `Desktop`, `Documents`, `Recordings`, `System`, `Trash` (fixed folders, `special` in listings) | always |
| `machine` | The whole computer (`machine:/` lists drives on Windows, `/` on Linux) | only with the developer setting `fullMachineFiles` on |

| Method and path | Body / result |
|---|---|
| `GET /api/v0/files/roots` | `FileRoot[]` |
| `GET /api/v0/files/list?path=…` | `FolderListing`. Folders first, then files, by name. |
| `GET /api/v0/files/content?path=…` | The raw bytes. `&download=1` adds `Content-Disposition: attachment`. |
| `PUT /api/v0/files/content?path=…` | Raw bytes as the body: creates or overwrites the file → `204`. Max 16 MB. |
| `POST /api/v0/files/folder` | `PathRequest` → `201` |
| `POST /api/v0/files/move` | `MoveRequest`: rename or move → `204` |
| `POST /api/v0/files/trash` | `PathsRequest`: moves to `home:/Trash` → `204` |
| `POST /api/v0/files/restore` | `PathsRequest` with Trash paths: back to where they came from → `204` |
| `POST /api/v0/files/delete` | `PathsRequest`: deletes **for good**. Only for paths in the Trash, or in `machine:` → `204` |
| `POST /api/v0/files/empty-trash` | → `204` |
| `POST /api/v0/files/samples` | Copies the sample LD19 recording into `home:/Recordings` → `201` |

Rules the core enforces (never the shell):
- A path can never leave its root: `..`, absolute paths inside a path and symbolic links pointing
  outside are refused (`bad-path`).
- The fixed folders can't be renamed, moved, deleted or trashed (`protected`).
- `home:/System` holds `parts.toml`, which is the machine config itself. Writing it is validated
  first: a broken config is refused with `422 config-invalid`, its message saying the line. A valid
  one is saved and applied at once. Nothing else can be created in System.
- Files in `home:/Recordings` ending in `.fbcap` are replay ports (`replay:<file name>`).

File error codes: `bad-path`, `not-found`, `already-exists`, `protected`, `not-a-folder`,
`not-a-file`, `too-large`, `config-invalid`, `machine-files-off`, `io-error`.

## Settings

Stored in `config/settings.toml`. One `Settings` object, shared by every browser and the attached
screen.

| Method and path | Body / result |
|---|---|
| `GET /api/v0/settings` | `Settings` |
| `PUT /api/v0/settings` | `Settings` → `Settings`. `developer` must be unchanged (`use-developer-endpoint` otherwise). |
| `POST /api/v0/settings/developer` | `DeveloperChange` → `Settings`. Turning `fullMachineFiles` on needs `acknowledgedRisk: true` (`risk-not-acknowledged` otherwise). |

The WebSocket topic `settings` sends the settings now and after every change.

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
