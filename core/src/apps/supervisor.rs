//! Runs one app's logic on its own thread, ticked at its `tick-hz`, and supervises it like a
//! part (docs/architecture.md §8): a crash (a trap, a limit, a broken rule) ends the run, its
//! leases end, and it restarts with growing backoff, until it has crashed too often in a row;
//! then it stays stopped.

use super::logic::Logic;
use super::manifest::PublishSpec;
use crate::clock::SharedClock;
use crate::names::SignalAddress;
use crate::router::Router;
use faber_connector::Problem;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// An app's place in its lifecycle: the same states as a part's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppStatus {
    /// It can't run as set up (a role has no part). Needs a change by the user.
    Invalid(Problem),
    /// A part filling one of its roles is missing or can't fill it any more.
    Conflict(Problem),
    Starting,
    Running,
    /// It crashed; Faber restarts it.
    Failed(Problem),
    /// It is off, or Faber gave up after repeated crashes.
    Stopped(Problem),
}

/// Every app's status, plus a change counter the API watches.
pub struct StatusBoard {
    statuses: Mutex<BTreeMap<String, AppStatus>>,
    changed: watch::Sender<u64>,
}

impl Default for StatusBoard {
    fn default() -> Self {
        StatusBoard { statuses: Mutex::new(BTreeMap::new()), changed: watch::channel(0).0 }
    }
}

impl StatusBoard {
    pub fn set(&self, app: &str, status: AppStatus) {
        let mut map = self.statuses.lock().expect("app status lock");
        if map.get(app) != Some(&status) {
            map.insert(app.to_string(), status);
            drop(map);
            self.changed.send_modify(|n| *n += 1);
        }
    }

    /// Says "something changed" without a status changing (an app's parts or switch).
    pub fn touch(&self) {
        self.changed.send_modify(|n| *n += 1);
    }

    pub fn remove(&self, app: &str) {
        if self.statuses.lock().expect("app status lock").remove(app).is_some() {
            self.changed.send_modify(|n| *n += 1);
        }
    }

    pub fn get(&self, app: &str) -> Option<AppStatus> {
        self.statuses.lock().expect("app status lock").get(app).cloned()
    }

    /// Fires whenever any app's status changes.
    pub fn watch(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
}

/// Timing of the supervisor, as for parts. Tests shrink these.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// First wait after a crash; doubles each time.
    pub backoff_start: Duration,
    /// Crashes in a row before the app is stopped.
    pub max_crashes: u32,
    /// A run longer than this resets the crash count.
    pub healthy_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            backoff_start: Duration::from_secs(1),
            max_crashes: 5,
            healthy_after: Duration::from_secs(30),
        }
    }
}

/// Makes a fresh run of the logic (a new instance and session); fails with the problem to show.
pub type MakeLogic = Box<dyn Fn() -> Result<Box<dyn Logic>, Problem> + Send>;

/// What one app runs.
pub struct RunSpec {
    pub id: String,
    pub tick_hz: u32,
    /// Its derived signals, declared while it runs.
    pub publishes: Vec<PublishSpec>,
    pub make: MakeLogic,
}

/// What every app shares.
#[derive(Clone)]
pub struct RunDeps {
    pub clock: SharedClock,
    pub router: Arc<Router>,
    pub status: Arc<StatusBoard>,
    pub timing: Timing,
}

/// A running app. Stopping it (or dropping it) calls its `stop`, ends its leases, saves its
/// state, removes its derived signals and ends its thread.
pub struct AppHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AppHandle {
    pub fn stop(mut self) {
        self.shutdown();
    }

    /// Asks the app to stop without waiting for it.
    pub fn signal_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            // Dropped on the app's own thread (it held the machine's last reference): it sees the
            // stop flag and ends by itself once this returns; waiting for it here never would.
            if t.thread().id() != std::thread::current().id() {
                let _ = t.join();
            }
        }
    }
}

impl Drop for AppHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Starts an app's thread, `app:<id>`.
pub fn start(spec: RunSpec, deps: RunDeps) -> AppHandle {
    // Before the thread exists, so nobody sees an earlier run's Running for this one.
    deps.status.set(&spec.id, AppStatus::Starting);
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name(format!("app:{}", spec.id))
        .spawn(move || supervise(spec, deps, thread_stop))
        .expect("spawn app thread");
    AppHandle { stop, thread: Some(thread) }
}

/// Waits for `d`, waking early when stopped. Returns false if stopped.
fn wait(d: Duration, stop: &AtomicBool) -> bool {
    let until = Instant::now() + d;
    while Instant::now() < until {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep((until - Instant::now()).min(Duration::from_millis(5)));
    }
    !stop.load(Ordering::Relaxed)
}

enum End {
    Stopped,
    Crashed(Problem),
}

fn supervise(spec: RunSpec, deps: RunDeps, stop: Arc<AtomicBool>) {
    let addresses: Vec<SignalAddress> =
        spec.publishes.iter().map(|p| SignalAddress::derived(&spec.id, p.name.clone())).collect();
    for (addr, p) in addresses.iter().zip(&spec.publishes) {
        deps.router.declare(addr.clone(), p.capability);
    }
    let mut crashes = 0u32;
    while !stop.load(Ordering::Relaxed) {
        deps.status.set(&spec.id, AppStatus::Starting);
        let started = Instant::now();
        let end = match (spec.make)() {
            Err(p) => End::Crashed(p),
            Ok(mut logic) => run_once(logic.as_mut(), &spec, &deps, &stop),
        };
        if started.elapsed() >= deps.timing.healthy_after {
            crashes = 0;
        }
        match end {
            End::Stopped => break,
            End::Crashed(p) => {
                crashes += 1;
                tracing::error!(target: "core::apps", app = spec.id.as_str(), "{} (crash {crashes})", p.message);
                if crashes >= deps.timing.max_crashes {
                    deps.status.set(
                        &spec.id,
                        AppStatus::Stopped(Problem::new(
                            format!(
                                "The app crashed {crashes} times in a row, so Faber stopped it. Last: {}",
                                p.message
                            ),
                            "Check the log for the cause. Turn the app off and on again to retry.",
                        )),
                    );
                    break;
                }
                deps.status.set(&spec.id, AppStatus::Failed(p));
                if !wait(deps.timing.backoff_start * 2u32.pow(crashes - 1), &stop) {
                    break;
                }
            }
        }
    }
    for addr in &addresses {
        deps.router.remove(addr);
    }
}

/// One run: start, tick until stopped or crashed, then stop. Its leases end and its state is
/// saved however the run ends.
fn run_once(logic: &mut dyn Logic, spec: &RunSpec, deps: &RunDeps, stop: &AtomicBool) -> End {
    let period = Duration::from_secs(1) / spec.tick_hz.max(1);
    let mut ran = || -> Result<(), Problem> {
        logic.start(deps.clock.now_us())?;
        deps.status.set(&spec.id, AppStatus::Running);
        let mut next = Instant::now();
        let mut saved = Instant::now();
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            logic.tick(deps.clock.now_us())?;
            if saved.elapsed() >= Duration::from_secs(1) {
                save(logic, &spec.id);
                saved = Instant::now();
            }
            next += period;
            let now = Instant::now();
            if next < now {
                next = now; // fell behind: carry on, don't catch up in a burst
            }
            if !wait(next - now, stop) {
                return Ok(());
            }
        }
    };
    let end = match ran() {
        Ok(()) => {
            if let Err(p) = logic.stop() {
                tracing::warn!(target: "core::apps", app = spec.id.as_str(), "stopping it failed: {}", p.message);
            }
            End::Stopped
        }
        Err(p) => End::Crashed(p),
    };
    logic.session().release_all();
    save(logic, &spec.id);
    end
}

fn save(logic: &mut dyn Logic, id: &str) {
    if let Err(e) = logic.session().state.save() {
        tracing::error!(target: "core::apps", app = id, "its state couldn't be saved: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::session::tests::{FakeDriver, role};
    use crate::apps::session::{Driver, Session};
    use crate::apps::state::SharedState;
    use crate::clock::MonotonicClock;
    use faber_capabilities::{Capability, Reading, distance};

    /// Takes the base's lease when it starts, publishes every tick, and crashes on the tick
    /// `crash_on` says (counting across runs).
    struct Fake {
        session: Session,
        ticks: Arc<Mutex<Vec<String>>>,
        crash_on: Option<usize>,
    }

    impl Logic for Fake {
        fn start(&mut self, _: u64) -> Result<(), Problem> {
            self.ticks.lock().unwrap().push("start".into());
            self.session.acquire("base", 0).map_err(|e| Problem::new(format!("{e:?}"), "h"))
        }
        fn tick(&mut self, now_us: u64) -> Result<(), Problem> {
            let mut t = self.ticks.lock().unwrap();
            t.push("tick".into());
            let n = t.iter().filter(|s| *s == "tick").count();
            drop(t);
            self.session.publish(
                "wall-distance",
                Reading::Distance(distance::Reading {
                    t_us: now_us,
                    meters: 0.3,
                    status: distance::Status::Ok,
                }),
            );
            if self.crash_on.is_some_and(|c| n >= c) {
                return Err(Problem::new("it trapped", "h"));
            }
            Ok(())
        }
        fn stop(&mut self) -> Result<(), Problem> {
            self.ticks.lock().unwrap().push("stop".into());
            Ok(())
        }
        fn session(&mut self) -> &mut Session {
            &mut self.session
        }
    }

    fn spec(
        router: &Arc<Router>,
        driver: &Arc<FakeDriver>,
        crash_on: Option<usize>,
    ) -> (RunSpec, Arc<Mutex<Vec<String>>>) {
        let ticks = Arc::new(Mutex::new(Vec::new()));
        let publishes = vec![PublishSpec { name: "wall-distance".into(), capability: Capability::Distance }];
        let (r, d, t, pb) = (Arc::clone(router), Arc::clone(driver), Arc::clone(&ticks), publishes.clone());
        let make: MakeLogic = Box::new(move || {
            let driver: Arc<dyn Driver> = d.clone();
            let session = Session::new(
                "wf",
                vec![role("base", Capability::PlanarBase, true, &["base"])],
                pb.clone(),
                Arc::clone(&r),
                driver,
                Arc::new(SharedState::in_memory("t")),
                Arc::new(crate::apps::Channel::new()),
            );
            Ok(Box::new(Fake { session, ticks: Arc::clone(&t), crash_on }) as Box<dyn Logic>)
        });
        (RunSpec { id: "wf".into(), tick_hz: 100, publishes, make }, ticks)
    }

    fn deps(router: &Arc<Router>) -> RunDeps {
        RunDeps {
            clock: Arc::new(MonotonicClock::new()),
            router: Arc::clone(router),
            status: Arc::new(StatusBoard::default()),
            timing: Timing {
                backoff_start: Duration::from_millis(10),
                max_crashes: 3,
                healthy_after: Duration::from_secs(30),
            },
        }
    }

    fn wait_until(what: &str, f: impl Fn() -> bool) {
        let until = Instant::now() + Duration::from_secs(5);
        while !f() {
            assert!(Instant::now() < until, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn an_app_ticks_publishes_and_stops_cleanly() {
        let router = Arc::new(Router::new());
        let driver = Arc::new(FakeDriver::default());
        let (spec, ticks) = spec(&router, &driver, None);
        let deps = deps(&router);
        let status = Arc::clone(&deps.status);
        let addr = SignalAddress::derived("wf", "wall-distance");
        let h = start(spec, deps);
        wait_until("ten ticks", || ticks.lock().unwrap().iter().filter(|t| *t == "tick").count() >= 10);
        assert_eq!(status.get("wf"), Some(AppStatus::Running));
        assert!(matches!(router.latest(&addr), Some(Some(_))), "its derived signal is published");
        h.stop();
        let t = ticks.lock().unwrap().clone();
        assert_eq!(
            (t.first().map(String::as_str), t.last().map(String::as_str)),
            (Some("start"), Some("stop"))
        );
        assert_eq!(router.latest(&addr), None, "its signals go with it");
        let log = driver.log.lock().unwrap().clone();
        assert_eq!(log, ["acquire base as app:wf", "release base lease-base"]);
    }

    /// An app's thread can end up dropping its own handle: a driver call holds the machine
    /// alive, and when that was the last reference the machine (and every app handle in it) is
    /// dropped on that thread. Joining itself would wait forever.
    #[test]
    fn a_handle_dropped_on_its_own_thread_doesnt_wait_for_itself() {
        let (tx, rx) = std::sync::mpsc::channel::<AppHandle>();
        let done = Arc::new(AtomicBool::new(false));
        let d = Arc::clone(&done);
        let thread = std::thread::spawn(move || {
            drop(rx.recv().unwrap());
            d.store(true, Ordering::Relaxed);
        });
        tx.send(AppHandle { stop: Arc::new(AtomicBool::new(false)), thread: Some(thread) }).unwrap();
        wait_until("the thread to let go of its own handle", || done.load(Ordering::Relaxed));
    }

    /// A restart (other parts, an update) must never be reported as the old run's Running: the
    /// API would say "running" while the new run hasn't started, and a window driving then is
    /// refused with app-not-running.
    #[test]
    fn a_new_run_reports_starting_before_start_returns() {
        let router = Arc::new(Router::new());
        let driver = Arc::new(FakeDriver::default());
        let (spec, _) = spec(&router, &driver, None);
        let deps = deps(&router);
        let status = Arc::clone(&deps.status);
        status.set("wf", AppStatus::Running); // the run before this one
        let h = start(spec, deps);
        assert_ne!(status.get("wf"), Some(AppStatus::Running), "still the old run's state");
        h.stop();
    }

    #[test]
    fn a_crash_ends_the_lease_restarts_with_backoff_then_stops() {
        let router = Arc::new(Router::new());
        let driver = Arc::new(FakeDriver::default());
        let (spec, ticks) = spec(&router, &driver, Some(1));
        let deps = deps(&router);
        let status = Arc::clone(&deps.status);
        let h = start(spec, deps);
        wait_until("it gives up", || matches!(status.get("wf"), Some(AppStatus::Stopped(_))));
        let Some(AppStatus::Stopped(p)) = status.get("wf") else { unreachable!() };
        assert!(p.message.contains("crashed 3 times") && p.message.contains("it trapped"), "{p:?}");
        let starts = ticks.lock().unwrap().iter().filter(|t| *t == "start").count();
        assert_eq!(starts, 3);
        assert!(!ticks.lock().unwrap().contains(&"stop".to_string()), "a crashed run isn't asked to stop");
        let log = driver.log.lock().unwrap().clone();
        assert_eq!(log.iter().filter(|l| l.starts_with("release")).count(), 3, "{log:?}");
        h.stop();
    }
}
