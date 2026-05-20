use std::{ffi::c_void, fs, io, mem, path::PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use svld_language::{ThrowException, throw};
use tokio::sync::oneshot;
use v8::{FunctionTemplate, Global, Object, PromiseResolver};

use crate::{
    bindings::{
        BindingBackend, BindingBackendMessage, BindingBackendRx, BindingBackendTx,
        backend::BindingClient, binding_backend_channel,
    },
    worker::WorkerState,
};

#[derive(Serialize, Deserialize)]
pub enum KvPayload {
    Get { key: String },
    Put { key: String, value: String },
    Delete { key: String },
    List { key: String },
}

type KvGetResult = Result<Option<String>, String>;

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
    /// `Ok(KvBackend)` if successful.
    #[inline]
    pub fn new<P: Into<PathBuf>>(save_path: P) -> Result<Self, KvBackendError> {
        let path = save_path.into();

        if path.exists() {
            fs::create_dir_all(&path).map_err(|e| KvBackendError::IoError(e))?;
        }

        let (tx, rx) = binding_backend_channel();
        Ok(Self { tx, rx, path })
    }
}

#[async_trait]
impl BindingBackend for KvBackend {
    #[inline(always)]
    fn get_tx(&self) -> BindingBackendTx {
        self.tx.clone()
    }

    #[inline(always)]
    fn create_client<K: ToString>(&self, env_name: K) -> Box<dyn BindingClient> {
        Box::new(KvClient::new(env_name.to_string()))
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
                            replier.send(json_error("failed to get key")).ok();
                            continue;
                        }
                    };

                    let reply = if let Some(data) = data {
                        let s = String::from_utf8_lossy(&data);
                        ijson::to_value::<KvGetResult>(Ok(Some(s.to_string()))).unwrap()
                    } else {
                        ijson::to_value::<KvGetResult>(Ok(None)).unwrap()
                    };
                    replier.send(reply).ok();
                }

                KvPayload::Delete { key } => {
                    // TODO: use `error` field instead
                    replier
                        .send(ijson::ijson!({"success": tree.remove(key).is_ok()}))
                        .ok();
                }

                KvPayload::Put { key, value } => {
                    // TODO: use `error` field instead
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
fn json_error<K: AsRef<str>>(reason: K) -> ijson::IValue {
    ijson::ijson!({"error": reason.as_ref()})
}

#[derive(Debug, thiserror::Error)]
pub enum KvBackendError {
    #[error(transparent)]
    IoError(#[from] io::Error),
}

struct EnvName {
    ptr: *const u8,
    len: usize,
}

impl EnvName {
    #[inline]
    fn new(name: String) -> Self {
        let name = name.into_boxed_str();

        let len = name.len();
        let name_ptr = Box::into_raw(name);

        Self {
            ptr: name_ptr as *mut u8,
            len,
        }
    }
}

unsafe impl Sync for EnvName {}
unsafe impl Send for EnvName {}

impl Drop for EnvName {
    fn drop(&mut self) {
        let slice_ptr = core::ptr::slice_from_raw_parts(self.ptr, self.len);
        let str_ptr = slice_ptr as *mut str; // a fat pointer
        let _ = unsafe { Box::from_raw(str_ptr) };
    }
}

#[repr(transparent)]
pub struct KvClient {
    env_name: EnvName,
}

impl KvClient {
    fn new(name: String) -> Self {
        Self {
            env_name: EnvName::new(name),
        }
    }

    #[inline(always)]
    const unsafe fn get_static_env_name(&self) -> &'static EnvName {
        unsafe { mem::transmute(&self.env_name) }
    }
}

impl BindingClient for KvClient {
    fn get_interface<'s>(&self, scope: &v8::PinScope<'s, '_>) -> Option<v8::Local<'s, v8::Value>> {
        let obj = Object::new(scope);

        {
            let fnk = FunctionTemplate::builder(
                |scope: &mut v8::PinScope,
                 args: v8::FunctionCallbackArguments,
                 mut rv: v8::ReturnValue| {
                    let key = {
                        let k = args.get(0);
                        if k.is_null_or_undefined() {
                            throw(scope, ThrowException::type_error("expected key name"));
                            return;
                        }

                        k.to_string(scope).unwrap().to_rust_string_lossy(scope)
                    };

                    let state = WorkerState::get_from_isolate(scope);
                    let name =
                        unsafe { &*(args.data().cast::<v8::External>().value() as *const EnvName) };

                    let name = {
                        let slice = unsafe { core::slice::from_raw_parts(name.ptr, name.len) };
                        unsafe { core::str::from_utf8_unchecked(slice) }
                    };

                    let tx = state.binding_store.get_binding_tx(name).unwrap();
                    let (reply, recv) = oneshot::channel();

                    let _ = tx.send(BindingBackendMessage {
                        worker: state.name.clone(),
                        data: ijson::to_value(KvPayload::Get { key }).unwrap(),
                        replier: reply,
                    });

                    let resolver = PromiseResolver::new(scope).unwrap();

                    let gresolver = Global::new(scope, resolver);

                    state.clone().tasks.spawn_local(async move {
                        let result =
                            ijson::from_value::<KvGetResult>(&recv.await.unwrap()).unwrap();

                        state.schedule_resolution_and_tick(gresolver, {
                            match result {
                                Ok(Some(s)) => Ok(Box::new(move |scope| {
                                    Some(v8::String::new(scope, &s)?.cast())
                                })),

                                Ok(None) => Ok(Box::new(|scope| Some(v8::undefined(scope).cast()))),

                                Err(e) => Err(ThrowException::error(e)),
                            }
                        });
                    });

                    rv.set(resolver.cast());
                },
            )
            .data(
                v8::External::new(scope, unsafe { self.get_static_env_name() } as *const _
                    as *mut c_void)
                .cast(),
            )
            .build(scope)
            .get_function(scope)?;
            obj.set(scope, v8::String::new(scope, "get")?.cast(), fnk.cast())?;
        }

        Some(obj.cast())
    }
}
