//! A mutex that knows who holds it and who is waiting for it, so a core that stops answering can
//! say why instead of going quiet. The machine's own locks use it, and a watchdog thread logs any
//! lock held too long, with the code that holds it and the code waiting for it.

use std::panic::Location;
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

pub struct Watched<T> {
    name: &'static str,
    inner: Mutex<T>,
    book: Mutex<Book>,
}

#[derive(Default)]
struct Book {
    holder: Option<Who>,
    waiting: Vec<(u64, Who)>,
    next_ticket: u64,
}

#[derive(Clone)]
struct Who {
    thread: String,
    at: &'static Location<'static>,
    since: Instant,
}

impl Who {
    fn here(at: &'static Location<'static>) -> Who {
        let t = std::thread::current();
        let thread = t.name().map_or_else(|| format!("{:?}", t.id()), str::to_string);
        Who { thread, at, since: Instant::now() }
    }

    fn describe(&self) -> String {
        format!(
            "{} at {}:{} ({:.1} s)",
            self.thread,
            self.at.file(),
            self.at.line(),
            self.since.elapsed().as_secs_f32()
        )
    }
}

/// A [`Watched`] lock, held. Dropping it lets go and clears the record.
pub struct WatchedGuard<'a, T> {
    guard: MutexGuard<'a, T>,
    owner: &'a Watched<T>,
}

impl<T> std::ops::Deref for WatchedGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T> std::ops::DerefMut for WatchedGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<T> Drop for WatchedGuard<'_, T> {
    fn drop(&mut self) {
        self.owner.book().holder = None;
    }
}

impl<T> Watched<T> {
    pub fn new(name: &'static str, value: T) -> Watched<T> {
        Watched { name, inner: Mutex::new(value), book: Mutex::new(Book::default()) }
    }

    /// Waits for the lock. A poisoned lock (a thread panicked holding it) panics here too, like
    /// `lock().expect(…)` did.
    #[track_caller]
    pub fn lock(&self) -> WatchedGuard<'_, T> {
        let me = Who::here(Location::caller());
        let ticket = {
            let mut b = self.book();
            b.next_ticket += 1;
            let t = b.next_ticket;
            b.waiting.push((t, me.clone()));
            t
        };
        let guard = self.inner.lock().unwrap_or_else(|e| panic!("{} lock: {e}", self.name));
        let mut b = self.book();
        b.waiting.retain(|(t, _)| *t != ticket);
        b.holder = Some(Who { since: Instant::now(), ..me });
        drop(b);
        WatchedGuard { guard, owner: self }
    }

    /// The lock if it is free right now.
    #[track_caller]
    pub fn try_lock(&self) -> Option<WatchedGuard<'_, T>> {
        let at = Location::caller();
        match self.inner.try_lock() {
            Ok(guard) => {
                self.book().holder = Some(Who::here(at));
                Some(WatchedGuard { guard, owner: self })
            }
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(e)) => panic!("{} lock: {e}", self.name),
        }
    }

    /// What to log when the lock has been held longer than `limit`: who holds it, where, and who
    /// waits. `None` while it is free or held briefly.
    pub fn held_too_long(&self, limit: Duration) -> Option<String> {
        let b = self.book();
        let holder = b.holder.as_ref().filter(|h| h.since.elapsed() >= limit)?;
        let waiting: Vec<String> = b.waiting.iter().map(|(_, w)| w.describe()).collect();
        Some(format!(
            "the {} lock is held by {}; waiting: {}",
            self.name,
            holder.describe(),
            if waiting.is_empty() { "nobody".to_string() } else { waiting.join(", ") }
        ))
    }

    fn book(&self) -> MutexGuard<'_, Book> {
        // The book is only ever held for a few instructions and never while waiting.
        self.book.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_lock_held_too_long_names_its_holder_and_who_waits() {
        let w = Arc::new(Watched::new("test", 0));
        let held = w.lock();
        assert_eq!(w.held_too_long(Duration::from_millis(20)), None, "not long yet");
        let w2 = Arc::clone(&w);
        let waiter = std::thread::Builder::new()
            .name("the-waiter".into())
            .spawn(move || {
                *w2.lock() += 1;
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(60));
        let report = w.held_too_long(Duration::from_millis(20)).expect("held too long");
        assert!(report.starts_with("the test lock is held by "), "{report}");
        assert!(report.contains("watched.rs:"), "where it was taken: {report}");
        assert!(report.contains("waiting: the-waiter at "), "{report}");
        drop(held);
        waiter.join().unwrap();
        let again = w.lock();
        assert_eq!(*again, 1);
        assert!(w.held_too_long(Duration::ZERO).unwrap().contains("waiting: nobody"));
        drop(again);
        assert_eq!(w.held_too_long(Duration::ZERO), None, "free again");
    }

    #[test]
    fn try_lock_says_when_it_is_taken() {
        let w = Watched::new("test", ());
        let held = w.lock();
        assert!(w.try_lock().is_none());
        drop(held);
        assert!(w.try_lock().is_some());
    }
}
