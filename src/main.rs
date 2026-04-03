mod compile;
mod intrinsics;
mod language;
mod macros;
mod runtime;

use std::{fs, path::PathBuf, time::Duration};

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
    let mut serverless = Serverless::start_one();
    let (pod_id, pod_worker_id) = serverless
        .create_worker(WorkerTask {
            source,
            source_name,
            platform: serverless.get_platform(),
        })
        .await
        .unwrap();

    println!("created at {pod_id}:{pod_worker_id}");

    tokio::time::sleep(Duration::from_hours(100)).await;
    serverless.halt().await;
}
