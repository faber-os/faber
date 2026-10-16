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
    /// Address to listen on. 0.0.0.0:7878 lets other computers sign in too.
    #[arg(long, default_value = "127.0.0.1:7878")]
    listen: SocketAddr,
    /// The attached screen's own listener (the kiosk browser opens it). Requests on it from this
    /// machine are signed in as the screen's role (Settings → This machine). Loopback only.
    #[arg(long)]
    screen_listen: Option<SocketAddr>,
    /// Development only: no sign-in, everyone is an admin. Refused unless --listen is loopback.
    #[arg(long)]
    no_auth: bool,
    /// Use a stand-in for arduino-cli that compiles any sketch to a made-up program (the
    /// end-to-end tests; docs/arduino-editor.md). Never for real boards.
    #[arg(long)]
    fake_arduino_cli: bool,
    /// For development and tests: simulated I²C, SPI and GPIO buses with simulated parts on them
    /// (a TMP102 at 0x48 on i2c-1), instead of the machine's own. Never touches real hardware.
    #[arg(long)]
    fake_hardware: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.no_auth && !args.listen.ip().is_loopback() {
        eprintln!(
            "faber-core: --no-auth makes everyone an admin, so it only works on this computer.\n\
             Use --listen 127.0.0.1:7878 with it, or leave --no-auth out and sign in."
        );
        return ExitCode::FAILURE;
    }
    if let Some(screen) = args.screen_listen
        && !screen.ip().is_loopback()
    {
        eprintln!(
            "faber-core: --screen-listen {screen} would let other computers act as the attached screen.\n\
             Use a loopback address, e.g. 127.0.0.1:7879."
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
        let screen = match args.screen_listen {
            Some(addr) => match Core::bind(addr).await {
                Ok(l) => Some(l),
                Err(e) => {
                    eprintln!("faber-core: could not listen on {addr} for the attached screen: {e}");
                    return ExitCode::FAILURE;
                }
            },
            None => None,
        };
        let hardware = if args.fake_hardware {
            tracing::info!(target: "core", "using fake I²C, SPI and GPIO buses (--fake-hardware)");
            Arc::new(faber_core::ports::fake::FakeHardware::standard())
                as Arc<dyn faber_core::ports::Hardware>
        } else {
            faber_core::ports::system_hardware()
        };
        let core = match Core::start_with_buses(args.data.clone(), logs, clock, hardware) {
            Ok(mut c) => {
                if args.no_auth {
                    c.auth = faber_core::api::auth::AuthMode::Off;
                }
                c
            }
            Err(e) => {
                eprintln!("faber-core: could not use the data folder {}: {e}", args.data.display());
                return ExitCode::FAILURE;
            }
        };
        if args.fake_arduino_cli {
            tracing::warn!(target: "core", "--fake-arduino-cli: sketches compile to made-up programs");
            core.machine.use_fake_arduino_cli();
        }
        let shutdown = async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!(target: "core", "stopping");
        };
        match core.serve_with_screen(listener, screen, shutdown).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("faber-core: the server stopped with an error: {e}");
                ExitCode::FAILURE
            }
        }
    })
}
