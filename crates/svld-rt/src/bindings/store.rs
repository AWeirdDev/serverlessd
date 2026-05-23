use std::{
    collections::{HashMap, hash_map},
    sync::Arc,
};

use tokio_util::task::TaskTracker;

use crate::bindings::{BindingBackend, BindingBackendTx, backend::BindingClient};

/// A store containing active bindings.
///
/// Note that this store only contains the **type** of the bindings,
/// not the **name** of them. See the module documentation for more.
#[derive(Default)]
pub struct BindingStore {
    /// The bindings (`{ binding_type: binding_item }`).
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
    #[inline]
    pub fn push_binding(&mut self, type_: &str, backend: Arc<dyn BindingBackend>) {
        let type_ = type_.to_string();
        self.bindings.insert(type_, BindingItem { backend });
    }

    /// Pushes a binding to the store and spawns the task.
    #[inline]
    pub fn push_binding_and_spawn<F>(
        &mut self,
        type_: &str,
        backend: Arc<dyn BindingBackend>,
        task: F,
    ) where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.push_binding(type_, backend);
        self.tasks.spawn(task);
    }

    /// Adds a binding, then returns `Self`.
    #[inline]
    pub fn add_binding(mut self, type_: &str, backend: Arc<dyn BindingBackend>) -> Self {
        self.push_binding(type_, backend);
        self
    }

    /// Adds a binding and spawns the task, then returns `Self`.
    #[inline]
    pub fn add_binding_and_spawn<F>(
        mut self,
        type_: &str,
        backend: Arc<dyn BindingBackend>,
        task: F,
    ) -> Self
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.push_binding_and_spawn(type_, backend, task);
        self
    }

    /// Gets a handle to the binding backend from the type of the binding.
    #[inline(always)]
    pub fn get_binding_tx(&self, type_: &str) -> Option<BindingBackendTx> {
        self.bindings.get(type_).map(|item| item.get_tx())
    }

    #[inline(always)]
    pub fn get_binding(&self, type_: &str) -> Option<&BindingItem> {
        self.bindings.get(type_)
    }

    /// Lists all bindings.
    ///
    /// Returns an iterator over `(binding_type, binding_item)`.
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

/// A thin wrapper around the binding backend.
#[repr(transparent)]
pub struct BindingItem {
    backend: Arc<dyn BindingBackend>,
}

impl BindingItem {
    #[inline(always)]
    pub fn create_client(&self) -> Box<dyn BindingClient> {
        self.backend.create_client()
    }

    #[inline(always)]
    pub fn get_tx(&self) -> BindingBackendTx {
        self.backend.get_tx()
    }
}
