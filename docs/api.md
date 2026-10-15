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
- **Units are SI, always** (D06, D43): metres, radians, seconds (µs for timestamps). The shell's
  units setting (metric/imperial, degrees/radians) only changes what the shell *shows*; it converts
  in one place (`shell/src/lib/viewers/units.ts`). The API, recordings and apps never change units,
  so capabilities stay comparable. Wall-clock times are milliseconds since 1970 (UTC); the shell
  shows them in the machine's time zone (D42).
- **Errors** (any non-2xx status) always have this body, so the shell can show *what happened* and
  *what to try* (docs/principles.md):

  ```json
  { "error": { "code": "port-claimed", "message": "COM3 is used by the part \"lidar\".", "hint": "Remove or stop \"lidar\" first, or pick another port." } }
  ```

## REST (JSON)

| Method and path | Body / result |
|---|---|
| `GET /api/v0/system` | `SystemInfo`. Its `api` is a fingerprint of the API types the core was built with (`core/build.rs`); the shell computes its own the same way and, when they differ, asks for the core to be restarted instead of failing request by request. `timeZone` is the machine's own IANA time zone (e.g. `Europe/Copenhagen`), read from its OS each time; `UTC` (with a log line) if the OS doesn't say. |
| `GET /api/v0/about` | `AboutInfo`: `version`, `api` (the fingerprint), `dataFolder` (where config and files live on this computer), `uptimeS` (seconds since the core started), `os`, `timeZone` (the machine's own). |
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
| `POST /api/v0/backups/restore` | `PathRequest` with a `.zip` in the home folder → `204`. Errors: `400 bad-path` (not in home), `404 not-found`, `422 backup-invalid`, `422 config-invalid` (with the line), `507 disk-full`. |

A backup (D41) is `home:/Backups/faber-backup-YYYY-MM-DD-HHMM.zip`, named after the minute it was
made in the shown time zone (`time.timeZone`, else the machine's); a second one in the same minute
gets ` (2)`. Inside, it is laid out like the data folder:

```
config/parts.toml
config/settings.toml        (if the settings were ever changed)
files/Desktop/…
files/Documents/…
files/Recordings/…          only with includeRecordings
```

**Restore checks everything before anything changes:** the zip must be a Faber backup
(`config/parts.toml`, and nothing outside the folders above: `backup-invalid`), `parts.toml` must
be valid (`config-invalid`, its message saying the line, exactly like saving it in Files),
`settings.toml` must be readable, every name must be a safe path, and the files must fit on the
disk. Then:

- `parts.toml` is saved and applied at once (parts start or stop).
- The settings are replaced, **except the developer settings**: a backup never turns
  whole-machine files on. A time zone this machine doesn't know falls back to its own.
- Files are put back where they were. A file that is already there with other contents is first
  moved to the **Trash** (so nothing is lost); one with the same contents is left alone. Files that
  aren't in the backup stay.
- `settings` and `files-changed` (`home:/`) are announced, and restored recordings become replay
  ports.

Backups of any size can be downloaded with `GET /api/v0/files/content?path=…&download=1`, which
streams from disk.

## Settings

Stored in `config/settings.toml`. One `Settings` object, shared by every browser and the attached
screen. Every field has a default, so a settings.toml from an older Faber still loads.

| Method and path | Body / result |
|---|---|
| `GET /api/v0/settings` | `Settings` |
| `PUT /api/v0/settings` | `Settings` → `Settings`. `developer` must be unchanged (`use-developer-endpoint` otherwise). `time.timeZone` must be empty or a zone the core knows (`invalid-time-zone`). |
| `POST /api/v0/settings/developer` | `DeveloperChange` → `Settings`. Turning `fullMachineFiles` on needs `acknowledgedRisk: true` (`risk-not-acknowledged` otherwise). |
| `POST /api/v0/settings/reset` | → `Settings`: every setting back to its default, **including** `developer` (whole-machine files off). Parts and files are not settings and stay. |

| Field | Values (default first) | Notes |
|---|---|---|
| `machine.name` | `faber`, or any host name (`a-z`, `0-9`, `-`, 1–63, no dash at the ends) | browser tab, About, backup names (`<name>-backup-….zip`), and `<name>.local` on the Pi image. `400 invalid-machine-name` otherwise. Reset keeps it. |
| `appearance.theme` | `light`, `dark`, `system` | |
| `appearance.style`, `appearance.wallpaper` | `aluminium`, … | |
| `accessibility.reduceMotion` | `system`, `on`, `off` | `system` follows `prefers-reduced-motion` |
| `accessibility.higherContrast` | `system`, `on`, `off` | `system` follows `prefers-contrast: more` |
| `accessibility.textSize` | `"100"`, `"115"`, `"130"`, `"150"` | percent of the normal text size |
| `units.length` | `metric`, `imperial` | display only (D43) |
| `time.timeZone` | `""`, or an IANA zone | `""` = the machine's own (`SystemInfo.timeZone`) |
| `time.clock` | `24h`, `12h` | |
| `taskbar.size`, `taskbar.pinned`, `desktop.icons` | | |
| `developer.fullMachineFiles` | `false`, `true` | only through `/settings/developer` |

Reset is its own endpoint rather than a `PUT` of the defaults because the defaults live in one
place (the core), and because reset must also turn whole-machine files off, which `PUT` refuses on
purpose.

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
