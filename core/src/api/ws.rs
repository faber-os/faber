//! The live WebSocket (docs/api.md): subscriptions to signals, parts, ports and logs, plus the
//! serial monitor. One writer task owns the socket; every subscription is its own task that
//! feeds it, so a slow topic never holds up another.

use super::{AppState, convert, dto};
use crate::apps::{Payload, ToWindow};
use crate::machine::{AppWindowLink, LeaseEvent, SerialSession};
use crate::names::SignalAddress;
use crate::ports::PortEvent;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Msgpack,
    Json,
}

#[derive(serde::Deserialize)]
pub struct Params {
    #[serde(default)]
    format: Format,
}

pub async fn upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(p): Query<Params>,
) -> Response {
    ws.on_upgrade(move |socket| session(socket, state, p.format))
}

/// How many messages may wait for a slow client before subscriptions pause.
const OUTBOX: usize = 64;

/// The `world` topic's rate: 20 times a second (docs/simulation.md, section 7).
const WORLD_EVERY: Duration = Duration::from_millis(50);

fn encode(msg: &dto::ServerMessage, format: Format) -> Message {
    match format {
        Format::Msgpack => {
            Message::Binary(rmp_serde::to_vec_named(msg).expect("server messages serialise").into())
        }
        Format::Json => Message::Text(serde_json::to_string(msg).expect("server messages serialise").into()),
    }
}

fn decode(msg: &Message) -> Option<Result<dto::ClientMessage, String>> {
    match msg {
        Message::Binary(b) => Some(rmp_serde::from_slice(b).map_err(|e| e.to_string())),
        Message::Text(t) => Some(serde_json::from_str(t).map_err(|e| e.to_string())),
        _ => None,
    }
}

fn error(
    id: Option<u32>,
    code: &str,
    message: impl Into<String>,
    hint: impl Into<String>,
) -> dto::ServerMessage {
    dto::ServerMessage::Error {
        id,
        error: dto::ErrorBody { code: code.into(), message: message.into(), hint: hint.into(), reason: None },
    }
}

async fn session(socket: WebSocket, state: AppState, format: Format) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<dto::ServerMessage>(OUTBOX);
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(encode(&msg, format)).await.is_err() {
                break;
            }
        }
    });
    let mut subs: HashMap<u32, JoinHandle<()>> = HashMap::new();
    let mut serial: HashMap<u32, Arc<SerialSession>> = HashMap::new();
    let mut windows = Windows::new();
    while let Some(Ok(msg)) = stream.next().await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
        let Some(decoded) = decode(&msg) else { continue };
        let msg = match decoded {
            Ok(m) => m,
            Err(e) => {
                let _ = out_tx
                    .send(error(
                        None,
                        "bad-message",
                        format!("Could not read that message: {e}"),
                        "Check it against docs/api.md.",
                    ))
                    .await;
                continue;
            }
        };
        if let dto::ClientMessage::AppWindowOpen { .. }
        | dto::ClientMessage::AppWindowSend { .. }
        | dto::ClientMessage::AppWindowHeartbeat { .. }
        | dto::ClientMessage::AppWindowDrive { .. }
        | dto::ClientMessage::AppWindowRelease { .. }
        | dto::ClientMessage::AppWindowStop { .. }
        | dto::ClientMessage::AppWindowClose { .. } = &msg
        {
            let in_use = |id: &u32| subs.contains_key(id) || serial.contains_key(id);
            windows.handle(msg, &state, &out_tx, in_use).await;
            continue;
        }
        if let dto::ClientMessage::Subscribe { id, .. } | dto::ClientMessage::SerialOpen { id, .. } = &msg
            && windows.has(*id)
        {
            let _ = out_tx.send(id_in_use(*id)).await;
            continue;
        }
        handle(msg, &state, &out_tx, &mut subs, &mut serial).await;
    }
    for (_, task) in subs.drain() {
        task.abort();
    }
    // The page's windows are gone with the connection: what they drove ends now.
    windows.close_all().await;
    let sessions: Vec<_> = serial.drain().map(|(_, s)| s).collect();
    let _ = tokio::task::spawn_blocking(move || drop(sessions)).await;
    drop(out_tx);
    let _ = writer.await;
}

async fn handle(
    msg: dto::ClientMessage,
    state: &AppState,
    out: &mpsc::Sender<dto::ServerMessage>,
    subs: &mut HashMap<u32, JoinHandle<()>>,
    serial: &mut HashMap<u32, Arc<SerialSession>>,
) {
    use dto::ClientMessage as C;
    match msg {
        C::Subscribe { id, topic } => {
            if subs.contains_key(&id) || serial.contains_key(&id) {
                let _ = out
                    .send(error(
                        Some(id),
                        "id-in-use",
                        format!("Id {id} is already in use."),
                        "Pick a new id for each subscription.",
                    ))
                    .await;
                return;
            }
            match subscribe(id, topic, state, out.clone()) {
                Ok(task) => {
                    subs.insert(id, task);
                }
                Err(e) => {
                    let _ = out.send(*e).await;
                }
            }
        }
        C::Unsubscribe { id } => {
            if let Some(task) = subs.remove(&id) {
                task.abort();
            }
        }
        C::SerialOpen { id, port, baud } => {
            if subs.contains_key(&id) || serial.contains_key(&id) {
                let _ = out
                    .send(error(
                        Some(id),
                        "id-in-use",
                        format!("Id {id} is already in use."),
                        "Pick a new id for each session.",
                    ))
                    .await;
                return;
            }
            let machine = Arc::clone(&state.machine);
            let tx = out.clone();
            let sink = move |e: PortEvent| match e {
                PortEvent::Data { t_us, bytes } => {
                    tx.blocking_send(dto::ServerMessage::SerialData { id, t_us, data: bytes }).is_ok()
                }
                PortEvent::Closed { reason } => {
                    let _ = tx.blocking_send(dto::ServerMessage::SerialClosed { id, reason: Some(reason) });
                    false
                }
            };
            match tokio::task::spawn_blocking(move || machine.open_serial(&port, baud)).await {
                Ok(Ok(mut session)) => {
                    // Announce first, then start reading: data never arrives before `serial-opened`.
                    let _ = out.send(dto::ServerMessage::SerialOpened { id, port: session.port() }).await;
                    session.start(sink);
                    serial.insert(id, Arc::new(session));
                }
                Ok(Err(e)) => {
                    let _ = out
                        .send(dto::ServerMessage::Error { id: Some(id), error: convert::error_body(&e) })
                        .await;
                }
                Err(_) => {
                    let _ = out
                        .send(error(
                            Some(id),
                            "internal",
                            "Opening the port failed inside the core.",
                            "This is a bug in Faber; the log has details.",
                        ))
                        .await;
                }
            }
        }
        C::SerialSend { id, data } => match serial.get(&id) {
            Some(s) => {
                if let Err(e) = s.write(&data) {
                    let _ = out
                        .send(error(
                            Some(id),
                            "serial-write-failed",
                            format!("Could not send: {e}"),
                            "Check that the port is still connected.",
                        ))
                        .await;
                }
            }
            None => {
                let _ = out
                    .send(error(
                        Some(id),
                        "unknown-session",
                        format!("No serial session {id} is open."),
                        "Open the port first.",
                    ))
                    .await;
            }
        },
        C::SerialClose { id } => {
            if let Some(s) = serial.remove(&id) {
                let machine = Arc::clone(&state.machine);
                // A recording in progress ends with the session; tell the client what was saved.
                let stopped = tokio::task::spawn_blocking(move || {
                    let stopped = machine.stop_recording(&s).ok();
                    drop(s);
                    stopped
                })
                .await
                .ok()
                .flatten();
                if let Some((path, bytes)) = stopped {
                    let _ = out.send(dto::ServerMessage::SerialRecordingStopped { id, path, bytes }).await;
                }
                let _ = out.send(dto::ServerMessage::SerialClosed { id, reason: None }).await;
            }
        }
        C::SerialRecordStart { id, name } => {
            let Some(session) = serial.get(&id).cloned() else {
                let _ = out.send(unknown_session(id)).await;
                return;
            };
            let machine = Arc::clone(&state.machine);
            let reply =
                match tokio::task::spawn_blocking(move || machine.start_recording(&session, &name)).await {
                    Ok(Ok(path)) => dto::ServerMessage::SerialRecording { id, path },
                    Ok(Err(e)) => dto::ServerMessage::Error { id: Some(id), error: convert::error_body(&e) },
                    Err(_) => internal(id),
                };
            let _ = out.send(reply).await;
        }
        // The session loop hands these to `Windows`.
        C::AppWindowOpen { .. }
        | C::AppWindowSend { .. }
        | C::AppWindowHeartbeat { .. }
        | C::AppWindowDrive { .. }
        | C::AppWindowRelease { .. }
        | C::AppWindowStop { .. }
        | C::AppWindowClose { .. } => {}
        C::SerialRecordStop { id } => {
            let Some(session) = serial.get(&id).cloned() else {
                let _ = out.send(unknown_session(id)).await;
                return;
            };
            let machine = Arc::clone(&state.machine);
            let reply = match tokio::task::spawn_blocking(move || machine.stop_recording(&session)).await {
                Ok(Ok((path, bytes))) => dto::ServerMessage::SerialRecordingStopped { id, path, bytes },
                Ok(Err(e)) => dto::ServerMessage::Error { id: Some(id), error: convert::error_body(&e) },
                Err(_) => internal(id),
            };
            let _ = out.send(reply).await;
        }
    }
}

fn id_in_use(id: u32) -> dto::ServerMessage {
    error(Some(id), "id-in-use", format!("Id {id} is already in use."), "Pick a new id for each session.")
}

fn payload_from(data: dto::AppMessageData) -> Payload {
    match data {
        dto::AppMessageData::Text(t) => Payload::Text(t),
        dto::AppMessageData::Bytes(b) => Payload::Bytes(b),
    }
}

fn data_of(payload: Payload) -> dto::AppMessageData {
    match payload {
        Payload::Text(t) => dto::AppMessageData::Text(t),
        Payload::Bytes(b) => dto::AppMessageData::Bytes(b),
    }
}

/// One connection's app window links (docs/apps.md §12), by the client's id. Each has a task
/// that forwards what the core tells the window.
struct Windows {
    open: HashMap<u32, (Arc<AppWindowLink>, JoinHandle<()>)>,
}

impl Windows {
    fn new() -> Windows {
        Windows { open: HashMap::new() }
    }

    fn has(&self, id: u32) -> bool {
        self.open.contains_key(&id)
    }

    async fn handle(
        &mut self,
        msg: dto::ClientMessage,
        state: &AppState,
        out: &mpsc::Sender<dto::ServerMessage>,
        in_use: impl Fn(&u32) -> bool,
    ) {
        use dto::ClientMessage as C;
        let reply = match msg {
            C::AppWindowOpen { id, app } => {
                if in_use(&id) || self.has(id) {
                    Some(id_in_use(id))
                } else {
                    self.open(id, app, state, out).await
                }
            }
            C::AppWindowSend { id, kind, data } => match self.link(id) {
                Ok(link) => link.send(&kind, payload_from(data)).err().map(|e| err(id, &e)),
                Err(e) => Some(*e),
            },
            C::AppWindowHeartbeat { id } => match self.link(id) {
                Ok(link) => link.heartbeat().err().map(|e| err(id, &e)),
                Err(e) => Some(*e),
            },
            C::AppWindowDrive { id, role, index } => match self.link(id) {
                Ok(link) => {
                    let r = role.clone();
                    let answer = tokio::task::spawn_blocking(move || link.drive_via(&r, index)).await;
                    Some(match answer {
                        Ok(Ok(())) => dto::ServerMessage::AppWindowDriving { id, role, index },
                        Ok(Err(e)) => dto::ServerMessage::AppWindowDriveRefused {
                            id,
                            role,
                            index,
                            error: convert::error_body(&e),
                        },
                        Err(_) => internal(id),
                    })
                }
                Err(e) => Some(*e),
            },
            C::AppWindowRelease { id, role, index } => match self.link(id) {
                Ok(link) => {
                    let _ = tokio::task::spawn_blocking(move || link.end_drive(&role, index)).await;
                    None
                }
                Err(e) => Some(*e),
            },
            C::AppWindowStop { id } => match self.link(id) {
                Ok(link) => {
                    let _ = tokio::task::spawn_blocking(move || link.stop()).await;
                    None
                }
                Err(e) => Some(*e),
            },
            C::AppWindowClose { id } => {
                if let Some((link, task)) = self.open.remove(&id) {
                    task.abort();
                    let _ = tokio::task::spawn_blocking(move || drop(link)).await;
                }
                None
            }
            _ => None,
        };
        if let Some(r) = reply {
            let _ = out.send(r).await;
        }
    }

    async fn open(
        &mut self,
        id: u32,
        app: String,
        state: &AppState,
        out: &mpsc::Sender<dto::ServerMessage>,
    ) -> Option<dto::ServerMessage> {
        // The core's side never waits: what it tells the window queues here (it is rate-limited),
        // and a task hands it to the connection's writer.
        let (tx, mut rx) = mpsc::unbounded_channel::<ToWindow>();
        let sink: crate::apps::channel::Sink = Arc::new(move |m| {
            let _ = tx.send(m);
        });
        let machine = Arc::clone(&state.machine);
        let link = match tokio::task::spawn_blocking(move || machine.open_app_window(&app, sink)).await {
            Ok(Ok(link)) => Arc::new(link),
            Ok(Err(e)) => return Some(err(id, &e)),
            Err(_) => return Some(internal(id)),
        };
        let window = link.id();
        // Opened is said before anything the core tells the window.
        let _ = out.send(dto::ServerMessage::AppWindowOpened { id, window }).await;
        let out = out.clone();
        let task = tokio::spawn(async move {
            while let Some(m) = rx.recv().await {
                let msg = match m {
                    ToWindow::Message { kind, payload } => {
                        dto::ServerMessage::AppWindowMessage { id, kind, data: data_of(payload) }
                    }
                    ToWindow::DriveEnded { role, index, reason } => dto::ServerMessage::AppWindowDriveEnded {
                        id,
                        role,
                        index,
                        reason: reason.id().into(),
                    },
                    ToWindow::Closed => {
                        dto::ServerMessage::AppWindowClosed { id, reason: "app-removed".into() }
                    }
                };
                if out.send(msg).await.is_err() {
                    return;
                }
            }
        });
        self.open.insert(id, (link, task));
        None
    }

    fn link(&self, id: u32) -> Result<Arc<AppWindowLink>, Box<dto::ServerMessage>> {
        self.open.get(&id).map(|(l, _)| Arc::clone(l)).ok_or_else(|| {
            Box::new(error(
                Some(id),
                "unknown-window",
                format!("No app window link {id} is open."),
                "Open it first with app-window-open.",
            ))
        })
    }

    async fn close_all(&mut self) {
        let links: Vec<_> = self
            .open
            .drain()
            .map(|(_, (link, task))| {
                task.abort();
                link
            })
            .collect();
        let _ = tokio::task::spawn_blocking(move || drop(links)).await;
    }
}

fn err(id: u32, e: &crate::machine::MachineError) -> dto::ServerMessage {
    dto::ServerMessage::Error { id: Some(id), error: convert::error_body(e) }
}

fn unknown_session(id: u32) -> dto::ServerMessage {
    error(Some(id), "unknown-session", format!("No serial session {id} is open."), "Open the port first.")
}

fn internal(id: u32) -> dto::ServerMessage {
    error(
        Some(id),
        "internal",
        "The core failed while handling this.",
        "This is a bug in Faber; the log has details.",
    )
}

fn subscribe(
    id: u32,
    topic: dto::Topic,
    state: &AppState,
    out: mpsc::Sender<dto::ServerMessage>,
) -> Result<JoinHandle<()>, Box<dto::ServerMessage>> {
    match topic {
        dto::Topic::Signal { address, max_hz } => {
            let unknown = || {
                error(
                    Some(id),
                    "unknown-signal",
                    format!("There is no signal \"{address}\" right now."),
                    "Signals look like part/name, e.g. lidar/laser-scan. The parts list shows each part's signals.",
                )
            };
            let addr = SignalAddress::parse(&address).ok_or_else(|| Box::new(unknown()))?;
            let mut sub = state.machine.router().subscribe(&addr).ok_or_else(|| Box::new(unknown()))?;
            let min_gap = max_hz.filter(|hz| *hz > 0.0).map(|hz| Duration::from_secs_f32(1.0 / hz));
            Ok(tokio::spawn(async move {
                while let Some(reading) = sub.next().await {
                    let msg = dto::ServerMessage::Signal {
                        id,
                        address: address.clone(),
                        reading: (*reading).clone(),
                    };
                    if out.send(msg).await.is_err() {
                        return;
                    }
                    if let Some(gap) = min_gap {
                        tokio::time::sleep(gap).await;
                    }
                }
                let _ = out
                    .send(error(
                        Some(id),
                        "signal-ended",
                        format!("The signal \"{address}\" ended because its part was removed or restarted."),
                        "Subscribe again when the part is back in the parts list.",
                    ))
                    .await;
            }))
        }
        dto::Topic::Parts => {
            let machine = Arc::clone(&state.machine);
            let mut changes = machine.parts_changed();
            Ok(tokio::spawn(async move {
                loop {
                    let parts = machine.parts().iter().map(convert::part).collect();
                    if out.send(dto::ServerMessage::Parts { id, parts }).await.is_err()
                        || changes.changed().await.is_err()
                    {
                        return;
                    }
                }
            }))
        }
        dto::Topic::Ports => {
            let machine = Arc::clone(&state.machine);
            let mut changes = machine.ports_changed();
            Ok(tokio::spawn(async move {
                loop {
                    let ports = machine.ports().iter().map(convert::port).collect();
                    let problems = machine.port_problems().iter().map(convert::port_problem).collect();
                    if out.send(dto::ServerMessage::Ports { id, ports, problems }).await.is_err()
                        || changes.changed().await.is_err()
                    {
                        return;
                    }
                }
            }))
        }
        dto::Topic::Settings => {
            let machine = Arc::clone(&state.machine);
            let mut changes = machine.settings_changed();
            Ok(tokio::spawn(async move {
                loop {
                    let settings = machine.settings();
                    if out.send(dto::ServerMessage::Settings { id, settings }).await.is_err()
                        || changes.changed().await.is_err()
                    {
                        return;
                    }
                }
            }))
        }
        dto::Topic::Packages => {
            let machine = Arc::clone(&state.machine);
            let mut changes = machine.packages_changed();
            Ok(tokio::spawn(async move {
                loop {
                    let msg = match machine.packages() {
                        Ok(list) => dto::ServerMessage::Packages {
                            id,
                            packages: list.iter().map(convert::package).collect(),
                        },
                        // packages.toml can't be read: say why (as GET does) and wait for a change.
                        Err(e) => error(Some(id), e.code, e.message, e.hint),
                    };
                    if out.send(msg).await.is_err() || changes.changed().await.is_err() {
                        return;
                    }
                }
            }))
        }
        dto::Topic::Apps => {
            let machine = Arc::clone(&state.machine);
            let mut changes = machine.apps_changed();
            Ok(tokio::spawn(async move {
                loop {
                    let list = {
                        let m = Arc::clone(&machine);
                        tokio::task::spawn_blocking(move || m.apps()).await.unwrap_or_default()
                    };
                    let msg =
                        dto::ServerMessage::Apps { id, apps: list.iter().map(convert::app_info).collect() };
                    if out.send(msg).await.is_err() || changes.changed().await.is_err() {
                        return;
                    }
                }
            }))
        }
        dto::Topic::Files => {
            let mut rx = state.machine.files_changed();
            Ok(tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(folders) => {
                            if out.send(dto::ServerMessage::FilesChanged { id, folders }).await.is_err() {
                                return;
                            }
                        }
                        // Missed some: say everything changed, so the client refreshes what it shows.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            let all = vec!["home:/".to_string()];
                            if out.send(dto::ServerMessage::FilesChanged { id, folders: all }).await.is_err()
                            {
                                return;
                            }
                        }
                        Err(_) => return,
                    }
                }
            }))
        }
        dto::Topic::Leases => {
            let machine = Arc::clone(&state.machine);
            // Subscribe before reading the list, so nothing falls between the two.
            let mut rx = machine.lease_events();
            Ok(tokio::spawn(async move {
                let leases = machine.leases().iter().map(convert::lease_info).collect();
                if out.send(dto::ServerMessage::Leases { id, leases }).await.is_err() {
                    return;
                }
                loop {
                    let msg = match rx.recv().await {
                        Ok(LeaseEvent::Granted(l)) => {
                            dto::ServerMessage::LeaseGranted { id, lease: convert::lease_info(&l) }
                        }
                        Ok(LeaseEvent::Ended(e)) => dto::ServerMessage::LeaseEnded {
                            id,
                            lease: convert::lease_info(&e.lease),
                            reason: convert::lease_end_reason(e.reason),
                        },
                        // Missed some: send the whole list again.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            dto::ServerMessage::Leases {
                                id,
                                leases: machine.leases().iter().map(convert::lease_info).collect(),
                            }
                        }
                        Err(_) => return,
                    };
                    if out.send(msg).await.is_err() {
                        return;
                    }
                }
            }))
        }
        dto::Topic::Estop => {
            let machine = Arc::clone(&state.machine);
            let mut rx = machine.estop_events();
            Ok(tokio::spawn(async move {
                let mut status = convert::estop_status(&machine.estop());
                status.history.clear();
                if out.send(dto::ServerMessage::Estop { id, status }).await.is_err() {
                    return;
                }
                loop {
                    let status = match rx.recv().await {
                        Ok(change) => dto::EstopStatus {
                            engaged: change.engaged,
                            engaged_by: change.engaged.then_some(change),
                            history: Vec::new(),
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            let mut s = convert::estop_status(&machine.estop());
                            s.history.clear();
                            s
                        }
                        Err(_) => return,
                    };
                    if out.send(dto::ServerMessage::Estop { id, status }).await.is_err() {
                        return;
                    }
                }
            }))
        }
        dto::Topic::World => {
            let machine = Arc::clone(&state.machine);
            let mut changes = machine.world_events();
            Ok(tokio::spawn(async move {
                let mut every = tokio::time::interval(WORLD_EVERY);
                every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    let msg = tokio::select! {
                        _ = every.tick() => convert::world_message(id, machine.world_state()),
                        changed = changes.recv() => match changed {
                            Ok(c) => dto::ServerMessage::WorldChanged { id, name: c.name },
                            // Missed some switches: say which world it is now.
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                let m = Arc::clone(&machine);
                                let Ok(view) = tokio::task::spawn_blocking(move || m.world()).await else {
                                    return;
                                };
                                dto::ServerMessage::WorldChanged { id, name: view.world.name }
                            }
                            Err(_) => return,
                        },
                    };
                    if out.send(msg).await.is_err() {
                        return;
                    }
                }
            }))
        }
        dto::Topic::AppState { app } => {
            let (values, mut rx) = state
                .machine
                .watch_app_state(&app)
                .map_err(|e| Box::new(error(Some(id), e.code, e.message, e.hint)))?;
            let machine = Arc::clone(&state.machine);
            Ok(tokio::spawn(async move {
                if out.send(dto::ServerMessage::AppState { id, app: app.clone(), values }).await.is_err() {
                    return;
                }
                loop {
                    let msg = match rx.recv().await {
                        Ok(c) => dto::ServerMessage::AppStateChanged {
                            id,
                            app: app.clone(),
                            key: c.key,
                            value: c.value,
                        },
                        // Missed some changes: send everything again.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            let (m, a) = (Arc::clone(&machine), app.clone());
                            match tokio::task::spawn_blocking(move || m.app_state(&a)).await {
                                Ok(Ok(values)) => {
                                    dto::ServerMessage::AppState { id, app: app.clone(), values }
                                }
                                _ => return,
                            }
                        }
                        Err(_) => return,
                    };
                    if out.send(msg).await.is_err() {
                        return;
                    }
                }
            }))
        }
        dto::Topic::Notices => {
            let mut rx = state.machine.notices();
            Ok(tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(n) => {
                            let msg = dto::ServerMessage::Notice { id, notice: convert::notice(&n) };
                            if out.send(msg).await.is_err() {
                                return;
                            }
                        }
                        // Fell behind: those were logged as well, and the next one comes.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => return,
                    }
                }
            }))
        }
        dto::Topic::Flash => {
            let mut rx = state.machine.flash_events();
            Ok(tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(e) => {
                            let msg = dto::ServerMessage::Flash {
                                id,
                                port: e.port,
                                stage: convert::flash_stage(e.progress.stage),
                                done: e.progress.done,
                                total: e.progress.total,
                            };
                            if out.send(msg).await.is_err() {
                                return;
                            }
                        }
                        // Missed some pages: the next one says where it is.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => return,
                    }
                }
            }))
        }
        dto::Topic::Logs => {
            let (recent, mut rx) = state.logs.follow();
            Ok(tokio::spawn(async move {
                for e in &recent {
                    if out.send(dto::ServerMessage::Log { id, entry: convert::log_entry(e) }).await.is_err() {
                        return;
                    }
                }
                loop {
                    match rx.recv().await {
                        Ok(e) => {
                            if out
                                .send(dto::ServerMessage::Log { id, entry: convert::log_entry(&e) })
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => return,
                    }
                }
            }))
        }
    }
}
