mod compile;
mod intrinsics;
mod language;
mod macros;
mod runtime;

use std::{
    fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

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
    /// Run a single worker. Takes ~8MB of memory.
    One(OneArgs),

    /// Run the full serverless runtime.
    /// The amount of memory needed is determined by the
    /// `n-pods` and `n-pods-per-worker` options.
    Run(RunArgs),
}

#[derive(clap::Args)]
struct OneArgs {
    /// The source file.
    file: PathBuf,

    /// The port to run. Defaults to 3000.
    #[arg(long, required = false)]
    port: Option<u16>,

    /// The host to run.
    #[arg(long, required = false)]
    host: Option<String>,
}

#[derive(clap::Args)]
struct RunArgs {
    /// The port to run. Defaults to 3000.
    #[arg(long, required = false)]
    port: Option<u16>,

    /// The host to run.
    #[arg(long, required = false)]
    host: Option<String>,

    /// The number of pods (threads) for serverless execution.
    #[arg(long, required = true)]
    n_pods: usize,

    /// The number of workers per pod (thread) for serverless execution.
    /// It's recommended to use a lower amount so the delay between
    /// switching await points (which is usually caused by CPU tasks)
    /// can be reduced.
    #[arg(long, required = true)]
    n_workers_per_pod: usize,
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
                .expect("failed to create async runtime");

            let source = fs::read_to_string(&args.file)?;
            let source_name = args.file.to_string_lossy().into_owned();

            rt.block_on(start_one(
                source,
                source_name,
                SocketAddr::new(
                    IpAddr::from_str(&args.host.as_ref().map(|k| &**k).unwrap_or("127.0.0.1"))
                        .expect("failed to parse ip addr"),
                    args.port.unwrap_or(3000),
                ),
            ));
        }

        Command::Run(args) => {
            tracing::info!("creating a full serverless runtime...");

            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to create async runtime");

            rt.block_on(start(
                args.n_pods,
                args.n_workers_per_pod,
                SocketAddr::new(
                    IpAddr::from_str(&args.host.as_ref().map(|k| &**k).unwrap_or("127.0.0.1"))
                        .expect("failed to parse ip addr"),
                    args.port.unwrap_or(3000),
                ),
            ));
        }
    }

    Ok(())
}

async fn start_one(source: String, source_name: String, addr: SocketAddr) {
    let serverless = Serverless::new_one();
    let platform = serverless.get_platform();

    let (svl, handle) = serverless.start(addr);

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
        tracing::error!(?e, "error while joining task handle");
    }
}

async fn start(n_workers: usize, n_workers_per_pod: usize, addr: SocketAddr) {
    let serverless = Serverless::new(n_workers, n_workers_per_pod);

    let (_svl, handle) = serverless.start(addr);

    if let Err(e) = handle.await {
        tracing::error!(?e, "error while joining task handle");
    }
}
