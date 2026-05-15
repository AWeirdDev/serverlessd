use std::collections::{HashMap, hash_map};

use tokio_util::task::TaskTracker;

use crate::bindings::{BindingBackend, BindingBackendTx};

/// A store containing active bindings.
#[derive(Default, Debug)]
pub struct BindingStore {
    bindings: HashMap<String, BindingBackendTx>,
    tasks: TaskTracker,
}

impl BindingStore {
    /// Creates a blank binding store.
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes a binding to the store.
    pub fn push_binding<K: ToString, B: BindingBackend + Send + 'static>(
        &mut self,
        name: K,
        mut backend: B,
    ) {
        let tx = backend.get_tx();

        self.bindings.insert(name.to_string(), tx);
        self.tasks.spawn(async move {
            backend.start().await;
        });
    }

    /// Adds a binding, then returns `Self`.
    #[inline]
    pub fn add_binding<K: ToString, B: BindingBackend + Send + 'static>(
        mut self,
        name: K,
        backend: B,
    ) -> Self {
        self.push_binding(name, backend);
        self
    }

    /// Gets a handle to the binding backend.
    #[inline(always)]
    pub fn get_binding_tx<K: AsRef<str>>(&self, name: K) -> Option<BindingBackendTx> {
        self.bindings.get(name.as_ref()).map(|item| item.clone())
    }

    /// Lists all bindings.
    #[inline(always)]
    pub fn list(&self) -> hash_map::Iter<'_, String, BindingBackendTx> {
        self.bindings.iter()
    }
}
