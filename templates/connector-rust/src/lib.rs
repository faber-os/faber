//! My sensor: a Faber connector, built as a sandboxed WebAssembly component.
//!
//! A connector never opens the port itself. Faber hands it the **bytes** that arrived and it
//! hands back **readings**. This one reads text lines like `D:123` (123 cm), the kind an Arduino
//! prints with `Serial.println`. Change [`parse_line`] for your device, and `faber.toml` for what
//! it provides. Guide: docs/your-first-connector.md in the Faber repository.

use faber_capabilities::{Capability, Info, Reading, Timestamp};
use faber_connector::{Connector, Context, Definition, Health, Manifest, Problem, Settings, SettingsError};

/// The manifest is `faber.toml`, read when the connector is built, so the two never disagree.
pub fn manifest() -> Manifest {
    faber_connector::manifest_file::parse(include_str!("../faber.toml"))
        .expect("faber.toml is valid (faber check says why when it isn't)")
        .manifest
}

pub fn definition() -> Definition {
    Definition::new(manifest(), create)
}

// The same code as a WebAssembly component: this line is all it takes.
#[cfg(target_arch = "wasm32")]
faber_connector_wasm::export!(definition);

fn create(settings: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    // Refuses settings the manifest doesn't declare, with a message for the user.
    faber_connector::settings::Reader::new(settings, &manifest().settings, "My sensor")?;
    // The range from faber.toml's info, to tell "too close" and "too far" from a real reading.
    let info = match manifest().provides.into_iter().next().and_then(|p| p.info) {
        Some(Info::Distance(info)) => info,
        _ => unreachable!("faber.toml gives distance@1 its info"),
    };
    Ok(Box::new(MySensor { info, line: Vec::new(), good_lines: 0, bad_lines: 0, health: None }))
}

/// One line from the device, or `None` if it isn't one we understand.
///
/// TODO: this is where your device's protocol goes. Here: `D:<centimetres>`.
pub fn parse_line(line: &str) -> Option<f32> {
    let cm: f32 = line.trim().strip_prefix("D:")?.parse().ok()?;
    Some(cm / 100.0)
}

/// Lines longer than this are noise (a wrong baud rate looks like one endless line).
const MAX_LINE: usize = 64;

struct MySensor {
    info: faber_capabilities::distance::Info,
    /// The bytes of the line that hasn't ended yet.
    line: Vec<u8>,
    good_lines: u64,
    bad_lines: u64,
    health: Option<Health>,
}

impl MySensor {
    fn report(&mut self, ctx: &mut dyn Context, health: Health) {
        if self.health.as_ref() != Some(&health) {
            ctx.set_health(health.clone());
            self.health = Some(health);
        }
    }

    fn handle_line(&mut self, ctx: &mut dyn Context, t_us: Timestamp) {
        let line = String::from_utf8_lossy(&self.line).into_owned();
        self.line.clear();
        if line.trim().is_empty() {
            return;
        }
        match parse_line(&line) {
            Some(meters) => {
                self.good_lines += 1;
                // Judges the value against the datasheet range: too close, too far or ok.
                let reading = self.info.reading(t_us, meters);
                ctx.publish(Capability::Distance.name(), Reading::Distance(reading));
            }
            None => self.bad_lines += 1,
        }
    }
}

impl Connector for MySensor {
    fn on_bytes(&mut self, ctx: &mut dyn Context, t_us: Timestamp, bytes: &[u8]) {
        for &b in bytes {
            match b {
                b'\n' => self.handle_line(ctx, t_us),
                _ if self.line.len() >= MAX_LINE => {
                    self.line.clear();
                    self.bad_lines += 1;
                }
                _ => self.line.push(b),
            }
        }
        let health = match (self.good_lines, self.bad_lines) {
            (0, 0) => Health::Starting,
            (0, _) => Health::Degraded(Problem::new(
                "Data arrives, but no line could be read.",
                "Check the baud rate in faber.toml, and look at the data in the serial monitor.",
            )),
            _ => Health::Running,
        };
        self.report(ctx, health);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_become_metres() {
        assert_eq!(parse_line("D:123\r"), Some(1.23));
        assert_eq!(parse_line("hello"), None);
    }
}
