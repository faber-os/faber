//! A fake FaberLink board for tests on the PC: the Arduino library's real protocol core
//! (`firmware/faberlink/src/FaberLinkCore.cpp`), compiled by `build.rs`, hosting a simulated motor
//! (part 0, `left`, max 10 rad/s) and a simulated distance sensor (part 1, `front`, 0.1–0.8 m).
//! [`FakeBoard::mega`] hosts two of each: motors `left` and `right` (parts 0 and 1), distance
//! sensors `front` and `rear` (parts 2 and 3), the way a robot's Mega would. [`FakeBoard::sennep`]
//! is SENNEP V2's Mega: a planar base, two distance sensors and a battery.
//!
//! The tests in `tests/` drive it from the Rust session in `faberlink`; the core's own tests use
//! it as the board behind a fake serial port.
//!
//! [`Sketch`] is a whole sketch instead: the library's Arduino glue (`FaberLink.cpp`: `FaberMotor`,
//! `FaberDistance`, …) on a stand-in Arduino core (`harness/arduino/`), so a test can see what the
//! sketch set each pin to.

/// The C ABI of `harness/harness.cpp` and `harness/glue.cpp`. This module is the only `unsafe`
/// code in the crate.
#[allow(unsafe_code)]
mod ffi {
    use std::ffi::c_void;

    unsafe extern "C" {
        pub fn flh_new(heartbeat_timeout_ms: u16) -> *mut c_void;
        pub fn flh_new_mega(heartbeat_timeout_ms: u16, motor_lease_ms: u16) -> *mut c_void;
        pub fn flh_new_sennep(heartbeat_timeout_ms: u16, base_lease_ms: u16) -> *mut c_void;
        pub fn flh_base_velocity(h: *mut c_void, out: *mut f32);
        pub fn flh_new_with_battery(
            heartbeat_timeout_ms: u16,
            chemistry: u8,
            cells: u8,
            capacity_ah: f32,
        ) -> *mut c_void;
        pub fn flh_set_battery_volts(h: *mut c_void, volts: f32);
        pub fn flh_battery_level_at(h: *mut c_void, volts: f32, previous: u8) -> u8;
        pub fn flh_motor_output_at(h: *mut c_void, i: i32) -> f32;
        pub fn flh_set_distance_at(h: *mut c_void, i: i32, meters: f32);
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
        pub fn flh_set_build_id(h: *mut c_void, id: u64);
        pub fn flh_sizeof_board() -> usize;
        pub fn flh_max_payload() -> usize;

        pub fn fla_new(parts: *const PartSpec, n: usize) -> *mut c_void;
        pub fn fla_free(h: *mut c_void);
        pub fn fla_feed(h: *mut c_void, data: *const u8, len: usize, now_ms: u32);
        pub fn fla_tick(h: *mut c_void, now_ms: u32);
        pub fn fla_take(h: *mut c_void, buf: *mut u8, cap: usize) -> i64;
        pub fn fla_baud(h: *mut c_void) -> u32;
        pub fn fla_pin(h: *mut c_void, pin: u8, mode: *mut u8, kind: *mut u8, value: *mut i32);
        pub fn fla_log_len(h: *mut c_void) -> usize;
        pub fn fla_log_at(h: *mut c_void, i: usize, pin: *mut u8, kind: *mut u8, value: *mut i32);
        pub fn fla_clear_log(h: *mut c_void);
        pub fn fla_set_analog(h: *mut c_void, pin: u8, value: i32);
        pub fn fla_set_echo_us(h: *mut c_void, us: u32);
        pub fn fla_set_digital(h: *mut c_void, pin: u8, level: i32);
    }

    /// `PartSpec` in glue.cpp.
    #[repr(C)]
    pub struct PartSpec {
        pub kind: u8,
        pub name: *const std::ffi::c_char,
        pub driver: u8,
        pub pins: [u8; 4],
        pub max_velocity: f32,
    }

    /// Owns one sketch from `fla_new`, valid until `Drop`. The sketch copies the names it is
    /// given and keeps no pointer into Rust memory.
    pub struct SketchHandle(*mut c_void);

    // SAFETY: the sketch is plain C++ data; `&mut self` makes every call exclusive, and each call
    // first points the stand-in Arduino core at this sketch.
    unsafe impl Send for SketchHandle {}

    impl SketchHandle {
        pub fn new(parts: &[PartSpec]) -> SketchHandle {
            // SAFETY: the pointer and length describe `parts`, whose names are NUL-terminated
            // strings that outlive the call; fla_new copies them.
            SketchHandle(unsafe { fla_new(parts.as_ptr(), parts.len()) })
        }

        pub fn feed(&mut self, bytes: &[u8], now_ms: u32) {
            // SAFETY: the pointer and length describe `bytes`, which outlives the call.
            unsafe { fla_feed(self.0, bytes.as_ptr(), bytes.len(), now_ms) }
        }

        pub fn tick(&mut self, now_ms: u32) {
            // SAFETY: self.0 is a live sketch.
            unsafe { fla_tick(self.0, now_ms) }
        }

        pub fn take(&mut self) -> Vec<u8> {
            let mut buf = vec![0u8; 16384];
            // SAFETY: buf is writable for buf.len() bytes during the call.
            let n = unsafe { fla_take(self.0, buf.as_mut_ptr(), buf.len()) };
            assert!(n >= 0, "the sketch wrote more than 16 KB between two takes");
            buf.truncate(n as usize);
            buf
        }

        pub fn baud(&mut self) -> u32 {
            // SAFETY: self.0 is a live sketch.
            unsafe { fla_baud(self.0) }
        }

        pub fn pin(&mut self, pin: u8) -> (u8, u8, i32) {
            let (mut mode, mut kind, mut value) = (0u8, 0u8, 0i32);
            // SAFETY: self.0 is a live sketch; the pointers are to locals; every u8 is a pin.
            unsafe { fla_pin(self.0, pin, &mut mode, &mut kind, &mut value) };
            (mode, kind, value)
        }

        pub fn log(&mut self) -> Vec<(u8, u8, i32)> {
            // SAFETY: self.0 is a live sketch.
            let n = unsafe { fla_log_len(self.0) };
            (0..n)
                .map(|i| {
                    let (mut pin, mut kind, mut value) = (0u8, 0u8, 0i32);
                    // SAFETY: i is below the log's length, and the pointers are to locals.
                    unsafe { fla_log_at(self.0, i, &mut pin, &mut kind, &mut value) };
                    (pin, kind, value)
                })
                .collect()
        }

        pub fn clear_log(&mut self) {
            // SAFETY: self.0 is a live sketch.
            unsafe { fla_clear_log(self.0) }
        }

        pub fn set_analog(&mut self, pin: u8, value: i32) {
            // SAFETY: self.0 is a live sketch; every u8 is a pin.
            unsafe { fla_set_analog(self.0, pin, value) }
        }

        pub fn set_echo_us(&mut self, us: u32) {
            // SAFETY: self.0 is a live sketch.
            unsafe { fla_set_echo_us(self.0, us) }
        }

        pub fn set_digital(&mut self, pin: u8, level: i32) {
            // SAFETY: self.0 is a live sketch.
            unsafe { fla_set_digital(self.0, pin, level) }
        }
    }

    impl Drop for SketchHandle {
        fn drop(&mut self) {
            // SAFETY: self.0 came from fla_new and is freed exactly once.
            unsafe { fla_free(self.0) }
        }
    }

    /// Owns one harness. Every call passes the pointer `flh_new` returned, which stays valid until
    /// `Drop`; the harness copies what it is given and keeps no pointer into Rust memory.
    pub struct Handle(*mut c_void);

    // SAFETY: the harness is plain C++ data with no thread affinity, and `&mut self` (or the
    // owner's own lock) makes every call exclusive, so it may move to another thread.
    unsafe impl Send for Handle {}

    impl Handle {
        pub fn new(heartbeat_timeout_ms: u16) -> Handle {
            // SAFETY: flh_new allocates a harness and returns a valid pointer to it.
            Handle(unsafe { flh_new(heartbeat_timeout_ms) })
        }

        pub fn new_mega(heartbeat_timeout_ms: u16, motor_lease_ms: u16) -> Handle {
            // SAFETY: flh_new_mega allocates a harness and returns a valid pointer to it.
            Handle(unsafe { flh_new_mega(heartbeat_timeout_ms, motor_lease_ms) })
        }

        pub fn new_sennep(heartbeat_timeout_ms: u16, base_lease_ms: u16) -> Handle {
            // SAFETY: flh_new_sennep allocates a harness and returns a valid pointer to it.
            Handle(unsafe { flh_new_sennep(heartbeat_timeout_ms, base_lease_ms) })
        }

        pub fn base_velocity(&self) -> [f32; 3] {
            let mut out = [0.0f32; 3];
            // SAFETY: self.0 is a live harness, and `out` has room for the three floats it writes.
            unsafe { flh_base_velocity(self.0, out.as_mut_ptr()) };
            out
        }

        pub fn new_with_battery(
            heartbeat_timeout_ms: u16,
            chemistry: u8,
            cells: u8,
            capacity_ah: f32,
        ) -> Handle {
            // SAFETY: flh_new_with_battery allocates a harness and returns a valid pointer to it;
            // an unknown chemistry is treated as Li-ion by the library.
            Handle(unsafe { flh_new_with_battery(heartbeat_timeout_ms, chemistry, cells, capacity_ah) })
        }

        pub fn set_battery_volts(&mut self, volts: f32) {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_set_battery_volts(self.0, volts) }
        }

        pub fn battery_level_at(&self, volts: f32, previous: u8) -> u8 {
            // SAFETY: self.0 is a live harness; any byte is a valid previous level to compare.
            unsafe { flh_battery_level_at(self.0, volts, previous) }
        }

        pub fn motor_output_at(&self, i: i32) -> f32 {
            // SAFETY: self.0 is a live harness; the harness picks motor 0 for anything but 1.
            unsafe { flh_motor_output_at(self.0, i) }
        }

        pub fn set_distance_at(&mut self, i: i32, meters: f32) {
            // SAFETY: self.0 is a live harness; the harness picks sensor 0 for anything but 1.
            unsafe { flh_set_distance_at(self.0, i, meters) }
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

        pub fn set_build_id(&mut self, id: u64) {
            // SAFETY: self.0 is a live harness.
            unsafe { flh_set_build_id(self.0, id) }
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

/// A motor on [`FakeBoard::mega`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motor {
    Left = 0,
    Right = 1,
}

/// A distance sensor on [`FakeBoard::mega`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sensor {
    Front = 0,
    Rear = 1,
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

    /// A board called `sim-mega` with motors `left` and `right` and distance sensors `front` and
    /// `rear`. Its motors stop by themselves after 250 ms without a command, as every motor does.
    pub fn mega(heartbeat_timeout_ms: u16) -> FakeBoard {
        FakeBoard::mega_with_motor_lease(heartbeat_timeout_ms, 250)
    }

    /// [`FakeBoard::mega`] with another lease time for its motors, so a test can tell the core's
    /// SAFE from the board's own lease running out.
    pub fn mega_with_motor_lease(heartbeat_timeout_ms: u16, motor_lease_ms: u16) -> FakeBoard {
        FakeBoard { handle: ffi::Handle::new_mega(heartbeat_timeout_ms, motor_lease_ms) }
    }

    /// SENNEP V2's Mega as it will be (`sim-sennep-mega`): a holonomic planar base `base` (part 0,
    /// at most 0.6 m/s and 1.5 rad/s), distance sensors `front` and `rear` (parts 1 and 2) and a 3S
    /// Li-ion `battery` (part 3). Its base stops by itself after `base_lease_ms` without a command.
    /// The library has no planar-base part type yet: the harness writes the layouts itself.
    pub fn sennep(heartbeat_timeout_ms: u16, base_lease_ms: u16) -> FakeBoard {
        FakeBoard { handle: ffi::Handle::new_sennep(heartbeat_timeout_ms, base_lease_ms) }
    }

    /// The velocity a [`FakeBoard::sennep`]'s base is driven at now: `[vx, vy, omega]`.
    pub fn base_velocity(&self) -> [f32; 3] {
        self.handle.base_velocity()
    }

    /// The small board plus a battery, `battery` (part 2), reporting every 100 ms. `chemistry` is
    /// its code in the FaberLink info layout (0 li-ion, 1 lipo, 2 lifepo4, 3 nimh, 4 lead-acid),
    /// `capacity_ah` 0 for unknown.
    pub fn with_battery(heartbeat_timeout_ms: u16, chemistry: u8, cells: u8, capacity_ah: f32) -> FakeBoard {
        FakeBoard {
            handle: ffi::Handle::new_with_battery(heartbeat_timeout_ms, chemistry, cells, capacity_ah),
        }
    }

    /// Sets the voltage the board's battery measures.
    pub fn set_battery_volts(&mut self, volts: f32) {
        self.handle.set_battery_volts(volts);
    }

    /// The level the C++ library gives `volts`, coming from `previous`, both as their codes in the
    /// reading layout (0 ok, 1 low, 2 critical).
    pub fn battery_level_at(&self, volts: f32, previous: u8) -> u8 {
        self.handle.battery_level_at(volts, previous)
    }

    /// What one motor of a [`FakeBoard::mega`] is driven at now: [`Motor::Left`] or
    /// [`Motor::Right`].
    pub fn output_of(&self, motor: Motor) -> f32 {
        self.handle.motor_output_at(motor as i32)
    }

    /// Sets what one distance sensor of a [`FakeBoard::mega`] measures.
    pub fn set_distance_of(&mut self, sensor: Sensor, meters: f32) {
        self.handle.set_distance_at(sensor as i32, meters);
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

    /// Makes the board carry this build id, as a sketch compiled by Faber's Arduino editor does
    /// (`-DFABER_BUILD_ID=…`).
    pub fn set_build_id(&mut self, id: u64) {
        self.handle.set_build_id(id);
    }

    /// Builder form of [`FakeBoard::set_build_id`].
    pub fn with_build_id(mut self, id: u64) -> FakeBoard {
        self.set_build_id(id);
        self
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

/// A part in a [`Sketch`]: the declaration a sketch would have.
#[derive(Clone, Debug, PartialEq)]
pub enum SketchPart {
    /// `FaberMotor name(name, pwm, dir, max_velocity)`: the PWM + DIR constructor every sketch
    /// used before driver shapes.
    Motor { name: &'static str, pwm: u8, dir: u8, max_velocity: f32 },
    /// `FaberMotor name(name, driver, pins[0], pins[1], pins[2], pins[3])`, with [`NO_PIN`] for a
    /// pin left out. A `max_velocity` above 0 is set with `setMaxVelocity()`.
    DriverMotor { name: &'static str, driver: Driver, pins: [u8; 4], max_velocity: f32 },
    /// `FaberDistance name(name, pin)`.
    Distance { name: &'static str, pin: u8 },
    /// `FaberUltrasonic name(name, trig, echo)`.
    Ultrasonic { name: &'static str, trig: u8, echo: u8 },
    /// `FaberBattery name(name, pin, 3.0, 3)`.
    Battery { name: &'static str, pin: u8 },
    /// `FaberButton name(name, pin)`, or with `faberlink::ACTIVE_HIGH`.
    Button { name: &'static str, pin: u8, active_high: bool },
    /// `FaberObstacle name(name, pin, kind, active, trigger)`; `kind` is `faberlink::ObstacleKind`.
    Obstacle { name: &'static str, pin: u8, kind: u8, active_high: bool, trigger_m: f32 },
}

impl SketchPart {
    fn name(&self) -> &'static str {
        match self {
            SketchPart::Motor { name, .. }
            | SketchPart::DriverMotor { name, .. }
            | SketchPart::Distance { name, .. }
            | SketchPart::Ultrasonic { name, .. }
            | SketchPart::Battery { name, .. }
            | SketchPart::Button { name, .. }
            | SketchPart::Obstacle { name, .. } => name,
        }
    }
}

/// `faberlink::MotorDriver`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Driver {
    PwmDir = 0,
    EnIn1In2 = 1,
    TwoPwm = 2,
}

/// `faberlink::NO_PIN`: a pin that isn't wired.
pub const NO_PIN: u8 = 0xFF;

/// What the sketch last did to a pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinOut {
    /// Never written.
    Unset,
    /// `digitalWrite`: HIGH is true.
    Digital(bool),
    /// `analogWrite`, 0..255.
    Pwm(u8),
}

impl PinOut {
    fn from(kind: u8, value: i32) -> PinOut {
        match kind {
            1 => PinOut::Digital(value != 0),
            2 => PinOut::Pwm(value.clamp(0, 255) as u8),
            _ => PinOut::Unset,
        }
    }

    /// The level a driver sees: HIGH for a digital HIGH or any PWM above 0.
    pub fn is_high(self) -> bool {
        matches!(self, PinOut::Digital(true)) || matches!(self, PinOut::Pwm(v) if v > 0)
    }
}

/// `pinMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinMode {
    Unset,
    Input,
    Output,
    InputPullup,
}

/// A whole sketch: `FaberLink link(Serial)` and `parts` declared as globals, after
/// `link.begin(115200, "sketch", "0.1.0")`, on a stand-in Arduino core whose pins are numbered as
/// on an Uno (A0 is pin 14). Time is a fake `millis()` passed to every call.
pub struct Sketch {
    handle: ffi::SketchHandle,
}

impl Sketch {
    pub fn new(parts: &[SketchPart]) -> Sketch {
        let names: Vec<std::ffi::CString> =
            parts.iter().map(|p| std::ffi::CString::new(p.name()).expect("a name without NUL")).collect();
        let specs: Vec<ffi::PartSpec> = parts
            .iter()
            .zip(&names)
            .map(|(p, name)| {
                let (kind, driver, pins, max_velocity) = match *p {
                    SketchPart::Motor { pwm, dir, max_velocity, .. } => {
                        (0, 0, [pwm, dir, NO_PIN, NO_PIN], max_velocity)
                    }
                    SketchPart::DriverMotor { driver, pins, max_velocity, .. } => {
                        (1, driver as u8, pins, max_velocity)
                    }
                    SketchPart::Distance { pin, .. } => (2, 0, [pin, NO_PIN, NO_PIN, NO_PIN], 0.0),
                    SketchPart::Ultrasonic { trig, echo, .. } => (3, 0, [trig, echo, NO_PIN, NO_PIN], 0.0),
                    SketchPart::Battery { pin, .. } => (4, 0, [pin, NO_PIN, NO_PIN, NO_PIN], 0.0),
                    SketchPart::Button { pin, active_high, .. } => {
                        (5, active_high as u8, [pin, NO_PIN, NO_PIN, NO_PIN], 0.0)
                    }
                    SketchPart::Obstacle { pin, kind, active_high, trigger_m, .. } => {
                        (6, active_high as u8, [pin, kind, NO_PIN, NO_PIN], trigger_m)
                    }
                };
                ffi::PartSpec { kind, name: name.as_ptr(), driver, pins, max_velocity }
            })
            .collect();
        Sketch { handle: ffi::SketchHandle::new(&specs) }
    }

    /// Bytes from the core, then one `link.poll()` at `now_ms`.
    pub fn feed(&mut self, bytes: &[u8], now_ms: u32) {
        self.handle.feed(bytes, now_ms);
    }

    /// One `link.poll()` at `now_ms`.
    pub fn tick(&mut self, now_ms: u32) {
        self.handle.tick(now_ms);
    }

    /// Everything the sketch has written to the serial port since the last call.
    pub fn take_output(&mut self) -> Vec<u8> {
        self.handle.take()
    }

    /// The baud rate `link.begin()` started the serial port at.
    pub fn baud(&mut self) -> u32 {
        self.handle.baud()
    }

    /// What the sketch last wrote to `pin`.
    pub fn pin(&mut self, pin: u8) -> PinOut {
        let (_, kind, value) = self.handle.pin(pin);
        PinOut::from(kind, value)
    }

    pub fn pin_mode(&mut self, pin: u8) -> PinMode {
        match self.handle.pin(pin).0 {
            0 => PinMode::Input,
            1 => PinMode::Output,
            2 => PinMode::InputPullup,
            _ => PinMode::Unset,
        }
    }

    /// Every pin write since the last [`Sketch::clear_writes`], in order.
    pub fn writes(&mut self) -> Vec<(u8, PinOut)> {
        self.handle.log().into_iter().map(|(pin, kind, value)| (pin, PinOut::from(kind, value))).collect()
    }

    pub fn clear_writes(&mut self) {
        self.handle.clear_log();
    }

    /// What `analogRead(pin)` returns from now on (0..1023).
    pub fn set_analog(&mut self, pin: u8, value: i32) {
        self.handle.set_analog(pin, value);
    }

    /// What drives input `pin` from outside from now on: `Some(true)` HIGH, `Some(false)` LOW,
    /// `None` nothing (its pull-up, if on, holds it HIGH).
    pub fn set_input(&mut self, pin: u8, level: Option<bool>) {
        self.handle.set_digital(pin, level.map_or(-1, i32::from));
    }

    /// What `pulseIn()` measures from now on (0: no echo).
    pub fn set_echo_us(&mut self, us: u32) {
        self.handle.set_echo_us(us);
    }
}
