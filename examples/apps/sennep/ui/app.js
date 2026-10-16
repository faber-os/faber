// SENNEP's window. It drives SENNEP by hand and shows what it sees, through the @faber-os/shell
// SDK. Its text is plain English: an app's own words aren't translated by Faber's catalogue.
//
// Driving goes through the app's logic (src/lib.rs), never around it: the window saves what the
// driver asks for in the app's state (`drive`), at least every 100 ms while a control is held, and
// the logic drives the base through its safety layer. If this page closes or hangs, the request
// stops changing and the logic stops the base within 300 ms.
import { applyTheme, faber } from "./faber-shell.js";
import {
  SENNEP,
  STILL,
  combine,
  driveKey,
  driveValue,
  isStill,
  keysToStick,
  nearness,
  padToStick,
  poseOf,
  readLimits,
  ringSegments,
  scanPoints,
  stopZone,
  topView,
  velocityText,
} from "./view.js";

const $ = (id) => document.getElementById(id);
const canvas = $("scan");
const message = $("message");
const problem = $("problem");

/** What the window knows, redrawn from whenever something changes. */
const seen = {
  roles: [],
  running: false,
  scan: null,
  scanAt: 0,
  proximity: [],
  limits: readLimits(null),
  footprint: { ...SENNEP },
  stop: 0.35,
  range: 3,
};

// ---- the look --------------------------------------------------------------------------------

faber.theme().then((t) => {
  applyTheme(t);
  draw();
}, showProblem);
faber.onTheme((t) => {
  applyTheme(t);
  draw();
});

function token(name, fallback) {
  return (
    getComputedStyle(document.documentElement).getPropertyValue(name).trim() ||
    fallback
  );
}

// ---- the app's state: settings and what the logic says ---------------------------------------

function num(text, fallback, lo, hi) {
  const v = Number(text);
  return text !== null && text !== undefined && Number.isFinite(v)
    ? Math.min(hi, Math.max(lo, v))
    : fallback;
}

const settings = {
  "max-speed": (v) =>
    setSlider("speed", num(v, 0.3, 0.05, 0.6), (x) => `${x.toFixed(2)} m/s`),
  "stop-distance": (v) => {
    seen.stop = num(v, 0.35, 0.1, 1.5);
    setSlider("stop-distance", seen.stop, (x) => `${x.toFixed(2)} m`);
    draw();
  },
  length: (v) => {
    seen.footprint.length = num(v, SENNEP.length, 0.1, 3);
    draw();
  },
  width: (v) => {
    seen.footprint.width = num(v, SENNEP.width, 0.1, 3);
    draw();
  },
  limits: (v) => {
    seen.limits = readLimits(v);
    showLimits();
    draw();
  },
};

const sliding = new Set();
function setSlider(id, v, text) {
  if (!sliding.has(id)) $(id).value = String(v);
  $(id === "speed" ? "speed-value" : "stop-value").textContent = text(v);
}

for (const key of Object.keys(settings)) {
  faber.state.get(key).then((v) => settings[key](v), showProblem);
}
faber.state.onChange((key, value) => settings[key]?.(value));

/** A slider saves its key while it moves, at most every 100 ms, and once more where it stops. */
function slider(id, key, text) {
  const input = $(id);
  let timer = null;
  const save = () => {
    timer = null;
    faber.state.set(key, input.value).catch(showProblem);
  };
  input.addEventListener("input", () => {
    sliding.add(id);
    $(id === "speed" ? "speed-value" : "stop-value").textContent = text(
      Number(input.value),
    );
    timer ??= setTimeout(save, 100);
  });
  input.addEventListener("change", () => {
    sliding.delete(id);
    if (timer !== null) clearTimeout(timer);
    save();
  });
}
slider("speed", "max-speed", (x) => `${x.toFixed(2)} m/s`);
slider("stop-distance", "stop-distance", (x) => `${x.toFixed(2)} m`);

function showLimits() {
  const l = seen.limits;
  let text = l.message;
  if (!text && !seen.running) text = "Turn SENNEP on in Apps to drive it.";
  if (!text)
    text = l.driving
      ? "Driving."
      : "Ready. Hold a key or drag the joystick to drive.";
  message.textContent = text;
  message.dataset.kind = /^(Stopped|Can't|Not turning)/.test(l.message)
    ? "stop"
    : l.message
      ? "warn"
      : "";
  for (const edge of document.querySelectorAll(".edge")) {
    edge.dataset.blocked = String(Boolean(l.blocked[edge.dataset.way]));
  }
}

// ---- on or off -------------------------------------------------------------------------------

function showStatus(status) {
  const was = seen.running;
  seen.running = status.enabled && status.state === "running";
  const state = $("state");
  state.dataset.on = String(status.enabled);
  state.textContent = !status.enabled
    ? "Off"
    : seen.running
      ? "On"
      : `On, but ${status.state}`;
  state.title = status.message ?? "";
  for (const id of ["turn-left", "turn-right", "reset-pose"])
    $(id).disabled = !seen.running;
  $("pad").setAttribute("aria-disabled", String(!seen.running));
  if (!seen.running) letGo();
  if (seen.running && !was) faber.roles().then(follow, showProblem);
  showLimits();
}
faber.status().then(showStatus, showProblem);
faber.onStatus(showStatus);

// ---- what it reads ---------------------------------------------------------------------------

/** Every subscription ends when the app stops; they start again once it runs. */
let following = false;
let generation = 0;
function follow(roles) {
  seen.roles = roles;
  if (following) return;
  following = true;
  const mine = ++generation;
  // The first of this round's subscriptions to end starts the next round (once it runs again).
  const ended = () => {
    if (mine !== generation || !following) return;
    following = false;
    if (seen.running)
      setTimeout(() => faber.roles().then(follow, () => {}), 1000);
  };
  const role = (name) => roles.find((r) => r.role === name);
  const lidarMount = role("lidar")?.mounts?.[0];

  faber.subscribe(
    "lidar",
    (r) => {
      seen.scan = scanPoints(r.reading, lidarMount);
      seen.scanAt = Date.now();
      $("scan-note").textContent = `${seen.scan.length} returns`;
      draw();
    },
    { maxHz: 10, onError: ended },
  );
  faber.subscribe("base", (r) => showVelocity(r.reading.commanded), {
    maxHz: 10,
    onError: ended,
  });
  faber.subscribePublished("pose", (r) => showPose(r.reading), {
    maxHz: 10,
    onError: ended,
  });
  faber.subscribePublished("nearest-obstacle", (r) => showNearest(r.reading), {
    maxHz: 5,
    onError: ended,
  });

  const proximity = role("proximity");
  const n = proximity?.parts ?? 0;
  seen.proximity = Array.from({ length: n }, (_, i) => ({
    mount: proximity.mounts[i],
    reading: null,
  }));
  drawRing();
  for (let i = 0; i < n; i++) {
    faber.subscribe(
      "proximity",
      (r) => {
        seen.proximity[i].reading = r.reading;
        drawRing();
        draw();
      },
      { index: i, maxHz: 5, onError: ended },
    );
  }
  if ((role("battery")?.parts ?? 0) > 0) {
    faber.subscribe("battery", (r) => showBattery(r.reading), {
      maxHz: 2,
      onError: ended,
    });
  }
}

function showVelocity(t) {
  $("velocity").textContent = velocityText(t);
}

function showNearest(d) {
  $("nearest").textContent =
    d.status === "ok"
      ? `Nearest in the way: ${d.meters.toFixed(2)} m`
      : "Nothing in the way";
}

function showPose(p) {
  const { x, y, yawDeg } = poseOf(p);
  $("pose").textContent =
    `x ${x.toFixed(2)} m, y ${y.toFixed(2)} m, ${yawDeg.toFixed(0)}°`;
}

function showBattery(b) {
  const bar = $("battery-bar");
  bar.dataset.level = b.level;
  bar.style.width =
    b.charge !== undefined ? `${Math.round(b.charge * 100)}%` : "100%";
  const level =
    { ok: "OK", low: "Low", critical: "Critical" }[b.level] ?? b.level;
  const charge =
    b.charge !== undefined ? `, ${Math.round(b.charge * 100)} %` : "";
  $("battery").textContent = `${b.volts.toFixed(2)} V, ${level}${charge}`;
}

$("reset-pose").addEventListener("click", () => {
  faber.state.set("reset-pose", String(Date.now())).catch(showProblem);
});

// ---- the top view ----------------------------------------------------------------------------

function fitCanvas() {
  const r = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(r.width * dpr));
  canvas.height = Math.max(1, Math.round(r.height * dpr));
  draw();
}
new ResizeObserver(fitCanvas).observe(canvas);

function draw() {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const w = canvas.width;
  const h = canvas.height;
  const size = Math.min(w, h);
  const view = topView(size, seen.range);
  const k = view.scale;
  const ox = (w - size) / 2;
  const oy = (h - size) / 2;
  const at = (p) => {
    const s = view.toScreen(p);
    return [ox + s.sx, oy + s.sy];
  };
  ctx.clearRect(0, 0, w, h);
  const dpr = window.devicePixelRatio || 1;

  // Rings every metre.
  ctx.strokeStyle = token("--line", "#ccc");
  ctx.fillStyle = token("--text-faint", "#999");
  ctx.lineWidth = dpr;
  ctx.font = `${11 * dpr}px ${token("--font-ui", "sans-serif")}`;
  const [cx, cy] = at({ x: 0, y: 0 });
  for (let m = 1; m <= seen.range; m++) {
    ctx.beginPath();
    ctx.arc(cx, cy, m * k, 0, 2 * Math.PI);
    ctx.stroke();
    ctx.fillText(`${m} m`, cx + 3 * dpr, cy - m * k - 3 * dpr);
  }

  // The stop zone: the footprint grown by the stop distance. Red where it blocks.
  const f = seen.footprint;
  const zone = stopZone(f, seen.stop);
  const blocked = seen.limits.blocked;
  ctx.setLineDash([6 * dpr, 4 * dpr]);
  ctx.strokeStyle = token("--warn", "#a60");
  ctx.lineWidth = 1.5 * dpr;
  roundRect(
    ctx,
    at({ x: zone.halfLength, y: zone.halfWidth }),
    2 * zone.halfWidth * k,
    2 * zone.halfLength * k,
    zone.radius * k,
  );
  ctx.stroke();
  ctx.setLineDash([]);
  ctx.strokeStyle = token("--danger", "#b00");
  ctx.lineWidth = 4 * dpr;
  const side = (from, to) => {
    ctx.beginPath();
    ctx.moveTo(...at(from));
    ctx.lineTo(...at(to));
    ctx.stroke();
  };
  const hl = f.length / 2;
  const hw = f.width / 2;
  if (blocked.forward)
    side({ x: zone.halfLength, y: hw }, { x: zone.halfLength, y: -hw });
  if (blocked.back)
    side({ x: -zone.halfLength, y: hw }, { x: -zone.halfLength, y: -hw });
  if (blocked.left)
    side({ x: hl, y: zone.halfWidth }, { x: -hl, y: zone.halfWidth });
  if (blocked.right)
    side({ x: hl, y: -zone.halfWidth }, { x: -hl, y: -zone.halfWidth });

  // SENNEP itself, its front marked.
  ctx.fillStyle = token("--surface-3", "#ddd");
  ctx.strokeStyle = token("--text-muted", "#666");
  ctx.lineWidth = 1.5 * dpr;
  const [fx, fy] = at({ x: hl, y: hw });
  ctx.fillRect(fx, fy, f.width * k, f.length * k);
  ctx.strokeRect(fx, fy, f.width * k, f.length * k);
  ctx.fillStyle = token("--text-muted", "#666");
  ctx.beginPath();
  ctx.moveTo(...at({ x: hl - 0.02, y: 0 }));
  ctx.lineTo(...at({ x: hl - 0.14, y: 0.08 }));
  ctx.lineTo(...at({ x: hl - 0.14, y: -0.08 }));
  ctx.fill();

  // The proximity sensors' readings, as short rays.
  ctx.strokeStyle = token("--info", "#25a");
  ctx.lineWidth = 2 * dpr;
  for (const p of seen.proximity) {
    const d = p.reading;
    if (!p.mount || !d || (d.status !== "ok" && d.status !== "too-close"))
      continue;
    const m = p.mount;
    side(
      { x: m.x, y: m.y },
      {
        x: m.x + d.meters * Math.cos(m.yaw),
        y: m.y + d.meters * Math.sin(m.yaw),
      },
    );
  }

  // The LiDAR's returns. Faded once the scan is old.
  if (seen.scan) {
    ctx.fillStyle = token("--accent", "#0a6");
    ctx.globalAlpha = Date.now() - seen.scanAt > 1000 ? 0.35 : 1;
    const dot = 2.5 * dpr;
    for (const p of seen.scan) {
      const [x, y] = at(p);
      if (x >= 0 && y >= 0 && x <= w && y <= h)
        ctx.fillRect(x - dot / 2, y - dot / 2, dot, dot);
    }
    ctx.globalAlpha = 1;
  }
}

/** A rectangle with round corners whose top-left (on screen) is `[x, y]`. */
function roundRect(ctx, [x, y], w, h, r) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}

const RANGES = [1.5, 3, 6, 12];
function zoom(by) {
  const i = Math.max(
    0,
    Math.min(RANGES.length - 1, RANGES.indexOf(seen.range) + by),
  );
  seen.range = RANGES[i];
  $("zoom-value").textContent = `${seen.range} m`;
  draw();
}
$("zoom-in").addEventListener("click", () => zoom(-1));
$("zoom-out").addEventListener("click", () => zoom(1));

// ---- the proximity ring ----------------------------------------------------------------------

const SVG = "http://www.w3.org/2000/svg";
function drawRing() {
  const ring = $("ring");
  ring.replaceChildren();
  const body = document.createElementNS(SVG, "rect");
  body.setAttribute("class", "body");
  const bw = 34;
  const bl = bw * (seen.footprint.length / seen.footprint.width);
  body.setAttribute("x", String(-bw / 2));
  body.setAttribute("y", String(-bl / 2));
  body.setAttribute("width", String(bw));
  body.setAttribute("height", String(bl));
  body.setAttribute("rx", "4");
  ring.append(body);
  const n = seen.proximity.length;
  $("ring-note").textContent =
    n === 0
      ? "No proximity sensors picked"
      : `${n} sensor${n === 1 ? "" : "s"}, by where each looks`;
  const segments = ringSegments(
    seen.proximity.map((p) => p.mount ?? { yaw: 0 }),
  );
  const R = 48;
  // Screen angle: 0 up (forward), counter-clockwise positive, so x = −sin, y = −cos.
  const pt = (a) => [-R * Math.sin(a), -R * Math.cos(a)];
  for (const s of segments) {
    const p = seen.proximity[s.index];
    const [x1, y1] = pt(s.from);
    const [x2, y2] = pt(s.to);
    const arc = document.createElementNS(SVG, "path");
    arc.setAttribute("class", "segment");
    // From `from` to `to` is counter-clockwise, which on screen is sweep-flag 0.
    arc.setAttribute(
      "d",
      `M ${x1.toFixed(2)} ${y1.toFixed(2)} A ${R} ${R} 0 0 0 ${x2.toFixed(2)} ${y2.toFixed(2)}`,
    );
    arc.dataset.near = nearness(p.reading, seen.stop);
    arc.dataset.seen = String(p.reading !== null);
    arc.dataset.testid = `proximity-${s.index}`;
    const title = document.createElementNS(SVG, "title");
    const d = p.reading;
    title.textContent =
      d && (d.status === "ok" || d.status === "too-close")
        ? `Sensor ${s.index + 1}: ${d.meters.toFixed(2)} m`
        : `Sensor ${s.index + 1}: nothing in range`;
    arc.append(title);
    ring.append(arc);
  }
}
drawRing();

// ---- driving ---------------------------------------------------------------------------------

const held = new Set();
let pad = STILL;
let turn = 0;
let seq = Date.now();
let timer = null;

function stick() {
  return combine(keysToStick(held), pad, { x: 0, y: 0, turn });
}

function send(s) {
  seq += 1;
  faber.state.set("drive", driveValue(seq, s)).catch(showProblem);
}

/** Sends the request now, and every 100 ms while it asks for something (the logic's dead man). */
function update() {
  const s = seen.running ? stick() : STILL;
  if (isStill(s)) {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
      send(STILL);
    }
    return;
  }
  send(s);
  timer ??= setInterval(() => send(stick()), 100);
}

/** Lets go of every control: the logic stops the base. */
function letGo() {
  held.clear();
  pad = STILL;
  turn = 0;
  setKnob(0, 0);
  for (const id of ["turn-left", "turn-right"])
    $(id).setAttribute("aria-pressed", "false");
  if (timer !== null) {
    clearInterval(timer);
    timer = null;
  }
  if (seen.running) send(STILL);
}

$("stop").addEventListener("click", letGo);

/** Keys that belong to the element itself (a slider's arrows, a button's Space). */
function ownsKeys(target) {
  return (
    target instanceof HTMLElement &&
    (target.matches("input, button") || target.isContentEditable)
  );
}

window.addEventListener("keydown", (e) => {
  const k = driveKey(e.code);
  if (!k || e.ctrlKey || e.metaKey || e.altKey || ownsKeys(e.target)) return;
  e.preventDefault();
  if (k === "stop") return letGo();
  if (held.has(k)) return;
  held.add(k);
  update();
});
window.addEventListener("keyup", (e) => {
  const k = driveKey(e.code);
  if (!k || !held.delete(k)) return;
  update();
});
// Keys held while the window loses focus never come up here: let go of everything.
window.addEventListener("blur", () => {
  if (held.size > 0 || !isStill(pad) || turn !== 0) letGo();
});

// The joystick: drag from the middle, let go to stop. Mouse, pen and touch.
const padEl = $("pad");
let pointer = null;
function setKnob(x, y) {
  $("knob").style.left = `${50 + x * 50}%`;
  $("knob").style.top = `${50 + y * 50}%`;
}
function padAt(e) {
  const r = padEl.getBoundingClientRect();
  const radius = Math.min(r.width, r.height) / 2 || 1;
  let x = (e.clientX - (r.left + r.width / 2)) / radius;
  let y = (e.clientY - (r.top + r.height / 2)) / radius;
  const len = Math.hypot(x, y);
  if (len > 1) {
    x /= len;
    y /= len;
  }
  setKnob(x, y);
  pad = padToStick(x, y);
  update();
}
padEl.addEventListener("pointerdown", (e) => {
  if (!seen.running || pointer !== null) return;
  pointer = e.pointerId;
  padEl.setPointerCapture?.(e.pointerId);
  e.preventDefault();
  padAt(e);
});
padEl.addEventListener("pointermove", (e) => {
  if (e.pointerId === pointer) padAt(e);
});
for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) {
  padEl.addEventListener(type, (e) => {
    if (e.pointerId !== pointer) return;
    pointer = null;
    setKnob(0, 0);
    pad = STILL;
    update();
  });
}

// Turning by touch: hold a button.
for (const [id, way] of [
  ["turn-left", 1],
  ["turn-right", -1],
]) {
  const b = $(id);
  b.addEventListener("pointerdown", (e) => {
    if (!seen.running) return;
    b.setPointerCapture?.(e.pointerId);
    b.setAttribute("aria-pressed", "true");
    turn = way;
    update();
  });
  for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) {
    b.addEventListener(type, () => {
      if (b.getAttribute("aria-pressed") !== "true") return;
      b.setAttribute("aria-pressed", "false");
      turn = 0;
      update();
    });
  }
}

function showProblem(e) {
  problem.hidden = false;
  problem.textContent = e.hint ? `${e.message} ${e.hint}` : e.message;
}
