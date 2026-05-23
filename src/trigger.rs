use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

use svld_rt::{
    serverless::{CodeStoreError, CreateWorkerError},
    triggers::PodTrigger,
};

#[derive(Debug)]
#[allow(dead_code)]
pub enum ServerlessTrigger {
    /// Creates a worker.
    CreateWorkerTask {
        name: String,
        reply: oneshot::Sender<Result<(usize, usize), CreateWorkerError>>,
    },

    /// Uploads worker code.
    UploadWorkerCode {
        name: String,
        code: Bytes,
        reply: oneshot::Sender<Result<(), CodeStoreError>>,
    },

    RemoveWorkerCode {
        name: String,
    },

    ToPod {
        id: usize,
        trigger: PodTrigger,
    },
}

pub type ServerlessTx = mpsc::Sender<ServerlessTrigger>;
pub type ServerlessRx = mpsc::Receiver<ServerlessTrigger>;
