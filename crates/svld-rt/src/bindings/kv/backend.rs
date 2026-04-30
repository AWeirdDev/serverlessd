use std::{fs, path::PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::bindings::{
    BindingBackend, BindingBackendMessage, BindingBackendRx, BindingBackendTx,
    binding_backend_channel,
};

#[derive(Serialize, Deserialize)]
pub enum KvPayload {
    Get { key: String },
    Put { key: String, value: String },
    Delete { key: String },
    List { key: String },
}

/// The backend for the KV binding.
pub struct KvBackend {
    tx: BindingBackendTx,
    rx: BindingBackendRx,
    path: PathBuf,
}

impl KvBackend {
    /// Creates a new key-value store backend.
    ///
    /// # Parameters
    /// - `save_path`: Where to save the KV store.
    ///
    /// # Returns
    /// `Some( KvBackend )` if successful, returns `None` if the database
    /// path can't be created.
    #[inline]
    pub fn new<P: Into<PathBuf>>(save_path: P) -> Option<Self> {
        let path = save_path.into();

        if path.exists() {
            fs::create_dir_all(&path).ok()?;
        }

        let (tx, rx) = binding_backend_channel();
        Some(Self { tx, rx, path })
    }
}

#[async_trait]
impl BindingBackend for KvBackend {
    #[inline(always)]
    fn get_tx(&self) -> BindingBackendTx {
        self.tx.clone()
    }

    async fn start(&mut self) {
        let db = sled::open(&self.path).expect("failed to open db");

        while let Some(BindingBackendMessage {
            worker,
            data,
            replier,
        }) = self.rx.recv().await
        {
            let Ok(message) = ijson::from_value::<KvPayload>(&data) else {
                continue;
            };
            let tree = match db.open_tree(worker) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("failed to open tree {e:?}");
                    continue;
                }
            };

            match message {
                KvPayload::Get { key } => {
                    let data = match tree.get(key) {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::error!("(kv binding) failed to get key: {e:?}");
                            replier.send(error("failed to get key")).ok();
                            continue;
                        }
                    };

                    let reply = if let Some(data) = data {
                        let s = String::from_utf8_lossy(&data);
                        ijson::ijson!({"data": s})
                    } else {
                        ijson::ijson!({"data": null})
                    };
                    replier.send(reply).ok();
                }

                KvPayload::Delete { key } => {
                    replier
                        .send(ijson::ijson!({"success": tree.remove(key).is_ok()}))
                        .ok();
                }

                KvPayload::Put { key, value } => {
                    replier
                        .send(
                            ijson::ijson!({"success": tree.insert(key, value.as_bytes()).is_ok()}),
                        )
                        .ok();
                }

                _ => (),
            }
        }
    }
}

#[inline(always)]
fn error<K: AsRef<str>>(reason: K) -> ijson::IValue {
    ijson::ijson!({"error": reason.as_ref()})
}
