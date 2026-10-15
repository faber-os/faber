//! The connectors built into this core: the LD19, plus the generic parts that can only be
//! simulated until bridges arrive (docs/simulation.md, section 3). From Phase 3 connectors come
//! from packages.

pub mod generic;

use faber_connector::{Definition, Problem};

/// An installed connector folder (`<data>/connectors/<id>/`) that couldn't be used, and why. The
/// core runs without it. Every engine that loads installed connectors (wasm, declarative) reports
/// with this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadProblem {
    /// The folder's name, e.g. `my-sensor`.
    pub folder: String,
    pub problem: Problem,
}

/// How a connector folder's files are named in a message, so a problem points at the file the
/// person has: `connectors/my-sensor/faber.toml` once installed, or
/// `my-sensor-0.1.0.faberpkg: faber.toml` while a package is still being checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownAs(String);

impl ShownAs {
    /// An installed connector, in `<data>/connectors/<folder>/`.
    pub fn installed(folder: &str) -> ShownAs {
        ShownAs(format!("connectors/{folder}/"))
    }

    /// The files inside a package file, e.g. `my-sensor-0.1.0.faberpkg`.
    pub fn package(file: &str) -> ShownAs {
        ShownAs(format!("{file}: "))
    }

    /// One file in the folder, e.g. `faber.toml`.
    pub fn file(&self, name: &str) -> String {
        format!("{}{name}", self.0)
    }
}

pub fn built_in() -> Vec<Definition> {
    let mut defs = vec![ldrobot_ld19::definition()];
    defs.extend(generic::definitions());
    defs
}

#[cfg(test)]
mod tests {
    #[test]
    fn files_are_named_where_the_person_finds_them() {
        assert_eq!(
            super::ShownAs::installed("my-sensor").file("faber.toml"),
            "connectors/my-sensor/faber.toml"
        );
        assert_eq!(
            super::ShownAs::package("my-sensor-0.1.0.faberpkg").file("faber.toml"),
            "my-sensor-0.1.0.faberpkg: faber.toml"
        );
    }

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
