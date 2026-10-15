//! `faber-core`: starts the machine and serves the shell (docs/development.md, "Running it").

use clap::Parser;
use faber_core::Core;
use faber_core::clock::{MonotonicClock, SharedClock};
use faber_core::logs::LogBook;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

#[derive(Parser)]
#[command(name = "faber-core", version, about = "The Faber core: runs your parts and serves the shell.")]
struct Args {
    /// Where Faber keeps its config and captures.
    #[arg(long, default_value = "./data")]
    data: PathBuf,
    /// Address to listen on. Phase 1 only allows this machine (127.0.0.1 or ::1).
    #[arg(long, default_value = "127.0.0.1:7878")]
    listen: SocketAddr,
}

fn main() -> ExitCode {
    let args = Args::parse();
    if !args.listen.ip().is_loopback() {
        eprintln!(
            "faber-core: {} is reachable from other computers, but Faber has no login yet.\n\
             Use 127.0.0.1 for now. Remote access arrives together with login (Phase 5).",
            args.listen
        );
        return ExitCode::FAILURE;
    }
    let clock: SharedClock = Arc::new(MonotonicClock::new());
    let logs = LogBook::new(clock.clone());
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())),
        )
        .with(logs.layer().with_filter(EnvFilter::new("info")))
        .init();

    let runtime = tokio::runtime::Runtime::new().expect("start the async runtime");
    runtime.block_on(async move {
        let listener = match Core::bind(args.listen).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!(
                    "faber-core: could not listen on {}: {e}. Is another faber-core already running?",
                    args.listen
                );
                return ExitCode::FAILURE;
            }
        };
        let core = match Core::start(args.data.clone(), logs, clock) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("faber-core: could not use the data folder {}: {e}", args.data.display());
                return ExitCode::FAILURE;
            }
        };
        let shutdown = async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!(target: "core", "stopping");
        };
        match core.serve(listener, shutdown).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("faber-core: the server stopped with an error: {e}");
                ExitCode::FAILURE
            }
        }
    })
}
