//! Layer 2: notices, things the core tells every screen once: the shell shows each one as a toast,
//! and each is also written to the log. The first kind (docs/capabilities.md, `battery@1`): "the
//! core raises a notification whenever `level` changes".
//!
//! [`Notices::observe`] looks at every published reading (the machine taps the router with it). It
//! remembers each battery signal's level, so a notice is raised only on a change: never for the
//! first `ok`, but for a first reading that is already `low` or `critical`. It keeps remembering
//! while a part restarts, so a battery swapped for a charged one says "ok again". The capability's
//! hysteresis lives in whoever computes the level, so a wobbling voltage doesn't flood this.

use crate::clock::SharedClock;
use crate::names::{Owner, SignalAddress};
use faber_capabilities::Reading;
use faber_capabilities::battery::Level;
use std::collections::BTreeMap;
use std::sync::Mutex;
use tokio::sync::broadcast;

/// One thing worth telling the user about.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// A battery's level changed.
    BatteryLevel {
        /// When it was noticed (µs, core clock).
        t_us: u64,
        address: SignalAddress,
        level: Level,
        /// The level before; `None` for a battery seen for the first time.
        previous: Option<Level>,
        volts: f32,
    },
}

pub struct Notices {
    clock: SharedClock,
    levels: Mutex<BTreeMap<SignalAddress, Level>>,
    tx: broadcast::Sender<Notice>,
}

impl Notices {
    pub fn new(clock: SharedClock) -> Notices {
        Notices { clock, levels: Mutex::new(BTreeMap::new()), tx: broadcast::channel(64).0 }
    }

    /// Every notice from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<Notice> {
        self.tx.subscribe()
    }

    /// Looks at one published reading, and raises (and logs) a notice if it is one. Cheap for
    /// anything that isn't a battery.
    pub fn observe(&self, address: &SignalAddress, reading: &Reading) -> Option<Notice> {
        let Reading::Battery(b) = reading else { return None };
        let previous = self.levels.lock().expect("notices lock").insert(address.clone(), b.level);
        if previous == Some(b.level) || (previous.is_none() && b.level == Level::Ok) {
            return None;
        }
        let notice = Notice::BatteryLevel {
            t_us: self.clock.now_us(),
            address: address.clone(),
            level: b.level,
            previous,
            volts: b.volts,
        };
        log(&notice);
        let _ = self.tx.send(notice.clone()); // no screen listening is fine
        Some(notice)
    }
}

/// The notice as a log entry, from the part (or app) whose signal it is.
fn log(notice: &Notice) {
    let Notice::BatteryLevel { address, level, volts, .. } = notice;
    let text = match level {
        Level::Ok => format!("The battery {address} is ok again ({volts:.2} V)."),
        Level::Low => format!("The battery {address} is low ({volts:.2} V). Charge it soon."),
        Level::Critical => format!(
            "The battery {address} is critical ({volts:.2} V). Stop and charge it now: going on can damage it."
        ),
    };
    match (&address.owner, level) {
        (Owner::App(app), Level::Ok) => tracing::info!(target: "core::notices", app = app.as_str(), "{text}"),
        (Owner::App(app), Level::Low) => {
            tracing::warn!(target: "core::notices", app = app.as_str(), "{text}")
        }
        (Owner::App(app), Level::Critical) => {
            tracing::error!(target: "core::notices", app = app.as_str(), "{text}")
        }
        (Owner::Part(p), Level::Ok) => tracing::info!(target: "core::notices", part = p.as_str(), "{text}"),
        (Owner::Part(p), Level::Low) => tracing::warn!(target: "core::notices", part = p.as_str(), "{text}"),
        (Owner::Part(p), Level::Critical) => {
            tracing::error!(target: "core::notices", part = p.as_str(), "{text}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use faber_capabilities::battery;
    use std::sync::Arc;
    use tracing_subscriber::layer::SubscriberExt;

    fn addr(s: &str) -> SignalAddress {
        SignalAddress::parse(s).unwrap()
    }

    fn reading(volts: f32, level: Level) -> Reading {
        Reading::Battery(battery::Reading::volts(1, volts, level))
    }

    #[test]
    fn a_notice_is_raised_when_a_level_changes_and_only_then() {
        let n = Notices::new(Arc::new(ManualClock::new(7)));
        let mut rx = n.subscribe();
        let a = addr("pack/battery");
        assert_eq!(n.observe(&a, &reading(12.0, Level::Ok)), None, "a first ok says nothing");
        assert_eq!(n.observe(&a, &reading(11.9, Level::Ok)), None);
        let low = n.observe(&a, &reading(10.4, Level::Low)).unwrap();
        assert_eq!(
            low,
            Notice::BatteryLevel {
                t_us: 7,
                address: a.clone(),
                level: Level::Low,
                previous: Some(Level::Ok),
                volts: 10.4
            }
        );
        assert_eq!(rx.try_recv().unwrap(), low, "every screen hears it");
        assert_eq!(n.observe(&a, &reading(10.3, Level::Low)), None);
        assert!(matches!(
            n.observe(&a, &reading(12.0, Level::Ok)),
            Some(Notice::BatteryLevel { level: Level::Ok, previous: Some(Level::Low), .. })
        ));
        let b = addr("other/battery");
        assert!(
            matches!(
                n.observe(&b, &reading(9.0, Level::Critical)),
                Some(Notice::BatteryLevel { previous: None, .. })
            ),
            "a battery that is critical from the start is told about"
        );
        let scan =
            Reading::LaserScan(faber_capabilities::laser_scan::Reading::even_bins(1, 0.0, 0.1, 0.1, vec![]));
        assert_eq!(n.observe(&a, &scan), None);
    }

    /// Every event as (level, its `part` or `app` field, message).
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<(tracing::Level, String, String)>>>);

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
        fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
            struct Fields(String, String);
            impl tracing::field::Visit for Fields {
                fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
                    match f.name() {
                        "message" => self.1 = format!("{v:?}"),
                        name => self.0 = format!("{name}:{}", format!("{v:?}").trim_matches('"')),
                    }
                }
            }
            let mut f = Fields(String::new(), String::new());
            event.record(&mut f);
            self.0.lock().unwrap().push((*event.metadata().level(), f.0, f.1));
        }
    }

    #[test]
    fn every_notice_is_logged_by_its_part_or_app() {
        let n = Notices::new(Arc::new(ManualClock::new(0)));
        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(capture.clone());
        tracing::subscriber::with_default(subscriber, || {
            n.observe(&addr("pack/battery"), &reading(10.0, Level::Low));
            n.observe(&addr("pack/battery"), &reading(9.0, Level::Critical));
            n.observe(&addr("app:monitor/battery"), &reading(10.0, Level::Low));
        });
        let seen = capture.0.lock().unwrap().clone();
        let who: Vec<_> = seen.iter().map(|(l, w, _)| (*l, w.as_str())).collect();
        assert_eq!(
            who,
            [
                (tracing::Level::WARN, "part:pack"),
                (tracing::Level::ERROR, "part:pack"),
                (tracing::Level::WARN, "app:monitor")
            ]
        );
        assert!(seen[0].2.contains("is low (10.00 V)"), "{seen:?}");
    }
}
