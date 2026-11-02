//! Lists audio sources and prints `hunch-core` session events live from one of them.

mod listen;
mod print;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use hunch_capture::{AudioBackend, SourceId, default_backend};

#[derive(Parser)]
#[command(
    version,
    about = "Capture audio from a device or app and analyze it live"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the devices and the apps with audio sessions, with their ids.
    List,
    /// Capture a source and print each session event as it happens.
    Listen {
        /// A source id from `list`.
        id: String,
        /// Stop after this many seconds. Runs until the source ends or Ctrl+C otherwise.
        #[arg(long)]
        seconds: Option<f32>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let backend = default_backend();
    let result = match cli.command {
        Command::List => list(backend.as_ref()),
        Command::Listen { id, seconds } => listen::run(listen::ListenArgs {
            backend: backend.as_ref(),
            id: SourceId::new(id),
            seconds,
        }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn list(backend: &dyn AudioBackend) -> Result<(), String> {
    let sources = backend.list_sources().map_err(|error| error.to_string())?;
    print::sources(&sources);
    if !backend.lists_apps() {
        println!("(this platform can't list apps)");
    }
    Ok(())
}
