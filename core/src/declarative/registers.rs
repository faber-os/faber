//! Runs a declarative I²C connector: reads each provide's register on its schedule, through the
//! device the core lent the part ([`faber_connector::Context::port`]), and publishes readings.
//! It only ever *reads*: a register read writes the register's number, then reads its bytes.

use super::engine::finish;
use super::manifest::{Declarative, Parse, RegisterRead};
use super::values;
use faber_capabilities::{Info, Timestamp, battery};
use faber_connector::bus::PortAccess;
use faber_connector::settings::Reader;
use faber_connector::{Connector, Context, Health, Problem, Settings, SettingsError};
use std::sync::Arc;

/// Failed reads in a row that make the part "failed" (one is only "degraded").
const FAIL_AFTER: u32 = 3;

struct Polled {
    provide: String,
    capability: faber_capabilities::Capability,
    info: Option<Info>,
    read: RegisterRead,
    scale: f64,
    offset: f64,
    set: serde_json::Map<String, serde_json::Value>,
    level: Option<battery::Level>,
    next_us: Option<Timestamp>,
}

pub fn create(package: &Arc<Declarative>, settings: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    let specs = &package.manifest.settings;
    let reader = Reader::new(settings, specs, &package.what())?;
    let mut polled = Vec::new();
    for (rule, provided) in package.rules.iter().zip(package.provides(settings)?) {
        let Parse::Register(read) = &rule.parse else {
            unreachable!("an I²C connector only has register rules")
        };
        polled.push(Polled {
            provide: rule.provide.clone(),
            capability: rule.capability,
            info: provided.info,
            scale: read.field.scale.resolve(&reader)?,
            offset: read.field.offset.resolve(&reader)?,
            set: read.set.resolve(&reader, specs)?,
            read: read.clone(),
            level: None,
            next_us: None,
        });
    }
    Ok(Box::new(RegisterConnector { polled, failures: 0, reported: None }))
}

struct RegisterConnector {
    polled: Vec<Polled>,
    failures: u32,
    reported: Option<Health>,
}

impl RegisterConnector {
    fn report(&mut self, ctx: &mut dyn Context, health: Health) {
        if self.reported.as_ref() != Some(&health) {
            ctx.set_health(health.clone());
            self.reported = Some(health);
        }
    }
}

/// Reads one register and makes its reading.
fn read_one(
    ctx: &mut dyn Context,
    p: &mut Polled,
    t_us: Timestamp,
) -> Result<faber_capabilities::Reading, String> {
    let Some(PortAccess::I2c(device)) = ctx.port() else {
        return Err("the part has no I²C device".into());
    };
    let mut buf = [0u8; 4];
    let bytes = &mut buf[..p.read.format.bytes()];
    device.write_read(&[p.read.register], bytes).map_err(|e| e.message)?;
    let raw = p.read.format.decode(bytes, p.read.shift);
    let mut obj = p.set.clone();
    values::set_path(&mut obj, &p.read.field.name, (raw * p.scale + p.offset).into());
    finish(p.capability, p.info.as_ref(), obj, (p.read.derive, &mut p.level), t_us)
}

impl Connector for RegisterConnector {
    fn on_bytes(&mut self, _: &mut dyn Context, _: Timestamp, _: &[u8]) {}

    fn on_tick(&mut self, ctx: &mut dyn Context, now_us: Timestamp) {
        let mut problem = None;
        let mut good = false;
        for p in &mut self.polled {
            if p.next_us.is_some_and(|due| now_us < due) {
                continue;
            }
            p.next_us = Some(now_us + p.read.period_us);
            match read_one(ctx, p, now_us) {
                Ok(r) => {
                    ctx.publish(&p.provide, r);
                    good = true;
                }
                Err(why) => problem = Some(why),
            }
        }
        let health = match problem {
            Some(why) => {
                self.failures += 1;
                let message = format!("The sensor didn't answer: {why}.");
                let hint = "Check its wiring (SDA, SCL, 3.3 V and ground) and its address. The I2C scanner shows what answers on the bus.";
                if self.failures >= FAIL_AFTER {
                    Health::Failed(Problem::new(message, hint))
                } else {
                    Health::Degraded(Problem::new(message, hint))
                }
            }
            None if good => {
                self.failures = 0;
                Health::Running
            }
            None => return,
        };
        self.report(ctx, health);
    }
}
