use std::path::PathBuf;

use clap::Parser;
use serde::{Deserialize, Serialize};
use svld_ipc_client::{ServerMessage, connect};

/// Serverless workers management architecture.
#[derive(clap::Parser)]
#[command(name = "svld-binding-kv", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Enable debugging logs.
    #[arg(short, long, default_value = "false")]
    debug: bool,

    /// Where the `.serverlessd` path is.
    #[arg(short, long, default_value = ".serverlessd")]
    path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn core::error::Error>> {
    let Cli {
        debug,
        path: base_path,
    } = Cli::parse();

    if debug {
        tracing_subscriber::fmt::init();
    }

    let socket_path = base_path.join("bindings.sock");
    let db_path = base_path.join("bindings__kv");

    tracing::info!("connecting to {:?}", &socket_path);

    let mut client = connect()
        .binding_type("kv".into())
        .function_names(vec![
            "get".into(),
            "delete".into(),
            "put".into(),
            "list".into(),
        ])
        .path(socket_path)
        .call()
        .await?
        .perform_handshake()
        .await?;

    let db = sled::open(db_path)?;

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        let server_message = tokio::select! {
            _ = &mut ctrl_c => {
                break;
            }
            msg = client.recv_message::<[ijson::IValue; 1]>() => msg
        };

        let ServerMessage {
            id,
            function_name,
            worker_name,
            args,
        } = match server_message {
            Ok(t) => t,
            Err(err) => {
                tracing::error!("error while receiving message: {err:?}");
                break;
            }
        };

        let tree = match db.open_tree(&worker_name) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("failed to open tree {e:?}");
                client.send_error(id, "Failed to open kv").await?;

                continue;
            }
        };

        macro_rules! unwrap_or_err {
            ($e:expr, $err_msg:expr) => {
                match $e {
                    Some(t) => t,
                    None => {
                        client.send_error(id, $err_msg).await?;
                        continue;
                    }
                }
            };
        }

        let payload = {
            match &*function_name {
                "get" => KvPayload::Get {
                    key: unwrap_or_err!(maybe_ivalue_as_str(args.get(0)), "invalid key type"),
                },

                "delete" => KvPayload::Delete {
                    key: unwrap_or_err!(maybe_ivalue_as_str(args.get(0)), "invalid key type"),
                },

                "put" => {
                    let key = maybe_ivalue_as_str(args.get(0));
                    let value = maybe_ivalue_as_str(args.get(1));

                    KvPayload::Put {
                        key: unwrap_or_err!(key, "invalid key type"),
                        value: unwrap_or_err!(value, "invalid value type"),
                    }
                }

                "list" => KvPayload::List,

                unknown => {
                    client
                        .send_error(id, format!("unknown function {unknown:?}"))
                        .await?;
                    continue;
                }
            }
        };

        match &payload {
            KvPayload::Get { key } => {
                match tree.get(&key) {
                    Ok(Some(data)) => {
                        let s = String::from_utf8_lossy(&data);
                        client.send_ok(id, s).await?;
                    }

                    Ok(None) => {
                        client.send_ok(id, payload).await?;
                    }

                    Err(e) => {
                        tracing::error!("failed to get key {key:?}: {e:?}");
                        client
                            .send_error(id, format!("Failed to get key {key:?}"))
                            .await?;
                    }
                };
            }

            KvPayload::Delete { key } => {
                match tree.remove(&key) {
                    Ok(_) => client.send_ok(id, true).await?,
                    Err(e) => {
                        tracing::error!("failed to remove key {key:?}: {e:?}");
                        client
                            .send_error(id, format!("Failed to delete key {key:?}"))
                            .await?;
                    }
                };
            }

            KvPayload::Put { key, value } => {
                match tree.insert(key, value.as_bytes()) {
                    Ok(_) => client.send_ok(id, true).await?,
                    Err(e) => {
                        tracing::error!("failed to put (insert) key {key:?}: {e:?}");
                        client
                            .send_error(id, format!("Failed to put key {key:?}"))
                            .await?;
                    }
                };
            }

            KvPayload::List => {
                let mut keys = vec![];
                for maybe_key in tree.iter().keys() {
                    if let Ok(key) = maybe_key {
                        let name = String::from_utf8_lossy(&key);
                        keys.push(ijson::ijson!({"name": name}));
                    }
                }

                client.send_ok(id, ijson::ijson!({"keys": keys})).await?;
            }
        }
    }

    tracing::info!("exit");

    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum KvPayload {
    Get { key: String },
    Delete { key: String },
    Put { key: String, value: String },
    List,
}

#[inline(always)]
fn maybe_ivalue_as_str(value: Option<&ijson::IValue>) -> Option<String> {
    value
        .and_then(|item| item.as_string())
        .map(|item| item.to_string())
}
