# Future ideas

See also the "Later" section of `docs/roadmap.md`.

## Ecosystem
- (medium) Home Assistant integration through MQTT discovery: Faber parts show up in HA.
- (low) ROS 2 bridge.

## Core
- **(High) Name who holds a busy port.** On Linux, "Device or resource busy" should say which program has it (read `/proc/*/fd`, e.g. "used by sennep-lidar, pid 1302") and, for a systemd service, how to stop it. Mikkel hit this on day one on the Pi (2026-10-17).
- (high, before the Pi image) Port problems on Linux ("plugged in, but the computer can't talk to it"; Windows works, `core/src/ports/usb_problems.rs`). A USB device that fails to enumerate gets no sysfs entry and no udev event; the only trace is the kernel log (`usb 1-1.2: device descriptor read/64, error -71`, `device not accepting address`, `unable to enumerate USB device`). Plan: give the core service `CAP_SYSLOG` (Debian sets `kernel.dmesg_restrict=1`), follow `/dev/kmsg` from the last boot, map those lines to the same `PortProblem` codes, and clear an entry when a device later enumerates on the same hub port or after a timeout. Check first whether newer kernels' `/sys/bus/usb/devices/*/usb*-port*/state` shows a port that sees a connection but no device: that would be cheaper and need no capability.

## Shell
- (medium) Typed visual wiring view (parts → apps), Node-RED-like but with capabilities.
- (medium) Record and replay signals (SENNEP's LiDAR recordings already prove the value).

## Settings
- (medium) Translation files (D44): extract shell strings, one file per language, Danish as the test language; community translations as store packages.
- (later) A switch in Settings to turn off "confirm before driving" (D46); on by default.
- See `private/settings-proposal.md` for the full list and what not to build.

## Branding
- (medium) Logo for the GitHub org `faber-os` (and the shell/boot splash, favicon). Mikkel asked for it, 2026-10-15.
