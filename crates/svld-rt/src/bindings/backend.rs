use tokio::sync::{mpsc, oneshot};

#[repr(packed)]
#[derive(bon::Builder)]
pub struct BindingBackendMessage {
    /// The name of the worker, for identification purposes.
    pub worker: String,

    /// The function to call.
    pub function_name: String,

    /// The data.
    pub args: ijson::IValue,

    /// Replier.
    pub replier: oneshot::Sender<ijson::IValue>,
}

/// Backend transmitter, for receiving requests from **multiple unique workers**.
pub type BindingBackendTx = mpsc::UnboundedSender<BindingBackendMessage>;

/// Backend receiver (intermediate), for receiving requests from **multiple unique workers**.
pub type BindingBackendRx = mpsc::UnboundedReceiver<BindingBackendMessage>;

/// A binding backend abstraction.
pub trait BindingBackend: Send + Sync + 'static {
    /// Gets a handle to this backend.
    fn get_tx(&self) -> BindingBackendTx;

    /// Creates a client from the env name for the worker.
    fn create_client(&self) -> Box<dyn BindingClient>;
}

/// Creates a binding backend channel.
#[inline(always)]
pub fn binding_backend_channel() -> (BindingBackendTx, BindingBackendRx) {
    mpsc::unbounded_channel()
}

/// Represents a binding client.
///
/// # Safety
/// `Send + Sync + 'static`.
pub trait BindingClient: Send + Sync + 'static {
    /// Creates the JavaScript interface of the binding.
    fn create_interface<'s>(
        &self,
        scope: &v8::PinScope<'s, '_>,
    ) -> Option<v8::Local<'s, v8::Value>>;
}
