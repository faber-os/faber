//! Users, roles and signing in through the API (docs/auth.md): every route's role boundary, the
//! first run, sessions on the core's clock, the sign-in rate limit, the Origin and Host checks,
//! API tokens, the attached screen and the WebSocket.

mod support;

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use faber_core::api::access::{Need, ROUTES};
use faber_core::api::auth::{AuthMode, OnScreen};
use faber_core::api::{self, AppState};
use faber_core::clock::ManualClock;
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::ports::{
    Discovered, FailedUsb, ReplayLibrary, SerialDiscovery, SystemOpener, UsbProblemDiscovery,
};
use faber_core::runtime::Timing;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tower::ServiceExt;

struct NoPorts;
impl SerialDiscovery for NoPorts {
    fn list(&self) -> Vec<Discovered> {
        Vec::new()
    }
}
impl UsbProblemDiscovery for NoPorts {
    fn list(&self) -> Vec<FailedUsb> {
        Vec::new()
    }
}
impl DiskDiscovery for NoPorts {
    fn list(&self) -> Vec<Disk> {
        Vec::new()
    }
}

const HOST: &str = "faber.local";
const ORIGIN: &str = "http://faber.local";

struct TestCore {
    app: axum::Router,
    machine: Arc<Machine>,
    clock: Arc<ManualClock>,
    logs: Arc<LogBook>,
    _dir: tempfile::TempDir,
}

impl Drop for TestCore {
    fn drop(&mut self) {
        self.machine.shutdown();
    }
}

/// A fresh machine with no users, on a clock the test moves by hand.
fn core() -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::new(1_000_000));
    let replays = ReplayLibrary::default();
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: clock.clone(),
        discovery: Arc::new(NoPorts),
        usb_problems: Arc::new(NoPorts),
        disks: Arc::new(NoPorts),
        opener: Arc::new(SystemOpener::new(replays.clone())),
        replays,
        connectors: faber_core::connectors::built_in(),
        timing: Timing::default(),
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    let logs = LogBook::new(clock.clone());
    let app = api::router(AppState::new(Arc::clone(&machine), Arc::clone(&logs)))
        .layer(MockConnectInfo(SocketAddr::from(([10, 0, 0, 5], 50000))));
    TestCore { app, machine, clock, logs, _dir: dir }
}

/// How a test request says who it is.
#[derive(Clone, Debug)]
enum As {
    Nobody,
    Cookie(String),
    Token(String),
}

struct Answer {
    status: StatusCode,
    body: Value,
    headers: axum::http::HeaderMap,
}

impl Answer {
    fn code(&self) -> &str {
        self.body["error"]["code"].as_str().unwrap_or("")
    }
}

/// A request as a browser on `faber.local` sends it: Host, and Origin on anything that changes.
async fn call(app: &axum::Router, who: &As, method: &str, path: &str, body: Option<Value>) -> Answer {
    let mut req = Request::builder().method(method).uri(path).header(header::HOST, HOST);
    if method != "GET" {
        req = req.header(header::ORIGIN, ORIGIN);
    }
    req = match who {
        As::Nobody => req,
        As::Cookie(c) => req.header(header::COOKIE, format!("faber-session={c}")),
        As::Token(t) => req.header(header::AUTHORIZATION, format!("Bearer {t}")),
    };
    let body = match body {
        Some(b) => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    send(app, req.body(body).unwrap()).await
}

async fn send(app: &axum::Router, req: Request<Body>) -> Answer {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Answer { status, body, headers }
}

/// The session cookie a sign-in or setup answer sets.
fn session_cookie(a: &Answer) -> String {
    let set = a.headers.get(header::SET_COOKIE).expect("a cookie").to_str().unwrap();
    assert!(set.contains("HttpOnly") && set.contains("SameSite=Strict"), "{set}");
    set.strip_prefix("faber-session=").unwrap().split(';').next().unwrap().to_string()
}

async fn setup(c: &TestCore) -> String {
    let a = call(
        &c.app,
        &As::Nobody,
        "POST",
        "/api/v0/setup",
        Some(json!({"name": "mikkel", "password": "correct horse"})),
    )
    .await;
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    session_cookie(&a)
}

async fn sign_in(c: &TestCore, name: &str, password: &str) -> String {
    let a = call(
        &c.app,
        &As::Nobody,
        "POST",
        "/api/v0/session",
        Some(json!({"name": name, "password": password})),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body);
    session_cookie(&a)
}

/// An admin (mikkel), an operator (olive) and a viewer (vic), each signed in.
async fn three_users(c: &TestCore) -> (String, String, String) {
    let admin = setup(c).await;
    for (name, role) in [("olive", "operator"), ("vic", "viewer")] {
        let a = call(
            &c.app,
            &As::Cookie(admin.clone()),
            "POST",
            "/api/v0/users",
            Some(json!({"name": name, "password": "password1", "role": role})),
        )
        .await;
        assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    }
    let operator = sign_in(c, "olive", "password1").await;
    let viewer = sign_in(c, "vic", "password1").await;
    (admin, operator, viewer)
}

/// A route pattern as a request path: each `{param}` becomes something that doesn't exist, so
/// allowed calls reach the handler and fail there (not found, bad request), never with a side
/// effect that matters.
fn concrete(pattern: &str) -> String {
    pattern
        .split('/')
        .map(|seg| if seg.starts_with('{') { "nothing-here" } else { seg })
        .collect::<Vec<_>>()
        .join("/")
}

fn rank(need: Need) -> u8 {
    match need {
        Need::Public => 0,
        Need::Role(r) => r as u8 + 1,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_route_keeps_its_role_boundary() {
    let c = core();
    let (admin, operator, viewer) = three_users(&c).await;
    let callers = [
        (0u8, "nobody", As::Nobody),
        (1, "viewer", As::Cookie(viewer)),
        (2, "operator", As::Cookie(operator)),
        (3, "admin", As::Cookie(admin)),
    ];
    // Signing out last, so every other call still has its session.
    let mut routes: Vec<_> = ROUTES.iter().collect();
    routes.sort_by_key(|(m, p, _)| (*m == "DELETE" && *p == "/api/v0/session", *p));
    for (method, pattern, need) in routes {
        let path = concrete(pattern);
        for (level, name, who) in &callers {
            let a = call(&c.app, who, method, &path, None).await;
            let what = format!("{method} {pattern} as {name}: {} {}", a.status, a.body);
            if *level >= rank(*need) {
                assert!(
                    a.code() != "not-signed-in"
                        && a.code() != "not-allowed"
                        && a.status != StatusCode::UNAUTHORIZED,
                    "allowed, but refused: {what}"
                );
            } else if *level == 0 {
                assert_eq!((a.status, a.code()), (StatusCode::UNAUTHORIZED, "not-signed-in"), "{what}");
            } else {
                assert_eq!((a.status, a.code()), (StatusCode::FORBIDDEN, "not-allowed"), "{what}");
                let Need::Role(role) = need else { unreachable!() };
                assert_eq!(a.body["error"]["needs"], role.as_str(), "{what}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_browser_creates_the_admin_once() {
    let c = core();
    let s = call(&c.app, &As::Nobody, "GET", "/api/v0/session", None).await;
    assert_eq!(s.body["needsSetup"], true, "{}", s.body);
    assert_eq!(s.body.get("identity"), None);
    assert_eq!(s.body["machineName"], "faber");
    // Nothing else works yet.
    assert_eq!(
        call(&c.app, &As::Nobody, "GET", "/api/v0/parts", None).await.status,
        StatusCode::UNAUTHORIZED
    );

    let cookie = setup(&c).await;
    let s = call(&c.app, &As::Cookie(cookie.clone()), "GET", "/api/v0/session", None).await;
    assert_eq!(s.body["needsSetup"], false);
    assert_eq!(s.body["identity"], json!({"user": "mikkel", "role": "admin", "via": "session"}));
    assert_eq!(call(&c.app, &As::Cookie(cookie), "GET", "/api/v0/parts", None).await.status, StatusCode::OK);

    let again = call(
        &c.app,
        &As::Nobody,
        "POST",
        "/api/v0/setup",
        Some(json!({"name": "eve", "password": "password1"})),
    )
    .await;
    assert_eq!((again.status, again.code()), (StatusCode::CONFLICT, "already-set-up"));
    assert!(again.headers.get(header::SET_COOKIE).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn signing_out_ends_the_session() {
    let c = core();
    let cookie = setup(&c).await;
    let out = call(&c.app, &As::Cookie(cookie.clone()), "DELETE", "/api/v0/session", None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert!(out.headers[header::SET_COOKIE].to_str().unwrap().contains("Max-Age=0"));
    let a = call(&c.app, &As::Cookie(cookie), "GET", "/api/v0/parts", None).await;
    assert_eq!((a.status, a.code()), (StatusCode::UNAUTHORIZED, "not-signed-in"));
    let bad = call(
        &c.app,
        &As::Nobody,
        "POST",
        "/api/v0/session",
        Some(json!({"name": "mikkel", "password": "nope nope"})),
    )
    .await;
    assert_eq!((bad.status, bad.code()), (StatusCode::UNAUTHORIZED, "sign-in-failed"));
    sign_in(&c, "mikkel", "correct horse").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_expires_on_the_cores_clock() {
    let c = core();
    let cookie = setup(&c).await;
    let day = 24 * 60 * 60 * 1_000_000;
    c.clock.advance(6 * day);
    assert_eq!(
        call(&c.app, &As::Cookie(cookie.clone()), "GET", "/api/v0/parts", None).await.status,
        StatusCode::OK
    );
    c.clock.advance(6 * day);
    assert_eq!(
        call(&c.app, &As::Cookie(cookie.clone()), "GET", "/api/v0/parts", None).await.status,
        StatusCode::OK,
        "used a day ago: the expiry slid"
    );
    c.clock.advance(7 * day);
    assert_eq!(
        call(&c.app, &As::Cookie(cookie), "GET", "/api/v0/parts", None).await.status,
        StatusCode::UNAUTHORIZED
    );

    let cookie = sign_in(&c, "mikkel", "correct horse").await;
    for _ in 0..29 {
        c.clock.advance(day);
        assert_eq!(
            call(&c.app, &As::Cookie(cookie.clone()), "GET", "/api/v0/parts", None).await.status,
            StatusCode::OK
        );
    }
    c.clock.advance(day);
    assert_eq!(
        call(&c.app, &As::Cookie(cookie), "GET", "/api/v0/parts", None).await.status,
        StatusCode::UNAUTHORIZED,
        "30 days at most"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_passwords_wait_longer_and_longer() {
    let c = core();
    setup(&c).await;
    let wrong = || json!({"name": "mikkel", "password": "wrong password"});
    for _ in 0..5 {
        let a = call(&c.app, &As::Nobody, "POST", "/api/v0/session", Some(wrong())).await;
        assert_eq!(a.code(), "sign-in-failed");
    }
    let a = call(
        &c.app,
        &As::Nobody,
        "POST",
        "/api/v0/session",
        Some(json!({"name": "mikkel", "password": "correct horse"})),
    )
    .await;
    assert_eq!(
        (a.status, a.code()),
        (StatusCode::TOO_MANY_REQUESTS, "too-many-attempts"),
        "even the right one waits"
    );
    assert_eq!(a.headers[header::RETRY_AFTER], "1");
    c.clock.advance(1_000_000);
    let a = call(&c.app, &As::Nobody, "POST", "/api/v0/session", Some(wrong())).await;
    assert_eq!(a.code(), "sign-in-failed");
    let a = call(&c.app, &As::Nobody, "POST", "/api/v0/session", Some(wrong())).await;
    assert_eq!(a.headers[header::RETRY_AFTER], "2", "doubled");
    // Another name from the same address waits too: the address is counted.
    let a =
        call(&c.app, &As::Nobody, "POST", "/api/v0/session", Some(json!({"name": "eve", "password": "x"})))
            .await;
    assert_eq!(a.status, StatusCode::TOO_MANY_REQUESTS);
    c.clock.advance(2_000_000);
    sign_in(&c, "mikkel", "correct horse").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn another_web_page_is_refused() {
    let c = core();
    let cookie = setup(&c).await;
    let req = |origin: Option<&str>| {
        let mut r = Request::builder()
            .method("POST")
            .uri("/api/v0/estop/release")
            .header(header::HOST, HOST)
            .header(header::COOKIE, format!("faber-session={cookie}"));
        if let Some(o) = origin {
            r = r.header(header::ORIGIN, o);
        }
        r.body(Body::empty()).unwrap()
    };
    let a = send(&c.app, req(Some("http://evil.example"))).await;
    assert_eq!((a.status, a.code()), (StatusCode::FORBIDDEN, "bad-origin"));
    let a = send(&c.app, req(Some("null"))).await;
    assert_eq!(a.code(), "bad-origin", "a sandboxed frame");
    assert_eq!(send(&c.app, req(Some(ORIGIN))).await.status, StatusCode::OK);
    assert_eq!(send(&c.app, req(None)).await.status, StatusCode::OK, "not from a web page");
    // Even signing in: a page can't sign a visitor into the attacker's account.
    let login = Request::builder()
        .method("POST")
        .uri("/api/v0/session")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "http://evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"name": "mikkel", "password": "correct horse"}).to_string()))
        .unwrap();
    assert_eq!(send(&c.app, login).await.code(), "bad-origin");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rebound_domain_is_refused() {
    let c = core();
    let req = Request::builder()
        .uri("/api/v0/session")
        .header(header::HOST, "evil.example.com")
        .body(Body::empty())
        .unwrap();
    let a = send(&c.app, req).await;
    assert_eq!((a.status, a.code()), (StatusCode::MISDIRECTED_REQUEST, "unknown-host"));
    let setup = Request::builder()
        .method("POST")
        .uri("/api/v0/setup")
        .header(header::HOST, "evil.example.com:7878")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"name": "eve", "password": "password1"}).to_string()))
        .unwrap();
    assert_eq!(send(&c.app, setup).await.code(), "unknown-host");
    assert!(c.machine.needs_setup(), "a rebound page can't claim a fresh machine");
}

#[tokio::test(flavor = "multi_thread")]
async fn tokens_are_shown_once_capped_and_revocable() {
    let c = core();
    let (admin, operator, viewer) = three_users(&c).await;
    let over = call(
        &c.app,
        &As::Cookie(operator.clone()),
        "POST",
        "/api/v0/tokens",
        Some(json!({"name": "x", "role": "admin"})),
    )
    .await;
    assert_eq!((over.status, over.code()), (StatusCode::FORBIDDEN, "role-too-high"));
    let made = call(
        &c.app,
        &As::Cookie(operator.clone()),
        "POST",
        "/api/v0/tokens",
        Some(json!({"name": "drive-square", "role": "operator"})),
    )
    .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let secret = made.body["secret"].as_str().unwrap().to_string();
    let id = made.body["token"]["id"].as_str().unwrap().to_string();
    assert!(secret.starts_with("faber_"));
    let token = As::Token(secret.clone());

    // It drives (the route lets an operator), and it is never shown again.
    let s = call(&c.app, &token, "GET", "/api/v0/session", None).await;
    assert_eq!(s.body["identity"], json!({"user": "olive", "role": "operator", "via": "token"}));
    let listed = call(&c.app, &As::Cookie(operator.clone()), "GET", "/api/v0/tokens", None).await;
    assert_eq!(listed.body.as_array().unwrap().len(), 1);
    assert!(!listed.body.to_string().contains(&secret));
    // A token can't make tokens.
    let a =
        call(&c.app, &token, "POST", "/api/v0/tokens", Some(json!({"name": "y", "role": "viewer"}))).await;
    assert_eq!(a.code(), "not-allowed");
    // Others see only their own; an admin sees all.
    assert_eq!(
        call(&c.app, &As::Cookie(viewer.clone()), "GET", "/api/v0/tokens", None).await.body,
        json!([])
    );
    assert_eq!(
        call(&c.app, &As::Cookie(admin.clone()), "GET", "/api/v0/tokens", None)
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let a = call(&c.app, &As::Cookie(viewer), "DELETE", &format!("/api/v0/tokens/{id}"), None).await;
    assert_eq!(a.code(), "not-allowed");
    // Demoting the user demotes the token.
    let a = call(
        &c.app,
        &As::Cookie(admin.clone()),
        "PUT",
        "/api/v0/users/olive/role",
        Some(json!({"role": "viewer"})),
    )
    .await;
    assert_eq!(a.status, StatusCode::NO_CONTENT);
    let a = call(&c.app, &token, "POST", "/api/v0/estop/release", None).await;
    assert_eq!((a.code(), a.body["error"]["needs"].as_str()), ("not-allowed", Some("operator")));
    // Revoked: a clear 401.
    assert_eq!(
        call(&c.app, &As::Cookie(operator), "DELETE", &format!("/api/v0/tokens/{id}"), None).await.status,
        StatusCode::NO_CONTENT
    );
    let a = call(&c.app, &token, "GET", "/api/v0/parts", None).await;
    assert_eq!((a.status, a.code()), (StatusCode::UNAUTHORIZED, "not-signed-in"));
    let a = call(&c.app, &As::Token("faber_made-up".into()), "GET", "/api/v0/session", None).await;
    assert_eq!(a.status, StatusCode::UNAUTHORIZED, "a wrong token is an error, not nobody");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_viewer_cannot_drive_but_can_press_stop() {
    let c = core();
    let (_, _, viewer) = three_users(&c).await;
    let v = As::Cookie(viewer);
    let a = call(&c.app, &v, "POST", "/api/v0/parts/base/lease", Some(json!({"holder": "vic"}))).await;
    assert_eq!((a.code(), a.body["error"]["needs"].as_str()), ("not-allowed", Some("operator")));
    let a = call(&c.app, &v, "POST", "/api/v0/estop", Some(json!({"source": "shell"}))).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.body["engaged"], true, "{}", a.body);
    assert_eq!(call(&c.app, &v, "POST", "/api/v0/estop/release", None).await.code(), "not-allowed");
    // Stop is never gated: not even on the sign-in screen.
    assert_eq!(call(&c.app, &As::Nobody, "POST", "/api/v0/estop", None).await.status, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn display_settings_are_for_operators_and_the_machines_own_for_admins() {
    let c = core();
    let (admin, operator, _) = three_users(&c).await;
    let op = As::Cookie(operator);
    let mut s = call(&c.app, &op, "GET", "/api/v0/settings", None).await.body;
    s["appearance"]["theme"] = json!("dark");
    assert_eq!(call(&c.app, &op, "PUT", "/api/v0/settings", Some(s.clone())).await.status, StatusCode::OK);
    s["machine"]["name"] = json!("sennep");
    let a = call(&c.app, &op, "PUT", "/api/v0/settings", Some(s.clone())).await;
    assert_eq!((a.code(), a.body["error"]["needs"].as_str()), ("not-allowed", Some("admin")));
    s["machine"]["name"] = json!("faber");
    s["screen"]["role"] = json!("operator");
    assert_eq!(call(&c.app, &op, "PUT", "/api/v0/settings", Some(s.clone())).await.code(), "not-allowed");
    assert_eq!(
        call(&c.app, &As::Cookie(admin), "PUT", "/api/v0/settings", Some(s)).await.status,
        StatusCode::OK
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn backups_and_the_machines_files_are_for_admins() {
    let c = core();
    let (admin, operator, _) = three_users(&c).await;
    let op = As::Cookie(operator);
    assert_eq!(
        call(&c.app, &op, "GET", "/api/v0/files/list?path=home:/Documents", None).await.status,
        StatusCode::OK
    );
    let a = call(&c.app, &op, "GET", "/api/v0/files/list?path=home:/Backups", None).await;
    assert_eq!((a.code(), a.body["error"]["needs"].as_str()), ("not-allowed", Some("admin")));
    let a = call(
        &c.app,
        &op,
        "POST",
        "/api/v0/files/move",
        Some(json!({"from": "home:/Backups/b.zip", "to": "home:/Desktop/b.zip"})),
    )
    .await;
    assert_eq!(a.code(), "not-allowed");
    assert_eq!(
        call(&c.app, &op, "GET", "/api/v0/files/list?path=machine:/", None).await.code(),
        "not-allowed"
    );
    assert_eq!(
        call(&c.app, &As::Cookie(admin), "GET", "/api/v0/files/list?path=home:/Backups", None).await.status,
        StatusCode::OK
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn passwords_and_users_are_managed_by_their_rules() {
    let c = core();
    let (admin, operator, viewer) = three_users(&c).await;
    // Your own needs the current one; it ends your other sessions but not this one.
    let other = sign_in(&c, "vic", "password1").await;
    let a = call(
        &c.app,
        &As::Cookie(viewer.clone()),
        "PUT",
        "/api/v0/users/vic/password",
        Some(json!({"password": "new password"})),
    )
    .await;
    assert_eq!(a.code(), "wrong-password");
    let a = call(
        &c.app,
        &As::Cookie(viewer.clone()),
        "PUT",
        "/api/v0/users/vic/password",
        Some(json!({"password": "new password", "current": "password1"})),
    )
    .await;
    assert_eq!(a.status, StatusCode::NO_CONTENT, "{}", a.body);
    assert_eq!(
        call(&c.app, &As::Cookie(viewer.clone()), "GET", "/api/v0/parts", None).await.status,
        StatusCode::OK
    );
    assert_eq!(
        call(&c.app, &As::Cookie(other), "GET", "/api/v0/parts", None).await.status,
        StatusCode::UNAUTHORIZED
    );
    // Someone else's: only an admin.
    let a = call(
        &c.app,
        &As::Cookie(operator.clone()),
        "PUT",
        "/api/v0/users/vic/password",
        Some(json!({"password": "taken over"})),
    )
    .await;
    assert_eq!(a.code(), "not-allowed");
    // Users are listed without passwords; the last admin stays.
    let users = call(&c.app, &As::Cookie(admin.clone()), "GET", "/api/v0/users", None).await.body;
    assert_eq!(
        users,
        json!([{"name": "mikkel", "role": "admin"}, {"name": "olive", "role": "operator"}, {"name": "vic", "role": "viewer"}])
    );
    let a = call(
        &c.app,
        &As::Cookie(admin.clone()),
        "PUT",
        "/api/v0/users/mikkel/role",
        Some(json!({"role": "viewer"})),
    )
    .await;
    assert_eq!((a.status, a.code()), (StatusCode::CONFLICT, "last-admin"));
    assert_eq!(
        call(&c.app, &As::Cookie(admin.clone()), "DELETE", "/api/v0/users/mikkel", None).await.code(),
        "remove-yourself"
    );
    // Removing someone signs them out.
    assert_eq!(
        call(&c.app, &As::Cookie(admin), "DELETE", "/api/v0/users/olive", None).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(&c.app, &As::Cookie(operator), "GET", "/api/v0/parts", None).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_attached_screen_is_signed_in_as_its_role() {
    let c = core();
    let screen = api::router(AppState::new(Arc::clone(&c.machine), Arc::clone(&c.logs)))
        .layer(axum::Extension(OnScreen))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 50000))));
    // Before anyone has set the machine up, the screen already works.
    let s = call(&screen, &As::Nobody, "GET", "/api/v0/session", None).await;
    assert_eq!(s.body["identity"], json!({"role": "viewer", "via": "screen"}));
    assert_eq!(call(&screen, &As::Nobody, "GET", "/api/v0/parts", None).await.status, StatusCode::OK);
    assert_eq!(call(&screen, &As::Nobody, "POST", "/api/v0/estop", None).await.status, StatusCode::OK);
    assert_eq!(call(&screen, &As::Nobody, "POST", "/api/v0/estop/release", None).await.code(), "not-allowed");
    // Someone on the network is not the screen, even on its listener.
    let remote = api::router(AppState::new(Arc::clone(&c.machine), Arc::clone(&c.logs)))
        .layer(axum::Extension(OnScreen))
        .layer(MockConnectInfo(SocketAddr::from(([10, 0, 0, 9], 50000))));
    assert_eq!(
        call(&remote, &As::Nobody, "GET", "/api/v0/parts", None).await.status,
        StatusCode::UNAUTHORIZED
    );
    // An admin makes it an operator.
    let admin = setup(&c).await;
    let mut s = call(&c.app, &As::Cookie(admin.clone()), "GET", "/api/v0/settings", None).await.body;
    s["screen"]["role"] = json!("operator");
    assert_eq!(
        call(&c.app, &As::Cookie(admin), "PUT", "/api/v0/settings", Some(s)).await.status,
        StatusCode::OK
    );
    assert_eq!(
        call(&screen, &As::Nobody, "POST", "/api/v0/estop/release", None).await.status,
        StatusCode::OK
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn without_auth_everyone_is_an_admin() {
    let c = core();
    let app = api::router(AppState {
        auth: AuthMode::Off,
        ..AppState::new(Arc::clone(&c.machine), Arc::clone(&c.logs))
    });
    let s = call(&app, &As::Nobody, "GET", "/api/v0/session", None).await;
    assert_eq!(s.body["signInOff"], true);
    assert_eq!(s.body["needsSetup"], false);
    assert_eq!(s.body["identity"], json!({"role": "admin", "via": "developer"}));
    assert_eq!(
        call(&app, &As::Nobody, "POST", "/api/v0/backups/setup", None).await.status,
        StatusCode::CREATED
    );
}

/// A WebSocket as a browser opens it: the cookie, and its page's Origin.
async fn open_ws(
    app: &axum::Router,
    cookie: &str,
    origin: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Error,
> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = app.clone();
    tokio::spawn(async move {
        axum::serve(listener, server.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap()
    });
    let mut req = format!("ws://{addr}/api/v0/ws?format=json").into_client_request().unwrap();
    req.headers_mut().insert(header::COOKIE, format!("faber-session={cookie}").parse().unwrap());
    req.headers_mut().insert(header::ORIGIN, origin.replace("ADDR", &addr.to_string()).parse().unwrap());
    tokio_tungstenite::connect_async(req).await.map(|(ws, _)| ws)
}

async fn next_json(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
) -> Option<Result<Value, Option<u16>>> {
    let m = tokio::time::timeout(Duration::from_secs(5), ws.next()).await.expect("an answer in time")?;
    match m.ok()? {
        Message::Text(t) => Some(Ok(serde_json::from_str(&t).unwrap())),
        Message::Close(frame) => Some(Err(frame.map(|f| f.code.into()))),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_websocket_checks_the_cookie_the_origin_and_each_message() {
    let c = core();
    let (admin, _, viewer) = three_users(&c).await;
    assert!(open_ws(&c.app, &viewer, "http://evil.example").await.is_err(), "another page's Origin");
    assert!(open_ws(&c.app, "not-a-session", "http://ADDR").await.is_err(), "not signed in");
    let mut ws = open_ws(&c.app, &viewer, "http://ADDR").await.unwrap();
    // A viewer watches, but can't open the serial monitor.
    ws.send(Message::Text(
        json!({"type": "serial-open", "id": 7, "port": "COM99", "baud": 9600}).to_string().into(),
    ))
    .await
    .unwrap();
    let m = next_json(&mut ws).await.unwrap().unwrap();
    assert_eq!(
        (m["type"].as_str(), m["error"]["code"].as_str(), m["error"]["needs"].as_str()),
        (Some("error"), Some("not-allowed"), Some("operator")),
        "{m}"
    );
    // Removing the user closes the socket: 4401.
    let a = call(&c.app, &As::Cookie(admin), "DELETE", "/api/v0/users/vic", None).await;
    assert_eq!(a.status, StatusCode::NO_CONTENT);
    loop {
        match next_json(&mut ws).await {
            Some(Err(code)) => {
                assert_eq!(code, Some(4401));
                break;
            }
            Some(Ok(_)) => continue,
            None => panic!("closed without a close frame"),
        }
    }
}
