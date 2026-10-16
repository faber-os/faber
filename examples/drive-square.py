#!/usr/bin/env python3
"""Drive a 0.8 m square with a Faber drive base, over plain HTTP.

What you need
    A Faber core running on this computer, with a drive base part. No hardware: add a
    simulated one in the shell (Add part → Generic drive base → tick Simulated), or put this in
    parts.toml:

        [parts.base]
        connector = "generic-planar-base"
        simulate = "manual"

    Or let it really move, in a simulated world (the World window shows it):

        [world]
        name = "apartment"

        [parts.base]
        connector = "generic-planar-base"
        simulate = "world"
        settings = { length = 0.8, width = 0.65 }

    From the apartment's start, the square (turning right) fits in the living room.

    An API token with the operator role: in the shell, Settings → Users → API tokens → Make
    token (name it drive-square, role Operator), and copy it. Faber shows it only once.

    Python 3.8 or newer. Nothing to install: only the standard library is used.

Steps
    Give the program the token in the environment variable FABER_TOKEN:

        export FABER_TOKEN=faber_…                  # Linux, macOS
        $env:FABER_TOKEN = "faber_…"                # Windows PowerShell

    python examples/drive-square.py                 # the part called "base" on 127.0.0.1:7878
    python examples/drive-square.py my-base http://127.0.0.1:7878

What you should see
    "Driving base." then one line per side and per turn, then "Done; base is stopped." Watch the
    base's viewer in the shell: its commanded velocity follows the program.

If it doesn't work
    "FABER_TOKEN isn't set."      Make a token (above) and set the variable in this terminal.
    "That API token isn't valid"  It was revoked, or mistyped. Make a new one.
    "This needs the operator role" The token's role is viewer. Make one with Operator.
    "Is the Faber core running?"  Start it, or check the address.
    "… is driving base."          Someone else holds the lease (the Drive window?). Let go there.
    "The e-stop is engaged …"     Release it in the shell when it is safe.
    "base bumped into something"   In a world, the square doesn't fit where it stands. Put it
                                   back (Reset position in the World window, or
                                   POST /api/v0/world/reset) and run it again.

How it works
    Anything that moves needs a *lease*: the right to drive it, for 250 ms at a time. Every
    command renews it, so a program sends its velocity about 10 times a second even while it
    stays the same. If the program stops (it crashes, you press Ctrl+C, the network drops), the
    lease runs out and Faber stops the base by itself. That is the lease's whole point.

    Every request carries the token as "Authorization: Bearer faber_…": that is how Faber knows
    who is driving, and that they may (docs/auth.md). Keep the token out of the code itself, so
    it isn't shared along with the program.
"""

import json
import math
import os
import sys
import time
import urllib.error
import urllib.request

PART = sys.argv[1] if len(sys.argv) > 1 else "base"
CORE = (sys.argv[2] if len(sys.argv) > 2 else "http://127.0.0.1:7878").rstrip("/")
TOKEN = os.environ.get("FABER_TOKEN", "").strip()

SIDE = 0.8  # m
SPEED = 0.25  # m/s along each side
TURN = -0.8  # rad/s when turning on the spot: negative is clockwise, to the right
EVERY = 0.1  # s between commands: well inside the 250 ms lease
CLAMPED = []  # non-empty once a clamp has been reported


class Bumped(Exception):
    """The base ran into something in the simulated world."""


class FaberError(Exception):
    """An error from the core: what happened, and what to try (docs/api.md)."""

    def __init__(self, status, code, message, hint):
        super().__init__(message)
        self.status, self.code, self.message, self.hint = status, code, message, hint


def call(method, path, body=None):
    """One request to the core. Returns the JSON answer, or raises FaberError."""
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(CORE + path, data=data, method=method)
    req.add_header("Content-Type", "application/json")
    req.add_header("Authorization", f"Bearer {TOKEN}")
    try:
        with urllib.request.urlopen(req, timeout=2) as res:
            text = res.read()
            return json.loads(text) if text else None
    except urllib.error.HTTPError as e:
        # Every error from the core has a code, a message and a hint.
        error = json.loads(e.read() or b"{}").get("error", {})
        raise FaberError(e.code, error.get("code", "?"), error.get("message", str(e)), error.get("hint", ""))


def drive_for(lease, seconds, vx=0.0, omega=0.0):
    """Sends one velocity again and again for `seconds`. Each command renews the lease."""
    command = {"capability": "planar-base@1", "command": {"kind": "set-velocity", "vx": vx, "vy": 0.0, "omega": omega}}
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        reply = call("POST", f"/api/v0/parts/{PART}/command", {"lease": lease, "command": command})
        if reply["clamped"] and not CLAMPED:
            # Faber clamps to the base's limits rather than refusing. Say so once.
            CLAMPED.append(True)
            print(f"  (the base's limits clamped this to {reply['applied']['command']})")
        if bumped():
            raise Bumped()
        time.sleep(EVERY)


def bumped():
    """Whether the base reports a bump (only a base in a simulated world can)."""
    value = call("GET", f"/api/v0/signals/{PART}/planar-base")
    reading = (value.get("reading") or {}).get("reading") or {}
    return bool(reading.get("bumped"))


def main():
    # The core's messages may hold arrows and quotes a Windows console can't show: never crash on one.
    sys.stdout.reconfigure(errors="replace")
    if not TOKEN:
        print(
            "FABER_TOKEN isn't set. Make an API token in the shell (Settings > Users > API tokens,\n"
            "role Operator), set it in this terminal (export FABER_TOKEN=faber_...) and run this again."
        )
        return 1
    try:
        grant = call("POST", f"/api/v0/parts/{PART}/lease", {"holder": "drive-square.py"})
    except urllib.error.URLError:
        print(f"Couldn't reach {CORE}. Is the Faber core running?")
        return 1
    except FaberError as e:
        print(f"{e.message}\n{e.hint}")
        return 1

    lease = grant["lease"]
    print(f"Driving {PART}.")
    try:
        for side in range(1, 5):
            print(f"Side {side}: {SIDE} m forward")
            drive_for(lease, SIDE / SPEED, vx=SPEED)
            print(f"Turn {side}: a quarter turn right")
            drive_for(lease, (math.pi / 2) / abs(TURN), omega=TURN)
    except FaberError as e:
        # 423: the e-stop was pressed. 403: the lease ended (someone took over, or it expired).
        # Either way the base is already stopped; tell the user why and stop driving.
        print(f"Stopped: {e.message}\n{e.hint}")
        return 1
    except Bumped:
        print(f"{PART} bumped into something. Put it back (Reset position in the World window) and try again.")
        return 1
    except KeyboardInterrupt:
        print("Interrupted.")
    except urllib.error.URLError as e:
        print(f"Lost the connection to the core ({e.reason}). The lease runs out and the base stops by itself.")
        return 1
    finally:
        # Release the lease: the base stops now instead of in 250 ms. If it already ended (the
        # e-stop, say), releasing fails harmlessly.
        try:
            call("DELETE", f"/api/v0/parts/{PART}/lease?lease={lease}")
        except (FaberError, urllib.error.URLError):
            pass
    print(f"Done; {PART} is stopped.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
