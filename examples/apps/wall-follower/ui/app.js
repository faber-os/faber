// The wall follower's window. Its logic (src/lib.rs) runs in the background whether the window is
// open or not; the window only shows what the logic does, through the @faber-os/shell SDK.
import { applyTheme, faber } from './faber-shell.js';

const state = document.getElementById('state');
const gap = document.getElementById('gap');
const bar = document.getElementById('bar');
const problem = document.getElementById('problem');

/** The bar shows 0 to 1 m; the target, 0.3 m, is marked on it. */
const BAR_M = 1;

// Look like the rest of Faber, light or dark, and follow it when it changes.
faber.theme().then(applyTheme, showProblem);
faber.onTheme(applyTheme);

// On or off, and how it runs.
function showStatus(status) {
  state.dataset.on = String(status.enabled);
  state.textContent = !status.enabled
    ? 'Off'
    : status.state === 'running'
      ? 'On, driving along the wall'
      : `On, but ${status.state}`;
  state.title = status.message ?? '';
}
faber.status().then(showStatus, showProblem);
faber.onStatus(showStatus);

// The app's own derived signal: app:wall-follower/wall-distance (a distance@1 reading).
function showGap(r) {
  const d = r.reading;
  if (d.status === 'ok') {
    gap.textContent = `${d.meters.toFixed(2)} m`;
    bar.style.width = `${Math.min(100, (d.meters / BAR_M) * 100)}%`;
  } else {
    gap.textContent = 'No wall in sight';
    bar.style.width = '0';
  }
}
faber.published('wall-distance').then((r) => r && showGap(r), showProblem);
faber.subscribePublished('wall-distance', showGap, { maxHz: 5, onError: showProblem });

function showProblem(e) {
  problem.hidden = false;
  problem.textContent = e.hint ? `${e.message} ${e.hint}` : e.message;
}
