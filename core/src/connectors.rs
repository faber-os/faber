//! The connectors built into this core (Phase 1). From Phase 3 they come from packages.

use faber_connector::Definition;

pub fn built_in() -> Vec<Definition> {
    vec![ldrobot_ld19::definition()]
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
        ids.dedup();
        assert_eq!(ids.len(), defs.len());
    }
}
