//! Bindings to extend worker capabilities.
//!
//! # Hierarchy
//! ```text
//! Backend ---(lower)--> Client ---(lower)--> JavaScript Interface
//! ```
//!
//! Additionally:
//!
//! - Binding **type**: The type/mechanism of the binding, for example: a key-value store.
//! - Binding **name**: The name of the binding to be used. It's user-defined for each worker.
//!     For example: `MY_BINDING`.
//!
//! For instance, you can have a binding **name** of `IMPORTANT_DATA` that links to the binding
//! **type** of a key-value store.

mod backend;
mod store;

pub mod ipc;

pub use backend::{
    BindingBackend, BindingBackendMessage, BindingBackendRx, BindingBackendTx,
    binding_backend_channel,
};

pub use store::BindingStore;
