//! From named values to a capability's reading or info, for any capability.
//!
//! Manifests name reading fields the way docs/capabilities.md does (`meters`, `latitude-deg`,
//! `position.x`). The capability types read camelCase JSON, so the names are converted and the
//! capability's own serde code does the checking: whatever it accepts is a valid reading.

use faber_capabilities::{Capability, Info, Reading, Timestamp};
use serde_json::{Map, Value};

/// `latitude-deg` → `latitudeDeg`.
pub fn camel(kebab: &str) -> String {
    let mut out = String::with_capacity(kebab.len());
    let mut upper = false;
    for c in kebab.chars() {
        if c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `latitudeDeg` → `latitude-deg`.
pub fn kebab(camel: &str) -> String {
    let mut out = String::with_capacity(camel.len() + 4);
    for c in camel.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A serde message about camelCase fields, in the manifest's kebab-case words.
pub fn kebab_message(message: &str) -> String {
    let mut out = String::new();
    for (i, part) in message.split('`').enumerate() {
        if i % 2 == 1 {
            out.push('`');
            out.push_str(&kebab(part));
            out.push('`');
        } else {
            out.push_str(part);
        }
    }
    out
}

/// Sets `path` (`position.x`, kebab-case) in `obj`, making nested objects on the way.
pub fn set_path(obj: &mut Map<String, Value>, path: &str, value: Value) {
    let mut parts = path.split('.').peekable();
    let mut here = obj;
    while let Some(p) = parts.next() {
        let key = camel(p);
        if parts.peek().is_none() {
            here.insert(key, value);
            return;
        }
        let next = here.entry(key).or_insert_with(|| Value::Object(Map::new()));
        if !next.is_object() {
            *next = Value::Object(Map::new());
        }
        here = next.as_object_mut().expect("just made an object");
    }
}

/// Removes `path` (kebab-case) from `obj`, if it is there.
pub fn remove_path(obj: &mut Map<String, Value>, path: &str) {
    match path.split_once('.') {
        None => {
            obj.remove(&camel(path));
        }
        Some((first, rest)) => {
            if let Some(Value::Object(inner)) = obj.get_mut(&camel(first)) {
                remove_path(inner, rest);
            }
        }
    }
}

/// Whether `path` (kebab-case) is present in `obj`.
pub fn has_path(obj: &Map<String, Value>, path: &str) -> bool {
    let mut here = obj;
    let mut parts = path.split('.').peekable();
    while let Some(p) = parts.next() {
        match here.get(&camel(p)) {
            Some(v) if parts.peek().is_none() => return !v.is_null(),
            Some(Value::Object(o)) => here = o,
            _ => return false,
        }
    }
    false
}

/// Every leaf path in `obj`, kebab-case (`position.x`).
pub fn leaf_paths(obj: &Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in obj {
        match v {
            Value::Object(inner) => {
                out.extend(leaf_paths(inner).into_iter().map(|p| format!("{}.{p}", kebab(k))))
            }
            _ => out.push(kebab(k)),
        }
    }
    out
}

/// A TOML table (kebab-case keys) as a JSON object (camelCase keys).
pub fn toml_to_json(table: &toml::Table) -> Map<String, Value> {
    table.iter().map(|(k, v)| (camel(k), toml_value(v))).collect()
}

fn toml_value(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(i) => Value::from(*i),
        toml::Value::Float(f) => Value::from(*f),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(a) => Value::Array(a.iter().map(toml_value).collect()),
        toml::Value::Table(t) => Value::Object(toml_to_json(t)),
    }
}

/// The reading `fields` (camelCase JSON) make for `capability`, or serde's reason in kebab-case.
pub fn reading(
    capability: Capability,
    t_us: Timestamp,
    mut fields: Map<String, Value>,
) -> Result<Reading, String> {
    fields.insert("tUs".into(), Value::from(t_us));
    let tagged = serde_json::json!({ "capability": capability.id(), "reading": fields });
    serde_json::from_value(tagged).map_err(|e| kebab_message(&e.to_string()))
}

/// The info `fields` make for `capability`, or why not.
pub fn info(capability: Capability, fields: Map<String, Value>) -> Result<Info, String> {
    let tagged = serde_json::json!({ "capability": capability.id(), "info": fields });
    serde_json::from_value(tagged).map_err(|e| kebab_message(&e.to_string()))
}

/// The paths of `given` the parsed value doesn't have: names the capability doesn't know
/// (serde quietly ignores those, so they are found by writing the value back out).
pub fn unknown_paths(given: &[String], parsed: &impl serde::Serialize, inner: &str) -> Vec<String> {
    let out = serde_json::to_value(parsed).expect("capability types serialise");
    let empty = Map::new();
    let obj = out.get(inner).and_then(Value::as_object).unwrap_or(&empty);
    given.iter().filter(|p| !has_path(obj, p)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::distance;

    #[test]
    fn names_convert_both_ways() {
        assert_eq!(camel("latitude-deg"), "latitudeDeg");
        assert_eq!(camel("speed-m-s"), "speedMS");
        assert_eq!(kebab("speedMS"), "speed-m-s");
        assert_eq!(kebab_message("missing field `fieldOfViewRad`"), "missing field `field-of-view-rad`");
    }

    #[test]
    fn nested_paths() {
        let mut o = Map::new();
        set_path(&mut o, "position.x", 1.into());
        set_path(&mut o, "position.y", 2.into());
        set_path(&mut o, "frame", "odom".into());
        let mut gone = o.clone();
        remove_path(&mut gone, "position.x");
        remove_path(&mut gone, "frame");
        assert_eq!(Value::Object(gone), serde_json::json!({"position": {"y": 2}}));
        assert_eq!(
            Value::Object(o.clone()),
            serde_json::json!({"position": {"x": 1, "y": 2}, "frame": "odom"})
        );
        assert!(has_path(&o, "position.y") && !has_path(&o, "position.z") && !has_path(&o, "frame.x"));
        let mut leaves = leaf_paths(&o);
        leaves.sort();
        assert_eq!(leaves, ["frame", "position.x", "position.y"]);
    }

    #[test]
    fn readings_and_info_are_checked_by_the_capability_itself() {
        let mut f = Map::new();
        set_path(&mut f, "meters", 1.5.into());
        set_path(&mut f, "status", "ok".into());
        assert_eq!(
            reading(Capability::Distance, 7, f).unwrap(),
            Reading::Distance(distance::Reading { t_us: 7, meters: 1.5, status: distance::Status::Ok })
        );
        let e = reading(Capability::Distance, 7, Map::new()).unwrap_err();
        assert!(e.contains("missing field `meters`"), "{e}");
        let t: toml::Table =
            toml::from_str("min-m = 0.02\nmax-m = 4\nfield-of-view-rad = 0.3\nkind = \"radar\"").unwrap();
        let i = info(Capability::Distance, toml_to_json(&t)).unwrap();
        assert!(matches!(i, Info::Distance(distance::Info { kind: distance::Kind::Radar, .. })));
        let given = vec!["max-m".to_string(), "max-range".to_string()];
        assert_eq!(unknown_paths(&given, &i, "info"), ["max-range"]);
    }
}
