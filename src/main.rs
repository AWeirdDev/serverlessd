mod app;
mod handle;
mod task;
mod trigger;

use std::{
    fs,
    io::{self, Write},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    process::exit,
    str::FromStr,
    sync::Arc,
};

use bytes::Bytes;

use clap::Parser;
use svld_rt::{
    bindings::{BindingStore, ipc},
    models::{BindingConfig, GlobalConfig, WorkerConfig},
    serverless::Serverless,
};
use tokio::sync::mpsc;

use svld_configs::WranglerConfig;

use crate::{handle::ServerlessHandle, task::serverless_task};

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
    ///
    /// The amount of memory needed is determined by the
    /// `pods` and `pods-per-worker` options.
    Run(RunArgs),

    /// Clean all stored workers.
    Clean(CleanArgs),

    /// Upload a worker in this working directory.
    Upload(UploadArgs),

    /// Initialize the environment in this directory.
    Init,
}

#[derive(clap::Args)]
struct OneArgs {
    /// The TOML configuration file (usually `wrangler.toml`)
    #[arg(short = 'w', long)]
    worker_config: PathBuf,

    /// The serverlessd config file (usually `serverlessd.toml`)
    #[arg(short = 's', long)]
    serverlessd_config: PathBuf,
}

#[derive(clap::Args)]
struct RunArgs {
    /// The serverlessd config file (usually `serverlessd.toml`)
    #[arg(short = 's', long)]
    serverlessd_config: PathBuf,
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

            let worker_config = {
                let file = read_file_to_string(&args.worker_config);
                match svld_configs::wrangler::from_str(&file) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("=====x error: failed to parse {:?}", &args.worker_config);
                        eprintln!("       error: {}", &e.to_string());
                        return;
                    }
                }
            };

            let serverlessd_config = {
                let file = read_file_to_string(&args.serverlessd_config);
                match svld_configs::serverlessd::from_str(&file) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!(
                            "=====x error: failed to parse {:?}",
                            &args.serverlessd_config
                        );
                        eprintln!("       error: {}", &e.to_string());
                        return;
                    }
                }
            };
            tracing::info!(?serverlessd_config);

            let config_dir = args
                .worker_config
                .parent()
                .expect("couldn't find any parent above the config");

            let source = read_file_to_string(&config_dir.join(&worker_config.main));

            rt.block_on(start_one(
                source,
                SocketAddr::new(
                    IpAddr::from_str(&serverlessd_config.host).expect("failed to parse ip addr"),
                    serverlessd_config.port,
                ),
                worker_config,
                Arc::new(
                    GlobalConfig::builder()
                        .max_memory(serverlessd_config.max_memory)
                        .determination_strategy(serverlessd_config.determination_strategy)
                        .build(),
                ),
                serverlessd_config.bindings,
            ));
            rt.shutdown_background();
        }

        Command::Run(args) => {
            tracing::info!("creating a full serverless runtime...");

            let serverlessd_config = {
                let file = read_file_to_string(&args.serverlessd_config);
                match svld_configs::serverlessd::from_str(&file) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!(
                            "=====x error: failed to parse {:?}",
                            &args.serverlessd_config
                        );
                        eprintln!("       error: {}", &e.to_string());
                        return;
                    }
                }
            };
            tracing::info!(?serverlessd_config);

            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to create async runtime");

            rt.block_on(start(
                serverlessd_config.pods,
                serverlessd_config.workers_per_pod,
                SocketAddr::new(
                    IpAddr::from_str(&serverlessd_config.host).expect("failed to parse ip addr"),
                    serverlessd_config.port,
                ),
                Arc::new(
                    GlobalConfig::builder()
                        .max_memory(serverlessd_config.max_memory)
                        .determination_strategy(serverlessd_config.determination_strategy)
                        .build(),
                ),
                serverlessd_config.bindings,
            ));
            rt.shutdown_background();
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
                    eprintln!("       help:  use `init` to initialize an environment.");
                    return;
                }
            }
        }

        Command::Init => {
            let workers_path = PathBuf::from(".serverlessd/workers");
            if !workers_path.exists() {
                if let Err(e) = fs::create_dir_all(&workers_path) {
                    eprintln!("=====x error: failed to create dir {:?}", &workers_path);
                    eprintln!("       error: {}", &e.to_string());
                    return;
                }

                println!("=====> created dir {:?}", &workers_path);
            }

            println!("\n=====> initialized. use `.gitignore` if you're using git version control.");
        }
    }
}

fn start_ipc_server(
    ipc_path: PathBuf,
    binding_types: Vec<String>,
    binding_store: &mut BindingStore,
) {
    let server = match ipc::IpcBindingsServer::builder()
        .path(ipc_path)
        .binding_types(binding_types)
        .binding_store(binding_store)
        .build()
    {
        Ok(server) => server,
        Err(err) => {
            tracing::error!("failed to start bindings server, error: {err:?}");
            return;
        }
    };

    tokio::task::spawn(server.start());
}

fn start_binding_backends(binding_types: Vec<String>) -> Arc<BindingStore> {
    tracing::info!("binding types to use: {binding_types:?}");

    let mut store = BindingStore::new();

    start_ipc_server(
        PathBuf::from(".serverlessd/bindings.sock"),
        binding_types,
        &mut store,
    );

    Arc::new(store)
}

async fn start_one(
    source: String,
    addr: SocketAddr,
    worker_config: WranglerConfig,
    global_config: Arc<GlobalConfig>,
    allowed_binding_types: Vec<String>,
) {
    let serverless = Serverless::builder()
        .n_workers(1)
        .n_pods(1)
        .binding_store(start_binding_backends(allowed_binding_types))
        .global_config(global_config.clone())
        .build();

    let worker_url = format!("http://{}/worker/one", addr);

    let (svl, handle) = {
        let (tx, rx) = mpsc::channel(512);
        let handle = tokio::task::spawn(serverless_task(
            serverless,
            rx,
            addr,
            ServerlessHandle::new(tx.clone(), global_config.clone()),
        ));

        (ServerlessHandle::new(tx, global_config), handle)
    };

    let res = svl
        .upload_worker(
            Bytes::from_owner(source),
            WorkerConfig::builder()
                .bindings(
                    worker_config
                        .get_bindings()
                        .into_iter()
                        .map(|(type_, name)| {
                            BindingConfig::builder().type_(type_).name(name).build()
                        })
                        .collect::<Vec<_>>(),
                )
                .name("one".to_string())
                .build(),
        )
        .await;
    if res.is_err() {
        tracing::error!("failed to upload one worker, reason: {res:?}");
        eprintln!("=====x error: failed to upload one worker");
        eprintln!("              this is usually due to a closed serverless runtime");
        return;
    }

    println!("=====> worker uploaded. visit: {}", worker_url);

    if let Err(e) = handle.await {
        tracing::error!(?e, "error while joining task handle");
    }
}

async fn start(
    n_workers: usize,
    n_workers_per_pod: usize,
    addr: SocketAddr,
    global_config: Arc<GlobalConfig>,
    allowed_binding_types: Vec<String>,
) {
    let serverless = Serverless::builder()
        .n_workers(n_workers)
        .n_pods(n_workers_per_pod)
        .binding_store(start_binding_backends(allowed_binding_types))
        .global_config(global_config.clone())
        .build();

    let (_svl, handle) = {
        let (tx, rx) = mpsc::channel(512);
        let handle = tokio::task::spawn(serverless_task(
            serverless,
            rx,
            addr,
            ServerlessHandle::new(tx.clone(), global_config.clone()),
        ));

        (ServerlessHandle::new(tx, global_config), handle)
    };

    if let Err(e) = handle.await {
        tracing::error!(?e, "error while joining task handle");
    }
}

fn read_file_to_string(path: &PathBuf) -> String {
    match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("=====x error: failed to open {:?}", path);
            eprintln!("       error: {}", &e.to_string());
            exit(-1);
        }
    }
}
