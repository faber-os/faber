//! The `faber` command (docs/packages.md §7, docs/your-first-connector.md): start a connector
//! from a template ([`template`]), check it against its recordings ([`check`]), and pack it as a
//! `.faberpkg` ([`pack`]). The checks and the sandbox are the core's own (`faber-core`), so what
//! `faber check` accepts is what a Faber machine installs and runs.

pub mod check;
pub mod pack;
pub mod template;

use faber_connector::Problem;

/// What went wrong and what to do about it (docs/principles.md: errors say what to do).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliError {
    pub message: String,
    pub hint: String,
}

impl CliError {
    pub fn new(message: impl Into<String>, hint: impl Into<String>) -> CliError {
        CliError { message: message.into(), hint: hint.into() }
    }
}

impl From<Problem> for CliError {
    fn from(p: Problem) -> CliError {
        CliError { message: p.message, hint: p.hint }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\n  {}", self.message, self.hint)
    }
}
