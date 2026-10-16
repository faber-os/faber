// The wall follower's window. Its logic (src/lib.rs) runs in the background whether the window is
// open or not; the window only shows what the logic does, through the @faber-os/shell SDK.
import { applyTheme, faber } from "./faber-shell.js";

const state = document.getElementById("state");
const gap = document.getElementById("gap");
const bar = document.getElementById("bar");
const problem = document.getElementById("problem");

/** The bar shows 0 to 1 m; the target, 0.3 m, is marked on it. */
const BAR_M = 1;

// Look like the rest of Faber, light or dark, and follow it when it changes.
faber.theme().then(applyTheme, showProblem);
faber.onTheme(applyTheme);

// On or off, and how it runs. The wall distance only exists while the logic runs, so the
// window follows it again each time the app starts.
let running = false;
function showStatus(status) {
  running = status.state === "running";
  if (running) follow();
  state.dataset.on = String(status.enabled);
  state.textContent = !status.enabled
    ? "Off"
    : status.state === "running"
      ? "On, driving along the wall"
      : `On, but ${status.state}`;
  state.title = status.message ?? "";
}
faber.status().then(showStatus, showProblem);
faber.onStatus(showStatus);

// The app's own derived signal: app:wall-follower/wall-distance (a distance@1 reading).
function showGap(r) {
  const d = r.reading;
  if (d.status === "ok") {
    gap.textContent = `${d.meters.toFixed(2)} m`;
    bar.style.width = `${Math.min(100, (d.meters / BAR_M) * 100)}%`;
  } else {
    gap.textContent = "No wall in sight";
    bar.style.width = "0";
  }
}
let following = false;
function follow() {
  if (following) return;
  following = true;
  // Not there yet (the logic only just started) is fine: the subscription brings the first one.
  faber.published("wall-distance").then(
    (r) => r && showGap(r),
    () => {},
  );
  faber.subscribePublished("wall-distance", showGap, {
    maxHz: 5,
    // It ends when the app stops (turned off, or its part went away): follow again once it runs.
    onError: () => {
      following = false;
      if (running) setTimeout(follow, 1000);
    },
  });
}

// The speed slider. The speed is the app's saved state ("speed"), shared with its logic, which
// reads it on every tick: moving the slider changes how fast the base drives at once.
const speed = document.getElementById("speed");
const speedValue = document.getElementById("speed-value");
const DEFAULT_SPEED = 0.25;
const MAX_SPEED = 0.5; // the logic never drives faster (src/lib.rs, MAX_SPEED)

function speedOf(text) {
  const v = Number(text);
  return text !== null && Number.isFinite(v) && v >= 0 && v <= MAX_SPEED ? v : DEFAULT_SPEED;
}
function showSpeed(v) {
  speedValue.textContent = `${v.toFixed(2)} m/s`;
}
function setSlider(v) {
  speed.value = String(v);
  showSpeed(v);
}
faber.state.get("speed").then((v) => setSlider(speedOf(v)), showProblem);
// Another window (or the logic) changed it: follow, unless the user is sliding right now.
let sliding = false;
faber.state.onChange((key, value) => {
  if (key === "speed" && !sliding) setSlider(speedOf(value));
});

// Save while sliding, at most every 100 ms, and once more where it stops.
let saveTimer = null;
function save() {
  saveTimer = null;
  faber.state.set("speed", speed.value).catch(showProblem);
}
speed.addEventListener("input", () => {
  sliding = true;
  showSpeed(Number(speed.value));
  saveTimer ??= setTimeout(save, 100);
});
speed.addEventListener("change", () => {
  sliding = false;
  if (saveTimer !== null) clearTimeout(saveTimer);
  save();
});

function showProblem(e) {
  problem.hidden = false;
  problem.textContent = e.hint ? `${e.message} ${e.hint}` : e.message;
}
