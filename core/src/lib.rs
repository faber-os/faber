//! The Faber core: the single process that owns the machine's hardware, runs its parts and
//! serves the shell. Module layers and rules: docs/development.md.

// Layer 1: small pure building blocks.
pub mod clock;
pub mod names;

// Layer 2: one job each; none of them knows about HTTP.
pub mod backup;
pub mod config;
pub mod connectors;
pub mod disks;
pub mod files;
pub mod host;
pub mod logs;
pub mod ports;
pub mod router;
pub mod runtime;
pub mod settings;
pub mod sim;
pub mod suggest;

// Layer 3: ties everything together.
pub mod machine;

// Layer 4: the wire.
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
}

impl Core {
    /// Starts the machine on real hardware: OS serial ports, plus every recording in the home
    /// folder's Recordings.
    pub fn start(
        data_dir: PathBuf,
        logs: Arc<logs::LogBook>,
        clock: clock::SharedClock,
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
            replays,
            connectors: connectors::built_in(),
            timing: runtime::Timing::default(),
            port_scan: Some(Duration::from_secs(2)),
        })?;
        Ok(Core { machine, logs })
    }

    pub fn router(&self) -> axum::Router {
        api::router(api::AppState { machine: Arc::clone(&self.machine), logs: Arc::clone(&self.logs) })
    }

    /// Serves the API on `listener` until `shutdown` resolves, then stops every part.
    pub async fn serve(
        self,
        listener: tokio::net::TcpListener,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> std::io::Result<()> {
        tracing::info!(target: "core", "Faber core listening on http://{}", listener.local_addr()?);
        axum::serve(listener, self.router()).with_graceful_shutdown(shutdown).await?;
        let machine = Arc::clone(&self.machine);
        let _ = tokio::task::spawn_blocking(move || machine.shutdown()).await;
        Ok(())
    }

    /// Binds `addr`; a separate step so callers can report "address in use" clearly.
    pub async fn bind(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
        tokio::net::TcpListener::bind(addr).await
    }
}
