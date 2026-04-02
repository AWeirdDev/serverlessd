mod compile;
mod intrinsics;
mod language;
mod macros;
mod runtime;

use std::{fs, path::PathBuf};

use clap::Parser;

use crate::runtime::WorkerTask;

/// Serverless workers management system.
#[derive(clap::Parser)]
#[command(name = "serverlessd", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Enable debugging logs.
    #[arg(short, long, global = true, default_value = "false")]
    debug: bool,

    /// The subcommand to run.
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Run a single worker.
    One(OneArgs),
}

#[derive(clap::Args)]
struct OneArgs {
    /// The source file.
    file: PathBuf,
}

fn main() -> Result<(), Box<dyn core::error::Error>> {
    let cli = Cli::parse();

    if cli.debug {
        tracing_subscriber::fmt::init();
    }

    match cli.command {
        Command::One(args) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create runtime");

            let source = fs::read_to_string(&args.file)?;
            let source_name = args.file.to_string_lossy().into_owned();
        }
    }

    Ok(())
}
