//! Signing in, and the middleware that checks every request (docs/auth.md): which host name it
//! was sent to, who sent it, whether its `Origin` is this machine, and whether their role may call
//! the route (`access.rs` has the table). Also the session, setup, users and token routes.

use super::access::{self, Need};
use super::{ApiJson, AppState, Failure, blocking, dto};
use crate::machine::{Credential, Identity, SignInError, Via};
use crate::users::{TokenView, UserRole, UserView};
use axum::extract::connect_info::MockConnectInfo;
use axum::extract::{ConnectInfo, MatchedPath, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Extension, Json};
use std::net::{IpAddr, SocketAddr};

/// The session cookie's name.
pub const COOKIE: &str = "faber-session";

/// Whether the core asks who you are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AuthMode {
    /// Every request is checked (the default).
    #[default]
    Required,
    /// `--no-auth`: every request is an admin called `developer`. Only on loopback.
    Off,
}

/// Marks requests that came in on the attached screen's own listener (`--screen-listen`).
#[derive(Clone, Copy, Debug)]
pub struct OnScreen;

/// A credential that outlives its request, so an open WebSocket can check it again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proof {
    Session(String),
    Token(String),
    Screen,
    Developer,
}

impl Proof {
    /// Who this still is, without counting as a use (an open WebSocket asking).
    pub fn still(&self, state: &AppState) -> Option<Identity> {
        match self {
            Proof::Session(t) => state.machine.still_signed_in(Credential::Session(t)),
            Proof::Token(t) => state.machine.still_signed_in(Credential::Token(t)),
            Proof::Screen => state.machine.still_signed_in(Credential::Screen),
            Proof::Developer => Some(Identity::developer()),
        }
    }
}

/// Who sent a request, as the middleware found out. Handlers take it as an `Extension`.
#[derive(Clone, Debug, Default)]
pub struct Caller {
    pub identity: Option<Identity>,
    pub proof: Option<Proof>,
    /// The TCP peer (never a header): the sign-in rate limit counts by it.
    pub peer: Option<IpAddr>,
}

impl Caller {
    /// The identity, for a route that needs one (the middleware made sure).
    pub fn who(&self) -> Result<&Identity, Failure> {
        self.identity.as_ref().ok_or_else(not_signed_in)
    }

    /// Refused unless the caller has at least `role`.
    pub fn require(&self, role: UserRole) -> Result<(), Failure> {
        let who = self.who()?;
        if who.role >= role { Ok(()) } else { Err(not_allowed(role, who.role)) }
    }

    fn session(&self) -> Option<&str> {
        match &self.proof {
            Some(Proof::Session(t)) => Some(t),
            _ => None,
        }
    }
}

pub fn not_signed_in() -> Failure {
    Failure::new(
        StatusCode::UNAUTHORIZED,
        "not-signed-in",
        "Sign in first.",
        "Open Faber in a browser and sign in. A program sends an API token as Authorization: Bearer faber_…",
    )
}

pub fn not_allowed(needs: UserRole, have: UserRole) -> Failure {
    Failure(
        StatusCode::FORBIDDEN,
        dto::ErrorBody {
            code: "not-allowed".into(),
            message: format!("This needs the {} role; you are {}.", needs.as_str(), article(have)),
            hint: "Ask an admin to change your role in Settings → Users.".into(),
            reason: None,
            needs: Some(needs),
        },
    )
}

fn article(role: UserRole) -> String {
    match role {
        UserRole::Admin | UserRole::Operator => format!("an {}", role.as_str()),
        UserRole::Viewer => "a viewer".into(),
    }
}

/// The middleware every route goes through (a `route_layer`, so the route is known).
pub async fn check(State(s): State<AppState>, mut req: Request, next: Next) -> Response {
    match admit(&s, &req) {
        Ok(caller) => {
            req.extensions_mut().insert(caller);
            next.run(req).await
        }
        Err(f) => f.into_response(),
    }
}

fn admit(s: &AppState, req: &Request) -> Result<Caller, Failure> {
    let headers = req.headers();
    if let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok())
        && !host_is_this_machine(host, &s.machine.settings().machine.name)
    {
        return Err(Failure::new(
            StatusCode::MISDIRECTED_REQUEST,
            "unknown-host",
            format!("Faber doesn't answer to the name \"{host}\"."),
            "Open it by its address, by name.local, or through Tailscale.",
        ));
    }
    let peer = peer(req);
    let on_screen = req.extensions().get::<OnScreen>().is_some() && peer.is_some_and(|p| p.is_loopback());
    let caller = find_caller(s, headers, peer, on_screen)?;
    let changes = req.method() != Method::GET && req.method() != Method::HEAD;
    let upgrade =
        headers.get(header::UPGRADE).is_some_and(|u| u.as_bytes().eq_ignore_ascii_case(b"websocket"));
    if (changes || upgrade) && !origin_is_this_machine(headers) {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            "bad-origin",
            "This request came from another web page, so Faber refused it.",
            "Use Faber from its own address. A program needs no Origin header.",
        ));
    }
    let route = req.extensions().get::<MatchedPath>().map(|m| m.as_str()).unwrap_or("");
    let Some(need) = access::route_need(req.method().as_str(), route) else {
        tracing::error!(target: "core::auth", "{} {route} has no entry in the access table: refused", req.method());
        return Err(Failure::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no-access-rule",
            "This endpoint has no access rule yet, so nobody may use it.",
            "This is a bug in Faber; please report it.",
        ));
    };
    if let Need::Role(role) = need {
        match &caller.identity {
            None => return Err(not_signed_in()),
            Some(who) if who.role < role => return Err(not_allowed(role, who.role)),
            Some(_) => {}
        }
    }
    Ok(caller)
}

/// The TCP peer's address, as axum's `ConnectInfo` extractor finds it (tests mock it).
fn peer(req: &Request) -> Option<IpAddr> {
    let ext = req.extensions();
    ext.get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
        .or_else(|| ext.get::<MockConnectInfo<SocketAddr>>().map(|m| m.0.ip()))
}

fn find_caller(
    s: &AppState,
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    on_screen: bool,
) -> Result<Caller, Failure> {
    let found =
        |identity: Identity, proof: Proof| Caller { identity: Some(identity), proof: Some(proof), peer };
    if s.auth == AuthMode::Off {
        return Ok(found(Identity::developer(), Proof::Developer));
    }
    if let Some(token) = bearer(headers) {
        // A wrong token is an error, not "nobody": the program should hear why.
        return match s.machine.identify(Credential::Token(token)) {
            Some(who) => Ok(found(who, Proof::Token(token.into()))),
            None => Err(Failure::new(
                StatusCode::UNAUTHORIZED,
                "not-signed-in",
                "That API token isn't valid. It may have been revoked.",
                "Make a new one in Settings → Users → API tokens.",
            )),
        };
    }
    if let Some(session) = cookie(headers)
        && let Some(who) = s.machine.identify(Credential::Session(session))
    {
        return Ok(found(who, Proof::Session(session.into())));
    }
    if on_screen && let Some(who) = s.machine.identify(Credential::Screen) {
        return Ok(found(who, Proof::Screen));
    }
    Ok(Caller { identity: None, proof: None, peer })
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

fn cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(COOKIE)?.strip_prefix('='))
}

/// A host name that can only be this machine: an IP address, a name without dots (the local
/// network's own names), or one under `.local`, `.internal`, `.localhost`, `.lan`, `.home.arpa`
/// or `.ts.net`, or the machine's own name. A web page that points its own domain at the machine
/// (DNS rebinding) sends its domain here, and is refused.
pub fn host_is_this_machine(host: &str, machine_name: &str) -> bool {
    if host.starts_with('[') {
        return true; // an IPv6 address
    }
    let name = match host.rsplit_once(':') {
        Some((n, port)) if port.bytes().all(|b| b.is_ascii_digit()) => n,
        _ => host,
    };
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if name.parse::<IpAddr>().is_ok() || (!name.is_empty() && !name.contains('.')) {
        return true;
    }
    const LOCAL: &[&str] = &[".local", ".internal", ".localhost", ".lan", ".home.arpa", ".ts.net"];
    LOCAL.iter().any(|suffix| name.ends_with(suffix)) || name == machine_name.to_ascii_lowercase()
}

/// The `Origin` is this machine, as the browser addressed it, or there is none. Every browser
/// sends `Origin` on anything but GET and HEAD and on WebSocket upgrades, so a request without one
/// isn't from a web page (and `SameSite=Strict` keeps the cookie off other sites' requests anyway).
fn origin_is_this_machine(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let origin = origin.as_bytes();
    origin == format!("http://{host}").as_bytes() || origin == format!("https://{host}").as_bytes()
}

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/v0/session", get(session).post(sign_in).delete(sign_out))
        .route("/api/v0/setup", post(setup))
        .route("/api/v0/users", get(users).post(add_user))
        .route("/api/v0/users/{name}", delete(remove_user))
        .route("/api/v0/users/{name}/role", put(set_role))
        .route("/api/v0/users/{name}/password", put(set_password))
        .route("/api/v0/tokens", get(tokens).post(create_token))
        .route("/api/v0/tokens/{id}", delete(revoke_token))
}

fn identity_info(who: &Identity) -> dto::IdentityInfo {
    dto::IdentityInfo {
        user: who.user.clone(),
        role: who.role,
        via: match who.via {
            Via::Session => dto::SignedInVia::Session,
            Via::Token { .. } => dto::SignedInVia::Token,
            Via::Screen => dto::SignedInVia::Screen,
            Via::Developer => dto::SignedInVia::Developer,
        },
    }
}

fn session_info(s: &AppState, who: Option<&Identity>) -> dto::SessionInfo {
    let settings = s.machine.settings();
    dto::SessionInfo {
        machine_name: settings.machine.name,
        language: settings.appearance.language,
        needs_setup: s.auth == AuthMode::Required && s.machine.needs_setup(),
        sign_in_off: s.auth == AuthMode::Off,
        identity: who.map(identity_info),
    }
}

/// The session cookie: HttpOnly, SameSite=Strict, for as long as a session can live.
fn set_cookie(token: &str) -> HeaderValue {
    let max_age = crate::sessions::LIFETIME_US / 1_000_000;
    HeaderValue::from_str(&format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}"))
        .expect("a session token is a valid header value")
}

fn clear_cookie() -> HeaderValue {
    HeaderValue::from_str(&format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"))
        .expect("a valid header value")
}

async fn session(State(s): State<AppState>, Extension(caller): Extension<Caller>) -> Json<dto::SessionInfo> {
    Json(session_info(&s, caller.identity.as_ref()))
}

async fn sign_in(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    ApiJson(body): ApiJson<dto::SignIn>,
) -> Result<Response, Failure> {
    let from = caller.peer.map(|p| p.to_string()).unwrap_or_else(|| "unknown".into());
    let machine = std::sync::Arc::clone(&s.machine);
    match blocking(move || machine.sign_in(&body.name, &body.password, &from)).await? {
        Ok((who, token)) => {
            let info = session_info(&s, Some(&who));
            Ok(([(header::SET_COOKIE, set_cookie(&token))], Json(info)).into_response())
        }
        Err(SignInError::Failed) => Err(Failure::new(
            StatusCode::UNAUTHORIZED,
            "sign-in-failed",
            "That name and password don't match.",
            "Check the name and the password (capital letters count). An admin can set a new password.",
        )),
        Err(SignInError::Wait(us)) => {
            let seconds = us.div_ceil(1_000_000).max(1);
            let mut response = Failure::new(
                StatusCode::TOO_MANY_REQUESTS,
                "too-many-attempts",
                format!("Too many wrong passwords. Wait {seconds} s before trying again."),
                "Each wrong password makes the wait longer. An admin can set a new password.",
            )
            .into_response();
            response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(seconds));
            Ok(response)
        }
        Err(SignInError::Refused(e)) => Err(e.into()),
    }
}

async fn sign_out(State(s): State<AppState>, Extension(caller): Extension<Caller>) -> Response {
    if let Some(token) = caller.session() {
        s.machine.sign_out(token);
    }
    (StatusCode::NO_CONTENT, [(header::SET_COOKIE, clear_cookie())]).into_response()
}

async fn setup(State(s): State<AppState>, ApiJson(body): ApiJson<dto::SignIn>) -> Result<Response, Failure> {
    if s.auth == AuthMode::Off {
        return Err(Failure::new(
            StatusCode::CONFLICT,
            "sign-in-off",
            "Sign-in is off (--no-auth), so there is no admin to create.",
            "Start the core without --no-auth to set up users.",
        ));
    }
    let machine = std::sync::Arc::clone(&s.machine);
    let (who, token) = blocking(move || machine.create_first_admin(&body.name, &body.password)).await??;
    let info = session_info(&s, Some(&who));
    Ok((StatusCode::CREATED, [(header::SET_COOKIE, set_cookie(&token))], Json(info)).into_response())
}

fn user_info(u: &UserView) -> dto::UserInfo {
    dto::UserInfo { name: u.name.clone(), role: u.role }
}

fn token_info(t: &TokenView) -> dto::TokenInfo {
    dto::TokenInfo {
        id: t.id.clone(),
        name: t.name.clone(),
        user: t.user.clone(),
        role: t.role,
        created: t.created.clone(),
    }
}

async fn users(State(s): State<AppState>) -> Json<Vec<dto::UserInfo>> {
    Json(s.machine.users().iter().map(user_info).collect())
}

async fn add_user(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::NewUser>,
) -> Result<(StatusCode, Json<dto::UserInfo>), Failure> {
    let user = blocking(move || s.machine.add_user(&body.name, &body.password, body.role)).await??;
    Ok((StatusCode::CREATED, Json(user_info(&user))))
}

async fn remove_user(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
) -> Result<StatusCode, Failure> {
    let who = caller.who()?.clone();
    blocking(move || s.machine.remove_user(&name, &who)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_role(
    State(s): State<AppState>,
    Path(name): Path<String>,
    ApiJson(body): ApiJson<dto::RoleChange>,
) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.set_user_role(&name, body.role)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_password(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
    ApiJson(body): ApiJson<dto::PasswordChange>,
) -> Result<StatusCode, Failure> {
    let who = caller.who()?.clone();
    let keep = caller.session().map(str::to_string);
    blocking(move || {
        s.machine.set_password(&name, &body.password, body.current.as_deref(), &who, keep.as_deref())
    })
    .await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn tokens(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
) -> Result<Json<Vec<dto::TokenInfo>>, Failure> {
    let who = caller.who()?;
    Ok(Json(s.machine.tokens(who).iter().map(token_info).collect()))
}

async fn create_token(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    ApiJson(body): ApiJson<dto::NewToken>,
) -> Result<(StatusCode, Json<dto::TokenCreated>), Failure> {
    let who = caller.who()?;
    let (token, secret) = s.machine.create_token(&body.name, body.role, who)?;
    Ok((StatusCode::CREATED, Json(dto::TokenCreated { token: token_info(&token), secret })))
}

async fn revoke_token(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
) -> Result<StatusCode, Failure> {
    let who = caller.who()?;
    s.machine.revoke_token(&id, who)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_names_that_can_be_this_machine_are_answered() {
        for ok in [
            "127.0.0.1:7878",
            "192.168.1.20",
            "[::1]:7878",
            "localhost:5173",
            "faber.local",
            "sennep.local:80",
            "faber.internal",
            "faber.tail1234.ts.net",
            "app.localhost",
            "faber",
            "SENNEP",
            "sennep.lan",
        ] {
            assert!(host_is_this_machine(ok, "faber"), "{ok}");
        }
        for bad in ["evil.example.com", "faber.local.evil.com", "attacker.net:7878", "localhost.evil.org"] {
            assert!(!host_is_this_machine(bad, "faber"), "{bad}");
        }
    }

    #[test]
    fn the_session_cookie_is_found_among_others() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("a=1; faber-session=abc; b=2"));
        assert_eq!(cookie(&h), Some("abc"));
        h.insert(header::COOKIE, HeaderValue::from_static("faber-sessionx=1"));
        assert_eq!(cookie(&h), None);
    }

    #[test]
    fn the_cookie_is_http_only_and_strict() {
        let c = set_cookie("t").to_str().unwrap().to_string();
        assert!(
            c.contains("HttpOnly") && c.contains("SameSite=Strict") && c.contains("Max-Age=2592000"),
            "{c}"
        );
    }

    #[test]
    fn an_origin_must_be_the_host_the_browser_used() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_static("faber.local:7878"));
        assert!(origin_is_this_machine(&h), "no Origin: not a web page");
        h.insert(header::ORIGIN, HeaderValue::from_static("http://faber.local:7878"));
        assert!(origin_is_this_machine(&h));
        h.insert(header::ORIGIN, HeaderValue::from_static("http://evil.example"));
        assert!(!origin_is_this_machine(&h));
        h.insert(header::ORIGIN, HeaderValue::from_static("null"));
        assert!(!origin_is_this_machine(&h), "a sandboxed frame");
    }
}
