use std::collections::HashMap;

use tokio::task::JoinHandle;

use crate::bindings::{BindingBackendRx, BindingBackendTx, binding_backend_channel};

struct ActiveBindingBackend {
    tx: BindingBackendTx,
    join_handle: JoinHandle<()>,
}

/// A store containing active bindings.
#[repr(transparent)]
#[derive(Default)]
pub struct BindingStore {
    bindings: HashMap<String, ActiveBindingBackend>,
}

impl BindingStore {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a bindign to the store.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let mut store = BindingStore::new();
    ///
    /// store.add_binding(
    ///     "kv",
    ///     |rx| {
    ///         tokio::task::spawn(binding_async_task(rx))
    ///     }
    /// );
    /// ```
    pub fn add_binding<K: ToString>(
        &mut self,
        name: K,
        invoke: impl FnOnce(BindingBackendRx) -> JoinHandle<()>,
    ) {
        let (tx, rx) = binding_backend_channel();
        self.bindings.insert(
            name.to_string(),
            ActiveBindingBackend {
                tx,
                join_handle: invoke(rx),
            },
        );
    }

    #[inline]
    pub fn with_binding<K: ToString>(
        mut self,
        name: K,
        invoke: impl FnOnce(BindingBackendRx) -> JoinHandle<()>,
    ) -> Self {
        self.add_binding(name, invoke);
        self
    }
}
