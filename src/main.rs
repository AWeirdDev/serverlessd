mod compile;
mod intrinsics;
mod language;
mod macros;
mod runtime;

use std::{
    fs,
    io::{self, Write},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

use bytes::Bytes;
use clap::Parser;

use crate::runtime::Serverless;

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

    /// Clean all stored workers.
    Clean(CleanArgs),

    /// Upload a worker in this working directory,
    /// instead of HTTP.
    Upload(UploadArgs),
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

#[derive(clap::Args)]
struct CleanArgs {
    /// Whether to forcefully clean them up.
    #[arg(short, required = false, default_value = "false")]
    y: bool,
}

#[derive(clap::Args)]
struct UploadArgs {
    /// The file to upload.
    #[arg(long)]
    file: PathBuf,

    /// The name of the worker.
    #[arg(long)]
    name: String,
}

fn main() {
    let cli = Cli::parse();
    dotenvy::dotenv_override().ok();

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

            let source = match fs::read_to_string(&args.file) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("=====x error: failed to open {:?}", &args.file);
                    eprintln!("       error: {}", &e.to_string());
                    return;
                }
            };

            let secret = dotenvy::var("SERVERLESSD_SECRET").unwrap_or_else(|_| {
                eprintln!("=====> couldn't find env 'SERVERLESSD_SECRET', using blank bytes");
                "0".repeat(32)
            });

            rt.block_on(start_one(
                source,
                SocketAddr::new(
                    IpAddr::from_str(&args.host.as_ref().map(|k| &**k).unwrap_or("127.0.0.1"))
                        .expect("failed to parse ip addr"),
                    args.port.unwrap_or(3000),
                ),
                secret,
            ));
        }

        Command::Run(args) => {
            tracing::info!("creating a full serverless runtime...");

            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to create async runtime");

            let Ok(secret) = dotenvy::var("SERVERLESSD_SECRET") else {
                eprintln!("=====x error: couldn't find env 'SERVERLESSD_SECRET'");
                return;
            };

            rt.block_on(start(
                args.n_pods,
                args.n_workers_per_pod,
                SocketAddr::new(
                    IpAddr::from_str(&args.host.as_ref().map(|k| &**k).unwrap_or("127.0.0.1"))
                        .expect("failed to parse ip addr"),
                    args.port.unwrap_or(3000),
                ),
                secret,
            ));
        }

        Command::Clean(args) => {
            if !args.y {
                let mut buf = String::with_capacity(1);
                print!("this will remove all workers in (.serverlessd/workers/) [y/N] ");
                io::stdout().flush().ok();
                io::stdin().read_line(&mut buf).ok();

                if !buf.to_lowercase().starts_with("y") {
                    eprintln!("=====> canceled.");
                    return;
                }
            }

            fs::remove_dir_all(".serverlessd/").ok();
            println!("=====> cleaned all workers.");
        }

        Command::Upload(args) => {
            let contents = match fs::read(&args.file) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("=====x error: failed to open {:?}", &args.file);
                    eprintln!("       error: {}", &e.to_string());
                    return;
                }
            };
            let res = fs::write(
                PathBuf::from(".serverlessd/workers/").join(&args.file),
                contents,
            );

            match res {
                Ok(_) => println!("=====> successfully written"),
                Err(e) => {
                    eprintln!("=====x error: failed to write {:?}", &args.file);
                    eprintln!("       error: {}", &e.to_string());
                    return;
                }
            }
        }
    }
}

async fn start_one(source: String, addr: SocketAddr, secret: String) {
    let serverless = Serverless::new_one();

    let worker_id = uuid::Uuid::new_v4().to_string();
    let worker_url = format!("{}/worker/{}", addr, worker_id);

    let (svl, handle) = serverless.start(addr, secret);

    let res = svl
        .upload_worker(worker_id.to_string(), Bytes::from_owner(source))
        .await;
    if res.is_some() {
        tracing::error!("failed to upload one worker, reason: {res:?}");
        eprintln!("=====x error: failed to upload one worker");
        eprintln!("              this is usually due to a closed serverless runtime");
        return;
    }

    println!("=====> worker uploaded. visit: {}", worker_url);

    if let Err(e) = handle.await {
        tracing::error!(?e, "error while joining task handle");
    }

    if fs::remove_file(format!(".serverlessd/workers/{}.js", worker_id)).is_err() {
        eprintln!("=====! failed to remove temp worker file");
    }
}

async fn start(n_workers: usize, n_workers_per_pod: usize, addr: SocketAddr, secret: String) {
    let serverless = Serverless::new(n_workers, n_workers_per_pod);

    let (_svl, handle) = serverless.start(addr, secret);

    if let Err(e) = handle.await {
        tracing::error!(?e, "error while joining task handle");
    }
}
