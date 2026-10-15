//! `faber`: make Faber connectors (docs/your-first-connector.md).

use clap::{Parser, Subcommand};
use faber_cli::check::{self, Options};
use faber_cli::template::{self, Kind};
use faber_cli::{CliError, pack};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "faber",
    version,
    about = "Make Faber connectors: start from a template, check against recordings, pack."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start something new from a template.
    New {
        #[command(subcommand)]
        what: New,
    },
    /// Build the connector in this folder, load it the way the core does, and replay every
    /// recording in captures/ through it.
    Check {
        /// The connector's folder (default: this one).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Check the connector, then write <id>-<version>.faberpkg, ready to install.
    Pack {
        /// The connector's folder (default: this one).
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Where to write the package (default: the connector's folder).
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum New {
    /// A connector project that builds and passes its tests before you change a line.
    Connector {
        /// Its id: lowercase words joined by -, like acme-ranger.
        name: String,
        /// wasm: Rust, sandboxed. declarative: only a faber.toml, for text-line devices.
        #[arg(long, default_value = "wasm")]
        kind: String,
    },
}

fn run(cli: Cli) -> Result<bool, CliError> {
    let mut out = std::io::stdout();
    match cli.command {
        Command::New { what: New::Connector { name, kind } } => {
            let kind = Kind::parse(&kind).ok_or_else(|| {
                CliError::new(
                    format!("--kind {kind} isn't a kind of connector."),
                    "Use --kind wasm or --kind declarative.",
                )
            })?;
            let here = std::env::current_dir().map_err(|e| {
                CliError::new(
                    format!("The current folder can't be read: {e}"),
                    "Run faber from a folder you can write to.",
                )
            })?;
            let dir = template::new_connector(&name, kind, &here)?;
            println!("Made {}.", dir.display());
            println!();
            println!("Next:");
            println!("  cd {name}");
            if kind == Kind::Wasm {
                println!("  cargo test      # the parser against captures/sample.fbcap");
            }
            println!("  faber check     # build it and replay every recording in captures/");
            println!("  faber pack      # make {name}-0.1.0.faberpkg, then double-click it in Faber's Files");
            Ok(true)
        }
        Command::Check { dir } => {
            let report = check::check(&dir, &Options::default(), &mut out)?;
            for line in report.lines() {
                println!("{line}");
            }
            if report.passed() {
                println!("All good. Run faber pack to make the package.");
            }
            Ok(report.passed())
        }
        Command::Pack { dir, out: out_dir } => {
            pack::pack(&dir, out_dir.as_deref(), &Options::default(), &mut out)?;
            println!("Install it by double-clicking it in Faber's Files, or dropping it on the desktop.");
            Ok(true)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {}", e.message);
            eprintln!("  {}", e.hint);
            ExitCode::FAILURE
        }
    }
}
