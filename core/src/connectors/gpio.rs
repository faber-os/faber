//! GPIO connectors built into the core: one header pin as a digital input (a button, a limit
//! switch) or a digital output (a relay, an LED). The core lends each part its own line
//! ([`faber_connector::bus`]); these only say what to do with it.
//!
//! Levels in readings and commands are *logical* (docs/capabilities.md, "GPIO level"): `true`
//! is active. `active-low` flips the pin, so a button to ground with the pull-up on reads `true`
//! while it is pressed, and an active-low relay board switches on with a low pin.

use faber_capabilities::{Capability, Command, Reading, Timestamp, digital_in, digital_out};
use faber_connector::bus::{Bias, BusError, GpioLine, PortAccess};
use faber_connector::settings::Reader;
use faber_connector::{
    Applied, Choice, CommandError, Connector, Context, Definition, Health, Manifest, PortRequirement,
    Problem, Provide, SettingSpec, Settings, SettingsError,
};

pub const INPUT: &str = "gpio-input";
pub const OUTPUT: &str = "gpio-output";

pub fn definitions() -> Vec<Definition> {
    vec![
        Definition::new(
            manifest(INPUT, "GPIO input (button, switch)", Capability::DigitalIn, input_settings()),
            create_input,
        ),
        Definition::new(
            manifest(OUTPUT, "GPIO output (relay, LED)", Capability::DigitalOut, output_settings()),
            create_output,
        ),
    ]
}

fn manifest(id: &str, name: &str, capability: Capability, settings: Vec<SettingSpec>) -> Manifest {
    Manifest {
        id: id.into(),
        name: name.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        port: PortRequirement::Gpio,
        provides: vec![Provide::default_for(capability)],
        settings,
        usb_hints: Vec::new(),
    }
}

fn active_low(default: bool, description: &str) -> SettingSpec {
    SettingSpec::bool("active-low", "Active when low", description, default)
}

/// Defaults for the most common input: a button between the pin and ground.
fn input_settings() -> Vec<SettingSpec> {
    vec![
        SettingSpec::choice(
            "pull",
            "Pull resistor",
            "The Pi's own resistor that holds the pin at a level while nothing drives it. A button to ground needs Up.",
            "up",
            vec![
                Choice::new("up", "Up (to 3.3 V)"),
                Choice::new("down", "Down (to ground)"),
                Choice::new("none", "None (something else drives the pin)"),
            ],
        ),
        active_low(
            true,
            "On for a button to ground: the pin is low while it is pressed, which then reads as on.",
        ),
    ]
}

fn output_settings() -> Vec<SettingSpec> {
    vec![active_low(
        false,
        "On for a relay board that switches on with a low pin (many do). Off means a high pin is on.",
    )]
}

fn create_input(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    let specs = input_settings();
    let r = Reader::new(s, &specs, "a GPIO input")?;
    let bias = match r.choice("pull")? {
        "up" => Bias::PullUp,
        "down" => Bias::PullDown,
        _ => Bias::None,
    };
    Ok(Box::new(Input { bias, active_low: r.bool("active-low")?, ready: false }))
}

fn create_output(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    let specs = output_settings();
    let r = Reader::new(s, &specs, "a GPIO output")?;
    Ok(Box::new(Output { active_low: r.bool("active-low")?, ready: false, last_tick_us: 0 }))
}

/// The part's line, or why there is none.
fn line(ctx: &mut dyn Context) -> Result<&mut dyn GpioLine, BusError> {
    match ctx.port() {
        Some(PortAccess::Gpio(l)) => Ok(l),
        _ => Err(BusError::new("the part has no GPIO line")),
    }
}

fn failed(e: &BusError) -> Health {
    Health::Failed(Problem::new(
        format!("The GPIO line doesn't work: {e}."),
        "Check that no other program uses the pin. Faber tries again when the part restarts.",
    ))
}

struct Input {
    bias: Bias,
    active_low: bool,
    ready: bool,
}

impl Input {
    fn publish(&self, ctx: &mut dyn Context, t_us: Timestamp, high: bool) {
        let level = high != self.active_low;
        ctx.publish("digital-in", Reading::DigitalIn(digital_in::Reading { t_us, level }));
    }

    fn tick(&mut self, ctx: &mut dyn Context, now_us: Timestamp) -> Result<(), BusError> {
        if !self.ready {
            let l = line(ctx)?;
            l.set_input(self.bias)?;
            let high = l.get()?;
            l.edges()?; // what happened before now is in `high`
            self.publish(ctx, now_us, high);
            self.ready = true;
            ctx.set_health(Health::Running);
            return Ok(());
        }
        // Every edge since the last tick, in order, so a short press is never lost.
        for high in line(ctx)?.edges()? {
            self.publish(ctx, now_us, high);
        }
        Ok(())
    }
}

impl Connector for Input {
    fn on_bytes(&mut self, _: &mut dyn Context, _: Timestamp, _: &[u8]) {}

    fn on_tick(&mut self, ctx: &mut dyn Context, now_us: Timestamp) {
        if let Err(e) = self.tick(ctx, now_us) {
            ctx.set_health(failed(&e));
            self.ready = false;
        }
    }
}

struct Output {
    active_low: bool,
    ready: bool,
    /// The core's time at the last tick, for the reading a command makes (a command has none).
    last_tick_us: Timestamp,
}

impl Output {
    fn high(&self, level: bool) -> bool {
        level != self.active_low
    }

    fn report(ctx: &mut dyn Context, t_us: Timestamp, level: bool) {
        ctx.publish("digital-out", Reading::DigitalOut(digital_out::Reading { t_us, level }));
    }
}

impl Connector for Output {
    fn on_bytes(&mut self, _: &mut dyn Context, _: Timestamp, _: &[u8]) {}

    /// Starts at the safe level (inactive), before anyone can lease it.
    fn on_tick(&mut self, ctx: &mut dyn Context, now_us: Timestamp) {
        self.last_tick_us = now_us;
        if self.ready {
            return;
        }
        let safe = self.high(false);
        match line(ctx).and_then(|l| l.set_output(safe)) {
            Ok(()) => {
                self.ready = true;
                Output::report(ctx, now_us, false);
                ctx.set_health(Health::Running);
            }
            Err(e) => ctx.set_health(failed(&e)),
        }
    }

    fn command(&mut self, ctx: &mut dyn Context, command: Command) -> Result<Applied, CommandError> {
        let Command::DigitalOut(digital_out::Command::Set { level }) = command else {
            return Err(CommandError::Unsupported);
        };
        if !self.ready {
            return Err(CommandError::NotReady(Problem::new(
                "The output isn't set up yet.",
                "Wait a moment; it is ready as soon as the part runs.",
            )));
        }
        let high = self.high(level);
        line(ctx).and_then(|l| l.set(high)).map_err(|e| {
            CommandError::NotReady(Problem::new(
                format!("The pin couldn't be set: {e}."),
                "Check the part in Parts.",
            ))
        })?;
        Output::report(ctx, self.last_tick_us, level);
        Ok(Applied { command })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_connector::testing::{Device, RecordingContext};
    use std::sync::{Arc, Mutex};

    /// One line's pin, shared with the test.
    #[derive(Default)]
    struct PinState {
        /// The level driven from outside (or by the pull resistor).
        driven: bool,
        /// The output level, once it is an output.
        output: Option<bool>,
        /// Edges not yet collected.
        edges: Vec<bool>,
    }

    #[derive(Clone, Default)]
    struct Pin(Arc<Mutex<PinState>>);

    impl Pin {
        fn drive(&self, high: bool) {
            let mut p = self.0.lock().unwrap();
            p.driven = high;
            p.edges.push(high);
        }
        fn level(&self) -> bool {
            let p = self.0.lock().unwrap();
            p.output.unwrap_or(p.driven)
        }
    }

    impl GpioLine for Pin {
        fn line(&self) -> u32 {
            17
        }
        fn set_input(&mut self, bias: Bias) -> Result<(), BusError> {
            self.0.lock().unwrap().driven = bias == Bias::PullUp;
            Ok(())
        }
        fn set_output(&mut self, high: bool) -> Result<(), BusError> {
            self.0.lock().unwrap().output = Some(high);
            Ok(())
        }
        fn get(&mut self) -> Result<bool, BusError> {
            Ok(self.level())
        }
        fn set(&mut self, high: bool) -> Result<(), BusError> {
            self.0.lock().unwrap().output = Some(high);
            Ok(())
        }
        fn edges(&mut self) -> Result<Vec<bool>, BusError> {
            Ok(std::mem::take(&mut self.0.lock().unwrap().edges))
        }
    }

    fn ctx_on(pin: &Pin) -> RecordingContext {
        RecordingContext { device: Some(Device::Gpio(Box::new(pin.clone()))), ..Default::default() }
    }

    fn levels(ctx: &RecordingContext) -> Vec<bool> {
        ctx.published
            .iter()
            .map(|(_, r)| match r {
                Reading::DigitalIn(r) => r.level,
                Reading::DigitalOut(r) => r.level,
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_button_to_ground_reads_on_while_pressed_and_no_press_is_lost() {
        let pin = Pin::default();
        let mut ctx = ctx_on(&pin);
        let mut c = create_input(&Settings::new()).unwrap();
        c.on_tick(&mut ctx, 1);
        assert_eq!(levels(&ctx), [false], "pulled up and not pressed: off");
        assert_eq!(ctx.last_health(), Some(&Health::Running));
        pin.drive(false); // pressed
        pin.drive(true); // released, before the next tick
        c.on_tick(&mut ctx, 2);
        assert_eq!(levels(&ctx), [false, true, false], "the short press is there");
        c.on_tick(&mut ctx, 3);
        assert_eq!(ctx.published.len(), 3, "nothing new, nothing published");
    }

    #[test]
    fn an_input_without_active_low_reads_the_pin() {
        let pin = Pin::default();
        let mut ctx = ctx_on(&pin);
        let s: Settings = toml::from_str("pull = \"down\"\nactive-low = false").unwrap();
        let mut c = create_input(&s).unwrap();
        c.on_tick(&mut ctx, 1);
        pin.drive(true);
        c.on_tick(&mut ctx, 2);
        assert_eq!(levels(&ctx), [false, true]);
        assert!(create_input(&toml::from_str("pull = \"sideways\"").unwrap()).is_err());
    }

    #[test]
    fn an_output_starts_inactive_and_active_low_flips_the_pin() {
        let pin = Pin::default();
        let mut ctx = ctx_on(&pin);
        let mut c = create_output(&toml::from_str("active-low = true").unwrap()).unwrap();
        let on = Command::DigitalOut(digital_out::Command::Set { level: true });
        assert!(matches!(c.command(&mut ctx, on), Err(CommandError::NotReady(_))), "not before it is set up");
        c.on_tick(&mut ctx, 1);
        assert!(pin.level(), "inactive is a high pin when active-low");
        assert_eq!(c.command(&mut ctx, on), Ok(Applied { command: on }));
        assert!(!pin.level());
        assert_eq!(levels(&ctx), [false, true]);
        let motor = Command::Motor(faber_capabilities::motor::Command::SetDuty { duty: 0.0 });
        assert_eq!(c.command(&mut ctx, motor), Err(CommandError::Unsupported));
    }

    #[test]
    fn a_part_without_its_line_fails_with_a_hint() {
        let mut ctx = RecordingContext::new();
        let mut c = create_output(&Settings::new()).unwrap();
        c.on_tick(&mut ctx, 1);
        let Some(Health::Failed(p)) = ctx.last_health() else { panic!("{:?}", ctx.health) };
        assert!(p.message.contains("no GPIO line"), "{p:?}");
    }
}
