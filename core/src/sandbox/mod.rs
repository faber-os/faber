//! Layer 2: the WebAssembly sandbox that wasm connectors (`wasm`) and app logic (`apps`) both
//! run in (docs/packages.md §3–4, docs/apps.md §4, D07, D09).
//!
//! One Wasmtime engine with epoch interruption, the limits every instance runs under, and the
//! WIT bindings and type conversions both hosts share. What each kind of component may call is
//! the host's business: this module only says which imports a component asks for that it isn't
//! allowed ([`forbidden_imports`]).

pub mod convert;

use std::sync::OnceLock;
use std::time::{Duration, Instant};
use wasmtime::component::Component;
use wasmtime::{Config, Engine, ResourceLimiter};

/// The connector world (`wit/connector`), and the capability types it uses.
#[allow(clippy::all)]
pub mod bindings {
    wasmtime::component::bindgen!({
        path: ["../wit/capabilities", "../wit/connector"],
        world: "faber:connector/connector@0.1.0",
    });
}

/// The app world (`wit/app`). Readings and commands are the connector world's own types, so a
/// reading crosses into an app exactly as a connector published it.
#[allow(clippy::all)]
pub mod app_bindings {
    wasmtime::component::bindgen!({
        path: ["../wit/capabilities", "../wit/connector", "../wit/app"],
        world: "faber:app/app@0.1.0",
        with: {
            "faber:capabilities/types@0.1.0": super::bindings::faber::capabilities::types,
            "faber:capabilities/laser-scan@0.1.0": super::bindings::faber::capabilities::laser_scan,
            "faber:capabilities/distance@0.1.0": super::bindings::faber::capabilities::distance,
            "faber:capabilities/motor@0.1.0": super::bindings::faber::capabilities::motor,
            "faber:capabilities/planar-base@0.1.0": super::bindings::faber::capabilities::planar_base,
            "faber:capabilities/estop@0.1.0": super::bindings::faber::capabilities::estop,
            "faber:capabilities/pose@0.1.0": super::bindings::faber::capabilities::pose,
            "faber:capabilities/position@0.1.0": super::bindings::faber::capabilities::position,
            "faber:connector/types@0.1.0": super::bindings::faber::connector::types,
        },
    });
}

/// What one instance may use (docs/packages.md §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Linear memory, per instance.
    pub memory_bytes: usize,
    /// Wall time per call into the component.
    pub call_time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { memory_bytes: 32 << 20, call_time: Duration::from_millis(20) }
    }
}

/// How often the engine's epoch advances; a call's deadline is counted in these.
pub const EPOCH_TICK: Duration = Duration::from_millis(5);

/// The prefix of the error log line a guest SDK writes just before it traps on a panic
/// (`faber_connector_wasm::PANIC_PREFIX`, `faber_app_wasm::PANIC_PREFIX`).
pub const PANIC_PREFIX: &str = "panicked: ";

/// Log lines one instance may write per second; the rest are dropped and counted.
pub const LOG_LINES_PER_S: u32 = 50;
/// The longest log line kept; longer ones are cut.
pub const LOG_LINE_MAX: usize = 2000;

/// The one engine every component runs on, with the thread that advances its epoch.
pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).expect("the wasm engine's configuration is valid");
        let ticking = engine.clone();
        std::thread::Builder::new()
            .name("wasm-epoch".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(EPOCH_TICK);
                    ticking.increment_epoch();
                }
            })
            .expect("spawn the wasm epoch thread");
        engine
    })
}

/// Epoch ticks for a call that may take `limit`: never less than `limit`, at most a tick more.
pub fn ticks(limit: Duration) -> u64 {
    (limit.as_micros().div_ceil(EPOCH_TICK.as_micros()) + 1) as u64
}

/// Keeps one instance under its memory limit, and remembers hitting it.
#[derive(Debug)]
pub struct MemoryLimit {
    pub max: usize,
    pub hit: bool,
}

impl MemoryLimit {
    pub fn new(max: usize) -> Self {
        MemoryLimit { max, hit: false }
    }
}

impl ResourceLimiter for MemoryLimit {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.max {
            self.hit = true;
            return Ok(false);
        }
        Ok(true)
    }

    fn table_growing(&mut self, _current: usize, desired: usize, _: Option<usize>) -> wasmtime::Result<bool> {
        Ok(desired <= 100_000)
    }
}

/// How many lines one instance logged this second.
#[derive(Debug)]
pub struct LogLimit {
    start: Instant,
    lines: u32,
    dropped: u32,
}

impl Default for LogLimit {
    fn default() -> Self {
        LogLimit { start: Instant::now(), lines: 0, dropped: 0 }
    }
}

impl LogLimit {
    /// The line to log, cut to [`LOG_LINE_MAX`], or `None` when this second's lines are used up.
    /// `id` names the instance in the note about dropped lines.
    pub fn admit(&mut self, id: &str, message: String) -> Option<String> {
        if self.start.elapsed() >= Duration::from_secs(1) {
            if self.dropped > 0 {
                tracing::warn!(target: "core::wasm", "{id}: {} log lines dropped (too many)", self.dropped);
            }
            *self = LogLimit::default();
        }
        if self.lines >= LOG_LINES_PER_S {
            self.dropped += 1;
            return None;
        }
        self.lines += 1;
        let mut text = message;
        if text.len() > LOG_LINE_MAX {
            let cut = (0..=LOG_LINE_MAX).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
            text.truncate(cut);
            text.push('…');
        }
        Some(text)
    }
}

/// Everything a component imports that it may not: anything outside `faber:`, and any function
/// outside the interfaces in `allowed` (the capability interfaces it takes types from also
/// declare commands, which no component calls directly).
pub fn forbidden_imports(engine: &Engine, component: &Component, allowed: &[&str]) -> Vec<String> {
    use wasmtime::component::types::ComponentItem;
    let mut out = Vec::new();
    for (name, item) in component.component_type().imports(engine) {
        if !name.starts_with("faber:") {
            out.push(name.to_string());
            continue;
        }
        match item.ty {
            ComponentItem::ComponentInstance(instance) if !allowed.contains(&name) => {
                for (func, kind) in instance.exports(engine) {
                    if matches!(kind.ty, ComponentItem::ComponentFunc(_)) {
                        out.push(format!("{name}#{func}"));
                    }
                }
            }
            ComponentItem::ComponentInstance(_) | ComponentItem::Type(_) => {}
            _ => out.push(name.to_string()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_gets_at_least_its_time_and_at_most_a_tick_more() {
        let t = ticks(Duration::from_millis(20));
        assert!(Duration::from_millis(20) <= EPOCH_TICK * (t as u32 - 1));
        assert!(EPOCH_TICK * (t as u32 - 1) <= Duration::from_millis(20) + EPOCH_TICK);
    }

    #[test]
    fn the_default_limits_are_the_documented_ones() {
        assert_eq!(
            Limits::default(),
            Limits { memory_bytes: 32 * 1024 * 1024, call_time: Duration::from_millis(20) }
        );
    }

    #[test]
    fn logging_is_limited_per_second_and_long_lines_are_cut() {
        let mut limit = LogLimit::default();
        for _ in 0..LOG_LINES_PER_S {
            assert!(limit.admit("x", "line".into()).is_some());
        }
        assert_eq!(limit.admit("x", "one too many".into()), None);
        let mut limit = LogLimit::default();
        let long = limit.admit("x", "é".repeat(LOG_LINE_MAX)).unwrap();
        assert!(long.len() <= LOG_LINE_MAX + '…'.len_utf8() && long.ends_with('…'));
    }
}
