//! The allocator's tests: suggestions on each board, and every rule of the whole-plan check.
//! Parts the library has no class for yet (servo, buzzer, IMU, RTC, encoder, LED strip, PCA9685)
//! are test templates written here, so the rules are tested before their templates exist.

use super::*;
use crate::arduino::wiring::parts::{self, SettingValue, templates};
use crate::arduino::wiring::pins::{self, HeaderPosition};

fn test_template(text: &str) -> Template {
    let t = parts::parse(text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    parts::check(&t).unwrap_or_else(|e| panic!("{}: {e}", t.id));
    t
}

/// A test template: one shape with these roles (`id:need`, or `id:need:provided` for a role that
/// takes a provided pin too).
fn simple(id: &str, roles: &[&str], extra: &str) -> Template {
    let roles_toml: Vec<String> = roles
        .iter()
        .map(|r| {
            let mut it = r.split(':');
            let (rid, need, provided) = (it.next().unwrap(), it.next().unwrap(), it.next().is_some());
            format!(
                "{{ id = \"{rid}\", name = \"{}\", needs = \"{need}\", help = \"\", provided = {provided} }}",
                rid.to_uppercase()
            )
        })
        .collect();
    let args: Vec<String> = std::iter::once("\"name\"".to_string())
        .chain(roles.iter().map(|r| format!("\"pin:{}\"", r.split(':').next().unwrap())))
        .collect();
    let wiring: Vec<String> =
        roles.iter().map(|r| format!("\"{0} → {{pin:{0}}}\"", r.split(':').next().unwrap())).collect();
    test_template(&format!(
        r#"faber-part = 1
id = "{id}"
name = "Test {id}"
description = "A test part."
class = "Test{id}"
include = "FaberLink.h"
default-name = "{default}"
{extra}

[power]
supply = "5V"

[[shapes]]
id = "only"
name = "Only"
args = [{args}]
wiring = [{wiring}]
roles = [{roles}]
"#,
        default = id.replace('-', "_"),
        args = args.join(", "),
        wiring = wiring.join(", "),
        roles = roles_toml.join(", "),
    ))
}

/// The built-in templates and the test ones.
fn catalog() -> Vec<Template> {
    let mut all: Vec<Template> = templates().to_vec();
    all.push(simple("servo", &["signal:digital-out"], "uses = [\"servo\"]"));
    all.push(simple("buzzer", &["out:digital-out"], "uses = [\"tone\"]"));
    all.push(simple("encoder", &["a:interrupt", "b:interrupt"], ""));
    all.push(simple("strip", &["din:digital-out"], "uses = [\"interrupts-off\"]"));
    all.push(simple("button", &["in:digital-in"], ""));
    all.push(simple(
        "imu",
        &[],
        "bus = \"i2c\"\ni2c-addresses = [0x68, 0x69]\naddress-setting = \"address\"\n\
         address-hint = \"Connect the IMU's AD0 to 5V: that gives it 0x69.\"\n\n\
         [[settings]]\nid = \"address\"\nname = \"Address\"\ntype = \"choice\"\ndefault = \"0x68\"\n\
         apply = \"build\"\noptions = [{ id = \"0x68\", name = \"0x68\", code = \"0x68\" }, \
         { id = \"0x69\", name = \"0x69\", code = \"0x69\" }]",
    ));
    all.push(simple("rtc", &[], "bus = \"i2c\"\ni2c-addresses = [0x68]"));
    all.push(simple(
        "pca9685",
        &[],
        "bus = \"i2c\"\ni2c-addresses = [0x40]\n\n[provides]\ncount = 16\nname = \"ch{n}\"\n\
         abilities = [\"pwm\"]\nvolts = 5.0",
    ));
    all.push(simple("pwm-servo", &["signal:pwm:provided"], ""));
    let mut heater = simple("heater", &["on:digital-out"], "");
    heater.power.current_ma = Some(300);
    all.push(heater);
    all
}

fn board(id: &str) -> PinMap {
    pins::arduino(id).unwrap()
}

fn part(template: &str, pins: &[(&str, &str)]) -> PlannedPart {
    PlannedPart {
        template: template.into(),
        shape: None,
        pins: pins.iter().map(|(r, p)| (r.to_string(), p.to_string())).collect(),
        settings: BTreeMap::new(),
    }
}

fn plan(board: &str, parts: Vec<(&str, PlannedPart)>) -> Plan {
    let mut p = Plan::new(board, "test");
    for (n, part) in parts {
        p.parts.insert(n.into(), part);
    }
    p.normalized(&self::board(board), &catalog())
}

fn want(template: &str) -> Candidate {
    Candidate { template: template.into(), ..Default::default() }
}

fn suggest_on(map: &PinMap, plan: &Plan, c: &Candidate) -> Suggestion {
    suggest(map, plan, &catalog(), &[], c).unwrap()
}

fn picks(s: &Suggestion) -> Vec<(&str, Option<&str>)> {
    s.roles.iter().map(|r| (r.role.as_str(), r.suggested.as_deref())).collect()
}

fn codes(problems: &[Problem]) -> Vec<&str> {
    problems.iter().map(|p| p.code.as_str()).collect()
}

fn errors(problems: &[Problem]) -> Vec<&Problem> {
    problems.iter().filter(|p| p.severity == Severity::Error).collect()
}

fn role<'a>(s: &'a Suggestion, id: &str) -> &'a RoleOptions {
    s.roles.iter().find(|r| r.role == id).unwrap()
}

fn reason<'a>(list: &'a [PinReason], pin: &str) -> &'a str {
    &list.iter().find(|r| r.pin == pin).unwrap_or_else(|| panic!("{pin} isn't in {list:?}")).reason
}

// ---- Suggestions ----

#[test]
fn a_sonar_on_an_empty_uno_takes_plain_pins_next_to_each_other() {
    let uno = board("uno");
    let s = suggest_on(&uno, &plan("uno", vec![]), &want("ultrasonic"));
    assert_eq!(s.name, "front");
    assert_eq!(s.shape, "trig-echo");
    assert_eq!(picks(&s), [("trig", Some("D4")), ("echo", Some("D7"))]);
    let trig = role(&s, "trig");
    assert_eq!(trig.reserved.iter().map(|r| r.pin.as_str()).collect::<Vec<_>>(), ["D0", "D1"]);
    assert!(reason(&trig.reserved, "D0").contains("USB link to Faber"));
    assert!(trig.unsuitable.is_empty(), "every Uno pin is digital: {:?}", trig.unsuitable);
    assert_eq!(trig.in_use, [PinUse { pin: "D7".into(), by: "front (Echo)".into() }], "its own other pin");
    assert_eq!(trig.suitable.first().map(String::as_str), Some("D8"), "the next plain pin");
    let at = |p: &str| trig.suitable.iter().position(|x| x == p);
    assert!(at("D2") > at("D12"), "interrupt pins last");
    assert_eq!(
        trig.suitable.len() + 1 + 2 + 1,
        20,
        "suggested + reserved + its own echo + suitable = every pin"
    );
    assert_eq!(s.part.pins["trig"], "D4");
    assert_eq!(s.part.settings["max-m"], SettingValue::Number(4.0), "every setting at its default");
    assert!(errors(&s.problems).is_empty(), "{:?}", s.problems);
}

#[test]
fn suggestions_are_stable_and_follow_the_plan() {
    let uno = board("uno");
    let empty = plan("uno", vec![]);
    let a = suggest_on(&uno, &empty, &want("ultrasonic"));
    assert_eq!(a, suggest_on(&uno, &empty, &want("ultrasonic")), "the same question, the same answer");
    let with = plan("uno", vec![("front", a.part.clone())]);
    let b = suggest_on(&uno, &with, &want("ultrasonic"));
    assert_eq!(b.name, "front_2");
    assert_eq!(picks(&b), [("trig", Some("D8")), ("echo", Some("D12"))]);
    assert_eq!(role(&b, "trig").in_use.iter().find(|u| u.pin == "D4").unwrap().by, "front (Trig)");
    // Editing a part: its own pins are free for it.
    let edit = Candidate { name: Some("front".into()), ..want("ultrasonic") };
    let e = suggest_on(&uno, &with, &edit);
    assert_eq!(picks(&e), picks(&a));
    // A pin chosen already is kept, and the other role is placed near it.
    let chosen = Candidate { pins: [("echo".to_string(), "A2".to_string())].into(), ..want("ultrasonic") };
    let c = suggest_on(&uno, &empty, &chosen);
    assert_eq!(picks(&c), [("trig", Some("A1")), ("echo", Some("A2"))]);
}

#[test]
fn a_motor_takes_a_pwm_pin_that_no_servo_or_tone_needs() {
    let uno = board("uno");
    let s = suggest_on(&uno, &plan("uno", vec![]), &want("motor"));
    assert_eq!(s.name, "left");
    assert_eq!(picks(&s), [("pwm", Some("D5")), ("dir", Some("D4"))]);
    let pwm = role(&s, "pwm");
    assert_eq!(
        reason(&pwm.unsuitable, "D7"),
        "PWM needs a PWM pin: D7 has no PWM; the Arduino Uno's PWM pins are D3, D5, D6, D9, D10 and D11."
    );
    assert!(s.problems.iter().any(|p| p.code == "own-supply"), "{:?}", s.problems);
    let mega = board("mega2560");
    let m = suggest_on(&mega, &plan("mega2560", vec![]), &want("motor"));
    let pwm = role(&m, "pwm").suggested.clone().unwrap();
    assert!(
        !["D44", "D45", "D46", "D9", "D10"].contains(&pwm.as_str()),
        "keeps the servo and tone() timers free: {pwm}"
    );
}

#[test]
fn an_ir_sensor_on_a_nano_takes_an_analog_pin_and_a6_is_analog_only() {
    let nano = board("nano");
    let s = suggest_on(&nano, &plan("nano", vec![]), &want("ir-distance"));
    assert_eq!(picks(&s), [("out", Some("A0"))]);
    let out = role(&s, "out");
    assert!(out.suitable.contains(&"A6".to_string()) && out.suitable.contains(&"A7".to_string()));
    assert_eq!(
        reason(&out.unsuitable, "D7"),
        "Out needs an analog input: D7 isn't an analog input; the Arduino Nano's are A0–A7."
    );
    let sonar = suggest_on(&nano, &plan("nano", vec![]), &want("ultrasonic"));
    assert_eq!(
        reason(&role(&sonar, "echo").unsuitable, "A6"),
        "Echo needs a digital pin: A6 on an Arduino Nano is analog only."
    );
    assert!(!role(&sonar, "echo").suitable.contains(&"A7".to_string()));
}

#[test]
fn encoders_get_the_interrupt_pins_and_nothing_else_when_they_are_gone() {
    let uno = board("uno");
    let s = suggest_on(&uno, &plan("uno", vec![]), &want("encoder"));
    assert_eq!(picks(&s), [("a", Some("D2")), ("b", Some("D3"))]);
    assert_eq!(
        reason(&role(&s, "a").unsuitable, "D4"),
        "A needs an external interrupt pin: D4 has no external interrupt; the Arduino Uno's are D2 and D3."
    );
    let full = plan("uno", vec![("left", part("encoder", &[("a", "D2"), ("b", "D3")]))]);
    let none = suggest_on(&uno, &full, &want("encoder"));
    assert_eq!(picks(&none), [("a", None), ("b", None)]);
    assert!(codes(&none.problems).contains(&"missing-pin"), "{:?}", none.problems);
    // On a Mega, with an I²C part, D20/D21 belong to the bus and D18/D19 cost Serial1.
    let mega = board("mega2560");
    let busy = plan("mega2560", vec![("imu", part("imu", &[]))]);
    let a = suggest_on(&mega, &busy, &want("encoder"));
    assert_eq!(picks(&a), [("a", Some("D2")), ("b", Some("D3"))]);
    let a_role = role(&a, "a");
    assert!(
        a_role.in_use.iter().any(|u| u.pin == "D20" && u.by == "the I²C bus, SDA (imu)"),
        "{:?}",
        a_role.in_use
    );
    assert_eq!(a_role.suitable, ["D18", "D19"].map(String::from), "{:?}", a_role.suitable);
}

#[test]
fn sonars_stay_off_the_interrupt_pins_and_a_warning_says_when_they_are_all_used() {
    let uno = board("uno");
    let p = plan("uno", vec![("front", part("ultrasonic", &[("trig", "D2"), ("echo", "D3")]))]);
    let problems = check(&uno, &p, &catalog(), &[]);
    let w = problems.iter().find(|p| p.code == "interrupt-pins-used").expect("warned");
    assert_eq!(w.severity, Severity::Warning);
    assert_eq!(
        w.message,
        "D2 and D3 are the Arduino Uno's only free interrupt pins, and they are used as plain pins: a wheel encoder added later will need one."
    );
    let ok = plan("uno", vec![("front", part("ultrasonic", &[("trig", "D2"), ("echo", "D7")]))]);
    assert!(!codes(&check(&uno, &ok, &catalog(), &[])).contains(&"interrupt-pins-used"));
}

// ---- The whole-plan check ----

#[test]
fn a_valid_plan_has_no_errors() {
    let uno = board("uno");
    let p = plan(
        "uno",
        vec![
            ("front", part("ultrasonic", &[("trig", "7"), ("echo", "6")])),
            ("left", part("motor", &[("pwm", "D5"), ("dir", "D4")])),
            ("battery", part("battery", &[("sense", "A1")])),
            ("side", part("ir-distance", &[("out", "A0")])),
        ],
    );
    let problems = check(&uno, &p, &catalog(), &[]);
    assert!(errors(&problems).is_empty(), "{problems:#?}");
    assert_eq!(codes(&problems), ["own-supply"], "only the motor's supply");
}

#[test]
fn two_parts_on_one_pin_is_an_error_for_the_second() {
    let uno = board("uno");
    let p = plan(
        "uno",
        vec![
            ("a", part("button", &[("in", "D7")])),
            ("b", part("ultrasonic", &[("trig", "D7"), ("echo", "D8")])),
        ],
    );
    let problems = check(&uno, &p, &catalog(), &[]);
    let e = errors(&problems);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!(e[0].code, "pin-used-twice");
    assert_eq!(e[0].part.as_deref(), Some("b"));
    assert_eq!(e[0].pin.as_deref(), Some("D7"));
    assert_eq!(e[0].message, "D7 is used by a (IN) and b (Trig).");
    // The same part twice on one pin.
    let twice = plan("uno", vec![("b", part("ultrasonic", &[("trig", "D7"), ("echo", "D7")]))]);
    assert!(codes(&check(&uno, &twice, &catalog(), &[])).contains(&"pin-used-twice"));
}

#[test]
fn pins_in_use_from_outside_the_plan_count_too() {
    // The hook for a code-mode board's PINS report (docs/wiring.md §6).
    let uno = board("uno");
    let external = [ExternalUse { pin: "4".into(), by: "rear (Echo), reported by the board".into() }];
    let s = suggest(&uno, &plan("uno", vec![]), &catalog(), &external, &want("ultrasonic")).unwrap();
    assert_ne!(role(&s, "trig").suggested.as_deref(), Some("D4"));
    assert!(role(&s, "trig").in_use.iter().any(|u| u.pin == "D4" && u.by.contains("reported by the board")));
    let p = plan("uno", vec![("front", part("ultrasonic", &[("trig", "D4"), ("echo", "D7")]))]);
    let e = check(&uno, &p, &catalog(), &external);
    assert_eq!(errors(&e)[0].message, "D4 is used by front (Trig) and rear (Echo), reported by the board.");
}

#[test]
fn the_first_servo_on_a_mega_takes_pwm_on_d44_to_d46() {
    let mega = board("mega2560");
    let p = plan("mega2560", vec![("left", part("motor", &[("pwm", "D44"), ("dir", "D22")]))]);
    assert!(errors(&check(&mega, &p, &catalog(), &[])).is_empty());
    let s = suggest_on(&mega, &p, &want("servo"));
    let conflict =
        s.problems.iter().find(|p| p.code == "servo-timer-conflict").expect("the servo's effect is shown");
    assert_eq!(conflict.part.as_deref(), Some("left"));
    assert_eq!(
        conflict.message,
        "left: D44's PWM is off while the Servo library takes Timer5 for the servos."
    );
    // And the other way round: with a servo in the plan, D44-D46 can't be a motor's PWM.
    let with_servo = plan("mega2560", vec![("arm", part("servo", &[("signal", "D30")]))]);
    let m = suggest_on(&mega, &with_servo, &want("motor"));
    let pwm = role(&m, "pwm");
    for pin in ["D44", "D45", "D46"] {
        assert!(reason(&pwm.unsuitable, pin).contains("Servo library takes Timer5"), "{pin}");
    }
    assert!(pwm.suitable.contains(&"D11".to_string()), "Timer1 only goes after 12 servos");
}

#[test]
fn a_servo_on_an_uno_takes_d9_and_d10_and_thirteen_are_too_many() {
    let uno = board("uno");
    let p = plan(
        "uno",
        vec![
            ("arm", part("servo", &[("signal", "D2")])),
            ("left", part("motor", &[("pwm", "D9"), ("dir", "D8")])),
        ],
    );
    let e = check(&uno, &p, &catalog(), &[]);
    assert_eq!(errors(&e).iter().map(|p| p.code.as_str()).collect::<Vec<_>>(), ["servo-timer-conflict"]);
    let names: Vec<String> = (0..13).map(|i| format!("s{i}")).collect();
    let pins = ["D2", "D3", "D4", "D5", "D6", "D7", "D8", "D9", "D10", "D11", "D12", "D13", "A0"];
    let many =
        names.iter().zip(pins).map(|(n, pin)| (n.as_str(), part("servo", &[("signal", pin)]))).collect();
    let e = check(&uno, &plan("uno", many), &catalog(), &[]);
    let too = e.iter().find(|p| p.code == "too-many-servos").unwrap();
    assert_eq!(
        too.message,
        "The plan has 13 servos; the Servo library drives at most 12 on the Arduino Uno."
    );
}

#[test]
fn tone_takes_timer2() {
    let uno = board("uno");
    let p = plan(
        "uno",
        vec![
            ("beep", part("buzzer", &[("out", "D8")])),
            ("left", part("motor", &[("pwm", "D3"), ("dir", "D4")])),
        ],
    );
    let e = check(&uno, &p, &catalog(), &[]);
    let t = e.iter().find(|p| p.code == "tone-timer-conflict").unwrap();
    assert_eq!(t.message, "left: D3's PWM is off while tone() takes Timer2.");
}

#[test]
fn i2c_pins_belong_to_the_bus_while_an_i2c_part_exists() {
    let uno = board("uno");
    let p = plan(
        "uno",
        vec![("imu", part("imu", &[])), ("front", part("ultrasonic", &[("trig", "A4"), ("echo", "D7")]))],
    );
    let e = check(&uno, &p, &catalog(), &[]);
    let b = errors(&e)[0];
    assert_eq!(b.code, "bus-pin-used");
    assert_eq!(b.message, "A4 is used by front (Trig) and the I²C bus, SDA (imu).");
    // Without the I²C part, A4 is an ordinary pin.
    let alone = plan("uno", vec![("front", part("ultrasonic", &[("trig", "A4"), ("echo", "D7")]))]);
    assert!(errors(&check(&uno, &alone, &catalog(), &[])).is_empty());
    // Adding an I²C part to a plan that uses A4 shows the conflict before it is added.
    let s = suggest_on(&uno, &alone, &want("imu"));
    assert!(codes(&s.problems).contains(&"bus-pin-used"), "{:?}", s.problems);
}

#[test]
fn i2c_addresses_that_clash_are_caught() {
    let uno = board("uno");
    let p = plan("uno", vec![("imu", part("imu", &[])), ("rtc", part("rtc", &[]))]);
    let e = check(&uno, &p, &catalog(), &[]);
    let c = e.iter().find(|p| p.code == "i2c-address-clash").unwrap();
    assert_eq!(c.message, "imu (Test imu) and rtc (Test rtc) share the I²C address 0x68.");
    assert_eq!(c.part.as_deref(), Some("rtc"));
    let mut moved = p.clone();
    moved.parts.get_mut("imu").unwrap().settings.insert("address".into(), SettingValue::Text("0x69".into()));
    assert!(!codes(&check(&uno, &moved, &catalog(), &[])).contains(&"i2c-address-clash"));
    // The hint is the template's.
    let mut rtc_first = Plan::new("uno", "t");
    rtc_first.parts.insert("a_rtc".into(), part("rtc", &[]));
    rtc_first.parts.insert("b_imu".into(), part("imu", &[]));
    let e = check(&uno, &rtc_first.normalized(&uno, &catalog()), &catalog(), &[]);
    assert!(e.iter().find(|p| p.code == "i2c-address-clash").unwrap().hint.contains("AD0"));
}

#[test]
fn a_pin_provider_adds_pins_that_only_roles_which_accept_them_take() {
    let uno = board("uno");
    let p = plan("uno", vec![("servos", part("pca9685", &[]))]);
    let map = effective_map(&uno, &p, &catalog());
    assert_eq!(map.pins.len(), 20 + 16, "the list grew by the PCA9685's channels");
    assert_eq!(map.pin("servos.ch15").unwrap().provided_by.as_deref(), Some("servos"));
    let s = suggest_on(&uno, &p, &want("pwm-servo"));
    let sig = role(&s, "signal");
    let all: Vec<&String> = sig.suggested.iter().chain(&sig.suitable).collect();
    assert!(all.iter().any(|p| p.as_str() == "servos.ch0"), "{all:?}");
    assert!(all.iter().any(|p| p.as_str() == "D5"));
    let sonar = suggest_on(&uno, &p, &want("ultrasonic"));
    assert_eq!(
        reason(&role(&sonar, "trig").unsuitable, "servos.ch3"),
        "servos.ch3 is on servos, not on the board itself; Trig needs a pin of the board."
    );
    let placed = plan(
        "uno",
        vec![("servos", part("pca9685", &[])), ("arm", part("pwm-servo", &[("signal", "servos.ch3")]))],
    );
    assert!(errors(&check(&uno, &placed, &catalog(), &[])).is_empty());
    // Without the provider, its pins don't exist.
    let gone = plan("uno", vec![("arm", part("pwm-servo", &[("signal", "servos.ch3")]))]);
    assert_eq!(codes(&check(&uno, &gone, &catalog(), &[])), ["unknown-pin"]);
}

#[test]
fn reserved_and_unknown_pins_and_bad_parts_are_errors() {
    let uno = board("uno");
    let p = plan("uno", vec![("front", part("ultrasonic", &[("trig", "D0"), ("echo", "D99")]))]);
    let e = check(&uno, &p, &catalog(), &[]);
    assert_eq!(codes(&e), ["reserved-pin", "unknown-pin"]);
    assert_eq!(e[0].message, "front: Trig can't use D0: D0 and D1 carry the USB link to Faber (Serial).");
    assert_eq!(e[1].message, "front: the Arduino Uno has no pin \"D99\" (for Echo).");

    let mut odd = part("ultrasonic", &[("trig", "D4"), ("sonar", "D5")]);
    odd.settings.insert("max-m".into(), SettingValue::Number(9.0));
    odd.settings.insert("colour".into(), SettingValue::Text("red".into()));
    let p = plan("uno", vec![("front", odd)]);
    let e = check(&uno, &p, &catalog(), &[]);
    assert_eq!(codes(&e), ["unknown-setting", "invalid-setting", "unknown-role", "missing-pin"]);
    assert_eq!(e[1].message, "front: Longest distance can be at most 4 m.");

    let p = plan("uno", vec![("loop", part("toaster", &[]))]);
    assert_eq!(codes(&check(&uno, &p, &catalog(), &[])), ["invalid-part-name", "unknown-template"]);
    let mut pending = part("motor", &[("en", "D5"), ("in1", "D4"), ("in2", "D7")]);
    pending.shape = Some("en-in1-in2".into());
    let e = check(&uno, &plan("uno", vec![("left", pending)]), &catalog(), &[]);
    assert_eq!(codes(&e), ["shape-not-ready"]);
    assert!(e[0].message.contains("EN + IN1 + IN2"), "{}", e[0].message);
    let wrong = Candidate { shape: Some("nope".into()), ..want("motor") };
    let err = suggest(&uno, &plan("uno", vec![]), &catalog(), &[], &wrong).unwrap_err();
    assert_eq!(err.code, "unknown-shape");
}

#[test]
fn part_names_must_be_names_the_sketch_can_use() {
    assert_eq!(check_part_name("front"), Ok(()));
    assert_eq!(check_part_name("front_2"), Ok(()));
    for bad in ["link", "loop", "setup", "int", "delay", "Front", "2front", "front.left", ""] {
        assert!(check_part_name(bad).is_err(), "{bad}");
    }
    let mut p = Plan::new("uno", "x");
    p.parts.insert("front".into(), part("ultrasonic", &[]));
    assert_eq!(free_name(&p, "front"), "front_2");
    assert_eq!(free_name(&p, "left"), "left");
}

#[test]
fn a_battery_divider_that_would_overload_the_pin_is_refused() {
    let uno = board("uno");
    let ok = plan("uno", vec![("battery", part("battery", &[("sense", "A1")]))]);
    assert!(
        errors(&check(&uno, &ok, &catalog(), &[])).is_empty(),
        "20 kΩ over 10 kΩ keeps 3S Li-ion at 4.2 V"
    );
    let mut hot = part("battery", &[("sense", "A1")]);
    hot.settings.insert("r1-ohm".into(), SettingValue::Number(10000.0));
    let e = check(&uno, &plan("uno", vec![("battery", hot)]), &catalog(), &[]);
    assert_eq!(codes(&e), ["divider-too-high"]);
    assert_eq!(
        e[0].message,
        "battery: a full battery (12.6 V) puts 6.30 V on the pin, above the Arduino Uno's 5 V reference: that can damage the board."
    );
    assert_eq!(e[0].hint, "Make R1 bigger or R2 smaller: R1 at least 15200 Ω with this R2.");
    assert_eq!(e[0].setting.as_deref(), Some("r1-ohm"));
}

#[test]
fn strips_that_turn_interrupts_off_are_warned_about() {
    let mega = board("mega2560");
    let p = plan("mega2560", vec![("lights", part("strip", &[("din", "D30")]))]);
    let w = check(&mega, &p, &catalog(), &[]);
    assert_eq!(
        w.iter().find(|p| p.code == "interrupts-off").unwrap().message,
        "lights turns interrupts off while it updates (about 30 µs per LED), so bytes from Faber can be lost on a long strip."
    );
    let busy = plan(
        "mega2560",
        vec![
            ("lights", part("strip", &[("din", "D30")])),
            ("left", part("motor", &[("pwm", "D5"), ("dir", "D31")])),
        ],
    );
    let w = check(&mega, &busy, &catalog(), &[]);
    assert!(w.iter().find(|p| p.code == "interrupts-off").unwrap().message.contains("and encoder edges"));
}

#[test]
fn power_and_logic_levels_are_warned_about() {
    let uno = board("uno");
    let p = plan("uno", vec![("a", part("heater", &[("on", "D4")])), ("b", part("heater", &[("on", "D7")]))]);
    let w = check(&uno, &p, &catalog(), &[]);
    assert_eq!(codes(&w), ["power-budget"]);
    assert_eq!(w[0].severity, Severity::Warning);
    // A 5 V sonar on the Pi's 3.3 V header.
    let header = [
        HeaderPosition { physical: 11, name: "GPIO17", line: Some(17) },
        HeaderPosition { physical: 13, name: "GPIO27", line: Some(27) },
    ];
    let pi = pins::pi_header(header);
    let mut sonar = Plan::new(pins::PI_HEADER_ID, "pi");
    sonar.parts.insert("front".into(), part("ultrasonic", &[("trig", "GPIO17"), ("echo", "GPIO27")]));
    let w = check(&pi, &sonar.normalized(&pi, &catalog()), &catalog(), &[]);
    assert_eq!(codes(&w), ["level-shift-needed"]);
    assert_eq!(
        w[0].message,
        "front: its signals are 5 V, but the Raspberry Pi 40-pin header's pins take 3.3 V."
    );
}
