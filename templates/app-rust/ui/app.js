// My app's window. The logic (src/lib.rs) runs whether this window is open or not; the window
// shows what it publishes, through the @faber-os/shell SDK in faber-shell.js. The page runs in a
// sandbox: it can load only its own files, and reaches Faber only through the SDK.
import { applyTheme, faber } from "./faber-shell.js";

const nearest = document.getElementById("nearest");
const problem = document.getElementById("problem");

// Look like the rest of Faber, light or dark, and follow it when it changes.
faber.theme().then(applyTheme, showProblem);
faber.onTheme(applyTheme);

// The app's own derived signal, app:my-app/nearest (a distance@1 reading), as it changes.
function show(r) {
  const d = r.reading;
  nearest.textContent = d.status === "ok" ? `${d.meters.toFixed(2)} m` : "Nothing in sight";
}
faber.published("nearest").then((r) => r && show(r), showProblem);
faber.subscribePublished("nearest", show, { maxHz: 5, onError: showProblem });

// Faber's errors say what happened and what to try next.
function showProblem(e) {
  problem.hidden = false;
  problem.textContent = e.hint ? `${e.message} ${e.hint}` : e.message;
}
