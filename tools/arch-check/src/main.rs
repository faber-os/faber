//! Enforces the crate dependency rules in docs/development.md ("Architecture rules").
//!
//! Run from the repository root: `cargo run -p faber-arch-check`. Exits non-zero and explains
//! every violation. Only normal and build dependencies count; dev-dependencies are free.

use serde_json::Value;
use std::process::{Command, ExitCode};

/// Which workspace crates may depend on what. `None` means anything is allowed.
#[derive(Debug, PartialEq)]
enum Rule {
    Only(&'static [&'static str]),
    Anything,
}

fn rule_for(name: &str, manifest_path: &str) -> Option<Rule> {
    let path = manifest_path.replace('\\', "/");
    // Connectors, and the core's misbehaving test connectors: bytes in, readings out, no I/O. The
    // wasm glue is only compiled for the wasm build of the same code.
    if path.contains("/connectors/") || path.contains("/wasm-guests/") {
        return Some(Rule::Only(&["faber-connector", "faber-capabilities", "toml", "faber-connector-wasm"]));
    }
    match name {
        "faber-capabilities" => Some(Rule::Only(&["serde", "ts-rs"])),
        "faber-connector" => Some(Rule::Only(&["faber-capabilities", "serde", "toml"])),
        "faber-connector-wasm" => {
            Some(Rule::Only(&["faber-connector", "faber-capabilities", "toml", "wit-bindgen"]))
        }
        "faber-core" => Some(Rule::Anything),
        "faber-arch-check" => Some(Rule::Only(&["serde_json"])),
        _ => None,
    }
}

/// Checks `cargo metadata --no-deps` output and returns one message per violation.
fn violations(metadata: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let Some(packages) = metadata["packages"].as_array() else {
        return vec!["cargo metadata output has no `packages` list".into()];
    };
    for pkg in packages {
        let name = pkg["name"].as_str().unwrap_or("?");
        let manifest = pkg["manifest_path"].as_str().unwrap_or("");
        let Some(rule) = rule_for(name, manifest) else {
            out.push(format!(
                "`{name}` has no dependency rule. Add it to docs/development.md and tools/arch-check."
            ));
            continue;
        };
        let Rule::Only(allowed) = rule else { continue };
        for dep in pkg["dependencies"].as_array().into_iter().flatten() {
            let is_dev = dep["kind"].as_str() == Some("dev");
            let dep_name = dep["name"].as_str().unwrap_or("?");
            if !is_dev && !allowed.contains(&dep_name) {
                out.push(format!(
                    "`{name}` depends on `{dep_name}`, which docs/development.md does not allow. \
                     Allowed: {}.",
                    allowed.join(", ")
                ));
            }
        }
    }
    out
}

fn main() -> ExitCode {
    let output =
        match Command::new(env!("CARGO")).args(["metadata", "--format-version", "1", "--no-deps"]).output() {
            Ok(o) if o.status.success() => o,
            Ok(o) => {
                eprintln!("cargo metadata failed:\n{}", String::from_utf8_lossy(&o.stderr));
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("could not run cargo metadata: {e}");
                return ExitCode::FAILURE;
            }
        };
    let metadata: Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cargo metadata returned invalid JSON: {e}");
            return ExitCode::FAILURE;
        }
    };
    let found = violations(&metadata);
    if found.is_empty() {
        println!("architecture: all crate dependencies follow docs/development.md");
        ExitCode::SUCCESS
    } else {
        for v in &found {
            eprintln!("architecture violation: {v}");
        }
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pkg(name: &str, path: &str, deps: &[(&str, Option<&str>)]) -> Value {
        json!({
            "name": name,
            "manifest_path": path,
            "dependencies": deps.iter().map(|(n, k)| json!({"name": n, "kind": k})).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn allowed_dependencies_pass() {
        let m = json!({"packages": [
            pkg("faber-capabilities", "/r/sdk/rust/capabilities/Cargo.toml", &[("serde", None), ("serde_json", Some("dev"))]),
            pkg("ldrobot-ld19", "/r/connectors/ldrobot-ld19/Cargo.toml", &[("faber-connector", None)]),
            pkg("faber-core", "/r/core/Cargo.toml", &[("tokio", None)]),
        ]});
        assert!(violations(&m).is_empty(), "{:?}", violations(&m));
    }

    #[test]
    fn connector_with_io_is_rejected() {
        let m = json!({"packages": [
            pkg("some-sensor", "C:\\r\\connectors\\some-sensor\\Cargo.toml", &[("serialport", None)]),
        ]});
        let v = violations(&m);
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("`some-sensor` depends on `serialport`"), "{v:?}");
    }

    #[test]
    fn sdk_may_not_depend_on_the_core() {
        let m = json!({"packages": [
            pkg("faber-connector", "/r/sdk/rust/connector/Cargo.toml", &[("faber-core", None)]),
        ]});
        assert_eq!(violations(&m).len(), 1);
    }

    #[test]
    fn the_wasm_glue_may_only_add_wit_bindgen() {
        let m = json!({"packages": [
            pkg("faber-connector-wasm", "/r/sdk/rust/connector-wasm/Cargo.toml", &[("wit-bindgen", None)]),
            pkg("ldrobot-ld19", "/r/connectors/ldrobot-ld19/Cargo.toml", &[("faber-connector-wasm", None)]),
            pkg("test-guest", "/r/core/tests/wasm-guests/x/Cargo.toml", &[("faber-connector-wasm", None)]),
        ]});
        assert!(violations(&m).is_empty(), "{:?}", violations(&m));
        let m = json!({"packages": [
            pkg("faber-connector-wasm", "/r/sdk/rust/connector-wasm/Cargo.toml", &[("tokio", None)]),
            pkg("test-guest", "/r/core/tests/wasm-guests/x/Cargo.toml", &[("serialport", None)]),
        ]});
        assert_eq!(violations(&m).len(), 2);
    }

    #[test]
    fn unknown_crates_need_a_rule() {
        let m = json!({"packages": [pkg("mystery", "/r/mystery/Cargo.toml", &[])]});
        assert!(violations(&m)[0].contains("no dependency rule"));
    }
}
