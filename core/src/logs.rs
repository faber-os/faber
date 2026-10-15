//! Layer 2: the log the shell shows. A `tracing` layer that keeps the recent entries and
//! broadcasts new ones. Parts log with a `part` field, so entries can say `part:lidar`.

use crate::clock::SharedClock;
use std::collections::VecDeque;
use std::fmt::Write;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;

const KEEP: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub t_us: u64,
    pub level: Level,
    /// `part:<name>` for part logs, otherwise the module, e.g. `core::ports`.
    pub source: String,
    pub message: String,
}

pub struct LogBook {
    clock: SharedClock,
    recent: Mutex<VecDeque<Entry>>,
    tx: broadcast::Sender<Entry>,
}

impl LogBook {
    pub fn new(clock: SharedClock) -> Arc<LogBook> {
        Arc::new(LogBook {
            clock,
            recent: Mutex::new(VecDeque::with_capacity(KEEP)),
            tx: broadcast::channel(256).0,
        })
    }

    pub fn push(&self, level: Level, source: String, message: String) {
        let entry = Entry { t_us: self.clock.now_us(), level, source, message };
        let mut recent = self.recent.lock().expect("log lock");
        if recent.len() == KEEP {
            recent.pop_front();
        }
        recent.push_back(entry.clone());
        drop(recent);
        let _ = self.tx.send(entry);
    }

    /// The recent entries (oldest first) and a receiver for everything after them, taken
    /// together so nothing falls in between.
    pub fn follow(&self) -> (Vec<Entry>, broadcast::Receiver<Entry>) {
        let recent = self.recent.lock().expect("log lock");
        let rx = self.tx.subscribe();
        (recent.iter().cloned().collect(), rx)
    }

    /// A `tracing` layer that writes into this log book.
    pub fn layer(self: &Arc<Self>) -> LogLayer {
        LogLayer(Arc::clone(self))
    }
}

pub struct LogLayer(Arc<LogBook>);

#[derive(Default)]
struct Fields {
    message: String,
    part: Option<String>,
    extra: String,
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => self.message = format!("{value:?}"),
            "part" => self.part = Some(format!("{value:?}")),
            name => {
                let _ = write!(self.extra, " {name}={value:?}");
            }
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message = value.to_string(),
            "part" => self.part = Some(value.to_string()),
            name => {
                let _ = write!(self.extra, " {name}={value}");
            }
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let level = match *meta.level() {
            tracing::Level::ERROR => Level::Error,
            tracing::Level::WARN => Level::Warn,
            tracing::Level::INFO => Level::Info,
            _ => Level::Debug,
        };
        let mut f = Fields::default();
        event.record(&mut f);
        let source = match f.part {
            Some(p) => format!("part:{p}"),
            None => meta.target().replace("faber_core", "core"),
        };
        self.0.push(level, source, format!("{}{}", f.message, f.extra));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn captures_events_with_part_and_target() {
        let book = LogBook::new(Arc::new(ManualClock::new(42)));
        let subscriber = tracing_subscriber::registry().with(book.layer());
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "core::ports", "could not list ports");
            tracing::error!(target: "core::runtime", part = "lidar", "connector crashed");
        });
        let (recent, _) = book.follow();
        assert_eq!(recent.len(), 2);
        assert_eq!(
            recent[0],
            Entry {
                t_us: 42,
                level: Level::Warn,
                source: "core::ports".into(),
                message: "could not list ports".into()
            }
        );
        assert_eq!(recent[1].source, "part:lidar");
        assert_eq!(recent[1].level, Level::Error);
    }

    #[test]
    fn keeps_only_the_most_recent_entries() {
        let book = LogBook::new(Arc::new(ManualClock::new(0)));
        for i in 0..KEEP + 5 {
            book.push(Level::Info, "t".into(), i.to_string());
        }
        let (recent, _) = book.follow();
        assert_eq!(recent.len(), KEEP);
        assert_eq!(recent[0].message, "5");
    }

    #[tokio::test]
    async fn followers_get_new_entries() {
        let book = LogBook::new(Arc::new(ManualClock::new(0)));
        let (_, mut rx) = book.follow();
        book.push(Level::Info, "t".into(), "hello".into());
        assert_eq!(rx.recv().await.unwrap().message, "hello");
    }
}
