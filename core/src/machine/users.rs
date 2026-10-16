//! Who is asking (docs/auth.md): signing in and out, the first admin, users and their API tokens,
//! and the attached screen's role. The users file and the sessions live in layer 2
//! (`crate::users`, `crate::sessions`); this ties them together, so a password change also ends
//! the sessions it should.

use super::{ErrorKind, Machine, MachineError};
use crate::settings::ScreenRole;
use crate::users::{TokenView, UserRole, UserView, UsersError};
use tokio::sync::watch;

/// How a request proves who sent it.
#[derive(Clone, Copy, Debug)]
pub enum Credential<'a> {
    /// The `faber-session` cookie.
    Session(&'a str),
    /// `Authorization: Bearer faber_…`.
    Token(&'a str),
    /// It came in on the attached screen's own listener.
    Screen,
}

/// How an identity was established.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Via {
    Session,
    Token {
        id: String,
    },
    Screen,
    /// `--no-auth`: every request is an admin.
    Developer,
}

/// Who is asking, and what they may do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// The user, for a session or a token; `None` for the attached screen and `--no-auth`.
    pub user: Option<String>,
    pub role: UserRole,
    pub via: Via,
}

impl Identity {
    /// Everyone under `--no-auth`.
    pub fn developer() -> Identity {
        Identity { user: None, role: UserRole::Admin, via: Via::Developer }
    }

    fn is(&self, name: &str) -> bool {
        self.user.as_deref() == Some(name)
    }
}

/// Why signing in didn't work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInError {
    /// Wrong name or password (never which).
    Failed,
    /// Too many failures: try again after this many µs.
    Wait(u64),
    Refused(MachineError),
}

impl From<UsersError> for MachineError {
    fn from(e: UsersError) -> Self {
        let kind = match e.code {
            "unknown-user" | "unknown-token" => ErrorKind::NotFound,
            "already-set-up" | "name-taken" | "last-admin" => ErrorKind::Conflict,
            "role-too-high" => ErrorKind::Forbidden,
            "users-not-saved" | "users-file-broken" => ErrorKind::Internal,
            "backup-invalid" => ErrorKind::Unprocessable,
            _ => ErrorKind::BadRequest,
        };
        MachineError {
            kind,
            code: e.code,
            message: e.message,
            hint: e.hint,
            lease_ended: None,
            plan_problems: Vec::new(),
        }
    }
}

pub(super) fn not_allowed(message: impl Into<String>, hint: impl Into<String>) -> MachineError {
    MachineError::new(ErrorKind::Forbidden, "not-allowed", message, hint)
}

/// Now, as `users.toml` keeps it (RFC 3339, UTC, whole seconds).
fn now_text() -> String {
    jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

impl Machine {
    /// No users yet: the first browser creates the admin.
    pub fn needs_setup(&self) -> bool {
        self.users.needs_setup()
    }

    /// Creates the first admin and signs them in. Returns the session token.
    pub fn create_first_admin(&self, name: &str, password: &str) -> Result<(Identity, String), MachineError> {
        let user = self.users.create_first_admin(name, password)?;
        tracing::info!(target: "core::users", "created the first admin, {}", user.name);
        Ok(self.start_session(user))
    }

    fn start_session(&self, user: UserView) -> (Identity, String) {
        let token = self.sessions.start(&user.name);
        (Identity { user: Some(user.name), role: user.role, via: Via::Session }, token)
    }

    /// Signs in with a name and password. `from` is the TCP peer's address (never a header),
    /// for the rate limit.
    pub fn sign_in(&self, name: &str, password: &str, from: &str) -> Result<(Identity, String), SignInError> {
        let keys = [format!("ip:{from}"), format!("user:{name}")];
        self.sign_in_limit.check(&keys).map_err(SignInError::Wait)?;
        match self.users.sign_in(name, password) {
            Ok(Some(user)) => {
                self.sign_in_limit.succeeded(&keys);
                tracing::info!(target: "core::users", "{} signed in", user.name);
                Ok(self.start_session(user))
            }
            Ok(None) => {
                self.sign_in_limit.failed(&keys);
                tracing::warn!(target: "core::users", "a sign-in as \"{name}\" from {from} failed");
                Err(SignInError::Failed)
            }
            Err(e) => Err(SignInError::Refused(e.into())),
        }
    }

    pub fn sign_out(&self, session: &str) {
        self.sessions.end(session);
    }

    /// Who this is, or `None`. A session's expiry slides with each use.
    pub fn identify(&self, credential: Credential) -> Option<Identity> {
        self.who(credential, true)
    }

    /// Like [`Machine::identify`], without counting as a use: an open WebSocket checking that its
    /// session still holds, and with which role.
    pub fn still_signed_in(&self, credential: Credential) -> Option<Identity> {
        self.who(credential, false)
    }

    fn who(&self, credential: Credential, used: bool) -> Option<Identity> {
        match credential {
            Credential::Session(token) => {
                let name = if used { self.sessions.user(token) } else { self.sessions.peek(token) }?;
                let user = self.users.get(&name)?;
                Some(Identity { user: Some(user.name), role: user.role, via: Via::Session })
            }
            Credential::Token(secret) => {
                let owner = self.users.check_token(secret)?;
                Some(Identity { user: Some(owner.user), role: owner.role, via: Via::Token { id: owner.id } })
            }
            Credential::Screen => Some(Identity { user: None, role: self.screen_role(), via: Via::Screen }),
        }
    }

    /// The role the attached screen is signed in as (Settings → This machine).
    pub fn screen_role(&self) -> UserRole {
        match self.settings.get().screen.role {
            ScreenRole::Viewer => UserRole::Viewer,
            ScreenRole::Operator => UserRole::Operator,
        }
    }

    /// Fire when who may do what could have changed: users or tokens, sessions ending, settings
    /// (the screen's role).
    pub fn access_changed(&self) -> [watch::Receiver<u64>; 3] {
        [self.users.watch(), self.sessions.watch(), self.settings.watch()]
    }

    pub fn users(&self) -> Vec<UserView> {
        self.users.list()
    }

    pub fn add_user(&self, name: &str, password: &str, role: UserRole) -> Result<UserView, MachineError> {
        let user = self.users.add(name, password, role)?;
        tracing::info!(target: "core::users", "added the user {} ({})", user.name, role.as_str());
        Ok(user)
    }

    /// Removes a user, their tokens and their sessions. Nobody removes themselves.
    pub fn remove_user(&self, name: &str, by: &Identity) -> Result<(), MachineError> {
        if by.is(name) {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "remove-yourself",
                "You can't remove yourself.",
                "Ask another admin to remove you.",
            ));
        }
        self.users.remove(name)?;
        self.sessions.end_user(name, None);
        tracing::info!(target: "core::users", "removed the user {name}");
        Ok(())
    }

    /// Changes a password. Your own needs the current one, and your other sessions end (`keep`
    /// is the one asking). An admin may set anyone's, which ends all of theirs.
    pub fn set_password(
        &self,
        name: &str,
        password: &str,
        current: Option<&str>,
        by: &Identity,
        keep: Option<&str>,
    ) -> Result<(), MachineError> {
        if by.is(name) {
            let ok = match current {
                Some(c) => self.users.sign_in(name, c)?.is_some(),
                None => false,
            };
            if !ok {
                return Err(MachineError::new(
                    ErrorKind::Forbidden,
                    "wrong-password",
                    "The current password isn't right.",
                    "Type your current password, then the new one.",
                ));
            }
            self.users.set_password(name, password)?;
            self.sessions.end_user(name, keep);
        } else if by.role == UserRole::Admin {
            self.users.set_password(name, password)?;
            self.sessions.end_user(name, None);
        } else {
            return Err(not_allowed(
                "Only an admin can change someone else's password.",
                "Ask an admin, or sign in as that user.",
            ));
        }
        tracing::info!(target: "core::users", "changed the password of {name}");
        Ok(())
    }

    /// Changes a user's role. It applies at once to everything they have open.
    pub fn set_user_role(&self, name: &str, role: UserRole) -> Result<(), MachineError> {
        self.users.set_role(name, role)?;
        tracing::info!(target: "core::users", "{name} is now {}", role.as_str());
        Ok(())
    }

    /// Your tokens; an admin sees everyone's.
    pub fn tokens(&self, by: &Identity) -> Vec<TokenView> {
        if by.role == UserRole::Admin && by.via != Via::Screen {
            self.users.tokens(None)
        } else {
            by.user.as_deref().map(|u| self.users.tokens(Some(u))).unwrap_or_default()
        }
    }

    /// A new token for the signed-in user, with at most their role. Returns it with its secret,
    /// shown this once.
    pub fn create_token(
        &self,
        name: &str,
        role: UserRole,
        by: &Identity,
    ) -> Result<(TokenView, String), MachineError> {
        let (Some(user), Via::Session) = (&by.user, &by.via) else {
            return Err(not_allowed(
                "API tokens belong to a user who signed in with a password.",
                "Sign in as yourself in a browser, then make the token in Settings → Users.",
            ));
        };
        let made = self.users.create_token(user, name, role, &now_text())?;
        tracing::info!(target: "core::users", "{user} made the API token \"{}\" ({})", made.0.name, role.as_str());
        Ok(made)
    }

    /// Revokes a token: your own, or anyone's for an admin.
    pub fn revoke_token(&self, id: &str, by: &Identity) -> Result<(), MachineError> {
        let token = self.users.token(id).ok_or_else(|| {
            MachineError::new(
                ErrorKind::NotFound,
                "unknown-token",
                format!("There is no token with the id \"{id}\"."),
                "Settings → Users → API tokens lists them; it may already be revoked.",
            )
        })?;
        if !by.is(&token.user) && by.role != UserRole::Admin {
            return Err(not_allowed("Only an admin can revoke someone else's token.", "Ask an admin."));
        }
        self.users.revoke_token(id)?;
        tracing::info!(target: "core::users", "revoked the API token \"{}\" of {}", token.name, token.user);
        Ok(())
    }

    /// `users.toml` as a backup holds it.
    pub(super) fn users_backup_text(&self) -> Option<String> {
        self.users.backup_text()
    }

    /// Replaces the users with a backup's, then ends every session.
    pub(super) fn restore_users(&self, text: &str) -> Result<(), MachineError> {
        self.users.restore(text)?;
        self.sessions.end_all();
        tracing::warn!(target: "core::users", "restored the users from a backup; everyone signs in again");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::tests::test_machine;

    #[test]
    fn the_first_admin_is_signed_in_and_setup_happens_once() {
        let (_d, m) = test_machine();
        assert!(m.needs_setup());
        let (who, session) = m.create_first_admin("mikkel", "correct horse").unwrap();
        assert_eq!(who.role, UserRole::Admin);
        assert_eq!(m.identify(Credential::Session(&session)), Some(who));
        assert!(!m.needs_setup());
        assert_eq!(m.create_first_admin("eve", "password1").unwrap_err().code, "already-set-up");
    }

    #[test]
    fn changing_your_password_needs_the_old_one_and_ends_your_other_sessions() {
        let (_d, m) = test_machine();
        let (me, keep) = m.create_first_admin("mikkel", "correct horse").unwrap();
        let (_, other) = m.sign_in("mikkel", "correct horse", "a").unwrap();
        assert_eq!(
            m.set_password("mikkel", "new password", Some("wrong"), &me, Some(&keep)).unwrap_err().code,
            "wrong-password"
        );
        m.set_password("mikkel", "new password", Some("correct horse"), &me, Some(&keep)).unwrap();
        assert!(m.identify(Credential::Session(&keep)).is_some());
        assert!(m.identify(Credential::Session(&other)).is_none());
    }

    #[test]
    fn an_admin_resetting_a_password_ends_all_their_sessions() {
        let (_d, m) = test_machine();
        let (admin, _) = m.create_first_admin("mikkel", "correct horse").unwrap();
        m.add_user("bo", "password1", UserRole::Operator).unwrap();
        let (bo, s) = m.sign_in("bo", "password1", "a").unwrap();
        assert_eq!(
            m.set_password("mikkel", "x".repeat(9).as_str(), None, &bo, None).unwrap_err().code,
            "not-allowed"
        );
        m.set_password("bo", "password2", None, &admin, None).unwrap();
        assert!(m.identify(Credential::Session(&s)).is_none());
    }

    #[test]
    fn a_role_change_applies_to_open_sessions_and_tokens() {
        let (_d, m) = test_machine();
        m.create_first_admin("mikkel", "correct horse").unwrap();
        m.add_user("bo", "password1", UserRole::Operator).unwrap();
        let (bo, s) = m.sign_in("bo", "password1", "a").unwrap();
        let (_, secret) = m.create_token("t", UserRole::Operator, &bo).unwrap();
        m.set_user_role("bo", UserRole::Viewer).unwrap();
        assert_eq!(m.identify(Credential::Session(&s)).unwrap().role, UserRole::Viewer);
        assert_eq!(m.identify(Credential::Token(&secret)).unwrap().role, UserRole::Viewer);
    }

    #[test]
    fn nobody_removes_themselves_and_removing_ends_sessions() {
        let (_d, m) = test_machine();
        let (admin, _) = m.create_first_admin("mikkel", "correct horse").unwrap();
        assert_eq!(m.remove_user("mikkel", &admin).unwrap_err().code, "remove-yourself");
        m.add_user("bo", "password1", UserRole::Viewer).unwrap();
        let (_, s) = m.sign_in("bo", "password1", "a").unwrap();
        m.remove_user("bo", &admin).unwrap();
        assert!(m.identify(Credential::Session(&s)).is_none());
    }

    #[test]
    fn tokens_are_made_from_a_session_and_revoked_by_their_user_or_an_admin() {
        let (_d, m) = test_machine();
        let (admin, _) = m.create_first_admin("mikkel", "correct horse").unwrap();
        m.add_user("bo", "password1", UserRole::Viewer).unwrap();
        m.add_user("cy", "password1", UserRole::Viewer).unwrap();
        let (bo, _) = m.sign_in("bo", "password1", "a").unwrap();
        let (cy, _) = m.sign_in("cy", "password1", "a").unwrap();
        let (token, secret) = m.create_token("reader", UserRole::Viewer, &bo).unwrap();
        let by_token = m.identify(Credential::Token(&secret)).unwrap();
        assert_eq!(m.create_token("again", UserRole::Viewer, &by_token).unwrap_err().code, "not-allowed");
        assert_eq!(m.tokens(&cy), vec![]);
        assert_eq!(m.tokens(&admin).len(), 1);
        assert_eq!(m.revoke_token(&token.id, &cy).unwrap_err().code, "not-allowed");
        m.revoke_token(&token.id, &bo).unwrap();
        assert!(m.identify(Credential::Token(&secret)).is_none());
    }

    #[test]
    fn the_screen_is_signed_in_as_its_setting() {
        let (_d, m) = test_machine();
        assert_eq!(m.identify(Credential::Screen).unwrap().role, UserRole::Viewer);
        let mut s = m.settings();
        s.screen.role = ScreenRole::Operator;
        m.update_settings(s).unwrap();
        assert_eq!(m.identify(Credential::Screen).unwrap().role, UserRole::Operator);
    }

    #[test]
    fn a_restore_keeps_this_machines_users_unless_asked() {
        let (_d, m) = test_machine();
        let (admin, session) = m.create_first_admin("mikkel", "correct horse").unwrap();
        let (path, _) = m.create_backup(false).unwrap();
        m.add_user("bo", "password1", UserRole::Viewer).unwrap();
        m.set_password("mikkel", "battery staple", Some("correct horse"), &admin, Some(&session)).unwrap();

        m.restore_backup(&path).unwrap();
        assert!(m.users.get("bo").is_some(), "kept");
        assert!(m.identify(Credential::Session(&session)).is_some());

        m.restore_backup_with_users(&path).unwrap();
        assert!(m.users.get("bo").is_none(), "replaced by the backup's");
        assert!(m.users.sign_in("mikkel", "correct horse").unwrap().is_some());
        assert!(m.identify(Credential::Session(&session)).is_none(), "everyone signs in again");
        m.shutdown();
    }

    #[test]
    fn a_setup_never_carries_the_users() {
        let (_d, m) = test_machine();
        m.create_first_admin("mikkel", "correct horse").unwrap();
        let (path, _) = m.share_setup().unwrap();
        m.add_user("bo", "password1", UserRole::Viewer).unwrap();
        m.restore_backup_with_users(&path).unwrap();
        assert!(m.users.get("bo").is_some());
        m.shutdown();
    }

    #[test]
    fn sign_in_failures_are_rate_limited() {
        let (_d, m) = test_machine();
        m.create_first_admin("mikkel", "correct horse").unwrap();
        for _ in 0..crate::sessions::FREE_FAILURES {
            assert_eq!(m.sign_in("mikkel", "wrong", "10.0.0.5").unwrap_err(), SignInError::Failed);
        }
        assert!(matches!(m.sign_in("mikkel", "correct horse", "10.0.0.5"), Err(SignInError::Wait(_))));
    }
}
