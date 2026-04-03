mod compile;
mod intrinsics;
mod language;
mod macros;
mod runtime;

use std::{fs, path::PathBuf};

use clap::Parser;

use crate::runtime::{Serverless, WorkerTask};

/// Serverless workers management architecture.
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
            tracing::info!("creating a runtime in this thread...");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create runtime");

            let source = fs::read_to_string(&args.file)?;
            let source_name = args.file.to_string_lossy().into_owned();

            rt.block_on(start_one(source, source_name));
        }
    }

    Ok(())
}

async fn start_one(source: String, source_name: String) {
    tracing::info!(target: "(one)", "creating runtime");

    let serverless = Serverless::new_one();
    let platform = serverless.get_platform();

    let (svl, handle) = serverless.start();

    let (pod_id, pod_worker_id) = svl
        .create_worker(WorkerTask {
            source,
            source_name,
            platform,
        })
        .await
        .expect("failed to create worker");

    tracing::info!("created one worker at {}:{}", pod_id, pod_worker_id);

    if let Err(e) = handle.await {
        tracing::error!(?e, "error while returning handle");
    }
}
