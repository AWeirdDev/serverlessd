use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

use crate::runtime::{PodTrigger, WorkerTask, serverless::code_store::CodeStoreError};

#[derive(Debug)]
pub enum ServerlessTrigger {
    CreateWorker {
        name: String,
        reply: oneshot::Sender<Option<(usize, usize)>>,
    },

    SetUniversalWorkerName {
        name: String,
        locator: (usize, usize),
    },

    RemoveUniversalWorkerName {
        name: String,
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
