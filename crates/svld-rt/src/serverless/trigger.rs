use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

use crate::{PodTrigger, serverless::code_store::CodeStoreError};

#[derive(Debug, thiserror::Error)]
pub enum CreateWorkerError {
    #[error("unknown worker {0:?}")]
    UnknownWorker(String),

    #[error("cannot create task: {0}")]
    CannotCreateTask(String),
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

pub type ServerlessTx = mpsc::Sender<ServerlessTrigger>;
pub type ServerlessRx = mpsc::Receiver<ServerlessTrigger>;
