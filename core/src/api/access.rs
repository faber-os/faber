//! Who may call what (docs/auth.md): the one table that gives every route its minimum role, the
//! WebSocket messages' rules, and the few rules that depend on more than the route (file paths,
//! settings). The middleware that applies them is in `auth.rs`; this module only answers.
//!
//! A route missing from [`ROUTES`] is refused for everyone, and a test reads the API's source and
//! fails if any route lacks an entry, so a new endpoint can't be forgotten.

use super::dto::ClientMessage;
use crate::files::{RootKey, VPath};
use crate::settings::Settings;
use crate::users::UserRole;

/// What a route needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Anyone who can reach the machine, signed in or not.
    Public,
    /// Signed in (or the attached screen) with at least this role.
    Role(UserRole),
}

use Need::Public;
const VIEWER: Need = Need::Role(UserRole::Viewer);
const OPERATOR: Need = Need::Role(UserRole::Operator);
const ADMIN: Need = Need::Role(UserRole::Admin);

/// Every route as axum matches it: method, path pattern, minimum role.
pub const ROUTES: &[(&str, &str, Need)] = &[
    // Before signing in: what the sign-in screen needs, and Stop, which is never gated.
    ("GET", "/api/v0/system", Public),
    ("GET", "/api/v0/session", Public),
    ("POST", "/api/v0/session", Public),
    ("DELETE", "/api/v0/session", Public),
    ("POST", "/api/v0/setup", Public),
    ("POST", "/api/v0/estop", Public),
    ("GET", "/api/v0/translations", Public),
    ("GET", "/api/v0/translations/{lang}", Public),
    // An app's UI files are its package's code, like the shell's own files (docs/auth.md).
    ("GET", "/apps/{id}/ui/{*path}", Public),
    ("GET", "/apps/{id}/icon.svg", Public),
    // Reading: everyone signed in.
    ("GET", "/api/v0/about", VIEWER),
    ("GET", "/api/v0/connectors", VIEWER),
    ("GET", "/api/v0/connectors/problems", VIEWER),
    ("GET", "/api/v0/ports", VIEWER),
    ("GET", "/api/v0/ports/problems", VIEWER),
    ("GET", "/api/v0/parts", VIEWER),
    ("GET", "/api/v0/signals/{part}/{name}", VIEWER),
    ("GET", "/api/v0/ws", VIEWER),
    ("GET", "/api/v0/files/roots", VIEWER),
    ("GET", "/api/v0/files/list", VIEWER),
    ("GET", "/api/v0/files/content", VIEWER),
    ("GET", "/api/v0/storage", VIEWER),
    ("GET", "/api/v0/settings", VIEWER),
    ("GET", "/api/v0/leases", VIEWER),
    ("GET", "/api/v0/parts/{name}/simulation", VIEWER),
    ("GET", "/api/v0/estop", VIEWER),
    ("GET", "/api/v0/apps", VIEWER),
    ("GET", "/api/v0/apps/{id}", VIEWER),
    ("GET", "/api/v0/apps/{id}/state", VIEWER),
    ("GET", "/api/v0/packages", VIEWER),
    ("GET", "/api/v0/flasher/boards", VIEWER),
    ("GET", "/api/v0/worlds", VIEWER),
    ("GET", "/api/v0/world", VIEWER),
    // Your own password and API tokens (the handlers check whose).
    ("PUT", "/api/v0/users/{name}/password", VIEWER),
    ("GET", "/api/v0/tokens", VIEWER),
    ("POST", "/api/v0/tokens", VIEWER),
    ("DELETE", "/api/v0/tokens/{id}", VIEWER),
    // Driving, simulating and the home folder's files.
    ("POST", "/api/v0/ports/check-board", OPERATOR),
    ("POST", "/api/v0/parts/{name}/lease", OPERATOR),
    ("DELETE", "/api/v0/parts/{name}/lease", OPERATOR),
    ("POST", "/api/v0/parts/{name}/lease/renew", OPERATOR),
    ("POST", "/api/v0/parts/{name}/command", OPERATOR),
    ("PUT", "/api/v0/parts/{name}/simulation", OPERATOR),
    ("POST", "/api/v0/estop/release", OPERATOR),
    ("PUT", "/api/v0/world", OPERATOR),
    ("POST", "/api/v0/world/reset", OPERATOR),
    ("PUT", "/api/v0/apps/{id}/state", OPERATOR),
    ("PUT", "/api/v0/files/content", OPERATOR),
    ("POST", "/api/v0/files/folder", OPERATOR),
    ("POST", "/api/v0/files/move", OPERATOR),
    ("POST", "/api/v0/files/trash", OPERATOR),
    ("POST", "/api/v0/files/restore", OPERATOR),
    ("POST", "/api/v0/files/delete", OPERATOR),
    ("POST", "/api/v0/files/empty-trash", OPERATOR),
    ("POST", "/api/v0/files/samples", OPERATOR),
    // Display settings; the machine's own settings in it need an admin (`settings_need`).
    ("PUT", "/api/v0/settings", OPERATOR),
    // The machine itself: parts, packages, apps, flashing, backups, settings and users.
    ("POST", "/api/v0/parts", ADMIN),
    ("DELETE", "/api/v0/parts/{name}", ADMIN),
    ("POST", "/api/v0/packages", ADMIN),
    ("DELETE", "/api/v0/packages/{id}", ADMIN),
    ("POST", "/api/v0/apps/{id}/enable", ADMIN),
    ("POST", "/api/v0/apps/{id}/disable", ADMIN),
    ("PUT", "/api/v0/apps/{id}/roles", ADMIN),
    ("POST", "/api/v0/ports/{id}/flash", ADMIN),
    ("POST", "/api/v0/backups", ADMIN),
    ("POST", "/api/v0/backups/restore", ADMIN),
    ("POST", "/api/v0/backups/setup", ADMIN),
    ("POST", "/api/v0/settings/reset", ADMIN),
    ("POST", "/api/v0/settings/developer", ADMIN),
    ("GET", "/api/v0/users", ADMIN),
    ("POST", "/api/v0/users", ADMIN),
    ("DELETE", "/api/v0/users/{name}", ADMIN),
    ("PUT", "/api/v0/users/{name}/role", ADMIN),
];

/// What `method` on the route `pattern` needs, or `None` when the table has no entry (refused).
/// `HEAD` is a `GET`.
pub fn route_need(method: &str, pattern: &str) -> Option<Need> {
    let method = if method == "HEAD" { "GET" } else { method };
    ROUTES.iter().find(|(m, p, _)| *m == method && *p == pattern).map(|(_, _, n)| *n)
}

/// What a WebSocket message needs. An exhaustive `match`: a new message doesn't compile without
/// a rule.
pub fn message_need(msg: &ClientMessage) -> UserRole {
    use ClientMessage as C;
    match msg {
        // Watching, and an app window's own lifecycle: everyone.
        C::Subscribe { .. }
        | C::Unsubscribe { .. }
        | C::AppWindowOpen { .. }
        | C::AppWindowHeartbeat { .. }
        | C::AppWindowClose { .. } => UserRole::Viewer,
        // Ending a drive and the window's Stop bar only ever stop things: never gated.
        C::AppWindowStop { .. } | C::AppWindowRelease { .. } => UserRole::Viewer,
        // A message to an app's logic could make it move things; driving through it does.
        C::AppWindowSend { .. } | C::AppWindowDrive { .. } => UserRole::Operator,
        // The serial monitor writes to a device.
        C::SerialOpen { .. }
        | C::SerialSend { .. }
        | C::SerialClose { .. }
        | C::SerialRecordStart { .. }
        | C::SerialRecordStop { .. } => UserRole::Operator,
    }
}

/// Whether a file path is read or changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileUse {
    Read,
    Change,
}

/// What a file path needs. The whole machine's files (D34) and the Backups folder (backups hold
/// the users' password hashes) are for admins; the rest of the home folder and removable disks
/// are for viewers to read and operators to change. A path that doesn't parse needs nothing more
/// than the route: the handler refuses it with the right message.
pub fn path_need(path: &str, use_: FileUse) -> UserRole {
    let Ok(p) = VPath::parse(path) else {
        return UserRole::Viewer;
    };
    let admin_only = p.root == RootKey::Machine
        || (p.root == RootKey::Home && p.parts.first().is_some_and(|f| f == "Backups"));
    match (admin_only, use_) {
        (true, _) => UserRole::Admin,
        (false, FileUse::Read) => UserRole::Viewer,
        (false, FileUse::Change) => UserRole::Operator,
    }
}

/// What changing the settings from `old` to `new` needs: the machine's name, its time and the
/// attached screen's role are the machine's own (admins); the look is everyone's who may change
/// anything (operators).
pub fn settings_need(old: &Settings, new: &Settings) -> UserRole {
    if old.admin_only_changes(new) { UserRole::Admin } else { UserRole::Operator }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every route call in the API's source (a path, then its `get(…)`, `post(…)` handlers), as
    /// (method, path).
    fn routes_in_source() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api");
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let text = std::fs::read_to_string(entry.path()).unwrap();
            // Only the routes themselves, not tests (like this one) that mention them.
            let mut rest = text.split("#[cfg(test)]").next().unwrap();
            while let Some(i) = rest.find(".route(") {
                let after = rest[i + ".route(".len()..].trim_start();
                let Some(after) = after.strip_prefix('"') else {
                    rest = after;
                    continue;
                };
                let end = after.find('"').unwrap();
                let path = &after[..end];
                // The handlers: up to the parenthesis that closes `.route(`.
                let mut depth = 1;
                let mut close = after.len();
                for (j, c) in after[end..].char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                close = end + j;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let handlers = &after[end..close];
                for (method, word) in
                    [("GET", "get("), ("POST", "post("), ("PUT", "put("), ("DELETE", "delete(")]
                {
                    let used = handlers.match_indices(word).any(|(k, _)| {
                        k == 0
                            || !(handlers.as_bytes()[k - 1].is_ascii_alphanumeric()
                                || handlers.as_bytes()[k - 1] == b'_')
                    });
                    if used {
                        found.push((method.to_string(), path.to_string()));
                    }
                }
                rest = &after[close..];
            }
        }
        found.sort();
        found
    }

    #[test]
    fn every_route_has_a_rule() {
        let routes = routes_in_source();
        assert!(routes.len() > 60, "the source scan found too few routes: {routes:?}");
        let missing: Vec<_> = routes.iter().filter(|(m, p)| route_need(m, p).is_none()).collect();
        assert!(missing.is_empty(), "routes without an entry in access::ROUTES: {missing:?}");
    }

    #[test]
    fn every_rule_has_a_route() {
        let routes = routes_in_source();
        let stale: Vec<_> = ROUTES
            .iter()
            .filter(|(m, p, _)| !routes.iter().any(|(rm, rp)| rm == m && rp == p))
            .map(|(m, p, _)| format!("{m} {p}"))
            .collect();
        assert!(stale.is_empty(), "entries in access::ROUTES for routes that don't exist: {stale:?}");
        let mut seen = std::collections::BTreeSet::new();
        for (m, p, _) in ROUTES {
            assert!(seen.insert((m, p)), "{m} {p} is in the table twice");
        }
    }

    #[test]
    fn stopping_is_never_gated() {
        assert_eq!(route_need("POST", "/api/v0/estop"), Some(Need::Public));
        assert_eq!(message_need(&ClientMessage::AppWindowStop { id: 1 }), UserRole::Viewer);
        assert_eq!(route_need("POST", "/api/v0/estop/release"), Some(OPERATOR));
    }

    #[test]
    fn the_machines_files_and_backups_are_for_admins() {
        assert_eq!(path_need("machine:/C:/Users", FileUse::Read), UserRole::Admin);
        assert_eq!(path_need("home:/Backups/x.zip", FileUse::Read), UserRole::Admin);
        assert_eq!(path_need("home:/Backups", FileUse::Read), UserRole::Admin);
        assert_eq!(path_need("home:/Documents/a.txt", FileUse::Read), UserRole::Viewer);
        assert_eq!(path_need("home:/Documents/a.txt", FileUse::Change), UserRole::Operator);
        assert_eq!(path_need("home:/", FileUse::Read), UserRole::Viewer);
    }

    #[test]
    fn head_is_get() {
        assert_eq!(route_need("HEAD", "/api/v0/parts"), Some(VIEWER));
        assert_eq!(route_need("PATCH", "/api/v0/parts"), None);
    }
}
