// What the SENNEP window shows, as plain functions of plain numbers: where a scan's returns are,
// where things sit on the top view, what the keys and the pad ask for, and how the proximity
// ring is cut. No DOM here, so the SDK's tests check it (sdk/js/shell/src/sennep-view.test.js).
//
// The machine frame (docs/capabilities.md): x forward, y left, metres; angles counter-clockwise
// from ahead. On screen, forward is up.

/** SENNEP V2: 800 × 650 mm. The app's state can say otherwise (`length`, `width`). */
export const SENNEP = { length: 0.8, width: 0.65 };

/** A part with no mount sits at the origin. */
const ORIGIN = { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 };

/** Each return of a laser-scan@1 reading, in the machine frame (the LiDAR sits at `mount`). */
export function scanPoints(scan, mount = ORIGIN) {
  const out = [];
  const n = scan.ranges.length;
  for (let i = 0; i < n; i++) {
    const r = scan.ranges[i];
    if (typeof r !== "number" || !Number.isFinite(r) || r <= 0) continue;
    const angle = scan.angles
      ? scan.angles[i]
      : scan.angleMin + i * scan.angleIncrement;
    const a = mount.yaw + angle;
    out.push({ x: mount.x + r * Math.cos(a), y: mount.y + r * Math.sin(a) });
  }
  return out;
}

/**
 * The top view: `size` pixels square, `range` metres from the middle to an edge. Forward is up
 * and left is left, so a point at (x, y) is drawn at (cx − y·k, cy − x·k).
 */
export function topView(size, range) {
  const k = size / 2 / range;
  return {
    scale: k,
    toScreen: (p) => ({ sx: size / 2 - p.y * k, sy: size / 2 - p.x * k }),
  };
}

/** The footprint grown by the stop distance: where nothing may come while driving. */
export function stopZone(footprint, stop) {
  return {
    halfLength: footprint.length / 2 + stop,
    halfWidth: footprint.width / 2 + stop,
    radius: stop,
  };
}

export const STILL = { x: 0, y: 0, turn: 0 };

/** Keys by where they sit on the keyboard (`code`), as in Faber's Drive window. */
export function driveKey(code) {
  switch (code) {
    case "KeyW":
    case "ArrowUp":
      return "forward";
    case "KeyS":
    case "ArrowDown":
      return "back";
    case "KeyA":
    case "ArrowLeft":
      return "left";
    case "KeyD":
    case "ArrowRight":
      return "right";
    case "KeyQ":
      return "turnLeft";
    case "KeyE":
      return "turnRight";
    case "Space":
      return "stop";
  }
  return null;
}

/** The keys held now as a stick (each axis −1..1). A diagonal is no faster than straight on. */
export function keysToStick(held) {
  const axis = (plus, minus) =>
    (held.has(plus) ? 1 : 0) - (held.has(minus) ? 1 : 0);
  let x = axis("forward", "back");
  let y = axis("left", "right");
  const len = Math.hypot(x, y);
  if (len > 1) {
    x /= len;
    y /= len;
  }
  return { x, y, turn: axis("turnLeft", "turnRight") };
}

/** Below this, a finger on the pad is resting, not driving. */
const DEAD_ZONE = 0.06;

/** A point on the pad from its middle, as a share of its radius (screen: x right, y down). */
export function padToStick(px, py) {
  let x = -py;
  let y = -px;
  const len = Math.hypot(x, y);
  if (len < DEAD_ZONE) return STILL;
  if (len > 1) {
    x /= len;
    y /= len;
  }
  return { x: round(x), y: round(y), turn: 0 };
}

/** Several controls at once: added, each axis at most full. */
export function combine(...sticks) {
  const sum = (k) =>
    Math.max(
      -1,
      Math.min(
        1,
        sticks.reduce((a, s) => a + s[k], 0),
      ),
    );
  return { x: sum("x"), y: sum("y"), turn: sum("turn") };
}

export function isStill(s) {
  return s.x === 0 && s.y === 0 && s.turn === 0;
}

/** A `drive` message to the app's logic: the stick, `"<x> <y> <turn>"` (see src/lib.rs). */
export function driveValue(s) {
  return `${round(s.x)} ${round(s.y)} ${round(s.turn)}`;
}

function round(v) {
  const r = Math.round(v * 1000) / 1000;
  return r === 0 ? 0 : r;
}

/**
 * One ring segment per proximity sensor, centred on where it looks (its mount's yaw), `width`
 * radians wide at most, never overlapping its neighbours. Angles are screen angles: 0 is up
 * (forward), counter-clockwise positive, like the machine's.
 */
export function ringSegments(mounts, width = Math.PI / 3) {
  const yaws = mounts.map((m) => normalise(m.yaw));
  return yaws.map((yaw, i) => {
    let half = width / 2;
    for (let j = 0; j < yaws.length; j++) {
      if (j === i) continue;
      const gap = Math.abs(normalise(yaws[j] - yaw));
      half = Math.min(half, gap / 2 - 0.02);
    }
    half = Math.max(half, 0.05);
    return { index: i, yaw, from: yaw - half, to: yaw + half };
  });
}

function normalise(a) {
  let v = a % (2 * Math.PI);
  if (v > Math.PI) v -= 2 * Math.PI;
  if (v < -Math.PI) v += 2 * Math.PI;
  return v;
}

/** How near a proximity reading is: `clear`, `near` (within twice the stop distance) or `stop`. */
export function nearness(reading, stop) {
  if (!reading || (reading.status !== "ok" && reading.status !== "too-close"))
    return "clear";
  if (reading.meters <= stop) return "stop";
  return reading.meters <= 2 * stop ? "near" : "clear";
}

/** A pose@1 reading as x, y (m) and yaw (degrees). */
export function poseOf(reading) {
  const q = reading.orientation;
  const yaw = Math.atan2(
    2 * (q.w * q.z + q.x * q.y),
    1 - 2 * (q.y * q.y + q.z * q.z),
  );
  return {
    x: reading.position.x,
    y: reading.position.y,
    yawDeg: (yaw * 180) / Math.PI,
  };
}

/** The `limits` the logic writes into the state (JSON), or what it means when there are none. */
export function readLimits(text) {
  const none = {
    driving: false,
    cap: 1,
    message: "",
    blocked: {
      forward: false,
      back: false,
      left: false,
      right: false,
      turn: false,
    },
  };
  if (!text) return none;
  try {
    const v = JSON.parse(text);
    return { ...none, ...v, blocked: { ...none.blocked, ...v.blocked } };
  } catch {
    return none;
  }
}

/** A velocity in words: "0.30 m/s ahead", "0.10 m/s to the left, turning", "Standing still". */
export function velocityText(t) {
  const speed = Math.hypot(t.vx, t.vy);
  const turning = Math.abs(t.omega) > 0.01;
  if (speed < 0.005 && !turning) return "Standing still";
  if (speed < 0.005) return t.omega > 0 ? "Turning left" : "Turning right";
  const ways = [];
  if (Math.abs(t.vx) >= 0.005) ways.push(t.vx > 0 ? "ahead" : "back");
  if (Math.abs(t.vy) >= 0.005)
    ways.push(t.vy > 0 ? "to the left" : "to the right");
  return `${speed.toFixed(2)} m/s ${ways.join(" and ")}${turning ? ", turning" : ""}`;
}
