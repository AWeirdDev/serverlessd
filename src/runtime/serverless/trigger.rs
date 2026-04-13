use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

use crate::runtime::{PodTrigger, serverless::code_store::CodeStoreError};

#[derive(Debug, thiserror::Error)]
pub enum CreateWorkerError {
    #[error("unknown worker {0:?}")]
    UnknownWorker(String),

    #[error("cannot create task")]
    CannotCreateTask,
}

#[derive(Debug)]
pub enum ServerlessTrigger {
    CreateWorker {
        name: String,
        reply: oneshot::Sender<Result<(usize, usize), CreateWorkerError>>,
    },

    UploadWorkerCode {
        name: String,
        code: Bytes,
        reply: oneshot::Sender<Option<CodeStoreError>>,
    },

    RemoveWorkerCode {
        name: String,
    },

    ToPod {
        id: usize,
        trigger: PodTrigger,
    },
}

pub(super) type ServerlessTx = mpsc::Sender<ServerlessTrigger>;
pub(super) type ServerlessRx = mpsc::Receiver<ServerlessTrigger>;
