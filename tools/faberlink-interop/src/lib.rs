//! A fake FaberLink board for tests on the PC: the Arduino library's real protocol core
//! (`firmware/faberlink/src/FaberLinkCore.cpp`), compiled by `build.rs`, hosting a simulated motor
//! (part 0, `left`, max 10 rad/s) and a simulated distance sensor (part 1, `front`, 0.1–0.8 m).
//!
//! The tests in `tests/` drive it from the Rust session in `faberlink`.

/// The C ABI of `harness/harness.cpp`. This module is the only `unsafe` code in the crate.
#[allow(unsafe_code)]
mod ffi {
    use std::ffi::c_void;

    unsafe extern "C" {
        pub fn flh_new(heartbeat_timeout_ms: u16) -> *mut c_void;
        pub fn flh_free(h: *mut c_void);
        pub fn flh_feed(h: *mut c_void, data: *const u8, len: usize, now_ms: u32);
        pub fn flh_tick(h: *mut c_void, now_ms: u32);
        pub fn flh_take(h: *mut c_void, buf: *mut u8, cap: usize) -> i64;
        pub fn flh_motor_output(h: *mut c_void) -> f32;
        pub fn flh_motor_drive_calls(h: *mut c_void) -> i32;
        pub fn flh_set_distance(h: *mut c_void, meters: f32);
        pub fn flh_hardware_estop(h: *mut c_void);
        pub fn flh_reset_hardware_estop(h: *mut c_void);
        pub fn flh_heartbeat_timeout(h: *mut c_void) -> u16;
        pub fn flh_estop_engaged(h: *mut c_void) -> i32;
        pub fn flh_speak_versions(h: *mut c_void, lo: u8, hi: u8);
        pub fn flh_sizeof_board() -> usize;
        pub fn flh_max_payload() -> usize;
    }

    /// Owns one harness. Every call passes the pointer `flh_new` returned, which stays valid until
    /// `Drop`; the harness copies what it is given and keeps no pointer into Rust memory.
    pub struct Handle(*mut c_void);

    impl Handle {
        pub fn new(heartbeat_timeout_ms: u16) -> Handle {
            // SAFETY: flh_new allocates a harness and returns a valid pointer to it.
            Handle(unsafe { flh_new(heartbeat_timeout_ms) })
        }

        pub fn feed(&mut self, bytes: &[u8], now_ms: u32) {
            // SAFETY: the pointer and length describe `bytes`, which outlives the call.
            unsafe { flh_feed(self.0, bytes.as_ptr(), bytes.len(), now_ms) }
        }

        pub fn tick(&mut self, now_ms: u32) {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_tick(self.0, now_ms) }
        }

        pub fn take(&mut self) -> Vec<u8> {
            let mut buf = vec![0u8; 16384];
            // SAFETY: buf is writable for buf.len() bytes during the call.
            let n = unsafe { flh_take(self.0, buf.as_mut_ptr(), buf.len()) };
            assert!(n >= 0, "the fake board wrote more than its 16 KB output buffer");
            buf.truncate(n as usize);
            buf
        }

        pub fn motor_output(&self) -> f32 {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_motor_output(self.0) }
        }

        pub fn motor_drive_calls(&self) -> i32 {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_motor_drive_calls(self.0) }
        }

        pub fn set_distance(&mut self, meters: f32) {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_set_distance(self.0, meters) }
        }

        pub fn hardware_estop(&mut self) {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_hardware_estop(self.0) }
        }

        pub fn reset_hardware_estop(&mut self) {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_reset_hardware_estop(self.0) }
        }

        pub fn heartbeat_timeout(&self) -> u16 {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_heartbeat_timeout(self.0) }
        }

        pub fn speak_versions(&mut self, lo: u8, hi: u8) {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_speak_versions(self.0, lo, hi) }
        }

        pub fn estop_engaged(&self) -> bool {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_estop_engaged(self.0) != 0 }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: self.0 came from flh_new and is freed exactly once.
            unsafe { flh_free(self.0) }
        }
    }

    pub fn sizeof_board() -> usize {
        // SAFETY: a pure function.
        unsafe { flh_sizeof_board() }
    }

    pub fn max_payload() -> usize {
        // SAFETY: a pure function.
        unsafe { flh_max_payload() }
    }
}

/// The C++ board with its simulated parts. Time is a fake `millis()` passed to every call.
pub struct FakeBoard {
    handle: ffi::Handle,
}

impl FakeBoard {
    /// A board with this heartbeat timeout (the library clamps it to 100..1000 ms).
    pub fn new(heartbeat_timeout_ms: u16) -> FakeBoard {
        FakeBoard { handle: ffi::Handle::new(heartbeat_timeout_ms) }
    }

    /// Bytes from the core.
    pub fn feed(&mut self, bytes: &[u8], now_ms: u32) {
        self.handle.feed(bytes, now_ms);
    }

    /// The library's `poll()` without the serial port: safety checks, status and readings.
    pub fn tick(&mut self, now_ms: u32) {
        self.handle.tick(now_ms);
    }

    /// Everything the board has written since the last call.
    pub fn take_output(&mut self) -> Vec<u8> {
        self.handle.take()
    }

    /// What the simulated motor is driven at now (-1..1).
    pub fn motor_output(&self) -> f32 {
        self.handle.motor_output()
    }

    /// How often the library has driven the motor's hardware.
    pub fn motor_drive_calls(&self) -> i32 {
        self.handle.motor_drive_calls()
    }

    pub fn set_distance(&mut self, meters: f32) {
        self.handle.set_distance(meters);
    }

    /// Presses the board's hardware stop button.
    pub fn press_estop(&mut self) {
        self.handle.hardware_estop();
    }

    /// Resets the hardware stop on the board.
    pub fn reset_estop(&mut self) {
        self.handle.reset_hardware_estop();
    }

    pub fn heartbeat_timeout_ms(&self) -> u16 {
        self.handle.heartbeat_timeout()
    }

    pub fn estop_engaged(&self) -> bool {
        self.handle.estop_engaged()
    }

    /// Makes the board answer as if its library spoke only protocol versions `lo..=hi`.
    pub fn speak_versions(&mut self, lo: u8, hi: u8) {
        self.handle.speak_versions(lo, hi);
    }

    /// `sizeof(faberlink::Board)` as compiled on this machine.
    pub fn sizeof_board() -> usize {
        ffi::sizeof_board()
    }

    /// The board's FABERLINK_MAX_PAYLOAD as compiled on this machine.
    pub fn max_payload() -> usize {
        ffi::max_payload()
    }
}
