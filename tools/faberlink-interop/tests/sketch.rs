//! The library's Arduino glue, pin by pin: whole sketches (`FaberMotor` in each driver shape,
//! `FaberDistance`, `FaberUltrasonic`, `FaberBattery`, `FaberButton`, `FaberObstacle`) on a
//! stand-in Arduino core, driven from the Rust session. What passes here is the glue that runs on the board; only `digitalWrite()` and
//! friends are stand-ins.

use faber_capabilities::{Command, Info, Reading, digital_out, motor, obstacle, pwm_out, servo};
use faberlink::message::{Applied, Cause, PartInfo, PartPins, PinRole, PinUse};
use faberlink::session::{Event, Session, SessionConfig, State};
use faberlink_interop::{Driver, NO_PIN, PinMode, PinOut, Sketch, SketchPart};

/// The core and a sketch, joined by an in-memory pipe, with a fake clock.
struct Link {
    session: Session,
    sketch: Sketch,
    now_ms: u32,
    events: Vec<Event>,
}

impl Link {
    /// A link to a sketch with `parts` that has finished the handshake.
    fn ready(parts: &[SketchPart]) -> Link {
        let mut l = Link {
            session: Session::new(SessionConfig::default(), 0),
            sketch: Sketch::new(parts),
            now_ms: 0,
            events: vec![],
        };
        l.run(20);
        assert_eq!(l.session.state(), State::Ready, "{:?}", l.events);
        l.events.clear();
        l
    }

    fn now_us(&self) -> u64 {
        self.now_ms as u64 * 1000
    }

    /// Runs both sides for `ms` milliseconds, one millisecond at a time.
    fn run(&mut self, ms: u32) {
        for _ in 0..ms {
            self.now_ms += 1;
            self.session.tick(self.now_us());
            self.flush();
            self.sketch.tick(self.now_ms);
            self.flush();
        }
    }

    fn flush(&mut self) {
        let out = self.session.take_output();
        self.sketch.feed(&out, self.now_ms);
        let back = self.sketch.take_output();
        let now = self.now_us();
        self.session.receive(&back, now);
        self.events.extend(self.session.take_events());
    }

    /// Sets motor `part` to `duty` and returns what the board says it did.
    fn duty(&mut self, part: u8, duty: f32) -> Applied {
        self.events.clear();
        self.session.command(part, Command::Motor(motor::Command::SetDuty { duty }), true).unwrap();
        self.flush();
        self.applied().expect("an APPLIED for every command")
    }

    fn applied(&self) -> Option<Applied> {
        self.events.iter().rev().find_map(|e| if let Event::Applied(a) = e { Some(*a) } else { None })
    }
}

fn pwm_dir() -> SketchPart {
    SketchPart::Motor { name: "left", pwm: 9, dir: 8, max_velocity: 0.0 }
}

fn en_in1_in2(stby: u8) -> SketchPart {
    SketchPart::DriverMotor {
        name: "left",
        driver: Driver::EnIn1In2,
        pins: [10, 8, 7, stby],
        max_velocity: 0.0,
    }
}

fn two_pwm(enable: u8) -> SketchPart {
    SketchPart::DriverMotor {
        name: "left",
        driver: Driver::TwoPwm,
        pins: [5, 6, enable, NO_PIN],
        max_velocity: 0.0,
    }
}

const HIGH: PinOut = PinOut::Digital(true);
const LOW: PinOut = PinOut::Digital(false);

/// The pins' outputs now, in the order given.
fn outputs(l: &mut Link, pins: &[u8]) -> Vec<PinOut> {
    pins.iter().map(|&p| l.sketch.pin(p)).collect()
}

#[test]
fn pwm_and_dir_the_constructor_every_older_sketch_uses() {
    let mut l = Link::ready(&[pwm_dir()]);
    assert_eq!((l.sketch.pin_mode(9), l.sketch.pin_mode(8)), (PinMode::Output, PinMode::Output));
    assert_eq!(outputs(&mut l, &[9, 8]), [PinOut::Pwm(0), LOW], "it starts stopped");
    l.duty(0, 0.5);
    assert_eq!(outputs(&mut l, &[9, 8]), [PinOut::Pwm(128), LOW]);
    l.duty(0, -0.5);
    assert_eq!(outputs(&mut l, &[9, 8]), [PinOut::Pwm(128), HIGH]);
    l.duty(0, -1.0);
    assert_eq!(outputs(&mut l, &[9, 8]), [PinOut::Pwm(255), HIGH]);
    l.session.safe(Some(0)).unwrap();
    l.flush();
    assert_eq!(l.sketch.pin(9), PinOut::Pwm(0));
}

#[test]
fn en_in1_in2_puts_the_speed_on_en_and_the_direction_on_in1_and_in2() {
    let mut l = Link::ready(&[en_in1_in2(NO_PIN)]);
    for p in [10, 8, 7] {
        assert_eq!(l.sketch.pin_mode(p), PinMode::Output, "pin {p}");
    }
    assert_eq!(outputs(&mut l, &[10, 8, 7]), [PinOut::Pwm(0), LOW, LOW], "it starts stopped");
    l.duty(0, 0.5);
    assert_eq!(outputs(&mut l, &[10, 8, 7]), [PinOut::Pwm(128), HIGH, LOW], "forward: IN1 high");
    l.duty(0, -0.25);
    assert_eq!(outputs(&mut l, &[10, 8, 7]), [PinOut::Pwm(64), LOW, HIGH], "reverse: IN2 high");
    l.duty(0, 0.0);
    // Stopped is all low: EN low lets an L298N's motor coast (its datasheet's "free running motor
    // stop"); EN high with IN1 = IN2 would brake it.
    assert_eq!(outputs(&mut l, &[10, 8, 7]), [PinOut::Pwm(0), LOW, LOW]);
}

#[test]
fn two_pwm_puts_the_speed_on_one_input_per_direction() {
    let mut l = Link::ready(&[two_pwm(NO_PIN)]);
    assert_eq!(outputs(&mut l, &[5, 6]), [PinOut::Pwm(0), PinOut::Pwm(0)], "it starts stopped");
    l.duty(0, 0.5);
    assert_eq!(outputs(&mut l, &[5, 6]), [PinOut::Pwm(128), PinOut::Pwm(0)]);
    l.duty(0, -1.0);
    assert_eq!(outputs(&mut l, &[5, 6]), [PinOut::Pwm(0), PinOut::Pwm(255)]);
    l.session.safe(None).unwrap();
    l.flush();
    assert_eq!(outputs(&mut l, &[5, 6]), [PinOut::Pwm(0), PinOut::Pwm(0)], "both low: a DRV8833 coasts");
}

#[test]
fn a_standby_or_enable_pin_is_held_high_even_while_the_motor_stops() {
    // TB6612 STBY (and DRV8833 nSLEEP) is shared by both of its motors: one motor stopping must
    // not stop the other.
    for part in [en_in1_in2(4), two_pwm(4)] {
        let mut l = Link::ready(std::slice::from_ref(&part));
        assert_eq!((l.sketch.pin_mode(4), l.sketch.pin(4)), (PinMode::Output, HIGH), "{part:?}");
        l.duty(0, 0.7);
        l.run(300); // no more commands: the motor's own lease stops it
        assert_eq!(l.applied().map(|a| a.cause), Some(Cause::LeaseExpired));
        assert_eq!(l.sketch.pin(4), HIGH, "{part:?}");
    }
}

/// Replays every pin write and fails if the two inputs were ever HIGH together: that brakes an
/// L298N or a DRV8833 on the way from one direction to the other.
/// The writes start from the motor stopped (both inputs low).
fn never_both_high(l: &mut Link, in1: u8, in2: u8) {
    let writes = l.sketch.writes();
    let (mut a, mut b) = (PinOut::Unset, PinOut::Unset);
    for (i, &(pin, out)) in writes.iter().enumerate() {
        if pin == in1 {
            a = out;
        }
        if pin == in2 {
            b = out;
        }
        assert!(!(a.is_high() && b.is_high()), "both inputs high after write {i}: {writes:?}");
    }
}

#[test]
fn turning_around_never_drives_both_inputs_high() {
    for (part, in1, in2) in [(en_in1_in2(NO_PIN), 8, 7), (two_pwm(NO_PIN), 5, 6)] {
        let mut l = Link::ready(std::slice::from_ref(&part));
        l.sketch.clear_writes();
        for duty in [1.0, -1.0, 0.3, -0.3, 0.0, -1.0, 1.0] {
            l.duty(0, duty);
        }
        never_both_high(&mut l, in1, in2);
    }
}

#[test]
fn every_way_a_motor_is_stopped_ends_with_its_pins_low() {
    for (part, pins) in
        [(pwm_dir(), vec![9]), (en_in1_in2(NO_PIN), vec![10, 8, 7]), (two_pwm(NO_PIN), vec![5, 6])]
    {
        let stopped = |l: &mut Link| pins.iter().all(|&p| !l.sketch.pin(p).is_high());
        // Faber's e-stop.
        let mut l = Link::ready(std::slice::from_ref(&part));
        l.duty(0, 0.8);
        assert!(!stopped(&mut l));
        l.session.set_estop(true, faber_capabilities::estop::Source::Shell, "test");
        l.flush();
        assert!(stopped(&mut l), "{part:?}: e-stop");
        // The heartbeat going quiet: the board stops on its own.
        let mut l = Link::ready(std::slice::from_ref(&part));
        l.duty(0, -0.8);
        for t in 1..=300 {
            l.sketch.tick(l.now_ms + t);
        }
        assert!(stopped(&mut l), "{part:?}: heartbeat lost");
    }
}

#[test]
fn a_motor_missing_a_pin_its_driver_needs_refuses_every_command() {
    let missing_in2 = SketchPart::DriverMotor {
        name: "left",
        driver: Driver::EnIn1In2,
        pins: [10, 8, NO_PIN, NO_PIN],
        max_velocity: 0.0,
    };
    let mut l = Link::ready(&[missing_in2]);
    let a = l.duty(0, 0.5);
    assert_eq!(a.cause, Cause::Refused);
    assert_eq!(l.sketch.pin(10), PinOut::Unset, "nothing was driven");
    assert!(l.sketch.writes().iter().all(|&(pin, _)| pin != NO_PIN), "never a write to pin 255");
}

#[test]
fn a_driver_motor_takes_set_velocity_once_its_top_speed_is_set() {
    let part = SketchPart::DriverMotor {
        name: "left",
        driver: Driver::TwoPwm,
        pins: [5, 6, NO_PIN, NO_PIN],
        max_velocity: 12.0,
    };
    let mut l = Link::ready(&[part]);
    assert_eq!(
        l.session.parts()[0].info,
        PartInfo::Known(Info::Motor(motor::Info {
            kind: motor::Kind::Dc,
            modes: motor::Modes { velocity: true, position: false, duty: true },
            max_velocity_rad_s: Some(12.0),
        }))
    );
    l.events.clear();
    l.session.command(0, Command::Motor(motor::Command::SetVelocity { velocity_rad_s: -6.0 }), true).unwrap();
    l.flush();
    assert_eq!(l.applied().map(|a| a.cause), Some(Cause::Command));
    assert_eq!(outputs(&mut l, &[5, 6]), [PinOut::Pwm(0), PinOut::Pwm(128)]);
}

fn pins(part: u8, list: &[(PinRole, u8)]) -> PartPins {
    PartPins {
        part,
        analog_base: Some(14),
        pins: list.iter().map(|&(role, pin)| PinUse { role, pin }).collect(),
    }
}

#[test]
fn every_part_class_reports_its_pins() {
    let l = Link::ready(&[
        pwm_dir(),
        SketchPart::DriverMotor {
            name: "right",
            driver: Driver::EnIn1In2,
            pins: [11, 12, 13, 4],
            max_velocity: 0.0,
        },
        SketchPart::DriverMotor {
            name: "arm",
            driver: Driver::TwoPwm,
            pins: [5, 6, NO_PIN, NO_PIN],
            max_velocity: 0.0,
        },
        SketchPart::Distance { name: "ir", pin: 14 },
        SketchPart::Ultrasonic { name: "sonar", trig: 7, echo: 2 },
        SketchPart::Battery { name: "battery", pin: 15 },
        SketchPart::Button { name: "start", pin: 3, active_high: false },
        SketchPart::Obstacle { name: "bumper", pin: 16, kind: 0, active_high: false, trigger_m: 0.0 },
    ]);
    use PinRole::*;
    assert_eq!(
        l.session.pins(),
        [
            pins(0, &[(Pwm, 9), (Dir, 8)]),
            pins(1, &[(En, 11), (In1, 12), (In2, 13), (Stby, 4)]),
            pins(2, &[(In1, 5), (In2, 6)]),
            pins(3, &[(Signal, 14)]),
            pins(4, &[(Trig, 7), (Echo, 2)]),
            pins(5, &[(Sense, 15)]),
            pins(6, &[(Signal, 3)]),
            pins(7, &[(Signal, 16)]),
        ]
    );
    let sonar = &l.session.pins()[4];
    let labels: Vec<String> = sonar.pins.iter().map(|p| sonar.label(p.pin)).collect();
    assert_eq!(labels, ["D7", "D2"]);
    assert_eq!(l.session.pins()[5].label(15), "A1");
}

#[test]
fn the_ultrasonic_and_the_analog_parts_still_measure() {
    let mut l = Link::ready(&[
        SketchPart::Ultrasonic { name: "sonar", trig: 7, echo: 6 },
        SketchPart::Distance { name: "ir", pin: 14 },
    ]);
    assert_eq!((l.sketch.pin_mode(7), l.sketch.pin_mode(6)), (PinMode::Output, PinMode::Input));
    l.sketch.set_echo_us(5831); // about 1 m there and back
    l.sketch.set_analog(14, 512);
    l.run(120);
    let meters: Vec<(u8, f32)> = l
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Reading { part, reading: faber_capabilities::Reading::Distance(r) } => {
                Some((*part, r.meters))
            }
            _ => None,
        })
        .collect();
    let last = |part: u8| meters.iter().rev().find(|m| m.0 == part).map(|m| m.1).unwrap();
    assert!((last(0) - 1.0).abs() < 0.01, "{meters:?}");
    assert!((last(1) - 0.4).abs() < 0.01, "512 of 1023 on a 0.8 m linear sensor: {meters:?}");
    assert_eq!(l.sketch.baud(), 115_200);
}

/// The digital levels part `part` reported, in order.
fn levels(l: &Link, part: u8) -> Vec<bool> {
    l.events
        .iter()
        .filter_map(|e| match e {
            Event::Reading { part: p, reading: Reading::DigitalIn(r) } if *p == part => Some(r.level),
            Event::Reading { part: p, reading: Reading::Obstacle(r) } if *p == part => Some(r.detected),
            _ => None,
        })
        .collect()
}

fn button(active_high: bool) -> SketchPart {
    SketchPart::Button { name: "start", pin: 4, active_high }
}

#[test]
fn a_button_to_gnd_needs_only_its_pull_up_and_reads_pressed_when_low() {
    let mut l = Link::ready(&[button(false)]);
    assert_eq!(l.sketch.pin_mode(4), PinMode::InputPullup);
    assert_eq!(l.session.parts()[0].capability, "digital-in@1");
    assert_eq!(l.session.parts()[0].info, PartInfo::Empty);
    l.sketch.set_input(4, Some(false));
    l.run(40);
    assert_eq!(levels(&l, 0), [true], "pressed");
}

#[test]
fn a_bouncing_button_is_one_press_once_it_has_held_for_the_debounce_time() {
    let mut l = Link::ready(&[button(false)]);
    l.run(20);
    l.events.clear();
    // A switch closing bounces for a few ms (on and off every millisecond here).
    for i in 0..8 {
        l.sketch.set_input(4, Some(i % 2 == 1));
        l.run(1);
    }
    l.sketch.set_input(4, Some(false));
    l.run(15);
    assert!(levels(&l, 0).is_empty(), "not yet: it has held for less than 20 ms");
    l.run(15);
    assert_eq!(levels(&l, 0), [true], "one press, no bounces");
    l.events.clear();
    l.sketch.set_input(4, None); // released: the pull-up takes it HIGH again
    l.run(30);
    assert_eq!(levels(&l, 0), [false]);
}

#[test]
fn an_unchanged_level_is_repeated_once_a_second() {
    let mut l = Link::ready(&[button(false)]);
    l.run(20);
    l.events.clear();
    l.run(2000);
    assert_eq!(levels(&l, 0), [false, false], "a Faber that connects late learns the level");
}

#[test]
fn an_active_high_input_has_no_pull_up_and_reads_high_as_pressed() {
    let mut l = Link::ready(&[button(true)]);
    assert_eq!(l.sketch.pin_mode(4), PinMode::Input);
    l.sketch.set_input(4, Some(true));
    l.run(40);
    assert_eq!(levels(&l, 0).last(), Some(&true));
}

#[test]
fn an_obstacle_says_its_kind_and_trigger_distance_and_when_it_is_detected() {
    let ir = SketchPart::Obstacle { name: "ir", pin: 5, kind: 1, active_high: false, trigger_m: 0.1 };
    let bumper = SketchPart::Obstacle { name: "bumper", pin: 6, kind: 0, active_high: false, trigger_m: 0.0 };
    let mut l = Link::ready(&[ir, bumper]);
    assert_eq!(
        l.session.parts()[0].info,
        PartInfo::Known(Info::Obstacle(obstacle::Info {
            kind: obstacle::Kind::Infrared,
            trigger_distance_m: Some(0.1)
        }))
    );
    assert_eq!(
        l.session.parts()[1].info,
        PartInfo::Known(Info::Obstacle(obstacle::Info {
            kind: obstacle::Kind::Bumper,
            trigger_distance_m: None
        }))
    );
    l.run(20);
    l.events.clear();
    l.sketch.set_input(6, Some(false)); // the bumper's switch closes to GND
    l.run(30);
    assert_eq!(levels(&l, 1), [true]);
    assert!(levels(&l, 0).is_empty(), "the infrared module saw nothing");
}

fn output(pwm: bool, active_low: bool) -> SketchPart {
    SketchPart::Output { name: "out", pin: 7, pwm, active_low }
}

fn set(l: &mut Link, part: u8, command: Command) -> Applied {
    l.events.clear();
    l.session.command(part, command, true).unwrap();
    l.flush();
    l.applied().expect("an APPLIED for every command")
}

fn on(level: bool) -> Command {
    Command::DigitalOut(digital_out::Command::Set { level })
}

fn duty(duty: f32) -> Command {
    Command::PwmOut(pwm_out::Command::SetDuty { duty })
}

#[test]
fn an_on_off_output_switches_its_pin_and_reports_digital_out() {
    let mut l = Link::ready(&[output(false, false)]);
    assert_eq!(l.session.parts()[0].capability, "digital-out@1");
    assert_eq!(l.session.parts()[0].lease_ms, 1000);
    assert_eq!((l.sketch.pin_mode(7), l.sketch.pin(7)), (PinMode::Output, LOW), "it starts off");
    assert_eq!(set(&mut l, 0, on(true)).cause, Cause::Command);
    assert_eq!(l.sketch.pin(7), HIGH);
    set(&mut l, 0, on(false));
    assert_eq!(l.sketch.pin(7), LOW);
    assert_eq!(l.session.pins()[0], pins(0, &[(PinRole::Out, 7)]));
}

#[test]
fn an_active_low_output_is_high_when_off_from_before_it_is_an_output() {
    let mut l = Link::ready(&[output(false, true)]);
    // The first write to the pin is HIGH, made before pinMode(OUTPUT): a relay module never
    // clicks on at start.
    assert_eq!(l.sketch.writes().first(), Some(&(7, HIGH)));
    assert_eq!(l.sketch.pin(7), HIGH);
    set(&mut l, 0, on(true));
    assert_eq!(l.sketch.pin(7), LOW, "on is LOW");
}

#[test]
fn a_pwm_output_writes_its_duty_and_clamps_it() {
    let mut l = Link::ready(&[output(true, false)]);
    assert_eq!(l.session.parts()[0].capability, "pwm-out@1");
    set(&mut l, 0, duty(0.5));
    assert_eq!(l.sketch.pin(7), PinOut::Pwm(128));
    let a = set(&mut l, 0, duty(1.5));
    assert!(a.clamped);
    assert_eq!(a.command, duty(1.0));
    assert_eq!(l.sketch.pin(7), PinOut::Pwm(255));
    let mut low = Link::ready(&[output(true, true)]);
    set(&mut low, 0, duty(0.25));
    assert_eq!(low.sketch.pin(7), PinOut::Pwm(191), "active low: 255 - 64");
}

#[test]
fn an_output_goes_off_when_its_lease_ends_on_the_board_and_on_the_e_stop() {
    for active_low in [false, true] {
        let off = if active_low { HIGH } else { LOW };
        let mut l = Link::ready(&[output(false, active_low)]);
        set(&mut l, 0, on(true));
        l.run(990);
        assert_ne!(l.sketch.pin(7), off, "still on inside its 1 s lease");
        l.run(20);
        assert_eq!(l.applied().map(|a| a.cause), Some(Cause::LeaseExpired));
        assert_eq!(l.sketch.pin(7), off, "off once nobody has commanded it for 1 s");
        set(&mut l, 0, on(true));
        l.session.set_estop(true, faber_capabilities::estop::Source::Shell, "test");
        l.flush();
        assert_eq!(l.sketch.pin(7), off, "the e-stop");
    }
    let mut l = Link::ready(&[output(true, false)]);
    set(&mut l, 0, duty(0.8));
    l.session.safe(Some(0)).unwrap();
    l.flush();
    assert_eq!(l.sketch.pin(7), PinOut::Pwm(0), "SAFE");
}

fn servo(release: bool) -> SketchPart {
    SketchPart::Servo { name: "arm", pin: 9, release }
}

fn angle(rad: f32) -> Command {
    Command::Servo(servo::Command::SetAngle { angle_rad: rad })
}

#[test]
fn a_servo_starts_released_and_turns_to_its_angle_with_the_matching_pulse() {
    let mut l = Link::ready(&[servo(false)]);
    assert_eq!(l.session.parts()[0].capability, "servo@1");
    let PartInfo::Known(Info::Servo(i)) = &l.session.parts()[0].info else { panic!() };
    assert!((i.max_angle_rad - std::f32::consts::PI).abs() < 1e-5);
    assert_eq!((i.min_angle_rad, i.safe_state), (0.0, servo::SafeState::Hold));
    assert_eq!(l.sketch.servo_pulse(9), None, "no pulses until the first angle: it doesn't jump");
    set(&mut l, 0, angle(std::f32::consts::FRAC_PI_2));
    // 90° is halfway from 544 to 2400 µs (the Servo library's defaults): 1472 µs.
    assert_eq!(l.sketch.servo_pulse(9), Some(1472));
    set(&mut l, 0, angle(0.0));
    assert_eq!(l.sketch.servo_pulse(9), Some(544));
    let a = set(&mut l, 0, angle(4.0));
    assert!(a.clamped, "past 180°");
    assert_eq!(l.sketch.servo_pulse(9), Some(2400));
    assert_eq!(l.session.pins()[0], pins(0, &[(PinRole::Signal, 9)]));
}

#[test]
fn a_holding_servo_keeps_its_angle_when_its_lease_ends_and_a_releasing_one_lets_go() {
    let mut l = Link::ready(&[servo(false)]);
    set(&mut l, 0, angle(1.0));
    let held = l.sketch.servo_pulse(9);
    l.run(1100);
    let a = l.applied().unwrap();
    assert_eq!((a.cause, a.command), (Cause::LeaseExpired, angle(1.0)), "it says it holds 1 rad");
    assert_eq!(l.sketch.servo_pulse(9), held, "hold: an arm doesn't drop what it carries");
    l.session.set_estop(true, faber_capabilities::estop::Source::Shell, "test");
    l.flush();
    assert_eq!(l.sketch.servo_pulse(9), held, "the e-stop holds it too");

    let mut l = Link::ready(&[servo(true)]);
    set(&mut l, 0, angle(1.0));
    l.run(1100);
    assert_eq!(l.applied().map(|a| a.command), Some(Command::Servo(servo::Command::Release)));
    assert_eq!(l.sketch.servo_pulse(9), None, "release: no more pulses");
}

#[test]
fn a_servo_can_be_released_and_reports_what_it_holds() {
    let mut l = Link::ready(&[servo(false)]);
    set(&mut l, 0, angle(0.5));
    l.events.clear();
    l.run(150);
    let last = |l: &Link| {
        l.events.iter().rev().find_map(|e| match e {
            Event::Reading { reading: Reading::Servo(r), .. } => Some(r.angle_rad),
            _ => None,
        })
    };
    assert_eq!(last(&l), Some(Some(0.5)));
    set(&mut l, 0, Command::Servo(servo::Command::Release));
    assert_eq!(l.sketch.servo_pulse(9), None);
    l.run(150);
    assert_eq!(last(&l), Some(None), "released: no angle");
}

fn encoder(b: u8, pulses_per_rev: f32) -> SketchPart {
    SketchPart::Encoder { name: "wheel", a: 2, b, pulses_per_rev }
}

/// Turns the encoder on A (D2) and B (`b`) by `steps` quadrature states: forward (A changes
/// first) for a positive number, backward otherwise. Starts from both channels high (their
/// pull-ups).
fn turn(l: &mut Link, b: u8, steps: i32) {
    // The states (A, B) forward: 11 → 01 → 00 → 10 → 11 (A changes first).
    const FORWARD: [(bool, bool); 4] = [(true, true), (false, true), (false, false), (true, false)];
    let mut at = 0usize;
    for _ in 0..steps.unsigned_abs() {
        at = if steps > 0 { (at + 1) % 4 } else { (at + 3) % 4 };
        let (a, bb) = FORWARD[at];
        l.sketch.set_input(2, Some(a));
        l.sketch.set_input(b, Some(bb));
    }
}

fn last_encoder(l: &Link) -> faber_capabilities::encoder::Reading {
    l.events
        .iter()
        .rev()
        .find_map(|e| match e {
            Event::Reading { reading: Reading::Encoder(r), .. } => Some(r.clone()),
            _ => None,
        })
        .expect("an encoder reading")
}

#[test]
fn an_encoder_on_two_interrupt_pins_counts_every_edge_both_ways() {
    let mut l = Link::ready(&[encoder(3, 0.0)]);
    assert_eq!(l.session.parts()[0].capability, "encoder@1");
    assert_eq!((l.sketch.pin_mode(2), l.sketch.pin_mode(3)), (PinMode::InputPullup, PinMode::InputPullup));
    turn(&mut l, 3, 8);
    l.run(60);
    assert_eq!(last_encoder(&l).count, 8, "4 counts a pulse: two whole pulses forward");
    turn(&mut l, 3, -12);
    l.run(60);
    let r = last_encoder(&l);
    assert_eq!((r.count, r.delta), (-4, -12), "and back past where it started");
    assert_eq!(r.velocity_rad_s, None, "no pulses per turn: no rad/s");
    assert_eq!(l.session.pins()[0], pins(0, &[(PinRole::A, 2), (PinRole::B, 3)]));
}

#[test]
fn an_encoder_with_only_a_on_an_interrupt_counts_its_edges_both_ways() {
    let mut l = Link::ready(&[encoder(4, 0.0)]);
    turn(&mut l, 4, 8);
    l.run(60);
    assert_eq!(last_encoder(&l).count, 4, "2 counts a pulse: A's two edges in each of two pulses");
    turn(&mut l, 4, -8);
    l.run(60);
    assert_eq!(last_encoder(&l).count, 0);
}

#[test]
fn an_encoder_that_knows_its_pulses_per_turn_reports_rad_s_and_feeds_its_motor() {
    // 11 pulses a turn on two interrupt pins: 44 counts a turn.
    let mut l =
        Link::ready(&[encoder(3, 11.0), SketchPart::MotorWithEncoder { name: "left", pwm: 9, dir: 8 }]);
    assert_eq!(
        l.session.parts()[0].info,
        PartInfo::Known(Info::Encoder(faber_capabilities::encoder::Info { counts_per_rev: Some(44.0) }))
    );
    l.run(60);
    turn(&mut l, 3, 22); // half a turn
    l.run(50);
    let r = last_encoder(&l);
    assert_eq!(r.count, 22);
    let rad_s = r.velocity_rad_s.unwrap();
    assert!((rad_s - r.counts_per_s / 44.0 * std::f32::consts::TAU).abs() < 1e-3, "{r:?}");
    let motor = l
        .events
        .iter()
        .rev()
        .find_map(|e| match e {
            Event::Reading { part: 1, reading: Reading::Motor(m) } => Some(m.clone()),
            _ => None,
        })
        .expect("the motor reports what its encoder measures");
    assert!((motor.position_rad.unwrap() - std::f32::consts::PI).abs() < 1e-4, "half a turn: {motor:?}");
    assert!(motor.velocity_rad_s.is_some());
}

#[test]
fn an_encoder_whose_a_is_no_interrupt_pin_counts_nothing() {
    let mut l = Link::ready(&[SketchPart::Encoder { name: "wheel", a: 4, b: 5, pulses_per_rev: 0.0 }]);
    l.sketch.set_input(4, Some(false));
    l.sketch.set_input(5, Some(false));
    l.run(60);
    assert_eq!(last_encoder(&l).count, 0);
}
