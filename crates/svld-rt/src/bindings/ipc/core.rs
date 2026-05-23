use std::{io, path::PathBuf, sync::Arc};

use interprocess::local_socket::{GenericFilePath, ListenerOptions, ToFsName, tokio::Listener};

use crate::bindings::{BindingStore, ipc::binding_backend::IpcBindingBackend};

pub struct IpcBindingsServer {
    listener: Listener,
    binding_types_and_backends: Vec<(String, Arc<IpcBindingBackend>)>,
}

#[bon::bon]
impl IpcBindingsServer {
    /// Creates the IPC binding server.
    #[builder]
    pub fn new(
        path: PathBuf,
        binding_types: Vec<String>,
        binding_store: &mut BindingStore,
    ) -> Result<Self, IpcConnectionError> {
        let name = path
            .to_fs_name::<GenericFilePath>()
            .map_err(|err| IpcConnectionError::NameNotSupported(err))?;

        let listener = ListenerOptions::new()
            .name(name)
            .create_tokio()
            .map_err(|err| IpcConnectionError::CreationError(err))?;

        let binding_backends = binding_types
            .into_iter()
            .map(|type_| {
                let backend = Arc::new(binding_backend::IpcBindingBackend::new(vec![]));
                binding_store.push_binding(&type_, backend.clone());

                (type_, backend)
            })
            .collect::<Vec<_>>();

        Ok(Self {
            listener,
            binding_types_and_backends: binding_backends,
        })
    }

    /// Starts the IPC binding server.
    #[inline]
    pub async fn start(self) {
        task::bindings_server_task()
            .listener(self.listener)
            .binding_types_and_backends(self.binding_types_and_backends)
            .call()
            .await;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IpcConnectionError {
    #[error("the name is not supported by the platform.")]
    NameNotSupported(io::Error),

    #[error("failed to create listener, error: {0:}")]
    CreationError(io::Error),
}

pub mod binding_backend {
    use std::{
        mem, ptr,
        sync::atomic::{self, AtomicBool, AtomicUsize},
    };

    use super::binding_client::IpcBindingClient;
    use crate::bindings::{BindingBackend, BindingBackendTx, backend::BindingClient};

    pub struct IpcBindingBackend {
        tx_rx: (
            AtomicBool,  // has data?
            AtomicUsize, // BindingBackendTx
        ),

        functions: Vec<String>,
    }

    const _: () = assert!(
        size_of::<BindingBackendTx>() == size_of::<usize>(),
        "expected pointer-width backend transmitter type"
    );

    impl IpcBindingBackend {
        #[inline(always)]
        pub const fn new(functions: Vec<String>) -> Self {
            Self {
                tx_rx: (AtomicBool::new(false), AtomicUsize::new(0)),
                functions,
            }
        }

        #[inline(always)]
        pub fn is_occupied(&self) -> bool {
            self.tx_rx.0.load(atomic::Ordering::Acquire)
        }

        pub fn set_tx(&self, tx: BindingBackendTx) -> Result<(), IpcBindingBackendError> {
            if self.is_occupied() {
                return Err(IpcBindingBackendError::AlreadySet);
            }

            if self
                .tx_rx
                .0
                .compare_exchange_weak(
                    false,
                    true,
                    atomic::Ordering::AcqRel,
                    atomic::Ordering::Relaxed,
                )
                .is_err()
            {
                return Err(IpcBindingBackendError::AlreadySet);
            }

            // SAFETY: asserted BindingBackendTx/Rx to be usize-sized
            self.tx_rx
                .1
                .store(unsafe { mem::transmute(tx) }, atomic::Ordering::Release);

            Ok(())
        }

        /// Gets the transmitter.
        pub fn get_atomic_tx(&self) -> Option<BindingBackendTx> {
            if !self.tx_rx.0.load(atomic::Ordering::Acquire) {
                return None;
            }

            // SAFETY: it's impossible to set back to `None`.
            // once it's set, it cannot be altered. so race conditions
            // do not apply here.
            let tx = unsafe {
                &*(self.tx_rx.1.load(atomic::Ordering::Acquire) as *const BindingBackendTx)
            }
            .clone();

            Some(tx)
        }

        #[inline(always)]
        #[must_use]
        pub fn take_atomic_tx(&self) -> Option<BindingBackendTx> {
            takeaway(&self.tx_rx.0, &self.tx_rx.1)
        }
    }

    #[inline]
    fn takeaway<T>(whether: &AtomicBool, item: &AtomicUsize) -> Option<T> {
        if !whether.load(atomic::Ordering::Acquire) {
            return None;
        }

        let raw = item.swap(0, atomic::Ordering::AcqRel);
        if raw == 0 {
            return None;
        }

        // SAFETY: only one owner at this point
        Some(unsafe { ptr::read(raw as *const T) })
    }

    impl BindingBackend for IpcBindingBackend {
        #[inline(always)]
        fn get_tx(&self) -> BindingBackendTx {
            self.get_atomic_tx()
                .expect("ipc binding backend not initialized yet")
        }

        #[inline]
        fn create_client(&self, binding_name: &str) -> Box<dyn BindingClient> {
            Box::new(
                IpcBindingClient::builder()
                    .binding_name(binding_name.to_string())
                    .functions(self.functions.clone())
                    .build(),
            )
        }
    }

    impl Drop for IpcBindingBackend {
        fn drop(&mut self) {
            let _ = self.take_atomic_tx();
        }
    }

    #[derive(Debug, thiserror::Error)]
    pub enum IpcBindingBackendError {
        #[error("the tx had already been set")]
        AlreadySet,
    }
}

mod binding_client {
    use std::{ffi::c_void, mem};

    use v8::{External, FunctionTemplate};

    use crate::{bindings::backend::BindingClient, utils::OwnedStr, worker::WorkerState};

    pub struct IpcBindingClient {
        functions: Vec<String>,
        binding_name: OwnedStr,
    }

    #[bon::bon]
    impl IpcBindingClient {
        #[builder]
        pub fn new(functions: Vec<String>, binding_name: String) -> Self {
            Self {
                functions,
                binding_name: binding_name.into(),
            }
        }

        #[inline(always)]
        const unsafe fn get_static_binding_name(&self) -> &'static OwnedStr {
            unsafe { mem::transmute(&self.binding_name) }
        }
    }

    impl BindingClient for IpcBindingClient {
        fn create_interface<'s>(
            &self,
            scope: &v8::PinScope<'s, '_>,
        ) -> Option<v8::Local<'s, v8::Value>> {
            let obj = v8::Object::new(scope);

            for function in self.functions.iter() {
                let fnk = FunctionTemplate::builder(
                    |scope: &mut v8::PinScope,
                     args: v8::FunctionCallbackArguments,
                     _rv: v8::ReturnValue| {
                        let name = unsafe {
                            &*(args.data().cast::<v8::External>().value() as *const OwnedStr)
                        }
                        .as_str();

                        let state = WorkerState::get_from_isolate(scope);
                        let tx = state.get_binding(name).unwrap();

                        println!("hello world {}", tx.is_closed());
                        // tx.send();
                    },
                )
                .data(
                    External::new(scope, unsafe {
                        self.get_static_binding_name() as *const OwnedStr as *mut c_void
                    })
                    .cast(),
                )
                .build(scope)
                .get_function(scope)?;

                obj.set(scope, v8::String::new(scope, function)?.cast(), fnk.cast());
            }

            None
        }
    }
}

mod task {
    use std::{
        collections::HashMap,
        io::{self, IoSlice},
        string::FromUtf8Error,
        sync::{
            Arc,
            atomic::{self, AtomicBool},
        },
    };

    use async_trait::async_trait;
    use interprocess::local_socket::{
        tokio::{Listener, RecvHalf, SendHalf, Stream},
        traits::tokio::{Listener as _, Stream as _},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use crate::bindings::{
        BindingBackendMessage, binding_backend_channel,
        ipc::binding_backend::{IpcBindingBackend, IpcBindingBackendError},
    };

    #[bon::builder]
    pub async fn bindings_server_task(
        listener: Listener,
        binding_types_and_backends: Vec<(String, Arc<IpcBindingBackend>)>,
    ) {
        let (binding_types, binding_backends): (Vec<_>, Vec<_>) =
            binding_types_and_backends.into_iter().unzip();

        let binding_connections = Arc::new(ExpectBindingConnections::new(
            binding_types,
            binding_backends,
        ));

        while let Ok(stream) = listener.accept().await {
            tokio::task::spawn(binding_identify_task(stream, binding_connections.clone()));
        }
    }

    /// A task for identifying what the binding is.
    async fn binding_identify_task(
        stream: Stream,
        binding_connections: Arc<ExpectBindingConnections>,
    ) -> Result<(), SingleTaskError> {
        let (mut recv, send) = stream.split();

        // type of the binding
        let binding_type = recv.read_parse_to_string().await?;

        let functions_len = recv.read_u32_le().await?;
        let mut functions = Vec::with_capacity(functions_len as usize);
        for _ in 0..functions_len {
            let function = recv.read_parse_to_string().await?;
            functions.push(function);
        }

        binding_connections.set_connected(&binding_type)?;

        let backend = binding_connections.get_backend(&binding_type).unwrap();

        tokio::task::spawn(async move {
            if let Err(err) = binding_task(recv, send, backend.clone()).await {
                tracing::error!(
                    "error occurred while handling binding {}: {:?}",
                    &binding_type,
                    err
                );

                // remember to remove the tx (gotta take it out)
                let _ = backend.take_atomic_tx();
            }
        });

        Ok(())
    }

    async fn binding_task(
        mut recv: RecvHalf,
        mut send: SendHalf,
        backend: Arc<IpcBindingBackend>,
    ) -> Result<(), SingleTaskError> {
        let (tx, mut rx) = binding_backend_channel();
        backend.set_tx(tx)?;

        let mut resolutions = HashMap::new();
        let mut roll_id = 0_u32;

        loop {
            // the reason why we're doing this is because `tokio::select!`
            // is a procedural macro, which means lsp support sucks
            // i aint having that
            enum Event {
                Internal(BindingBackendMessage),
                External(Message),
            }

            let event = tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(t) => Event::Internal(t),
                        None => break,
                    }
                },

                msg = recv.read_parse_message() => {
                    match msg {
                        Ok(t) => Event::External(t),
                        Err(e) => {
                            tracing::error!("failed to read & parse client message, breaking: {e:?}");
                            break;
                        }
                    }
                }
            };

            match event {
                Event::Internal(BindingBackendMessage {
                    worker,
                    data,
                    replier,
                }) => {
                    roll_id = roll_id.wrapping_add(1);
                    resolutions.insert(roll_id, replier);
                    send.send_message(Message {
                        id: roll_id,
                        payload: ijson::ijson!({"worker": worker, "data": data}),
                    })
                    .await?;
                }

                Event::External(Message { id, payload }) => {
                    if let Some(replier) = resolutions.remove(&id) {
                        let _ = replier.send(payload);
                    }
                }
            }
        }

        Ok(())
    }

    #[derive(Debug, thiserror::Error)]
    enum SingleTaskError {
        #[error(transparent)]
        IoError(#[from] io::Error),

        #[error("invalid binding type (non-utf8): {0:?}")]
        InvalidBindingType(#[from] FromUtf8Error),

        #[error(transparent)]
        BindingConnectionError(#[from] ExpectBindingError),

        #[error(transparent)]
        SerializationError(#[from] serde_json::Error),

        #[error(transparent)]
        BackendError(#[from] IpcBindingBackendError),
    }

    // ===== models =====

    struct Message {
        /// The ID of the client message.
        id: u32,

        /// The payload of the message.
        payload: ijson::IValue,
    }

    // ===== traits =====

    #[async_trait]
    trait RecvExt {
        async fn read_parse_to_boxed_arr(&mut self) -> Result<Box<[u8]>, SingleTaskError>;
        async fn read_parse_to_string(&mut self) -> Result<String, SingleTaskError>;
        async fn read_parse_message(&mut self) -> Result<Message, SingleTaskError>;
    }

    #[async_trait]
    impl RecvExt for RecvHalf {
        async fn read_parse_to_boxed_arr(&mut self) -> Result<Box<[u8]>, SingleTaskError> {
            // quick reminder for myself:
            // are you fucking retarded? the client has no reason to do some fucking
            // "concurrent writes" like how does that make any sense? it's THEIR job to
            // get this over with, and we're just reading from them

            let len = self.read_u32_le().await? as usize;
            let mut buf = Box::<[u8]>::new_uninit_slice(len);

            let slice = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, len) };
            self.read_exact(slice).await?;

            Ok(unsafe { buf.assume_init() })
        }

        #[inline(always)]
        async fn read_parse_to_string(&mut self) -> Result<String, SingleTaskError> {
            Ok(String::from_utf8(
                self.read_parse_to_boxed_arr().await?.into_vec(),
            )?)
        }

        async fn read_parse_message(&mut self) -> Result<Message, SingleTaskError> {
            // first we obtain the id
            let id = self.read_u32_le().await?;

            // then we'll get the payload
            let raw_payload = self.read_parse_to_boxed_arr().await?;
            let payload = serde_json::from_slice::<ijson::IValue>(&raw_payload)?;

            // good. fuck you and eat it up
            Ok(Message { id, payload })
        }
    }

    #[async_trait]
    trait SendExt {
        async fn send_message(&mut self, message: Message) -> Result<(), SingleTaskError>;
    }

    #[async_trait]
    impl SendExt for SendHalf {
        async fn send_message(&mut self, message: Message) -> Result<(), SingleTaskError> {
            let id_raw = message.id.to_le_bytes();

            let payload_raw = serde_json::to_vec(&message.payload)?;
            let len_raw = (payload_raw.len() as u32).to_le_bytes();

            let slices = [
                IoSlice::new(&id_raw),
                IoSlice::new(&len_raw),
                IoSlice::new(&payload_raw),
            ];
            self.write_vectored(&slices).await?;

            // fah
            Ok(())
        }
    }

    // ===== utils =====

    /// Expect binding type connections.
    struct ExpectBindingConnections {
        names: HashMap<String, usize>,
        flags: Box<[AtomicBool]>,
        backends: Box<[Arc<IpcBindingBackend>]>,
    }

    impl ExpectBindingConnections {
        fn new(mut binding_types: Vec<String>, backends: Vec<Arc<IpcBindingBackend>>) -> Self {
            let mut names = HashMap::with_capacity(binding_types.len());
            let mut flags = Vec::with_capacity(binding_types.len());

            for (idx, name) in binding_types.drain(..).enumerate() {
                names.insert(name, idx);
                *flags.get_mut(idx).unwrap() = AtomicBool::new(false);
            }

            let flags = flags.into_boxed_slice();
            let backends = backends.into_boxed_slice();

            Self {
                names,
                flags,
                backends,
            }
        }

        /// Sets a binding to connected.
        fn set_connected(&self, name: &str) -> Result<(), ExpectBindingError> {
            if let Some(&idx) = self.names.get(name) {
                let atom = self.flags.get(idx).unwrap();

                let is_connected = atom.load(atomic::Ordering::Acquire);
                if is_connected {
                    return Err(ExpectBindingError::BindingAlreadyConnected);
                }

                match atom.compare_exchange_weak(
                    is_connected,
                    true,
                    atomic::Ordering::AcqRel,
                    atomic::Ordering::Relaxed,
                ) {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ExpectBindingError::BindingAlreadyConnected),
                }
            } else {
                Err(ExpectBindingError::BindingNotFound)
            }
        }

        #[inline(always)]
        #[must_use]
        fn get_backend(&self, name: &str) -> Option<Arc<IpcBindingBackend>> {
            self.names
                .get(name)
                .and_then(|&idx| self.backends.get(idx))
                .map(|item| item.clone())
        }
    }

    #[derive(Debug, thiserror::Error)]
    enum ExpectBindingError {
        #[error("the binding was not found")]
        BindingNotFound,

        #[error("the binding has already connected")]
        BindingAlreadyConnected,
    }
}
