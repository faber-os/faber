//! Layer 2: users, their roles and their API tokens (docs/auth.md), kept in `config/users.toml`.
//!
//! Passwords are stored as argon2id hashes, tokens as their SHA-256. Nothing here knows about
//! HTTP, cookies or sessions: it answers "who is this name and password?" and "whose token is
//! this?", and keeps the file.

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use subtle::ConstantTimeEq;
use tokio::sync::watch;
use ts_rs::TS;

pub const FILE_NAME: &str = "users.toml";

const HEADER: &str = "# Faber's users and API tokens. Passwords and tokens are stored as hashes only.\n\
# Manage them in Settings → Users. Delete this file (with Faber stopped) to start over with a new admin.\n";

/// What a user (or a token, or the attached screen) may do. Each role can do everything the one
/// before it can: `viewer < operator < admin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum UserRole {
    /// Sees everything and can press Stop; changes nothing.
    Viewer,
    /// Also drives, releases the e-stop and sets simulated values.
    Operator,
    /// Everything: users, installs, flashing, backups, the machine's settings.
    Admin,
}

impl UserRole {
    pub fn as_str(self) -> &'static str {
        match self {
            UserRole::Viewer => "viewer",
            UserRole::Operator => "operator",
            UserRole::Admin => "admin",
        }
    }
}

/// One user, as `users.toml` keeps them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub name: String,
    pub role: UserRole,
    /// The argon2id hash, as a PHC string.
    password: String,
}

/// One API token, as `users.toml` keeps it: never the token itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// A short public id, to list and revoke it.
    pub id: String,
    /// What the user called it, e.g. `drive-square`.
    pub name: String,
    /// Whose token it is.
    pub user: String,
    /// The most it may do; its user's current role caps it.
    pub role: UserRole,
    /// `sha256:<hex>` of the token.
    hash: String,
    /// When it was made (RFC 3339, UTC).
    pub created: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct UsersFile {
    #[serde(default)]
    users: Vec<User>,
    #[serde(default)]
    tokens: Vec<Token>,
}

/// Why a request about users was refused, in words the user can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsersError {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl UsersError {
    fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        UsersError { code, message: message.into(), hint: hint.into() }
    }

    fn unknown(name: &str) -> Self {
        UsersError::new(
            "unknown-user",
            format!("There is no user called \"{name}\"."),
            "Settings → Users lists every user.",
        )
    }

    fn broken(problem: &str) -> Self {
        UsersError::new(
            "users-file-broken",
            format!("users.toml can't be read: {problem}"),
            "Fix config/users.toml in the data folder, or (with Faber stopped) delete it to create a new admin.",
        )
    }
}

/// A user, without the password hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserView {
    pub name: String,
    pub role: UserRole,
}

/// A token as it can be shown: everything but its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenView {
    pub id: String,
    pub name: String,
    pub user: String,
    pub role: UserRole,
    pub created: String,
}

impl From<&Token> for TokenView {
    fn from(t: &Token) -> Self {
        TokenView {
            id: t.id.clone(),
            name: t.name.clone(),
            user: t.user.clone(),
            role: t.role,
            created: t.created.clone(),
        }
    }
}

/// Who a valid token belongs to and what it may do now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenOwner {
    pub id: String,
    pub user: String,
    /// The lower of the token's role and its user's current role.
    pub role: UserRole,
}

/// argon2id with 64 MiB, 3 passes and 1 lane (docs/auth.md).
pub const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
pub const ARGON2_PASSES: u32 = 3;
pub const ARGON2_LANES: u32 = 1;

pub const MIN_PASSWORD_CHARS: usize = 8;
pub const MAX_PASSWORD_BYTES: usize = 256;
pub const MAX_NAME_CHARS: usize = 32;
/// The prefix of every API token, so one is easy to recognise (and to find if it leaks).
pub const TOKEN_PREFIX: &str = "faber_";

fn argon2() -> argon2::Argon2<'static> {
    let params = argon2::Params::new(ARGON2_MEMORY_KIB, ARGON2_PASSES, ARGON2_LANES, None)
        .expect("valid argon2 parameters");
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
}

/// One hash at a time: each one takes 64 MiB, and a burst of sign-ins must not take more.
static HASHING: Mutex<()> = Mutex::new(());

fn hash_password(password: &str) -> String {
    use argon2::password_hash::PasswordHasher;
    let _one = HASHING.lock().unwrap_or_else(|e| e.into_inner());
    let salt = random_bytes::<16>();
    argon2().hash_password_with_salt(password.as_bytes(), &salt).expect("argon2 hashes").to_string()
}

fn password_matches(password: &str, hash: &str) -> bool {
    use argon2::password_hash::PasswordVerifier;
    let _one = HASHING.lock().unwrap_or_else(|e| e.into_inner());
    // The parameters come from the hash itself, so older hashes keep working.
    argon2().verify_password(password.as_bytes(), hash).is_ok()
}

/// Checked when a name doesn't exist, so a wrong name takes as long as a wrong password.
fn dummy_hash() -> &'static str {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DUMMY.get_or_init(|| hash_password("not anyone's password"))
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("the operating system's random numbers");
    b
}

/// 256 random bits as URL-safe text (43 characters).
pub fn random_secret() -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes::<32>())
}

/// `sha256:<hex>` of a token.
pub fn token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// A user name: 1–32 of `a-z`, `0-9`, `-`, `_` and `.`, starting with a letter or digit.
pub fn check_name(name: &str) -> Result<(), UsersError> {
    let ok = (1..=MAX_NAME_CHARS).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
        && name.as_bytes()[0].is_ascii_alphanumeric();
    if ok {
        Ok(())
    } else {
        Err(UsersError::new(
            "invalid-name",
            format!("\"{name}\" can't be a user name."),
            "Use 1 to 32 lowercase letters, digits, dashes, dots or underscores, starting with a letter or digit.",
        ))
    }
}

pub fn check_password(password: &str) -> Result<(), UsersError> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(UsersError::new(
            "password-too-short",
            format!("The password is shorter than {MIN_PASSWORD_CHARS} characters."),
            "Pick a longer one: a few words in a row are easy to remember and hard to guess.",
        ));
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(UsersError::new(
            "password-too-long",
            format!("The password is longer than {MAX_PASSWORD_BYTES} bytes."),
            "Pick a shorter one.",
        ));
    }
    Ok(())
}

fn check_token_name(name: &str) -> Result<(), UsersError> {
    if name.trim().is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(UsersError::new(
            "invalid-token-name",
            "A token needs a name of 1 to 64 characters.",
            "Name it after the program that uses it, e.g. drive-square.",
        ));
    }
    Ok(())
}

pub struct UsersStore {
    path: PathBuf,
    file: Mutex<UsersFile>,
    /// Set when users.toml exists but can't be read: then nobody can sign in, and setup is
    /// refused too (a broken file must never look like a fresh machine).
    broken: Mutex<Option<String>>,
    changed: watch::Sender<u64>,
}

impl UsersStore {
    /// Loads `config/users.toml`. A missing file means no users yet (first run).
    pub fn load(config_dir: &Path) -> UsersStore {
        let path = config_dir.join(FILE_NAME);
        let (file, broken) = match std::fs::read_to_string(&path) {
            Ok(text) => match parse(&text) {
                Ok(f) => (f, None),
                Err(e) => {
                    tracing::error!(target: "core::users", "users.toml can't be read: {e}. Nobody can sign in until it is fixed.");
                    (UsersFile::default(), Some(e))
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => (UsersFile::default(), None),
            Err(e) => {
                tracing::error!(target: "core::users", "users.toml can't be read: {e}");
                (UsersFile::default(), Some(e.to_string()))
            }
        };
        UsersStore { path, file: Mutex::new(file), broken: Mutex::new(broken), changed: watch::channel(0).0 }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Fires whenever a user or token is added, removed or changed.
    pub fn watch(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, UsersFile> {
        self.file.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn check_readable(&self) -> Result<(), UsersError> {
        match self.broken.lock().unwrap_or_else(|e| e.into_inner()).as_deref() {
            Some(problem) => Err(UsersError::broken(problem)),
            None => Ok(()),
        }
    }

    /// No users yet, and the file isn't broken: the machine waits for its first admin.
    pub fn needs_setup(&self) -> bool {
        self.check_readable().is_ok() && self.lock().users.is_empty()
    }

    /// Creates the first admin. Refused once any user exists, under the same lock that checks it,
    /// so two browsers racing can't both create one.
    pub fn create_first_admin(&self, name: &str, password: &str) -> Result<UserView, UsersError> {
        self.check_readable()?;
        check_name(name)?;
        check_password(password)?;
        let hash = hash_password(password);
        let mut file = self.lock();
        if !file.users.is_empty() {
            return Err(UsersError::new(
                "already-set-up",
                "This machine already has an admin.",
                "Sign in instead. An admin can add more users in Settings → Users.",
            ));
        }
        let mut new = file.clone();
        new.users.push(User { name: name.into(), role: UserRole::Admin, password: hash });
        self.save(&mut file, new)?;
        Ok(UserView { name: name.into(), role: UserRole::Admin })
    }

    /// The user with this name and password, or `None`. Takes as long for an unknown name.
    pub fn sign_in(&self, name: &str, password: &str) -> Result<Option<UserView>, UsersError> {
        self.check_readable()?;
        let user = self.lock().users.iter().find(|u| u.name == name).cloned();
        if password.len() > MAX_PASSWORD_BYTES {
            return Ok(None);
        }
        match user {
            Some(u) if password_matches(password, &u.password) => {
                Ok(Some(UserView { name: u.name, role: u.role }))
            }
            Some(_) => Ok(None),
            None => {
                let _ = password_matches(password, dummy_hash());
                Ok(None)
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<UserView> {
        self.lock()
            .users
            .iter()
            .find(|u| u.name == name)
            .map(|u| UserView { name: u.name.clone(), role: u.role })
    }

    pub fn list(&self) -> Vec<UserView> {
        self.lock().users.iter().map(|u| UserView { name: u.name.clone(), role: u.role }).collect()
    }

    pub fn add(&self, name: &str, password: &str, role: UserRole) -> Result<UserView, UsersError> {
        self.check_readable()?;
        check_name(name)?;
        check_password(password)?;
        let hash = hash_password(password);
        let mut file = self.lock();
        if file.users.iter().any(|u| u.name == name) {
            return Err(UsersError::new(
                "name-taken",
                format!("There is already a user called \"{name}\"."),
                "Pick another name.",
            ));
        }
        let mut new = file.clone();
        new.users.push(User { name: name.into(), role, password: hash });
        self.save(&mut file, new)?;
        Ok(UserView { name: name.into(), role })
    }

    /// Removes a user and their tokens. The last admin can't be removed.
    pub fn remove(&self, name: &str) -> Result<(), UsersError> {
        self.check_readable()?;
        let mut file = self.lock();
        let user = file.users.iter().find(|u| u.name == name).ok_or_else(|| UsersError::unknown(name))?;
        if user.role == UserRole::Admin && admins(&file) == 1 {
            return Err(last_admin());
        }
        let mut new = file.clone();
        new.users.retain(|u| u.name != name);
        new.tokens.retain(|t| t.user != name);
        self.save(&mut file, new)
    }

    pub fn set_password(&self, name: &str, password: &str) -> Result<(), UsersError> {
        self.check_readable()?;
        check_password(password)?;
        if self.get(name).is_none() {
            return Err(UsersError::unknown(name));
        }
        let hash = hash_password(password);
        let mut file = self.lock();
        let mut new = file.clone();
        let user = new.users.iter_mut().find(|u| u.name == name).ok_or_else(|| UsersError::unknown(name))?;
        user.password = hash;
        self.save(&mut file, new)
    }

    /// Changes a user's role. The last admin can't be demoted.
    pub fn set_role(&self, name: &str, role: UserRole) -> Result<(), UsersError> {
        self.check_readable()?;
        let mut file = self.lock();
        let user = file.users.iter().find(|u| u.name == name).ok_or_else(|| UsersError::unknown(name))?;
        if user.role == UserRole::Admin && role != UserRole::Admin && admins(&file) == 1 {
            return Err(last_admin());
        }
        let mut new = file.clone();
        if let Some(u) = new.users.iter_mut().find(|u| u.name == name) {
            u.role = role;
        }
        self.save(&mut file, new)
    }

    /// Makes a token for `user` with at most their role. Returns it with the secret, which is
    /// never kept: this is the only time anyone sees it.
    pub fn create_token(
        &self,
        user: &str,
        name: &str,
        role: UserRole,
        created: &str,
    ) -> Result<(TokenView, String), UsersError> {
        self.check_readable()?;
        check_token_name(name)?;
        let mut file = self.lock();
        let owner = file.users.iter().find(|u| u.name == user).ok_or_else(|| UsersError::unknown(user))?;
        if role > owner.role {
            return Err(UsersError::new(
                "role-too-high",
                format!("A token can't do more than its user: {user} is {}.", owner.role.as_str()),
                "Pick the same role or a lower one.",
            ));
        }
        let secret = format!("{TOKEN_PREFIX}{}", random_secret());
        let id: String = random_bytes::<4>().iter().map(|b| format!("{b:02x}")).collect();
        let token = Token {
            id,
            name: name.trim().into(),
            user: user.into(),
            role,
            hash: token_hash(&secret),
            created: created.into(),
        };
        let view = TokenView::from(&token);
        let mut new = file.clone();
        new.tokens.push(token);
        self.save(&mut file, new)?;
        Ok((view, secret))
    }

    /// Every token, or only `user`'s.
    pub fn tokens(&self, user: Option<&str>) -> Vec<TokenView> {
        self.lock().tokens.iter().filter(|t| user.is_none_or(|u| t.user == u)).map(TokenView::from).collect()
    }

    pub fn token(&self, id: &str) -> Option<TokenView> {
        self.lock().tokens.iter().find(|t| t.id == id).map(TokenView::from)
    }

    pub fn revoke_token(&self, id: &str) -> Result<(), UsersError> {
        let mut file = self.lock();
        if !file.tokens.iter().any(|t| t.id == id) {
            return Err(UsersError::new(
                "unknown-token",
                format!("There is no token with the id \"{id}\"."),
                "Settings → Users → API tokens lists them; it may already be revoked.",
            ));
        }
        let mut new = file.clone();
        new.tokens.retain(|t| t.id != id);
        self.save(&mut file, new)
    }

    /// Whose token this is, comparing hashes in constant time.
    pub fn check_token(&self, secret: &str) -> Option<TokenOwner> {
        if !secret.starts_with(TOKEN_PREFIX) || self.check_readable().is_err() {
            return None;
        }
        let hash = token_hash(secret);
        let file = self.lock();
        let mut found = None;
        for t in &file.tokens {
            if bool::from(t.hash.as_bytes().ct_eq(hash.as_bytes())) {
                found = Some(t);
            }
        }
        let t = found?;
        let owner = file.users.iter().find(|u| u.name == t.user)?;
        Some(TokenOwner { id: t.id.clone(), user: t.user.clone(), role: t.role.min(owner.role) })
    }

    /// The file as text, for a backup. `None` while there are no users.
    pub fn backup_text(&self) -> Option<String> {
        let file = self.lock();
        (!file.users.is_empty()).then(|| render(&file))
    }

    /// Replaces every user and token with those in `text` (a backup's users.toml). Checked first:
    /// a file without an admin is refused.
    pub fn restore(&self, text: &str) -> Result<(), UsersError> {
        let restored = parse(text).map_err(|e| {
            UsersError::new(
                "backup-invalid",
                format!("The backup's users.toml can't be read: {e}"),
                "Restore without its users, or pick another backup.",
            )
        })?;
        if admins(&restored) == 0 {
            return Err(UsersError::new(
                "backup-invalid",
                "The backup's users.toml has no admin.",
                "Restore without its users, or pick another backup.",
            ));
        }
        let mut file = self.lock();
        self.save(&mut file, restored)?;
        *self.broken.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }

    fn save(&self, cur: &mut UsersFile, new: UsersFile) -> Result<(), UsersError> {
        write_private(&self.path, &render(&new)).map_err(|e| {
            UsersError::new(
                "users-not-saved",
                format!("Could not save users.toml: {e}"),
                "Check that the data folder is writable and the disk isn't full.",
            )
        })?;
        *cur = new;
        self.changed.send_modify(|n| *n += 1);
        Ok(())
    }
}

/// Checks a backup's users.toml without changing anything.
pub fn check_backup_text(text: &str) -> Result<(), UsersError> {
    let parsed = parse(text).map_err(|e| {
        UsersError::new(
            "backup-invalid",
            format!("The backup's users.toml can't be read: {e}"),
            "Restore without its users, or pick another backup.",
        )
    })?;
    if admins(&parsed) == 0 {
        return Err(UsersError::new(
            "backup-invalid",
            "The backup's users.toml has no admin.",
            "Restore without its users, or pick another backup.",
        ));
    }
    Ok(())
}

fn admins(file: &UsersFile) -> usize {
    file.users.iter().filter(|u| u.role == UserRole::Admin).count()
}

fn last_admin() -> UsersError {
    UsersError::new(
        "last-admin",
        "This is the only admin, and a machine always needs one.",
        "Make another user an admin first.",
    )
}

fn parse(text: &str) -> Result<UsersFile, String> {
    let file: UsersFile = toml::from_str(text).map_err(|e| e.to_string())?;
    for u in &file.users {
        check_name(&u.name).map_err(|e| e.message)?;
    }
    let mut names: Vec<_> = file.users.iter().map(|u| &u.name).collect();
    names.sort();
    if names.windows(2).any(|w| w[0] == w[1]) {
        return Err("a user name appears twice".into());
    }
    Ok(file)
}

fn render(file: &UsersFile) -> String {
    format!("{HEADER}\n{}", toml::to_string(file).expect("users serialise"))
}

/// Written beside and renamed into place, readable only by Faber's own user on Linux.
fn write_private(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, UsersStore) {
        let d = tempfile::tempdir().unwrap();
        let s = UsersStore::load(d.path());
        (d, s)
    }

    #[test]
    fn roles_are_ordered() {
        assert!(UserRole::Viewer < UserRole::Operator && UserRole::Operator < UserRole::Admin);
    }

    #[test]
    fn a_fresh_machine_needs_setup_once() {
        let (d, s) = store();
        assert!(s.needs_setup());
        s.create_first_admin("mikkel", "correct horse").unwrap();
        assert!(!s.needs_setup());
        assert_eq!(s.create_first_admin("eve", "another one").unwrap_err().code, "already-set-up");
        // It's on disk, as a hash only.
        let text = std::fs::read_to_string(d.path().join(FILE_NAME)).unwrap();
        assert!(text.contains("$argon2id$v=19$m=65536,t=3,p=1$"), "{text}");
        assert!(!text.contains("correct horse"));
        assert!(!UsersStore::load(d.path()).needs_setup());
    }

    #[test]
    fn a_broken_file_is_not_a_fresh_machine() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(FILE_NAME), "users = 5").unwrap();
        let s = UsersStore::load(d.path());
        assert!(!s.needs_setup());
        assert_eq!(s.create_first_admin("eve", "password1").unwrap_err().code, "users-file-broken");
        assert_eq!(s.sign_in("eve", "password1").unwrap_err().code, "users-file-broken");
    }

    #[test]
    fn signing_in_checks_the_password() {
        let (_d, s) = store();
        s.create_first_admin("mikkel", "correct horse").unwrap();
        assert_eq!(s.sign_in("mikkel", "correct horse").unwrap().unwrap().role, UserRole::Admin);
        assert_eq!(s.sign_in("mikkel", "wrong horse").unwrap(), None);
        assert_eq!(s.sign_in("nobody", "correct horse").unwrap(), None);
        assert_eq!(s.sign_in("mikkel", &"x".repeat(1000)).unwrap(), None);
    }

    #[test]
    fn names_and_passwords_are_checked() {
        let (_d, s) = store();
        assert_eq!(s.create_first_admin("Mikkel", "correct horse").unwrap_err().code, "invalid-name");
        assert_eq!(s.create_first_admin("-x", "correct horse").unwrap_err().code, "invalid-name");
        assert_eq!(s.create_first_admin("mikkel", "short").unwrap_err().code, "password-too-short");
        assert_eq!(s.create_first_admin("mikkel", &"x".repeat(300)).unwrap_err().code, "password-too-long");
        assert!(s.needs_setup());
    }

    #[test]
    fn the_last_admin_stays() {
        let (_d, s) = store();
        s.create_first_admin("mikkel", "correct horse").unwrap();
        assert_eq!(s.remove("mikkel").unwrap_err().code, "last-admin");
        assert_eq!(s.set_role("mikkel", UserRole::Viewer).unwrap_err().code, "last-admin");
        s.add("anna", "password1", UserRole::Admin).unwrap();
        s.set_role("mikkel", UserRole::Operator).unwrap();
        assert_eq!(s.get("mikkel").unwrap().role, UserRole::Operator);
        assert_eq!(s.add("anna", "password1", UserRole::Viewer).unwrap_err().code, "name-taken");
    }

    #[test]
    fn tokens_are_stored_as_hashes_and_capped_by_their_user() {
        let (d, s) = store();
        s.create_first_admin("mikkel", "correct horse").unwrap();
        s.add("bo", "password1", UserRole::Operator).unwrap();
        assert_eq!(
            s.create_token("bo", "x", UserRole::Admin, "2026-10-16T00:00:00Z").unwrap_err().code,
            "role-too-high"
        );
        let (view, secret) =
            s.create_token("bo", "drive-square", UserRole::Operator, "2026-10-16T00:00:00Z").unwrap();
        assert!(secret.starts_with(TOKEN_PREFIX) && secret.len() == TOKEN_PREFIX.len() + 43);
        let text = std::fs::read_to_string(d.path().join(FILE_NAME)).unwrap();
        assert!(!text.contains(&secret));
        let owner = s.check_token(&secret).unwrap();
        assert_eq!(
            (owner.user.as_str(), owner.role, owner.id.as_str()),
            ("bo", UserRole::Operator, view.id.as_str())
        );
        assert_eq!(s.check_token("faber_wrong"), None);
        assert_eq!(s.check_token(&secret[1..]), None);
        // Demoting the user demotes the token.
        s.set_role("bo", UserRole::Viewer).unwrap();
        assert_eq!(s.check_token(&secret).unwrap().role, UserRole::Viewer);
        // Revoked: gone.
        s.revoke_token(&view.id).unwrap();
        assert_eq!(s.check_token(&secret), None);
        assert_eq!(s.revoke_token(&view.id).unwrap_err().code, "unknown-token");
    }

    #[test]
    fn removing_a_user_removes_their_tokens() {
        let (_d, s) = store();
        s.create_first_admin("mikkel", "correct horse").unwrap();
        s.add("bo", "password1", UserRole::Viewer).unwrap();
        let (_, secret) = s.create_token("bo", "t", UserRole::Viewer, "now").unwrap();
        s.remove("bo").unwrap();
        assert_eq!(s.check_token(&secret), None);
        assert!(s.tokens(None).is_empty());
    }

    #[test]
    fn a_backup_restores_users_that_include_an_admin() {
        let (_d, a) = store();
        a.create_first_admin("mikkel", "correct horse").unwrap();
        let text = a.backup_text().unwrap();
        let (_d2, b) = store();
        b.create_first_admin("other", "password1").unwrap();
        assert!(check_backup_text(&text).is_ok());
        b.restore(&text).unwrap();
        assert!(b.sign_in("mikkel", "correct horse").unwrap().is_some());
        assert!(b.get("other").is_none());
        assert_eq!(
            check_backup_text("[[users]]\nname = \"v\"\nrole = \"viewer\"\npassword = \"x\"\n")
                .unwrap_err()
                .code,
            "backup-invalid"
        );
    }

    #[test]
    fn changes_are_announced() {
        let (_d, s) = store();
        let mut rx = s.watch();
        s.create_first_admin("mikkel", "correct horse").unwrap();
        assert!(rx.has_changed().unwrap());
        rx.mark_unchanged();
        s.set_password("mikkel", "battery staple").unwrap();
        assert!(rx.has_changed().unwrap());
        assert!(s.sign_in("mikkel", "battery staple").unwrap().is_some());
    }
}
