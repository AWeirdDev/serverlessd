use std::collections::{HashMap, hash_map};

use tokio_util::task::TaskTracker;

use crate::bindings::{BindingBackend, BindingBackendTx, backend::BindingClient};

/// A store containing active bindings.
#[derive(Default)]
pub struct BindingStore {
    bindings: HashMap<String, BindingItem>,
    tasks: TaskTracker,
}

// ==== impl BindingStore ====

impl BindingStore {
    /// Creates a blank binding store.
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes a binding to the store.
    pub fn push_spawn_binding<K: ToString, B: BindingBackend + Send + 'static>(
        &mut self,
        name: K,
        mut backend: B,
    ) {
        let name = name.to_string();
        let tx = backend.get_tx();
        let client = backend.create_client(&name);

        self.bindings.insert(name, BindingItem { tx, client });
        self.tasks.spawn(async move {
            backend.start().await;
        });
    }

    /// Adds a binding, then returns `Self`.
    #[inline]
    pub fn add_spawn_binding<K: ToString, B: BindingBackend + Send + 'static>(
        mut self,
        name: K,
        backend: B,
    ) -> Self {
        self.push_spawn_binding(name, backend);
        self
    }

    /// Gets a handle to the binding backend.
    #[inline(always)]
    pub fn get_binding_tx<K: AsRef<str>>(&self, name: K) -> Option<BindingBackendTx> {
        self.bindings.get(name.as_ref()).map(|item| item.tx.clone())
    }

    /// Lists all bindings.
    #[inline(always)]
    pub fn list(&self) -> hash_map::Iter<'_, String, BindingItem> {
        self.bindings.iter()
    }
}

impl std::fmt::Debug for BindingStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BindingStore")
    }
}

/// A thin wrapper around the binding backend transmitter (backend tx)
/// and the binding client.
pub struct BindingItem {
    pub tx: BindingBackendTx,
    pub client: Box<dyn BindingClient>,
}
