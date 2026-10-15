//! Runs a declarative connector: bytes in, lines, readings out. Like every connector it does no
//! I/O; the core hands it bytes and the time they arrived.

use super::lines::{Frame, Framer};
use super::manifest::{Declarative, Derive, FieldRule, Parse, Rule, provide};
use super::nmea;
use super::values;
use faber_capabilities::{Info, Reading, Timestamp, battery};
use faber_connector::settings::Reader;
use faber_connector::{
    Connector, Context, Health, PortRequirement, Problem, Provide, Settings, SettingsError,
};
use serde_json::{Map, Value};
use std::sync::Arc;

/// Lines shown in a problem are cut to this many characters.
const SHOWN_CHARS: usize = 60;

impl Declarative {
    /// "the Acme ranger connector", for messages about settings.
    fn what(&self) -> String {
        format!("the {} connector", self.manifest.name)
    }

    /// What a part with these settings provides.
    pub fn provides(&self, settings: &Settings) -> Result<Vec<Provide>, SettingsError> {
        let specs = &self.manifest.settings;
        let reader = Reader::new(settings, specs, &self.what())?;
        self.rules.iter().map(|r| provide(r, &reader, specs)).collect()
    }

    /// A running connector for one part.
    pub fn create(self: &Arc<Self>, settings: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
        let specs = &self.manifest.settings;
        let reader = Reader::new(settings, specs, &self.what())?;
        let mut rules = Vec::new();
        for (rule, provided) in self.rules.iter().zip(self.provides(settings)?) {
            let parse = match &rule.parse {
                Parse::Nmea => RunParse::Nmea,
                Parse::Pattern { fields, set, derive, .. } => RunParse::Pattern {
                    scales: fields
                        .iter()
                        .map(|f| Ok((f.scale.resolve(&reader)?, f.offset.resolve(&reader)?)))
                        .collect::<Result<_, SettingsError>>()?,
                    set: set.resolve(&reader, specs)?,
                    derive: *derive,
                    level: None,
                },
            };
            rules.push(RunRule { info: provided.info, parse });
        }
        Ok(Box::new(LineConnector::new(Arc::clone(self), rules)))
    }
}

/// A rule with this part's settings filled in.
struct RunRule {
    info: Option<Info>,
    parse: RunParse,
}

enum RunParse {
    Pattern {
        /// (scale, offset) per pattern field, in order.
        scales: Vec<(f64, f64)>,
        set: Map<String, Value>,
        derive: Derive,
        /// The battery level reported last, for [`Derive::LevelFromVolts`]'s hysteresis.
        level: Option<battery::Level>,
    },
    Nmea,
}

/// Why a line wasn't used.
struct BadLine {
    line: String,
    why: String,
}

pub struct LineConnector {
    package: Arc<Declarative>,
    rules: Vec<RunRule>,
    framer: Framer,
    nmea: nmea::Assembler,
    reported: Option<Health>,
    started_us: Option<Timestamp>,
    last_bytes_us: Option<Timestamp>,
    last_good_us: Option<Timestamp>,
    good_lines: u64,
    bad_lines: u64,
    bad_in_a_row: u32,
    last_bad: Option<BadLine>,
}

impl LineConnector {
    fn new(package: Arc<Declarative>, rules: Vec<RunRule>) -> LineConnector {
        let framer = Framer::new(package.lines.ending, package.lines.max_length);
        LineConnector {
            package,
            rules,
            framer,
            nmea: nmea::Assembler::default(),
            reported: None,
            started_us: None,
            last_bytes_us: None,
            last_good_us: None,
            good_lines: 0,
            bad_lines: 0,
            bad_in_a_row: 0,
            last_bad: None,
        }
    }

    fn report(&mut self, ctx: &mut dyn Context, health: Health) {
        if self.reported.as_ref() != Some(&health) {
            ctx.set_health(health.clone());
            self.reported = Some(health);
        }
    }

    fn baud(&self) -> String {
        match self.package.manifest.port {
            PortRequirement::Serial { baud } => format!("{baud} baud"),
            PortRequirement::Bridge => "its bridge".into(),
        }
    }

    /// How it's doing from the lines alone (silence is judged on ticks).
    fn line_health(&self) -> Health {
        if self.bad_in_a_row >= self.package.lines.warn_after_bad_lines {
            let last = self.last_bad.as_ref().expect("a bad line was seen");
            return Health::Degraded(Problem::new(
                format!(
                    "The last {} lines from the sensor couldn't be read ({} bad lines so far). The latest: \"{}\" ({}).",
                    self.bad_in_a_row, self.bad_lines, last.line, last.why
                ),
                format!(
                    "Check that the port runs at {} and that the sensor sends what the connector's faber.toml expects. The serial monitor shows exactly what arrives.",
                    self.baud()
                ),
            ));
        }
        if self.good_lines == 0 { Health::Starting } else { Health::Running }
    }

    fn handle_line(&mut self, ctx: &mut dyn Context, t_us: Timestamp, bytes: &[u8]) {
        let Ok(line) = std::str::from_utf8(bytes) else {
            return self.bad(String::from_utf8_lossy(bytes).into_owned(), "it isn't text".into());
        };
        let mut why = None;
        let mut used = false;
        for (rule, run) in self.package.rules.iter().zip(&mut self.rules) {
            match &mut run.parse {
                RunParse::Nmea => match nmea::parse(line) {
                    Ok(sentence) => {
                        used = true;
                        if let Some(r) = self.nmea.push(t_us, sentence) {
                            ctx.publish(&rule.provide, Reading::Position(r));
                        }
                    }
                    Err(e) => {
                        why.get_or_insert(e);
                    }
                },
                RunParse::Pattern { scales, set, derive, level } => {
                    match pattern_reading(rule, run.info.as_ref(), scales, set, (*derive, level), t_us, line)
                    {
                        Ok(None) => {}
                        Ok(Some(r)) => {
                            used = true;
                            ctx.publish(&rule.provide, r);
                        }
                        Err(e) => {
                            why = Some(e);
                        }
                    }
                }
            }
        }
        if used {
            self.good_lines += 1;
            self.bad_in_a_row = 0;
            self.last_good_us = Some(t_us);
        } else {
            let why = why.unwrap_or_else(|| "it matches none of the connector's patterns".into());
            self.bad(line.to_string(), why);
        }
    }

    fn bad(&mut self, line: String, why: String) {
        self.bad_lines += 1;
        self.bad_in_a_row = self.bad_in_a_row.saturating_add(1);
        let mut shown: String = line.chars().take(SHOWN_CHARS).collect();
        if shown.len() < line.len() {
            shown.push('…');
        }
        self.last_bad = Some(BadLine { line: shown, why });
    }
}

/// The reading a pattern rule makes of `line`: `Ok(None)` if the line has another shape.
fn pattern_reading(
    rule: &Rule,
    info: Option<&Info>,
    scales: &[(f64, f64)],
    set: &Map<String, Value>,
    (derive, level): (Derive, &mut Option<battery::Level>),
    t_us: Timestamp,
    line: &str,
) -> Result<Option<Reading>, String> {
    let Parse::Pattern { pattern, fields, .. } = &rule.parse else { unreachable!("a pattern rule") };
    let Some(raw) = pattern.read(line)? else { return Ok(None) };
    let mut obj = set.clone();
    for ((field, (_, raw)), (scale, offset)) in fields.iter().zip(raw).zip(scales) {
        values::set_path(&mut obj, &field.name, value(field, raw, *scale, *offset));
    }
    match derive {
        Derive::StatusFromRange => values::set_path(&mut obj, "status", "ok".into()),
        Derive::LevelFromVolts => values::set_path(&mut obj, "level", "ok".into()),
        Derive::Nothing => {}
    }
    let mut reading = values::reading(rule.capability, t_us, obj)?;
    match (derive, &mut reading, info) {
        (Derive::StatusFromRange, Reading::Distance(d), Some(Info::Distance(info))) => {
            *d = info.reading(t_us, d.meters);
        }
        (Derive::LevelFromVolts, Reading::Battery(b), Some(Info::Battery(info))) => {
            b.level = info.level(b.volts, *level);
            *level = Some(b.level);
        }
        _ => {}
    }
    debug_assert_eq!(reading.capability(), rule.capability);
    Ok(Some(reading))
}

fn value(field: &FieldRule, raw: f64, scale: f64, offset: f64) -> Value {
    if field.stays_whole() {
        return Value::from(raw as i64);
    }
    Value::from(raw * scale + offset)
}

impl Connector for LineConnector {
    fn on_bytes(&mut self, ctx: &mut dyn Context, t_us: Timestamp, bytes: &[u8]) {
        self.started_us.get_or_insert(t_us);
        self.last_bytes_us = Some(t_us);
        let mut lines: Vec<Option<Vec<u8>>> = Vec::new();
        self.framer.push(bytes, |f| {
            lines.push(match f {
                Frame::Line(l) => Some(l.to_vec()),
                Frame::TooLong => None,
            })
        });
        // Every line is stamped with the time of the chunk that ended it: when its last byte
        // arrived.
        for line in lines {
            match line {
                Some(l) => self.handle_line(ctx, t_us, &l),
                None => {
                    let max = self.package.lines.max_length;
                    self.bad(String::new(), format!("it is longer than {max} bytes"));
                    if let Some(b) = &mut self.last_bad {
                        b.line = format!("(a line over {max} bytes)");
                    }
                }
            }
        }
        let health = self.line_health();
        self.report(ctx, health);
    }

    fn on_tick(&mut self, ctx: &mut dyn Context, now_us: Timestamp) {
        let started = *self.started_us.get_or_insert(now_us);
        let since = |t: Option<Timestamp>| now_us.saturating_sub(t.unwrap_or(started));
        let timeout = self.package.lines.timeout_us;
        let seconds = timeout as f64 / 1e6;
        let health = if since(self.last_bytes_us) >= timeout {
            Health::Failed(Problem::new(
                format!("Nothing has arrived from the sensor for {seconds} s."),
                "Check that the sensor has power, that its TX goes to the port's RX, and that this is the right port.",
            ))
        } else if since(self.last_good_us) >= timeout {
            Health::Failed(Problem::new(
                format!("Data arrives, but no line could be read for {seconds} s."),
                format!(
                    "Check that the port runs at {} and that the sensor sends what the connector's faber.toml expects. The serial monitor shows what arrives.",
                    self.baud()
                ),
            ))
        } else {
            self.line_health()
        };
        self.report(ctx, health);
    }
}
