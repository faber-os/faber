//! Who is using which port. One user per port, always (docs/architecture.md §10).

use super::PortId;
use crate::names::PartName;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claimant {
    Part(PartName),
    /// A built-in tool, e.g. the serial monitor.
    Tool(&'static str),
}

impl fmt::Display for Claimant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Claimant::Part(p) => write!(f, "{p}"),
            Claimant::Tool(t) => write!(f, "tool:{t}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClaimError {
    Taken(Claimant),
}

#[derive(Debug, Default)]
pub struct Claims {
    by_port: BTreeMap<PortId, Claimant>,
}

impl Claims {
    pub fn claim(&mut self, port: &PortId, who: Claimant) -> Result<(), ClaimError> {
        match self.by_port.get(port) {
            Some(existing) if *existing == who => Ok(()),
            Some(existing) => Err(ClaimError::Taken(existing.clone())),
            None => {
                self.by_port.insert(port.clone(), who);
                Ok(())
            }
        }
    }

    /// Releases the port if `who` holds it. Releasing someone else's claim does nothing.
    pub fn release(&mut self, port: &PortId, who: &Claimant) {
        if self.by_port.get(port) == Some(who) {
            self.by_port.remove(port);
        }
    }

    pub fn holder(&self, port: &PortId) -> Option<&Claimant> {
        self.by_port.get(port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(n: &str) -> Claimant {
        Claimant::Part(PartName::parse(n).unwrap())
    }

    #[test]
    fn one_claimant_per_port() {
        let port = PortId::Serial("COM3".into());
        let mut c = Claims::default();
        c.claim(&port, part("lidar")).unwrap();
        assert_eq!(c.claim(&port, part("lidar")), Ok(()), "claiming again is harmless");
        assert_eq!(c.claim(&port, Claimant::Tool("serial-monitor")), Err(ClaimError::Taken(part("lidar"))));
        assert_eq!(c.holder(&port).unwrap().to_string(), "lidar");
    }

    #[test]
    fn only_the_holder_can_release() {
        let port = PortId::Serial("COM3".into());
        let mut c = Claims::default();
        c.claim(&port, Claimant::Tool("serial-monitor")).unwrap();
        c.release(&port, &part("lidar"));
        assert_eq!(c.holder(&port).unwrap().to_string(), "tool:serial-monitor");
        c.release(&port, &Claimant::Tool("serial-monitor"));
        assert_eq!(c.holder(&port), None);
    }
}
