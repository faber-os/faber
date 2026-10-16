//! The Faber core: the single process that owns the machine's hardware, runs its parts and
//! serves the shell. Module layers and rules: docs/development.md.

// Layer 1: small pure building blocks.
pub mod clock;
pub mod names;
pub mod watched;

// Layer 2: one job each; none of them knows about HTTP.
pub mod arduino;
pub mod backup;
pub mod config;
pub mod connectors;
pub mod disks;
pub mod estop;
pub mod files;
pub mod host;
pub mod leases;
pub mod logs;
pub mod notices;
pub mod packages;
pub mod ports;
pub mod roles;
pub mod router;
pub mod sandbox;
pub mod sessions;

pub mod runtime;
pub mod settings;
pub mod sim;
pub mod simulation;
pub mod suggest;
pub mod translations;
pub mod users;

// Layer 3: runs parts (`runtime` and `suggest` too; see docs/development.md).
pub mod apps;
pub mod bridge;
pub mod declarative;
pub mod simparts;
pub mod wasm;

// Layer 4: ties everything together.
pub mod machine;

// Layer 5: the wire.
pub mod api;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Everything needed to run the core, in one place, so `main` and the integration tests start
/// it the same way.
pub struct Core {
    pub machine: Arc<machine::Machine>,
    pub logs: Arc<logs::LogBook>,
    /// Whether requests must say who sent them (`--no-auth` turns it off; docs/auth.md).
    pub auth: api::auth::AuthMode,
}

impl Core {
    /// Starts the machine on real hardware: OS serial ports, the machine's own I²C, SPI and GPIO
    /// (Linux only), plus every recording in the home folder's Recordings.
    pub fn start(
        data_dir: PathBuf,
        logs: Arc<logs::LogBook>,
        clock: clock::SharedClock,
    ) -> std::io::Result<Core> {
        Core::start_with_buses(data_dir, logs, clock, ports::system_hardware())
    }

    /// The same, with these I²C, SPI and GPIO buses instead of the machine's own
    /// (`--fake-hardware` passes [`ports::fake::FakeHardware::standard`]).
    pub fn start_with_buses(
        data_dir: PathBuf,
        logs: Arc<logs::LogBook>,
        clock: clock::SharedClock,
        hardware: Arc<dyn ports::Hardware>,
    ) -> std::io::Result<Core> {
        // Replay ports come from home:/Recordings (D35); the machine keeps this in sync.
        let replays = ports::ReplayLibrary::default();
        let machine = machine::Machine::start(machine::MachineOptions {
            data_dir,
            clock,
            discovery: Arc::new(ports::SystemDiscovery),
            usb_problems: Arc::new(ports::SystemUsbProblems),
            disks: Arc::new(disks::SystemDisks),
            opener: Arc::new(ports::SystemOpener::new(replays.clone())),
            hardware,
            replays,
            connectors: connectors::built_in(),
            timing: runtime::Timing::default(),
            port_scan: Some(Duration::from_secs(2)),
            world: Arc::new(simparts::SimWorld::new()),
        })?;
        Ok(Core { machine, logs, auth: api::auth::AuthMode::Required })
    }

    pub fn router(&self) -> axum::Router {
        api::router(api::AppState {
            machine: Arc::clone(&self.machine),
            logs: Arc::clone(&self.logs),
            auth: self.auth,
        })
    }

    /// Serves the API on `listener` until `shutdown` resolves, then stops every part.
    pub async fn serve(
        self,
        listener: tokio::net::TcpListener,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> std::io::Result<()> {
        self.serve_with_screen(listener, None, shutdown).await
    }

    /// Like [`Core::serve`], plus the attached screen's own listener (`--screen-listen`): requests
    /// on it from this machine are signed in as the screen's role (docs/auth.md).
    pub async fn serve_with_screen(
        self,
        listener: tokio::net::TcpListener,
        screen: Option<tokio::net::TcpListener>,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> std::io::Result<()> {
        use std::net::SocketAddr;
        tracing::info!(target: "core", "Faber core listening on http://{}", listener.local_addr()?);
        if self.auth == api::auth::AuthMode::Off {
            tracing::warn!(target: "core", "sign-in is off (--no-auth): everyone who can reach this core is an admin");
        }
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let stopped = |mut rx: tokio::sync::watch::Receiver<bool>| async move {
            let _ = rx.wait_for(|s| *s).await;
        };
        let screen_task = match screen {
            Some(l) => {
                tracing::info!(target: "core", "the attached screen's listener is on http://{}", l.local_addr()?);
                let app = self.router().layer(axum::Extension(api::auth::OnScreen));
                let until = stopped(stop_rx.clone());
                Some(tokio::spawn(async move {
                    axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
                        .with_graceful_shutdown(until)
                        .await
                }))
            }
            None => None,
        };
        tokio::spawn(async move {
            shutdown.await;
            let _ = stop_tx.send(true);
        });
        axum::serve(listener, self.router().into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(stopped(stop_rx))
            .await?;
        if let Some(t) = screen_task {
            let _ = t.await;
        }
        let machine = Arc::clone(&self.machine);
        let _ = tokio::task::spawn_blocking(move || machine.shutdown()).await;
        Ok(())
    }

    /// Binds `addr`; a separate step so callers can report "address in use" clearly.
    pub async fn bind(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
        tokio::net::TcpListener::bind(addr).await
    }
}

/// Every connector this core offers: the built-in ones, then those installed in
/// `<data>/connectors/<id>/` (each folder read by the engine for its kind). An installed one can't
/// replace a built-in one.
pub fn all_connectors(data_dir: &std::path::Path) -> Vec<faber_connector::Definition> {
    all_connectors_with_problems(data_dir).0
}

/// [`all_connectors`], plus the installed folders that couldn't be used and why, for
/// `GET /api/v0/connectors/problems`. Never fails: a bad folder is logged and left out.
pub fn all_connectors_with_problems(
    data_dir: &std::path::Path,
) -> (Vec<faber_connector::Definition>, Vec<connectors::LoadProblem>) {
    machine::installed_connectors(connectors::built_in(), &data_dir.join("connectors"))
}
