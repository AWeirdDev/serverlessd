//! Bindings to extend worker capabilities.

mod backend;
mod general;

pub mod kv;

pub use backend::{
    BindingBackend, BindingBackendMessage, BindingBackendRx, BindingBackendTx,
    binding_backend_channel,
};
