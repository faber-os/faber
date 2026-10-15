//! The connectors built into this core: the LD19, plus the generic parts that can only be
//! simulated until bridges arrive (docs/simulation.md, section 3). From Phase 3 connectors come
//! from packages.

pub mod generic;

use faber_connector::Definition;

pub fn built_in() -> Vec<Definition> {
    let mut defs = vec![ldrobot_ld19::definition()];
    defs.extend(generic::definitions());
    defs
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_built_in_manifest_is_valid_and_unique() {
        let defs = super::built_in();
        let mut ids: Vec<_> = defs.iter().map(|d| d.manifest.id.clone()).collect();
        for d in &defs {
            assert_eq!(d.manifest.validate(), Ok(()), "{}", d.manifest.id);
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), defs.len());
    }

    #[test]
    fn the_generic_parts_are_built_in() {
        let ids: Vec<_> = super::built_in().into_iter().map(|d| d.manifest.id).collect();
        for id in [super::generic::PLANAR_BASE, super::generic::MOTOR, super::generic::DISTANCE] {
            assert!(ids.iter().any(|i| i == id), "{id}");
        }
    }
}
