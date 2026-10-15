# Core API (v0)

How the shell (and later app UIs) talk to `faber-core`. **v0 is internal and may change.** The
types are defined once in Rust (`core/src/api/dto.rs`, `sdk/rust/capabilities`), and the TypeScript
versions in `shell/src/lib/api/generated/` are generated from them by `cargo test`. Never edit
the generated files by hand.

Phase 1 listens on **`127.0.0.1:7878` only** and has no login (D16: auth comes before any remote
access, in Phase 5).

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
| `ports` | — | a `ports` message now and after every change |
| `logs` | — | the recent log as `log` messages, then each new entry |

**Core → client** (`ServerMessage`, tagged by `type`): `signal`, `parts`, `ports`, `log`,
`serial-data`, `serial-closed`, `error`. Each carries the `id` of the subscription or serial session
it belongs to. `error` may come without an `id`.

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
| `serial:<name>` | A serial port without USB information, e.g. `serial:COM3` or `serial:/dev/ttyAMA0` |
| `replay:<name>` | A capture played back in a loop at real speed. `replay:demo-ld19` is built into the core, so Faber can be tried without hardware. |

## Capture files (`.fbcap`)

The raw bytes from a serial port, with the time each chunk arrived. Used for replay ports, tests
and (later) recording from the serial monitor. Little endian:

```
"FABERCAP1\n"                          10-byte magic
repeat:  t_us u64 | len u32 | len bytes   one chunk as read from the port
```

A truncated last chunk is ignored. This is the same layout as SENNEP's `.ldr` recordings (only
the magic differs).
