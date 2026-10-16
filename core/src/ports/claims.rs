//! Who is using which port (docs/architecture.md §10). One user per port, except an I²C bus,
//! which is claimed per address: two parts on one bus at different addresses are fine, two at the
//! same address are a conflict.

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

/// A whole port (`None`), or one 7-bit address on an I²C bus.
type Key = (PortId, Option<u8>);

#[derive(Debug, Default)]
pub struct Claims {
    by_port: BTreeMap<Key, Claimant>,
}

impl Claims {
    /// Claims the whole port. Refused while anyone holds it or any address on it.
    pub fn claim(&mut self, port: &PortId, who: Claimant) -> Result<(), ClaimError> {
        self.claim_key((port.clone(), None), who)
    }

    /// Claims one address on an I²C bus. Refused while someone holds that address or the whole
    /// bus; other addresses on the bus don't matter.
    pub fn claim_address(&mut self, port: &PortId, address: u8, who: Claimant) -> Result<(), ClaimError> {
        self.claim_key((port.clone(), Some(address)), who)
    }

    fn claim_key(&mut self, key: Key, who: Claimant) -> Result<(), ClaimError> {
        if let Some(existing) = self.by_port.get(&key) {
            return if *existing == who { Ok(()) } else { Err(ClaimError::Taken(existing.clone())) };
        }
        let in_the_way = match key.1 {
            // The whole port: anyone on any of its addresses is in the way.
            None => self.by_port.iter().find(|((p, _), _)| *p == key.0).map(|(_, c)| c),
            // One address: only someone holding the whole port is.
            Some(_) => self.by_port.get(&(key.0.clone(), None)),
        };
        if let Some(other) = in_the_way {
            return Err(ClaimError::Taken(other.clone()));
        }
        self.by_port.insert(key, who);
        Ok(())
    }

    /// Releases the whole port if `who` holds it. Releasing someone else's claim does nothing.
    pub fn release(&mut self, port: &PortId, who: &Claimant) {
        self.release_key(&(port.clone(), None), who);
    }

    /// Releases one address if `who` holds it.
    pub fn release_address(&mut self, port: &PortId, address: u8, who: &Claimant) {
        self.release_key(&(port.clone(), Some(address)), who);
    }

    fn release_key(&mut self, key: &Key, who: &Claimant) {
        if self.by_port.get(key) == Some(who) {
            self.by_port.remove(key);
        }
    }

    /// Who holds the whole port.
    pub fn holder(&self, port: &PortId) -> Option<&Claimant> {
        self.by_port.get(&(port.clone(), None))
    }

    /// Who holds one address on a bus: its own claimant, or whoever holds the whole bus.
    pub fn address_holder(&self, port: &PortId, address: u8) -> Option<&Claimant> {
        self.by_port.get(&(port.clone(), Some(address))).or_else(|| self.holder(port))
    }

    /// The addresses claimed on a bus, lowest first.
    pub fn addresses(&self, port: &PortId) -> Vec<(u8, &Claimant)> {
        self.by_port
            .iter()
            .filter(|((p, _), _)| p == port)
            .filter_map(|((_, a), c)| a.map(|a| (a, c)))
            .collect()
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

    #[test]
    fn an_i2c_bus_is_shared_by_address() {
        let bus = PortId::I2c(1);
        let mut c = Claims::default();
        c.claim_address(&bus, 0x48, part("cabin_temp")).unwrap();
        c.claim_address(&bus, 0x76, part("weather")).unwrap();
        assert_eq!(
            c.claim_address(&bus, 0x48, part("other_temp")),
            Err(ClaimError::Taken(part("cabin_temp"))),
            "two parts at one address are a conflict"
        );
        assert_eq!(c.claim_address(&bus, 0x48, part("cabin_temp")), Ok(()));
        assert_eq!(c.claim(&bus, Claimant::Tool("x")), Err(ClaimError::Taken(part("cabin_temp"))));
        assert_eq!(c.holder(&bus), None, "nobody holds the whole bus");
        let held: Vec<_> = c.addresses(&bus).into_iter().map(|(a, who)| (a, who.to_string())).collect();
        assert_eq!(held, [(0x48, "cabin_temp".to_string()), (0x76, "weather".to_string())]);
        assert!(c.addresses(&PortId::I2c(0)).is_empty(), "another bus is another bus");
        c.claim_address(&PortId::I2c(0), 0x48, part("other_temp")).unwrap();

        c.release_address(&bus, 0x48, &part("weather"));
        assert_eq!(c.address_holder(&bus, 0x48), Some(&part("cabin_temp")), "only its holder releases it");
        c.release_address(&bus, 0x48, &part("cabin_temp"));
        assert_eq!(c.address_holder(&bus, 0x48), None);
    }

    #[test]
    fn a_whole_bus_claim_holds_every_address() {
        let bus = PortId::I2c(1);
        let mut c = Claims::default();
        c.claim(&bus, Claimant::Tool("everything")).unwrap();
        assert_eq!(
            c.claim_address(&bus, 0x48, part("cabin_temp")),
            Err(ClaimError::Taken(Claimant::Tool("everything")))
        );
        assert_eq!(c.address_holder(&bus, 0x10), Some(&Claimant::Tool("everything")));
    }
}
