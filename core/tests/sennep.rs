//! The SENNEP app (examples/apps/sennep) proven in the apartment (Phase 5, docs/roadmap.md): a
//! simulated SENNEP V2 (its base, an LD19 0.2 m ahead of the centre, a distance sensor at each
//! corner and a 3S Li-ion battery) gets the packed app, and a client drives it the way the shell
//! does for the app's window: over the WebSocket, it drives the base through the app's logic
//! (`app-window-drive`, as after the D46 confirm) and sends `drive` messages every 100 ms, with
//! the page's heartbeat, so every command passes the app's logic and its safety layer.
//!
//! It shows that the app stops SENNEP short of a wall without a bump, that its dead-reckoned pose
//! follows the world's true pose over a short drive, that a critical battery caps the speed, that
//! the shell's Stop, the window going quiet or closing, and turning the app off stop the base, and
//! that driving saves nothing.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_capabilities::{Reading, Twist2d, planar_base};
use faber_core::api;
use faber_core::clock::MonotonicClock;
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::names::SignalAddress;
use faber_core::ports::{
    Discovered, FailedUsb, ReplayLibrary, SerialDiscovery, SystemOpener, UsbProblemDiscovery,
};
use faber_core::router::Subscription;
use faber_core::runtime::Timing;
use serde_json::{Value, json};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::{AppWindow, check_e2e_fixture, component, example_package};
use tower::ServiceExt;

/// The package the shell's end-to-end test installs (shell/e2e/sennep.spec.ts).
#[test]
fn the_e2e_package_is_the_packed_sennep_app() {
    let Some(wasm) = component("sennep") else { return };
    check_e2e_fixture("sennep-0.1.0.faberpkg", example_package("sennep", &wasm));
}

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

struct TestCore {
    app: axum::Router,
    machine: Arc<Machine>,
    _dir: tempfile::TempDir,
}

impl Drop for TestCore {
    fn drop(&mut self) {
        self.machine.shutdown();
    }
}

fn core() -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(MonotonicClock::new());
    let replays = ReplayLibrary::default();
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: clock.clone(),
        discovery: Arc::new(NoPorts),
        usb_problems: Arc::new(NoPorts),
        disks: Arc::new(NoPorts),
        opener: Arc::new(SystemOpener::new(replays.clone())),
        hardware: Arc::new(faber_core::ports::NoHardware),
        replays,
        connectors: faber_core::connectors::built_in(),
        timing: Timing::default(),
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    let app = api::router(api::AppState { machine: Arc::clone(&machine), logs: LogBook::new(clock) });
    TestCore { app, machine, _dir: dir }
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    let body = match body {
        Some(b) => {
            req = req.header("content-type", "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn latest(c: &TestCore, address: &str) -> Option<Reading> {
    match c.machine.router().subscribe(&SignalAddress::parse(address).unwrap())? {
        Subscription::Latest(rx) => rx.borrow().as_deref().cloned(),
        Subscription::Every(_) => panic!("{address} is a latest-value signal"),
    }
}

fn base(c: &TestCore) -> planar_base::Reading {
    match latest(c, "base/planar-base") {
        Some(Reading::PlanarBase(r)) => r,
        other => panic!("no base reading: {other:?}"),
    }
}

/// x, y and yaw of a pose signal.
fn pose_of(c: &TestCore, address: &str) -> Option<(f32, f32, f32)> {
    match latest(c, address)? {
        Reading::Pose(p) => {
            let q = p.orientation;
            let yaw = (2.0 * (q.w * q.z + q.x * q.y)).atan2(1.0 - 2.0 * (q.y * q.y + q.z * q.z));
            Some((p.position.x, p.position.y, yaw))
        }
        other => panic!("{address} is not a pose: {other:?}"),
    }
}

fn world_pose(c: &TestCore) -> (f32, f32, f32) {
    pose_of(c, "world/pose").expect("world/pose")
}

fn holder(c: &TestCore, part: &str) -> Option<String> {
    c.machine.leases().into_iter().find(|l| l.part.as_str() == part).map(|l| l.holder)
}

async fn wait_for<T>(what: &str, secs: f32, mut f: impl FnMut() -> Option<T>) -> T {
    let until = Instant::now() + Duration::from_secs_f32(secs);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < until, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn app_state(c: &TestCore) -> Value {
    call(&c.app, "GET", "/api/v0/apps/sennep/state", None).await.1["values"].clone()
}

/// Driving is live messages, never the app's saved state: nothing of it is there.
async fn nothing_saved(c: &TestCore) {
    let state = app_state(c).await;
    for key in ["drive", "limits", "reset-pose"] {
        assert!(state.get(key).is_none(), "{key} in the app's state: {state}");
    }
    let file = c._dir.path().join("apps/sennep/state.toml");
    let saved = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(!saved.contains("drive") && !saved.contains("limits"), "{saved}");
}

async fn wait_running(c: &TestCore) {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let (_, a) = call(&c.app, "GET", "/api/v0/apps/sennep", None).await;
        if a["state"]["kind"] == "running" {
            return;
        }
        assert!(Instant::now() < until, "the app never ran: {a}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// SENNEP's window, opened, driving the base through the app's logic while `x y turn` is held.
async fn drive(c: &TestCore, x: f32, y: f32, turn: f32) -> AppWindow {
    let mut w = AppWindow::open(&c.app, "sennep").await;
    w.drive_via("base").await;
    w.hold(x, y, turn);
    w
}

/// SENNEP V2 simulated in the apartment's living room at `start`, with the app installed, its
/// roles bound and on. Two batteries: the world's (it drains as SENNEP drives), bound to the
/// battery role, and a manual one to set by hand.
async fn sennep_with_the_app(start: (f32, f32, f32)) -> TestCore {
    let wasm = component("sennep").expect("checked by the caller");
    let c = core();
    let world = json!({"name": "apartment", "start": {"x": start.0, "y": start.1, "yaw": start.2}});
    assert_eq!(call(&c.app, "PUT", "/api/v0/world", Some(world)).await.0, StatusCode::OK);
    let mut parts = vec![
        json!({"name": "base", "connector": "generic-planar-base", "simulate": "world",
               "settings": {"length": 0.8, "width": 0.65, "max-vx": 0.6, "max-vy": 0.6, "max-omega": 1.5, "max-accel": 1.0}}),
        json!({"name": "lidar", "connector": "ldrobot-ld19", "simulate": "world", "mount": {"x": 0.2, "z": 0.3}}),
        json!({"name": "pack", "connector": "generic-battery", "simulate": "world",
               "settings": {"chemistry": "li-ion", "cells-in-series": 3}}),
        json!({"name": "spare", "connector": "generic-battery", "simulate": "manual", "settings": {"rate-hz": 20}}),
    ];
    for (name, x, y, yaw) in [
        ("front_left", 0.4, 0.325, FRAC_PI_4),
        ("front_right", 0.4, -0.325, -FRAC_PI_4),
        ("rear_left", -0.4, 0.325, 3.0 * FRAC_PI_4),
        ("rear_right", -0.4, -0.325, -3.0 * FRAC_PI_4),
    ] {
        parts.push(json!({"name": name, "connector": "generic-distance", "simulate": "world",
                          "mount": {"x": x, "y": y, "z": 0.2, "yaw": yaw}}));
    }
    for p in parts {
        let (s, body) = call(&c.app, "POST", "/api/v0/parts", Some(p)).await;
        assert_eq!(s, StatusCode::CREATED, "{body}");
    }
    wait_for("the first scan, pose and battery", 5.0, || {
        let scan = matches!(latest(&c, "lidar/laser-scan"), Some(Reading::LaserScan(_)));
        let pose = matches!(latest(&c, "world/pose"), Some(Reading::Pose(_)));
        let pack = matches!(latest(&c, "pack/battery"), Some(Reading::Battery(_)));
        (scan && pose && pack).then_some(())
    })
    .await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/v0/packages?name=sennep-0.1.0.faberpkg")
        .header("content-type", "application/octet-stream")
        .body(Body::from(example_package("sennep", &wasm)))
        .unwrap();
    let res = c.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let roles = json!({"roles": {
        "lidar": "lidar",
        "base": "base",
        "proximity": ["front_left", "front_right", "rear_left", "rear_right"],
        "battery": "pack",
    }});
    let (s, a) = call(&c.app, "PUT", "/api/v0/apps/sennep/roles", Some(roles)).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    let (s, a) = call(&c.app, "POST", "/api/v0/apps/sennep/enable", None).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    wait_running(&c).await;
    c
}

/// Facing the living room's south wall (y = 0), 1.2 m from SENNEP's front edge.
const FACING_THE_WALL: (f32, f32, f32) = (8.0, 1.6, -FRAC_PI_2);

#[tokio::test(flavor = "multi_thread")]
async fn driving_at_a_wall_the_app_stops_sennep_at_the_stop_distance_without_a_bump() {
    if component("sennep").is_none() {
        return;
    }
    let c = sennep_with_the_app(FACING_THE_WALL).await;
    assert_eq!(holder(&c, "base"), None, "nobody drives until the window asks");

    // Full speed ahead, held, as the window does while W is down.
    let mut window = drive(&c, 1.0, 0.0, 0.0).await;
    wait_for("the app to drive", 3.0, || {
        (holder(&c, "base").as_deref() == Some("app:sennep") && base(&c).commanded.vx > 0.2).then_some(())
    })
    .await;
    let started = Instant::now();
    let mut stopped_for = Duration::ZERO;
    while started.elapsed() < Duration::from_secs(10) && stopped_for < Duration::from_millis(800) {
        let b = base(&c);
        assert!(!b.bumped, "SENNEP bumped into something at {:?}", world_pose(&c));
        if b.commanded.vx == 0.0 && b.measured.is_some_and(|m| m.vx.abs() < 1e-3) {
            stopped_for += Duration::from_millis(20);
        } else {
            stopped_for = Duration::ZERO;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(stopped_for >= Duration::from_millis(800), "it never came to a stop");

    // The front edge (0.4 m ahead of the centre, facing −y) is at the stop distance from y = 0.
    let (_, y, _) = world_pose(&c);
    let gap = y - 0.4;
    assert!((0.33..=0.40).contains(&gap), "the front edge stopped {gap} m from the wall");
    assert_eq!(holder(&c, "base").as_deref(), Some("app:sennep"), "it still holds the base while asked");
    let l = window.limits();
    assert_eq!(l["blocked"]["forward"], true, "{l}");
    assert!(l["message"].as_str().unwrap().starts_with("Stopped: something 0.3"), "{l}");
    assert!(l["message"].as_str().unwrap().ends_with("m ahead."), "{l}");
    let (s, n) = call(&c.app, "GET", "/api/v0/signals/app:sennep/nearest-obstacle", None).await;
    assert_eq!(s, StatusCode::OK, "{n}");
    let nearest = n["reading"]["reading"]["meters"].as_f64().unwrap();
    assert!((nearest - f64::from(gap)).abs() < 0.05, "the nearest obstacle ahead: {nearest}, the gap {gap}");

    // Backing away is fine.
    window.hold(-1.0, 0.0, 0.0);
    wait_for("it to back away", 3.0, || (base(&c).commanded.vx < -0.2).then_some(())).await;
    window.let_go("base");
    wait_for("the base to stop and be let go", 2.0, || {
        (holder(&c, "base").is_none() && base(&c).commanded == Twist2d::ZERO).then_some(())
    })
    .await;
    nothing_saved(&c).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_apps_pose_follows_the_worlds_over_a_short_drive() {
    if component("sennep").is_none() {
        return;
    }
    // In the open, between the TV bench and the coffee table, facing north.
    let start = (7.2, 2.0, FRAC_PI_2);
    let c = sennep_with_the_app(start).await;
    let (x0, y0, yaw0) = world_pose(&c);
    // The app's pose starts at 0 where the app started; SENNEP hasn't moved since.
    let p = wait_for("the app's pose", 3.0, || pose_of(&c, "app:sennep/pose")).await;
    assert!(p.0.abs() < 1e-3 && p.1.abs() < 1e-3 && p.2.abs() < 1e-3, "{p:?}");
    let (s, v) = call(&c.app, "GET", "/api/v0/signals/app:sennep/pose", None).await;
    assert_eq!((s, v["capability"].as_str()), (StatusCode::OK, Some("pose@1")), "{v}");
    assert_eq!(v["reading"]["reading"]["frame"], "odom");

    // Ahead, then sideways while turning, then let go.
    let mut window = AppWindow::open(&c.app, "sennep").await;
    for (x, y, turn, ms) in [(1.0, 0.0, 0.0, 1500), (0.0, -1.0, 0.5, 1500)] {
        window.drive_via("base").await;
        window.hold(x, y, turn);
        tokio::time::sleep(Duration::from_millis(ms)).await;
        window.let_go("base");
    }
    wait_for("SENNEP to stand still", 3.0, || {
        base(&c).measured.is_some_and(|m| m == Twist2d::ZERO).then_some(())
    })
    .await;
    tokio::time::sleep(Duration::from_millis(150)).await;

    let (x, y, yaw) = world_pose(&c);
    let moved = (x - x0).hypot(y - y0);
    assert!(moved > 0.45, "it drove: {moved} m");
    // The world's pose, seen from where SENNEP started: what dead reckoning should say.
    let (dx, dy) = (x - x0, y - y0);
    let (s, co) = (-yaw0).sin_cos();
    let want = (dx * co - dy * s, dx * s + dy * co, yaw - yaw0);
    let got = pose_of(&c, "app:sennep/pose").unwrap();
    let off = (got.0 - want.0).hypot(got.1 - want.1);
    assert!(off < 0.05, "dead reckoning is {off} m off after {moved} m: {got:?}, want {want:?}");
    assert!((got.2 - want.2).abs() < 0.05, "its heading: {got:?}, want {want:?}");

    // Reset pose (the window's button): back to 0, here.
    window.send("reset-pose", "");
    wait_for("the pose to reset", 2.0, || {
        pose_of(&c, "app:sennep/pose").filter(|p| p.0.abs() < 1e-3 && p.1.abs() < 1e-3).map(|_| ())
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_critical_battery_caps_the_speed_to_30_percent() {
    if component("sennep").is_none() {
        return;
    }
    let c = sennep_with_the_app((8.0, 2.0, FRAC_PI_2)).await;
    // The battery role gets the manual pack, set below 3S Li-ion's critical 9.9 V.
    let low = json!({"pattern": {"kind": "steady", "value": 9.6}});
    assert_eq!(call(&c.app, "PUT", "/api/v0/parts/spare/simulation", Some(low)).await.0, StatusCode::OK);
    wait_for("a critical reading", 3.0, || match latest(&c, "spare/battery") {
        Some(Reading::Battery(b)) if b.level == faber_capabilities::battery::Level::Critical => Some(()),
        _ => None,
    })
    .await;
    let roles = json!({"roles": {"lidar": "lidar", "base": "base", "battery": "spare"}});
    let (s, a) = call(&c.app, "PUT", "/api/v0/apps/sennep/roles", Some(roles)).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    wait_running(&c).await;

    let mut window = drive(&c, 1.0, 0.0, 0.0).await;
    wait_for("it to drive, slowly", 3.0, || (base(&c).commanded.vx > 0.0).then_some(())).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let vx = base(&c).commanded.vx;
    assert!((vx - 0.09).abs() < 1e-3, "30 % of 0.3 m/s: {vx}");
    let l = window.limits();
    assert_eq!(l["cap"], 0.3, "{l}");
    assert!(l["message"].as_str().unwrap().contains("Battery critical: speed capped to 30 %."), "{l}");
    window.let_go("base");
}

/// What the shell's Stop bar, a page that hangs, a window that closes and the app being turned off
/// each do to SENNEP while a key is held.
#[tokio::test(flavor = "multi_thread")]
async fn stop_a_quiet_or_closed_window_or_turning_the_app_off_stops_the_base() {
    if component("sennep").is_none() {
        return;
    }
    let c = sennep_with_the_app((8.0, 2.0, FRAC_PI_2)).await;
    let drives = || async {
        wait_for("the app to drive", 3.0, || {
            (holder(&c, "base").as_deref() == Some("app:sennep") && base(&c).commanded.vx > 0.2).then_some(())
        })
        .await
    };
    let stops = |within: Duration| {
        let c = &c;
        async move {
            let from = Instant::now();
            wait_for("the base to stop", 2.0, || {
                (holder(c, "base").is_none() && base(c).commanded == Twist2d::ZERO).then_some(())
            })
            .await;
            assert!(from.elapsed() < within, "it took {:?}", from.elapsed());
        }
    };

    // The user presses the shell's Stop: the core ends the drive and the lease at once, though
    // the page keeps asking (its key is still down), and the window hears why.
    let window = drive(&c, 1.0, 0.0, 0.0).await;
    drives().await;
    window.press_stop();
    stops(Duration::from_millis(100)).await;
    let ended = window.wait_for("drive-ended", |m| m["type"] == "app-window-drive-ended").await;
    assert_eq!(ended["reason"], "stopped", "{ended}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(holder(&c, "base"), None, "the page's messages alone drive nothing");
    assert_eq!(base(&c).commanded, Twist2d::ZERO);
    // This page ignored it and keeps asking, so the logic tries, and the core refuses.
    let l = window.limits();
    assert!(l["message"].as_str().unwrap().contains("no window is driving"), "{l}");
    drop(window);

    // The page hangs while a key is held: no messages, no heartbeat.
    let mut window = drive(&c, 1.0, 0.0, 0.0).await;
    drives().await;
    window.hang();
    stops(Duration::from_millis(500)).await;
    let ended = window.wait_for("drive-ended", |m| m["type"] == "app-window-drive-ended").await;
    assert_eq!(ended["reason"], "heartbeat-lost", "{ended}");
    let said = window
        .wait_for("the logic to say why", |m| {
            m["kind"] == "limits"
                && m["data"]["text"].as_str().is_some_and(|t| t.contains("Stopped: the window stopped"))
        })
        .await;
    assert!(said["data"]["text"].as_str().unwrap().contains("\"driving\":false"), "{said}");
    drop(window);

    // The window closes while a key is held: at once.
    let window = drive(&c, 1.0, 0.0, 0.0).await;
    drives().await;
    drop(window);
    stops(Duration::from_millis(250)).await;

    // Turned off while driving: the lease ends at once.
    let window = drive(&c, 0.0, 1.0, 0.0).await;
    wait_for("the app to drive", 3.0, || (base(&c).commanded.vy > 0.2).then_some(())).await;
    let (s, a) = call(&c.app, "POST", "/api/v0/apps/sennep/disable", None).await;
    assert_eq!((s, a["state"]["kind"].as_str()), (StatusCode::OK, Some("stopped")), "{a}");
    assert_eq!(holder(&c, "base"), None, "turning it off ended its lease");
    assert_eq!(base(&c).commanded, Twist2d::ZERO, "and the base got its safe state");
    // The window keeps asking, but an app that is off drives nothing.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(holder(&c, "base"), None);
    drop(window);
    nothing_saved(&c).await;
}
