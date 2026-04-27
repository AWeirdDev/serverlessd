use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

type BindingName = String;

#[repr(packed)]
#[derive(bon::Builder)]
pub struct BindingBackendMessage {
    /// The name of the worker, for identification purposes.
    pub worker: String,

    /// The data.
    pub data: ijson::IValue,

    /// Replier.
    pub replier: oneshot::Sender<ijson::IValue>,
}

/// Backend transmitter, for receiving requests from **multiple unique workers**.
pub type BindingBackendTx = mpsc::UnboundedSender<BindingBackendMessage>;

/// Backend receiver (intermediate), for receiving requests from **multiple unique workers**.
pub type BindingBackendRx = mpsc::UnboundedReceiver<BindingBackendMessage>;

/// A binding backend.
#[async_trait]
pub trait BindingBackend {
    async fn start(&mut self);
}
