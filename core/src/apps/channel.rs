//! The live channel between an app's windows and its own logic (docs/apps.md §12).
//!
//! **Messages.** A window sends its logic small typed messages (`kind` and a text or bytes
//! payload); the logic picks up everything that came since its last tick (`messages.poll`) and
//! answers one window or all of them (`messages.send`). Nothing here is ever saved: messages that
//! arrive while the logic isn't running are dropped, and a window's are gone when it closes. The
//! core caps each message at [`MAX_MESSAGE_BYTES`] and each window at [`MAX_MESSAGES_PER_S`] a
//! second, each way.
//!
//! **Driving through the logic.** A role declared `drive = "window"` may only be driven by the
//! logic while one of the app's windows says it is driving it (a *drive intent*, taken after the
//! shell's D46 confirm, with the shell's Stop bar showing). The intent ends when the window lets
//! go, the user presses Stop, the window closes, or its heartbeat is missing for
//! [`HEARTBEAT_TIMEOUT`]. Then the logic hears `drive-ended`, and the core ends the logic's lease
//! on the role itself and refuses a new one: the guarantee holds whatever the app's code does.
//!
//! One [`Channel`] per app, held by the machine like its state, so windows stay open across runs
//! of the logic. Plain Rust with the time passed in, so it is tested without threads or wasm.

use super::session::Driver;
use crate::names::PartName;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The largest message: its kind and payload together, in bytes.
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024;
/// The longest kind, in bytes.
pub const MAX_KIND_BYTES: usize = 64;
/// The most messages a second one window may send its logic, and the logic that window.
pub const MAX_MESSAGES_PER_S: u32 = 50;
/// A window driving through the logic must beat at least this often (the SDK beats every 100 ms).
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(300);
/// The most messages from windows waiting for the logic's next tick.
pub const MAX_INBOX: usize = 1024;
/// The kind of the message the core gives the logic when a window starts driving a role:
/// payload `"<role> <index>"`.
pub const DRIVE_STARTED: &str = "drive-started";
/// The kind of the message the core gives the logic when a window's drive ends: payload
/// `"<role> <index> <reason>"` ([`EndReason::id`]).
pub const DRIVE_ENDED: &str = "drive-ended";
/// Kinds only the core sends the logic.
const RESERVED: &[&str] = &[DRIVE_STARTED, DRIVE_ENDED];

/// What a message carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    Text(String),
    Bytes(Vec<u8>),
}

impl Payload {
    pub fn len(&self) -> usize {
        match self {
            Payload::Text(t) => t.len(),
            Payload::Bytes(b) => b.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One message for the logic: from which window, of which kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub window: u64,
    pub kind: String,
    pub payload: Payload,
}

/// Why a window's drive ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// The window let go.
    Released,
    /// The user pressed the shell's Stop.
    Stopped,
    WindowClosed,
    /// The window stopped beating: its page hung or went away.
    HeartbeatLost,
    /// The logic stopped (turned off, crashed, restarted with other parts).
    AppStopped,
}

impl EndReason {
    pub fn id(self) -> &'static str {
        match self {
            EndReason::Released => "released",
            EndReason::Stopped => "stopped",
            EndReason::WindowClosed => "window-closed",
            EndReason::HeartbeatLost => "heartbeat-lost",
            EndReason::AppStopped => "app-stopped",
        }
    }
}

/// What the core tells a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToWindow {
    /// From the logic.
    Message { kind: String, payload: Payload },
    /// The window no longer drives `role` through the logic.
    DriveEnded { role: String, index: u32, reason: EndReason },
    /// The app was uninstalled: this window's channel is gone.
    Closed,
}

/// Where a window's messages go. It must not block (the core holds the channel's lock).
pub type Sink = Arc<dyn Fn(ToWindow) + Send + Sync>;

/// Why the channel said no.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelError {
    /// A kind that isn't 1–64 bytes of `a-z`, `0-9` and `-`.
    BadKind,
    /// A kind only the core sends (`drive-started`, `drive-ended`).
    Reserved(String),
    /// Kind and payload together are this many bytes, over [`MAX_MESSAGE_BYTES`].
    TooLarge(usize),
    /// Over [`MAX_MESSAGES_PER_S`], or the logic's inbox is full.
    TooFast,
    NoSuchWindow,
    /// The logic isn't running, so nothing can be driven through it.
    NotRunning,
    UnknownRole(String),
    /// The role isn't declared `drive = "window"` (or doesn't take commands).
    NotWindowDriven(String),
    /// No part at this index fills the role.
    NoPart(String),
}

impl ChannelError {
    /// Stable and kebab-case, for the API.
    pub fn code(&self) -> &'static str {
        match self {
            ChannelError::BadKind => "bad-kind",
            ChannelError::Reserved(_) => "reserved-kind",
            ChannelError::TooLarge(_) => "message-too-large",
            ChannelError::TooFast => "too-many-messages",
            ChannelError::NoSuchWindow => "unknown-window",
            ChannelError::NotRunning => "app-not-running",
            ChannelError::UnknownRole(_) => "unknown-role",
            ChannelError::NotWindowDriven(_) => "not-window-driven",
            ChannelError::NoPart(_) => "no-part",
        }
    }

    /// What happened, in words.
    pub fn message(&self) -> String {
        match self {
            ChannelError::BadKind => {
                format!("A message's kind is 1 to {MAX_KIND_BYTES} characters of a-z, 0-9 and -.")
            }
            ChannelError::Reserved(k) => format!("Only Faber sends messages of the kind \"{k}\"."),
            ChannelError::TooLarge(n) => {
                format!("The message is {n} bytes; a message may be at most {MAX_MESSAGE_BYTES}.")
            }
            ChannelError::TooFast => {
                format!("More than {MAX_MESSAGES_PER_S} messages a second; this one was dropped.")
            }
            ChannelError::NoSuchWindow => "That window is closed.".into(),
            ChannelError::NotRunning => "The app's logic isn't running.".into(),
            ChannelError::UnknownRole(r) => format!("The app has no role called \"{r}\"."),
            ChannelError::NotWindowDriven(r) => {
                format!("The role \"{r}\" isn't driven from a window (drive = \"window\").")
            }
            ChannelError::NoPart(r) => format!("No part fills the role \"{r}\"."),
        }
    }

    /// The next thing to try.
    pub fn hint(&self) -> &'static str {
        match self {
            ChannelError::BadKind | ChannelError::Reserved(_) => {
                "Name messages with lowercase words joined by -, like \"drive\"."
            }
            ChannelError::TooLarge(_) => "Send less at once, or split it into several messages.",
            ChannelError::TooFast => {
                "Send at most 50 messages a second, e.g. one every 100 ms while driving."
            }
            ChannelError::NoSuchWindow => "Open the app's window again.",
            ChannelError::NotRunning => "Turn the app on in Apps, and check that every role has a part.",
            ChannelError::UnknownRole(_) => "Use a role from the app's faber.toml.",
            ChannelError::NotWindowDriven(_) => {
                "Declare the role with command = true and drive = \"window\", or drive it with faber.drive()."
            }
            ChannelError::NoPart(_) => "Pick a part for the role in Apps.",
        }
    }
}

/// A command role of a running logic, as the channel checks drive intents against it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrivenRole {
    pub role: String,
    /// Declared `drive = "window"`: driven only while a window drives it.
    pub window: bool,
    /// How many parts fill it.
    pub parts: u32,
}

/// Counts a window's messages one way, a second at a time.
#[derive(Clone, Copy)]
struct Rate {
    start: Instant,
    count: u32,
}

impl Rate {
    fn new(now: Instant) -> Rate {
        Rate { start: now, count: 0 }
    }

    fn admit(&mut self, now: Instant) -> bool {
        if now.saturating_duration_since(self.start) >= Duration::from_secs(1) {
            self.start = now;
            self.count = 0;
        }
        self.count += 1;
        self.count <= MAX_MESSAGES_PER_S
    }
}

type Key = (String, u32);

struct Window {
    sink: Sink,
    beat: Instant,
    inbound: Rate,
    outbound: Rate,
    /// The roles (and part indexes) it drives through the logic.
    drives: BTreeSet<Key>,
}

/// The logic, while it runs.
struct Run {
    token: u64,
    roles: Vec<DrivenRole>,
    inbox: VecDeque<Message>,
    driver: Arc<dyn Driver>,
    /// The leases the logic holds on window-driven roles: the core ends them when the last
    /// window driving the role lets go, whatever the logic does.
    leases: BTreeMap<Key, (PartName, String)>,
}

#[derive(Default)]
struct Inner {
    next_window: u64,
    next_token: u64,
    windows: BTreeMap<u64, Window>,
    run: Option<Run>,
}

/// A lease to end once the lock is let go (ending one takes the machine's lock).
type Release = (Arc<dyn Driver>, PartName, String);

/// One app's channel.
#[derive(Default)]
pub struct Channel {
    inner: Mutex<Inner>,
}

fn kind_ok(kind: &str) -> bool {
    !kind.is_empty()
        && kind.len() <= MAX_KIND_BYTES
        && kind.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn check(kind: &str, payload: &Payload) -> Result<(), ChannelError> {
    if !kind_ok(kind) {
        return Err(ChannelError::BadKind);
    }
    let size = kind.len() + payload.len();
    if size > MAX_MESSAGE_BYTES {
        return Err(ChannelError::TooLarge(size));
    }
    Ok(())
}

/// What to do once the channel's lock is let go: end leases, then tell windows. In that order,
/// so a window told its drive ended never finds the lease still held.
#[derive(Default)]
struct After {
    releases: Vec<Release>,
    tell: Vec<(Sink, ToWindow)>,
}

impl After {
    fn push(&mut self, release: Release) {
        self.releases.push(release);
    }

    fn finish(self) {
        for (driver, part, lease) in self.releases {
            driver.release(&part, &lease);
        }
        for (sink, message) in self.tell {
            sink(message);
        }
    }
}

impl Inner {
    /// Ends `window`'s drive of `key`: the window hears it (unless it is closing), the logic gets
    /// `drive-ended`, and when no window drives the role any more the logic's lease ends.
    fn end(&mut self, window: u64, key: &Key, reason: EndReason, out: &mut After) {
        let Some(w) = self.windows.get_mut(&window) else { return };
        if !w.drives.remove(key) {
            return;
        }
        if reason != EndReason::WindowClosed {
            let ended = ToWindow::DriveEnded { role: key.0.clone(), index: key.1, reason };
            out.tell.push((Arc::clone(&w.sink), ended));
        }
        self.after_end(window, key, reason, out);
    }

    fn after_end(&mut self, window: u64, key: &Key, reason: EndReason, out: &mut After) {
        let still = self.windows.values().any(|w| w.drives.contains(key));
        let Some(run) = self.run.as_mut() else { return };
        run.inbox.push_back(Message {
            window,
            kind: DRIVE_ENDED.into(),
            payload: Payload::Text(format!("{} {} {}", key.0, key.1, reason.id())),
        });
        if !still && let Some((part, lease)) = run.leases.remove(key) {
            out.push((Arc::clone(&run.driver), part, lease));
        }
    }

    fn end_all(&mut self, window: u64, reason: EndReason, out: &mut After) {
        let keys: Vec<Key> =
            self.windows.get(&window).map(|w| w.drives.iter().cloned().collect()).unwrap_or_default();
        for key in keys {
            self.end(window, &key, reason, out);
        }
    }

    /// Ends the drives of every window whose heartbeat is overdue.
    fn expire(&mut self, now: Instant, out: &mut After) {
        let late: Vec<u64> = self
            .windows
            .iter()
            .filter(|(_, w)| {
                !w.drives.is_empty() && now.saturating_duration_since(w.beat) > HEARTBEAT_TIMEOUT
            })
            .map(|(id, _)| *id)
            .collect();
        for id in late {
            self.end_all(id, EndReason::HeartbeatLost, out);
        }
    }

    /// Ends the run: every window's drive ends (`app-stopped`), its leases end, its inbox goes.
    fn end_run(&mut self, out: &mut After) {
        if self.run.is_none() {
            return;
        }
        let ids: Vec<u64> = self.windows.keys().copied().collect();
        for id in ids {
            self.end_all(id, EndReason::AppStopped, out);
        }
        if let Some(run) = self.run.take() {
            for (_, (part, lease)) in run.leases {
                out.push((Arc::clone(&run.driver), part, lease));
            }
        }
    }

    fn driven(&self, key: &Key) -> bool {
        self.windows.values().any(|w| w.drives.contains(key))
    }
}

impl Channel {
    pub fn new() -> Channel {
        Channel::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("app channel lock")
    }

    /// Runs `f` with the lock, then ends the leases it collected without it.
    fn with<T>(&self, f: impl FnOnce(&mut Inner, &mut After) -> T) -> T {
        let mut out = After::default();
        let value = f(&mut self.lock(), &mut out);
        out.finish();
        value
    }

    // ---- windows ------------------------------------------------------------------------------

    /// A window opened; what the core tells it goes to `sink`. Answers its id.
    pub fn open_window(&self, sink: Sink, now: Instant) -> u64 {
        let mut inner = self.lock();
        inner.next_window += 1;
        let id = inner.next_window;
        let w = Window {
            sink,
            beat: now,
            inbound: Rate::new(now),
            outbound: Rate::new(now),
            drives: BTreeSet::new(),
        };
        inner.windows.insert(id, w);
        id
    }

    /// The window closed: what it drove ends (`window-closed`).
    pub fn close_window(&self, window: u64) {
        self.with(|inner, out| {
            let Some(w) = inner.windows.remove(&window) else { return };
            for key in w.drives {
                inner.after_end(window, &key, EndReason::WindowClosed, out);
            }
        })
    }

    /// The app is going away (uninstalled): every window hears it and is forgotten.
    pub fn close_all(&self) {
        self.with(|inner, out| {
            for (id, w) in std::mem::take(&mut inner.windows) {
                (w.sink)(ToWindow::Closed);
                for key in w.drives {
                    inner.after_end(id, &key, EndReason::WindowClosed, out);
                }
            }
        })
    }

    /// How many windows are open (tests, diagnostics).
    pub fn windows(&self) -> usize {
        self.lock().windows.len()
    }

    /// The window is alive (its SDK beats every 100 ms while it drives through the logic).
    pub fn heartbeat(&self, window: u64, now: Instant) -> Result<(), ChannelError> {
        let mut inner = self.lock();
        let w = inner.windows.get_mut(&window).ok_or(ChannelError::NoSuchWindow)?;
        w.beat = now;
        Ok(())
    }

    /// A message from a window for the logic. Dropped (without an error) while the logic isn't
    /// running: nothing is kept for later.
    pub fn from_window(
        &self,
        window: u64,
        kind: &str,
        payload: Payload,
        now: Instant,
    ) -> Result<(), ChannelError> {
        check(kind, &payload)?;
        if RESERVED.contains(&kind) {
            return Err(ChannelError::Reserved(kind.into()));
        }
        let mut inner = self.lock();
        let w = inner.windows.get_mut(&window).ok_or(ChannelError::NoSuchWindow)?;
        if !w.inbound.admit(now) {
            return Err(ChannelError::TooFast);
        }
        let Some(run) = inner.run.as_mut() else { return Ok(()) };
        if run.inbox.len() >= MAX_INBOX {
            return Err(ChannelError::TooFast);
        }
        run.inbox.push_back(Message { window, kind: kind.into(), payload });
        Ok(())
    }

    /// The window drives `role` (part `index`) through the logic, from now until it lets go,
    /// stops, closes or misses its heartbeat. The shell asks the D46 confirm before this.
    pub fn drive_via(&self, window: u64, role: &str, index: u32, now: Instant) -> Result<(), ChannelError> {
        self.with(|inner, out| {
            inner.expire(now, out);
            let run = inner.run.as_ref().ok_or(ChannelError::NotRunning)?;
            let r = run
                .roles
                .iter()
                .find(|r| r.role == role)
                .ok_or_else(|| ChannelError::UnknownRole(role.into()))?;
            if !r.window {
                return Err(ChannelError::NotWindowDriven(role.into()));
            }
            if index >= r.parts {
                return Err(ChannelError::NoPart(role.into()));
            }
            let w = inner.windows.get_mut(&window).ok_or(ChannelError::NoSuchWindow)?;
            w.beat = now;
            let key = (role.to_string(), index);
            if w.drives.insert(key) {
                let run = inner.run.as_mut().expect("checked above");
                run.inbox.push_back(Message {
                    window,
                    kind: DRIVE_STARTED.into(),
                    payload: Payload::Text(format!("{role} {index}")),
                });
            }
            Ok(())
        })
    }

    /// The window lets go of `role` (part `index`).
    pub fn end_drive(&self, window: u64, role: &str, index: u32) {
        self.with(|inner, out| inner.end(window, &(role.to_string(), index), EndReason::Released, out))
    }

    /// The user pressed Stop for this window (the shell's bar, outside the app's page).
    pub fn stop(&self, window: u64) {
        self.with(|inner, out| inner.end_all(window, EndReason::Stopped, out))
    }

    /// Ends the drives of windows whose heartbeat is overdue (the machine's sweeper, every 10 ms).
    pub fn sweep(&self, now: Instant) {
        self.with(|inner, out| inner.expire(now, out))
    }

    // ---- the logic ----------------------------------------------------------------------------

    /// A run of the logic starts, with its command roles; answers the token that ends it.
    pub fn attach(&self, roles: Vec<DrivenRole>, driver: Arc<dyn Driver>) -> u64 {
        self.with(|inner, out| {
            // Runs are one after another; an old one still here ends now, like `detach`.
            inner.end_run(out);
            inner.next_token += 1;
            let token = inner.next_token;
            inner.run = Some(Run { token, roles, inbox: VecDeque::new(), driver, leases: BTreeMap::new() });
            token
        })
    }

    /// The run ended: every window's drive ends (`app-stopped`), and what waited for it is dropped.
    pub fn detach(&self, token: u64) {
        self.with(|inner, out| {
            if inner.run.as_ref().is_some_and(|r| r.token == token) {
                inner.end_run(out);
            }
        })
    }

    /// Everything that came for the logic since it last asked.
    pub fn poll(&self, token: u64) -> Vec<Message> {
        let mut inner = self.lock();
        match inner.run.as_mut() {
            Some(run) if run.token == token => run.inbox.drain(..).collect(),
            _ => Vec::new(),
        }
    }

    /// A message from the logic for one window, or every window with `None`. A window over its
    /// rate misses a broadcast; one named on its own says so.
    pub fn to_windows(
        &self,
        window: Option<u64>,
        kind: &str,
        payload: Payload,
        now: Instant,
    ) -> Result<(), ChannelError> {
        check(kind, &payload)?;
        let mut inner = self.lock();
        let msg = || ToWindow::Message { kind: kind.into(), payload: payload.clone() };
        match window {
            Some(id) => {
                let w = inner.windows.get_mut(&id).ok_or(ChannelError::NoSuchWindow)?;
                if !w.outbound.admit(now) {
                    return Err(ChannelError::TooFast);
                }
                (w.sink)(msg());
            }
            None => {
                for w in inner.windows.values_mut() {
                    if w.outbound.admit(now) {
                        (w.sink)(msg());
                    }
                }
            }
        }
        Ok(())
    }

    /// Whether some window drives `role` (part `index`) now. Overdue heartbeats end first.
    pub fn may_drive(&self, role: &str, index: u32, now: Instant) -> bool {
        self.with(|inner, out| {
            inner.expire(now, out);
            inner.driven(&(role.to_string(), index))
        })
    }

    /// The logic took the lease on a window-driven role: the core keeps it, to end it when the
    /// last window lets go. False when no window drives the role (any more): the caller lets go.
    pub fn took_lease(
        &self,
        token: u64,
        role: &str,
        index: u32,
        part: PartName,
        lease: String,
        now: Instant,
    ) -> bool {
        self.with(|inner, out| {
            inner.expire(now, out);
            let key = (role.to_string(), index);
            if !inner.driven(&key) {
                return false;
            }
            match inner.run.as_mut() {
                Some(run) if run.token == token => {
                    run.leases.insert(key, (part, lease));
                    true
                }
                _ => false,
            }
        })
    }

    /// The logic let go of a role's lease itself.
    pub fn dropped_lease(&self, token: u64, role: &str, index: u32) {
        let mut inner = self.lock();
        if let Some(run) = inner.run.as_mut().filter(|r| r.token == token) {
            run.leases.remove(&(role.to_string(), index));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::session::tests::{FakeDriver, p};

    fn sink() -> (Sink, Arc<Mutex<Vec<ToWindow>>>) {
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = Arc::clone(&got);
        (Arc::new(move |m| g.lock().unwrap().push(m)), got)
    }

    fn text(s: &str) -> Payload {
        Payload::Text(s.into())
    }

    fn base(window: bool) -> Vec<DrivenRole> {
        vec![DrivenRole { role: "base".into(), window, parts: 1 }]
    }

    fn running() -> (Channel, Arc<FakeDriver>, u64, Instant) {
        let c = Channel::new();
        let driver = Arc::new(FakeDriver::default());
        let token = c.attach(base(true), driver.clone());
        (c, driver, token, Instant::now())
    }

    #[test]
    fn messages_reach_the_logic_with_their_window_and_are_polled_once() {
        let (c, _, token, t0) = running();
        let (s, _) = sink();
        let a = c.open_window(s.clone(), t0);
        let b = c.open_window(s, t0);
        c.from_window(a, "drive", text("1 0 0"), t0).unwrap();
        c.from_window(b, "blob", Payload::Bytes(vec![1, 2, 3]), t0).unwrap();
        assert_eq!(
            c.poll(token),
            [
                Message { window: a, kind: "drive".into(), payload: text("1 0 0") },
                Message { window: b, kind: "blob".into(), payload: Payload::Bytes(vec![1, 2, 3]) },
            ]
        );
        assert!(c.poll(token).is_empty(), "each message once");
        assert!(c.poll(token + 1).is_empty(), "never to another run");
    }

    #[test]
    fn the_logic_answers_one_window_or_all_of_them() {
        let (c, _, _, t0) = running();
        let (sa, got_a) = sink();
        let (sb, got_b) = sink();
        let a = c.open_window(sa, t0);
        let _b = c.open_window(sb, t0);
        c.to_windows(Some(a), "limits", text("{}"), t0).unwrap();
        c.to_windows(None, "hello", text("all"), t0).unwrap();
        let m = |k: &str, v: &str| ToWindow::Message { kind: k.into(), payload: text(v) };
        assert_eq!(*got_a.lock().unwrap(), [m("limits", "{}"), m("hello", "all")]);
        assert_eq!(*got_b.lock().unwrap(), [m("hello", "all")]);
        assert_eq!(c.to_windows(Some(99), "x", text(""), t0), Err(ChannelError::NoSuchWindow));
    }

    #[test]
    fn kinds_sizes_and_rates_are_capped_both_ways() {
        let (c, _, token, t0) = running();
        let (s, got) = sink();
        let w = c.open_window(s, t0);
        for bad in ["", "Drive", "drive now", "drive/1", &"k".repeat(65)] {
            assert_eq!(c.from_window(w, bad, text(""), t0), Err(ChannelError::BadKind), "{bad:?}");
            assert_eq!(c.to_windows(Some(w), bad, text(""), t0), Err(ChannelError::BadKind), "{bad:?}");
        }
        for reserved in [DRIVE_STARTED, DRIVE_ENDED] {
            assert_eq!(
                c.from_window(w, reserved, text(""), t0),
                Err(ChannelError::Reserved(reserved.into()))
            );
        }
        let big = text(&"x".repeat(MAX_MESSAGE_BYTES - 3));
        assert_eq!(c.from_window(w, "big", big.clone(), t0), Ok(()), "4 KB with its kind is fine");
        let too_big = Payload::Bytes(vec![0; MAX_MESSAGE_BYTES]);
        assert_eq!(
            c.from_window(w, "big", too_big.clone(), t0),
            Err(ChannelError::TooLarge(MAX_MESSAGE_BYTES + 3))
        );
        assert_eq!(
            c.to_windows(None, "big", too_big, t0),
            Err(ChannelError::TooLarge(MAX_MESSAGE_BYTES + 3))
        );

        // 50 a second, each way: one already went in.
        for i in 1..MAX_MESSAGES_PER_S {
            assert_eq!(c.from_window(w, "n", text(&i.to_string()), t0), Ok(()), "{i}");
        }
        assert_eq!(c.from_window(w, "n", text("51"), t0), Err(ChannelError::TooFast));
        assert_eq!(c.from_window(w, "n", text("again"), t0 + Duration::from_secs(1)), Ok(()));
        assert_eq!(c.poll(token).len(), MAX_MESSAGES_PER_S as usize + 1);
        for _ in 0..MAX_MESSAGES_PER_S {
            c.to_windows(Some(w), "n", text(""), t0).unwrap();
        }
        assert_eq!(c.to_windows(Some(w), "n", text(""), t0), Err(ChannelError::TooFast));
        c.to_windows(None, "n", text(""), t0).unwrap();
        assert_eq!(
            got.lock().unwrap().len(),
            MAX_MESSAGES_PER_S as usize,
            "a broadcast skips a window over its rate"
        );
    }

    #[test]
    fn nothing_is_kept_while_the_logic_is_not_running() {
        let c = Channel::new();
        let (s, _) = sink();
        let t0 = Instant::now();
        let w = c.open_window(s, t0);
        assert_eq!(c.from_window(w, "drive", text("1 0 0"), t0), Ok(()), "dropped, not refused");
        let token = c.attach(base(true), Arc::new(FakeDriver::default()));
        assert!(c.poll(token).is_empty(), "what came before the run is gone");
        c.from_window(w, "drive", text("2 0 0"), t0).unwrap();
        c.detach(token);
        let next = c.attach(base(true), Arc::new(FakeDriver::default()));
        assert!(c.poll(next).is_empty(), "and what the last run didn't read");
    }

    #[test]
    fn a_full_inbox_refuses_more_until_the_logic_reads_it() {
        let (c, _, token, t0) = running();
        let (s, _) = sink();
        let windows: Vec<u64> =
            (0..=(MAX_INBOX as u32 / MAX_MESSAGES_PER_S)).map(|_| c.open_window(s.clone(), t0)).collect();
        let mut refused = 0;
        for w in &windows {
            for _ in 0..MAX_MESSAGES_PER_S {
                if c.from_window(*w, "n", text(""), t0) == Err(ChannelError::TooFast) {
                    refused += 1;
                }
            }
        }
        assert!(refused > 0);
        assert_eq!(c.poll(token).len(), MAX_INBOX);
    }

    #[test]
    fn a_drive_intent_is_checked_against_the_roles_and_needs_the_logic() {
        let c = Channel::new();
        let (s, _) = sink();
        let t0 = Instant::now();
        let w = c.open_window(s, t0);
        assert_eq!(c.drive_via(w, "base", 0, t0), Err(ChannelError::NotRunning));
        let roles = vec![
            DrivenRole { role: "base".into(), window: true, parts: 1 },
            DrivenRole { role: "arm".into(), window: false, parts: 1 },
        ];
        let token = c.attach(roles, Arc::new(FakeDriver::default()));
        assert_eq!(c.drive_via(w, "lidar", 0, t0), Err(ChannelError::UnknownRole("lidar".into())));
        assert_eq!(c.drive_via(w, "arm", 0, t0), Err(ChannelError::NotWindowDriven("arm".into())));
        assert_eq!(c.drive_via(w, "base", 1, t0), Err(ChannelError::NoPart("base".into())));
        assert_eq!(c.drive_via(99, "base", 0, t0), Err(ChannelError::NoSuchWindow));
        assert!(!c.may_drive("base", 0, t0));
        c.drive_via(w, "base", 0, t0).unwrap();
        c.drive_via(w, "base", 0, t0).unwrap();
        assert!(c.may_drive("base", 0, t0));
        assert_eq!(
            c.poll(token),
            [Message { window: w, kind: DRIVE_STARTED.into(), payload: text("base 0") }],
            "once, however often it asks"
        );
    }

    /// The core's side of the guarantee: however the intent ends, the logic's lease ends with it,
    /// and the logic hears `drive-ended` (which it may ignore).
    #[test]
    fn every_way_an_intent_ends_ends_the_logics_lease() {
        for (how, reason) in [
            ("released", EndReason::Released),
            ("stopped", EndReason::Stopped),
            ("closed", EndReason::WindowClosed),
            ("quiet", EndReason::HeartbeatLost),
            ("app-stopped", EndReason::AppStopped),
        ] {
            let (c, driver, token, t0) = running();
            let (s, got) = sink();
            let w = c.open_window(s, t0);
            c.drive_via(w, "base", 0, t0).unwrap();
            assert!(c.took_lease(token, "base", 0, p("base"), "lease-1".into(), t0), "{how}");
            c.poll(token);
            let later = t0 + Duration::from_millis(250);
            c.heartbeat(w, later).unwrap();
            match reason {
                EndReason::Released => c.end_drive(w, "base", 0),
                EndReason::Stopped => c.stop(w),
                EndReason::WindowClosed => c.close_window(w),
                EndReason::HeartbeatLost => {
                    c.sweep(later + Duration::from_millis(300));
                    assert!(driver.log.lock().unwrap().is_empty(), "300 ms after a beat is still fine");
                    c.sweep(later + Duration::from_millis(301));
                }
                EndReason::AppStopped => c.detach(token),
            }
            assert_eq!(*driver.log.lock().unwrap(), ["release base lease-1"], "{how}");
            assert!(!c.may_drive("base", 0, later), "{how}");
            if reason != EndReason::AppStopped {
                let ended = Message {
                    window: w,
                    kind: DRIVE_ENDED.into(),
                    payload: text(&format!("base 0 {}", reason.id())),
                };
                assert_eq!(c.poll(token), [ended], "{how}");
            }
            let told = got.lock().unwrap().clone();
            if reason == EndReason::WindowClosed {
                assert!(told.is_empty(), "a closed window hears nothing");
            } else {
                assert_eq!(told, [ToWindow::DriveEnded { role: "base".into(), index: 0, reason }], "{how}");
            }
            // The logic may try again: there is no intent, so no lease is kept for it.
            assert!(!c.took_lease(token, "base", 0, p("base"), "lease-2".into(), later), "{how}");
        }
    }

    /// A window told its drive ended must never find the logic's lease still held: the lease
    /// ends first, then the window hears it.
    #[test]
    fn the_lease_has_ended_by_the_time_the_window_hears_it() {
        for reason in
            [EndReason::Released, EndReason::Stopped, EndReason::HeartbeatLost, EndReason::AppStopped]
        {
            let (c, driver, token, t0) = running();
            let released_when_told = Arc::new(Mutex::new(None));
            let (d, r) = (Arc::clone(&driver), Arc::clone(&released_when_told));
            let s: Sink = Arc::new(move |m| {
                if matches!(m, ToWindow::DriveEnded { .. }) {
                    *r.lock().unwrap() = Some(!d.log.lock().unwrap().is_empty());
                }
            });
            let w = c.open_window(s, t0);
            c.drive_via(w, "base", 0, t0).unwrap();
            assert!(c.took_lease(token, "base", 0, p("base"), "lease-1".into(), t0));
            match reason {
                EndReason::Released => c.end_drive(w, "base", 0),
                EndReason::Stopped => c.stop(w),
                EndReason::HeartbeatLost => c.sweep(t0 + Duration::from_secs(1)),
                _ => c.detach(token),
            }
            assert_eq!(*released_when_told.lock().unwrap(), Some(true), "{reason:?}");
        }
    }

    #[test]
    fn the_lease_stays_while_another_window_still_drives() {
        let (c, driver, token, t0) = running();
        let (s, _) = sink();
        let a = c.open_window(s.clone(), t0);
        let b = c.open_window(s, t0);
        c.drive_via(a, "base", 0, t0).unwrap();
        c.drive_via(b, "base", 0, t0).unwrap();
        assert!(c.took_lease(token, "base", 0, p("base"), "l".into(), t0));
        c.close_window(a);
        assert!(driver.log.lock().unwrap().is_empty());
        assert!(c.may_drive("base", 0, t0));
        c.stop(b);
        assert_eq!(*driver.log.lock().unwrap(), ["release base l"]);
    }

    #[test]
    fn a_window_that_doesnt_drive_needs_no_heartbeat() {
        let (c, _, token, t0) = running();
        let (s, got) = sink();
        let w = c.open_window(s, t0);
        c.sweep(t0 + Duration::from_secs(10));
        c.from_window(w, "hello", text(""), t0 + Duration::from_secs(10)).unwrap();
        assert_eq!(c.poll(token).len(), 1);
        assert!(got.lock().unwrap().is_empty());
    }

    #[test]
    fn uninstalling_closes_every_window() {
        let (c, driver, token, t0) = running();
        let (s, got) = sink();
        let w = c.open_window(s, t0);
        c.drive_via(w, "base", 0, t0).unwrap();
        c.took_lease(token, "base", 0, p("base"), "l".into(), t0);
        c.close_all();
        assert_eq!(*got.lock().unwrap(), [ToWindow::Closed]);
        assert_eq!(*driver.log.lock().unwrap(), ["release base l"]);
        assert_eq!(c.windows(), 0);
        assert_eq!(c.heartbeat(w, t0), Err(ChannelError::NoSuchWindow));
    }

    #[test]
    fn a_stale_run_can_neither_read_nor_end_the_new_one() {
        let (c, _, old, t0) = running();
        let new = c.attach(base(true), Arc::new(FakeDriver::default()));
        let (s, _) = sink();
        let w = c.open_window(s, t0);
        c.drive_via(w, "base", 0, t0).unwrap();
        c.detach(old);
        assert!(c.may_drive("base", 0, t0), "the old run's end doesn't end the new run's drives");
        assert!(!c.took_lease(old, "base", 0, p("base"), "l".into(), t0));
        assert_eq!(c.poll(new).len(), 1);
    }
}
