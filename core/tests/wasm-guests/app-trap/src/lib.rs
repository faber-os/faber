//! An app that drives its base forward and traps on purpose, to test the app sandbox in
//! `core/tests/apps.rs`. The debug hook is its state: with `trap-after = "<n>"` saved, it panics
//! on its n-th tick (counting from its start); with `hog = "yes"` it asks for 64 MB on its first
//! tick instead.
//!
//! It is also the app that ignores the rules: it never reads its messages (so never hears
//! `drive-ended`) and takes the base's lease again on every tick it lost it. With the base
//! declared `drive = "window"`, it proves the core ends and refuses the lease on its own.

use faber_app_wasm::{App, Command, Host};
use faber_capabilities::{Twist2d, planar_base};

#[derive(Default)]
pub struct Trapper {
    ticks: u32,
    trap_after: Option<u32>,
    hog: bool,
}

impl App for Trapper {
    fn start(&mut self, host: &mut dyn Host, _now_us: u64) {
        self.trap_after = host.state_get("trap-after").and_then(|v| v.parse().ok());
        self.hog = host.state_get("hog").is_some();
        let _ = host.acquire("base", 0);
    }

    fn tick(&mut self, host: &mut dyn Host, _now_us: u64) {
        self.ticks += 1;
        if self.hog {
            let hog = vec![1u8; 64 << 20];
            std::hint::black_box(&hog);
        }
        if self.trap_after.is_some_and(|n| self.ticks >= n) {
            panic!("trap requested after {} ticks", self.ticks);
        }
        let go = Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(0.2, 0.0, 0.0)));
        if host.send("base", 0, go).is_err() && host.acquire("base", 0).is_ok() {
            let _ = host.send("base", 0, go);
        }
    }
}

#[cfg(target_arch = "wasm32")]
faber_app_wasm::export!(Trapper);
