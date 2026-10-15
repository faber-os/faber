//! Layer 2: the router. Moves signals from parts to whoever subscribes (docs/architecture.md §9).
//!
//! Each signal address has one channel, chosen by its capability's delivery mode:
//! latest-value (a slow subscriber gets the newest reading and skips the rest, so it can never
//! slow a fast part down) or every-message (events; none may be lost).

use crate::names::SignalAddress;
use faber_capabilities::{Capability, Delivery, Reading};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::{broadcast, watch};

/// How many events an every-message subscriber may fall behind before it loses some.
const EVENT_BACKLOG: usize = 256;

enum Channel {
    Latest(watch::Sender<Option<Arc<Reading>>>),
    Every(broadcast::Sender<Arc<Reading>>),
}

struct Signal {
    capability: Capability,
    channel: Channel,
    /// The newest reading, for one-off reads (`GET /api/v0/signals/…`), events included.
    last: Option<Arc<Reading>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PublishError {
    /// Nobody declared this address (a bug in the runtime, not in a connector).
    Undeclared,
    /// The reading's capability doesn't match the declared one (a connector bug).
    WrongCapability { declared: Capability, got: Capability },
}

/// Sees every reading that is published, after it is delivered (see [`Router::set_tap`]).
pub type Tap = Arc<dyn Fn(&SignalAddress, &Reading) + Send + Sync>;

#[derive(Default)]
pub struct Router {
    signals: Mutex<BTreeMap<SignalAddress, Signal>>,
    tap: RwLock<Option<Tap>>,
}

pub enum Subscription {
    Latest(watch::Receiver<Option<Arc<Reading>>>),
    Every(broadcast::Receiver<Arc<Reading>>),
}

impl Subscription {
    /// The next reading, or `None` once the signal is gone (its part was removed or stopped).
    pub async fn next(&mut self) -> Option<Arc<Reading>> {
        match self {
            Subscription::Latest(rx) => loop {
                rx.changed().await.ok()?;
                if let Some(r) = rx.borrow_and_update().clone() {
                    return Some(r);
                }
            },
            Subscription::Every(rx) => loop {
                match rx.recv().await {
                    Ok(r) => return Some(r),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(target: "core::router", "a subscriber fell behind and missed {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            },
        }
    }
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates the channel for `address`. Declaring an address again replaces it, which ends
    /// the old subscriptions.
    pub fn declare(&self, address: SignalAddress, capability: Capability) {
        let channel = match capability.delivery() {
            Delivery::LatestValue => Channel::Latest(watch::channel(None).0),
            Delivery::EveryMessage => Channel::Every(broadcast::channel(EVENT_BACKLOG).0),
        };
        self.signals.lock().expect("router lock").insert(address, Signal { capability, channel, last: None });
    }

    /// Removes `address`. Its subscribers get `None` from `next()`.
    pub fn remove(&self, address: &SignalAddress) {
        self.signals.lock().expect("router lock").remove(address);
    }

    /// Calls `tap` with every reading published from now on, after its subscribers have it and
    /// outside the router's lock. It must be quick: it runs on the publisher's thread.
    pub fn set_tap(&self, tap: Tap) {
        *self.tap.write().expect("router tap lock") = Some(tap);
    }

    pub fn publish(&self, address: &SignalAddress, reading: Reading) -> Result<(), PublishError> {
        let reading = self.deliver(address, reading)?;
        if let Some(tap) = self.tap.read().expect("router tap lock").as_ref() {
            tap(address, &reading);
        }
        Ok(())
    }

    fn deliver(&self, address: &SignalAddress, reading: Reading) -> Result<Arc<Reading>, PublishError> {
        let mut signals = self.signals.lock().expect("router lock");
        let signal = signals.get_mut(address).ok_or(PublishError::Undeclared)?;
        if reading.capability() != signal.capability {
            return Err(PublishError::WrongCapability {
                declared: signal.capability,
                got: reading.capability(),
            });
        }
        let reading = Arc::new(reading);
        signal.last = Some(Arc::clone(&reading));
        match &signal.channel {
            Channel::Latest(tx) => {
                tx.send_replace(Some(Arc::clone(&reading)));
            }
            Channel::Every(tx) => {
                let _ = tx.send(Arc::clone(&reading)); // no subscribers is fine
            }
        }
        Ok(reading)
    }

    /// Subscribes to `address`, or `None` if no such signal exists right now.
    pub fn subscribe(&self, address: &SignalAddress) -> Option<Subscription> {
        let signals = self.signals.lock().expect("router lock");
        Some(match &signals.get(address)?.channel {
            Channel::Latest(tx) => {
                let mut rx = tx.subscribe();
                rx.mark_changed(); // deliver the current value (if any) right away
                Subscription::Latest(rx)
            }
            Channel::Every(tx) => Subscription::Every(tx.subscribe()),
        })
    }

    /// The newest reading of `address`: `None` if no such signal exists, `Some(None)` if it
    /// exists but nothing has been published yet.
    pub fn latest(&self, address: &SignalAddress) -> Option<Option<Arc<Reading>>> {
        self.signals.lock().expect("router lock").get(address).map(|s| s.last.clone())
    }

    /// Every declared signal and its capability.
    pub fn signals(&self) -> Vec<(SignalAddress, Capability)> {
        self.signals.lock().expect("router lock").iter().map(|(a, s)| (a.clone(), s.capability)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::laser_scan;
    use std::time::Duration;

    fn addr() -> SignalAddress {
        SignalAddress::parse("lidar/laser-scan").unwrap()
    }

    fn scan(t: u64) -> Reading {
        Reading::LaserScan(laser_scan::Reading::even_bins(t, 0.0, 0.1, 0.1, vec![1.0]))
    }

    async fn next_t(sub: &mut Subscription) -> Option<u64> {
        tokio::time::timeout(Duration::from_secs(1), sub.next()).await.expect("no hang").map(|r| r.t_us())
    }

    #[tokio::test]
    async fn latest_value_skips_what_a_slow_subscriber_missed() {
        let router = Router::new();
        router.declare(addr(), Capability::LaserScan);
        let mut sub = router.subscribe(&addr()).unwrap();
        for t in 1..=5 {
            router.publish(&addr(), scan(t)).unwrap();
        }
        assert_eq!(next_t(&mut sub).await, Some(5), "only the newest reading");
    }

    #[tokio::test]
    async fn a_new_subscriber_gets_the_current_value() {
        let router = Router::new();
        router.declare(addr(), Capability::LaserScan);
        router.publish(&addr(), scan(7)).unwrap();
        let mut sub = router.subscribe(&addr()).unwrap();
        assert_eq!(next_t(&mut sub).await, Some(7));
    }

    #[tokio::test]
    async fn removing_a_signal_ends_its_subscriptions() {
        let router = Router::new();
        router.declare(addr(), Capability::LaserScan);
        let mut sub = router.subscribe(&addr()).unwrap();
        router.remove(&addr());
        assert_eq!(next_t(&mut sub).await, None);
        assert!(router.subscribe(&addr()).is_none());
        assert_eq!(router.publish(&addr(), scan(1)), Err(PublishError::Undeclared));
    }

    #[test]
    fn a_tap_sees_every_published_reading_but_not_refused_ones() {
        let router = Router::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&seen);
        router.set_tap(Arc::new(move |a: &SignalAddress, r: &Reading| {
            s.lock().unwrap().push((a.to_string(), r.t_us()));
        }));
        router.declare(addr(), Capability::LaserScan);
        router.publish(&addr(), scan(3)).unwrap();
        let other = SignalAddress::parse("nobody/laser-scan").unwrap();
        assert!(router.publish(&other, scan(4)).is_err());
        assert_eq!(*seen.lock().unwrap(), [("lidar/laser-scan".to_string(), 3)]);
    }

    #[test]
    fn signals_lists_what_is_declared() {
        let router = Router::new();
        router.declare(addr(), Capability::LaserScan);
        assert_eq!(router.signals(), vec![(addr(), Capability::LaserScan)]);
    }

    #[test]
    fn the_latest_reading_can_be_read_without_subscribing() {
        let r = Router::new();
        assert!(r.latest(&addr()).is_none(), "unknown address");
        r.declare(addr(), Capability::LaserScan);
        assert_eq!(r.latest(&addr()), Some(None), "declared, nothing published yet");
        r.publish(&addr(), scan(1)).unwrap();
        r.publish(&addr(), scan(2)).unwrap();
        assert_eq!(r.latest(&addr()).unwrap().unwrap().t_us(), 2);
        r.remove(&addr());
        assert!(r.latest(&addr()).is_none());
    }
}
