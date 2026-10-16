//! Layer 2: sign-in sessions and the sign-in rate limit (docs/auth.md), in memory, on the core's
//! clock.
//!
//! A session is a random 256-bit token the browser keeps in a cookie. Only its SHA-256 is kept
//! here, so nothing in memory or in a log can be replayed as a cookie.

use crate::clock::SharedClock;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::watch;

const DAY_US: u64 = 24 * 60 * 60 * 1_000_000;
/// A session ends after this long without being used…
pub const IDLE_US: u64 = 7 * DAY_US;
/// …and in any case this long after signing in.
pub const LIFETIME_US: u64 = 30 * DAY_US;
/// The oldest of a user's sessions ends when they open more than this many.
const MAX_PER_USER: usize = 50;

struct Session {
    user: String,
    started_us: u64,
    used_us: u64,
}

pub struct Sessions {
    clock: SharedClock,
    by_hash: Mutex<HashMap<[u8; 32], Session>>,
    /// Fires when sessions end (sign-out, password change, removal), so open WebSockets check
    /// theirs.
    ended: watch::Sender<u64>,
}

fn key(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

impl Sessions {
    pub fn new(clock: SharedClock) -> Sessions {
        Sessions { clock, by_hash: Mutex::new(HashMap::new()), ended: watch::channel(0).0 }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<[u8; 32], Session>> {
        self.by_hash.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn watch(&self) -> watch::Receiver<u64> {
        self.ended.subscribe()
    }

    fn announce(&self) {
        self.ended.send_modify(|n| *n += 1);
    }

    /// Starts a session for `user` and returns its token (the cookie's value).
    pub fn start(&self, user: &str) -> String {
        let token = random_token();
        let now = self.clock.now_us();
        let mut map = self.lock();
        map.retain(|_, s| alive(s, now));
        let mut theirs: Vec<([u8; 32], u64)> =
            map.iter().filter(|(_, s)| s.user == user).map(|(k, s)| (*k, s.started_us)).collect();
        if theirs.len() >= MAX_PER_USER {
            theirs.sort_by_key(|(_, started)| *started);
            for (k, _) in theirs.iter().take(theirs.len() + 1 - MAX_PER_USER) {
                map.remove(k);
            }
        }
        map.insert(key(&token), Session { user: user.into(), started_us: now, used_us: now });
        token
    }

    /// The user whose session this is, and marks it used (sliding expiry). `None` when it ended or
    /// never existed.
    pub fn user(&self, token: &str) -> Option<String> {
        let now = self.clock.now_us();
        let mut map = self.lock();
        let k = key(token);
        let s = map.get_mut(&k)?;
        if !alive(s, now) {
            map.remove(&k);
            return None;
        }
        s.used_us = now;
        Some(s.user.clone())
    }

    /// Like [`Sessions::user`], without counting as a use (an open WebSocket checking it is still
    /// signed in).
    pub fn peek(&self, token: &str) -> Option<String> {
        let now = self.clock.now_us();
        self.lock().get(&key(token)).filter(|s| alive(s, now)).map(|s| s.user.clone())
    }

    /// Signing out.
    pub fn end(&self, token: &str) {
        if self.lock().remove(&key(token)).is_some() {
            self.announce();
        }
    }

    /// Ends every session of `user` except `keep` (the one that changed its own password).
    pub fn end_user(&self, user: &str, keep: Option<&str>) {
        let keep = keep.map(key);
        self.lock().retain(|k, s| s.user != user || Some(*k) == keep);
        self.announce();
    }

    /// Ends every session (a restore replaced the users).
    pub fn end_all(&self) {
        self.lock().clear();
        self.announce();
    }

    /// How many sessions are open (for tests and About).
    pub fn count(&self) -> usize {
        let now = self.clock.now_us();
        self.lock().values().filter(|s| alive(s, now)).count()
    }
}

fn alive(s: &Session, now: u64) -> bool {
    now.saturating_sub(s.used_us) < IDLE_US && now.saturating_sub(s.started_us) < LIFETIME_US
}

/// 256 random bits from the operating system, as URL-safe text (43 characters).
fn random_token() -> String {
    use base64::Engine;
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("the operating system's random numbers");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Failed sign-ins per address and per name. The first [`FREE_FAILURES`] are free; after that
/// each attempt waits 1 s, 2 s, 4 s … up to 15 minutes after the previous failure.
pub struct LoginLimiter {
    clock: SharedClock,
    seen: Mutex<HashMap<String, Failures>>,
}

#[derive(Clone, Copy)]
struct Failures {
    count: u32,
    last_us: u64,
}

pub const FREE_FAILURES: u32 = 5;
const FIRST_WAIT_US: u64 = 1_000_000;
const MAX_WAIT_US: u64 = 15 * 60 * 1_000_000;
/// Failures are forgotten this long after the last one.
const FORGET_US: u64 = 60 * 60 * 1_000_000;
/// The most addresses and names remembered at once; the oldest go first.
const MAX_REMEMBERED: usize = 10_000;

fn wait_after(f: Failures) -> u64 {
    if f.count < FREE_FAILURES {
        return 0;
    }
    let doublings = (f.count - FREE_FAILURES).min(20);
    (FIRST_WAIT_US << doublings).min(MAX_WAIT_US)
}

impl LoginLimiter {
    pub fn new(clock: SharedClock) -> LoginLimiter {
        LoginLimiter { clock, seen: Mutex::new(HashMap::new()) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Failures>> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `Err(wait_us)` when any of `keys` must still wait before it may try again.
    pub fn check(&self, keys: &[String]) -> Result<(), u64> {
        let now = self.clock.now_us();
        let seen = self.lock();
        let wait = keys
            .iter()
            .filter_map(|k| seen.get(k))
            .map(|f| (f.last_us + wait_after(*f)).saturating_sub(now))
            .max()
            .unwrap_or(0);
        if wait > 0 { Err(wait) } else { Ok(()) }
    }

    pub fn failed(&self, keys: &[String]) {
        let now = self.clock.now_us();
        let mut seen = self.lock();
        seen.retain(|_, f| now.saturating_sub(f.last_us) < FORGET_US);
        for k in keys {
            let f = seen.entry(k.clone()).or_insert(Failures { count: 0, last_us: now });
            f.count += 1;
            f.last_us = now;
        }
        if seen.len() > MAX_REMEMBERED {
            let mut by_age: Vec<(String, u64)> = seen.iter().map(|(k, f)| (k.clone(), f.last_us)).collect();
            by_age.sort_by_key(|(_, t)| *t);
            for (k, _) in by_age.into_iter().take(seen.len() - MAX_REMEMBERED) {
                seen.remove(&k);
            }
        }
    }

    pub fn succeeded(&self, keys: &[String]) {
        let mut seen = self.lock();
        for k in keys {
            seen.remove(k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use std::sync::Arc;

    fn sessions() -> (Arc<ManualClock>, Sessions) {
        let clock = Arc::new(ManualClock::new(0));
        (Arc::clone(&clock), Sessions::new(clock))
    }

    #[test]
    fn a_session_is_a_random_256_bit_token() {
        let (_c, s) = sessions();
        let a = s.start("mikkel");
        let b = s.start("mikkel");
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(s.user(&a).as_deref(), Some("mikkel"));
        assert_eq!(s.user("not-a-session"), None);
    }

    #[test]
    fn a_session_ends_after_a_week_unused() {
        let (c, s) = sessions();
        let t = s.start("mikkel");
        c.advance(IDLE_US - 1);
        assert!(s.user(&t).is_some(), "used just in time");
        c.advance(IDLE_US - 1);
        assert!(s.user(&t).is_some(), "using it slid the expiry");
        c.advance(IDLE_US);
        assert_eq!(s.user(&t), None);
    }

    #[test]
    fn a_session_ends_after_thirty_days_even_in_use() {
        let (c, s) = sessions();
        let t = s.start("mikkel");
        for _ in 0..29 {
            c.advance(DAY_US);
            assert!(s.user(&t).is_some());
        }
        c.advance(DAY_US);
        assert_eq!(s.user(&t), None);
    }

    #[test]
    fn peeking_does_not_slide_the_expiry() {
        let (c, s) = sessions();
        let t = s.start("mikkel");
        c.advance(IDLE_US - 1);
        assert!(s.peek(&t).is_some());
        c.advance(2);
        assert_eq!(s.peek(&t), None);
    }

    #[test]
    fn signing_out_and_password_changes_end_sessions() {
        let (_c, s) = sessions();
        let rx = s.watch();
        let a = s.start("mikkel");
        let b = s.start("mikkel");
        let other = s.start("bo");
        s.end(&a);
        assert!(rx.has_changed().unwrap());
        assert_eq!(s.user(&a), None);
        let c = s.start("mikkel");
        s.end_user("mikkel", Some(&c));
        assert_eq!(s.user(&b), None);
        assert!(s.user(&c).is_some(), "the session that changed the password stays");
        assert!(s.user(&other).is_some());
        s.end_all();
        assert_eq!(s.count(), 0);
    }

    #[test]
    fn a_user_has_at_most_fifty_sessions() {
        let (c, s) = sessions();
        let first = s.start("mikkel");
        for _ in 0..MAX_PER_USER {
            c.advance(1);
            s.start("mikkel");
        }
        assert_eq!(s.count(), MAX_PER_USER);
        assert_eq!(s.user(&first), None, "the oldest went");
    }

    #[test]
    fn sign_in_failures_wait_longer_and_longer() {
        let clock = Arc::new(ManualClock::new(0));
        let l = LoginLimiter::new(clock.clone());
        let keys = vec!["ip:10.0.0.5".to_string(), "user:mikkel".to_string()];
        for _ in 0..FREE_FAILURES {
            assert_eq!(l.check(&keys), Ok(()));
            l.failed(&keys);
        }
        assert_eq!(l.check(&keys), Err(1_000_000));
        clock.advance(1_000_000);
        assert_eq!(l.check(&keys), Ok(()));
        l.failed(&keys);
        assert_eq!(l.check(&keys), Err(2_000_000));
        // Another address trying the same name waits too.
        assert!(l.check(&["ip:10.0.0.9".into(), "user:mikkel".into()]).is_err());
        assert_eq!(l.check(&["ip:10.0.0.9".into(), "user:bo".into()]), Ok(()));
        for _ in 0..30 {
            clock.advance(MAX_WAIT_US);
            l.failed(&keys);
        }
        assert_eq!(l.check(&keys), Err(MAX_WAIT_US), "capped at 15 minutes");
        l.succeeded(&keys);
        assert_eq!(l.check(&keys), Ok(()));
    }

    #[test]
    fn failures_are_forgotten_after_an_hour() {
        let clock = Arc::new(ManualClock::new(0));
        let l = LoginLimiter::new(clock.clone());
        let keys = vec!["ip:a".to_string()];
        for _ in 0..FREE_FAILURES + 3 {
            l.failed(&keys);
        }
        clock.advance(FORGET_US);
        l.failed(&["ip:b".to_string()]);
        assert_eq!(l.check(&keys), Ok(()));
    }
}
