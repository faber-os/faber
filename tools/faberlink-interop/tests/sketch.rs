//! The library's Arduino glue, pin by pin: whole sketches (`FaberMotor` in each driver shape,
//! `FaberDistance`, `FaberUltrasonic`, `FaberBattery`) on a stand-in Arduino core, driven from the
//! Rust session. What passes here is the glue that runs on the board; only `digitalWrite()` and
//! friends are stand-ins.

use faber_capabilities::{Command, Info, motor};
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
