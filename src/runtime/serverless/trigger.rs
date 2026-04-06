use tokio::sync::{mpsc, oneshot};

use crate::runtime::{PodTrigger, WorkerTask, serverless::code_store::CodeStoreError};

#[derive(Debug)]
pub enum ServerlessTrigger {
    CreateWorker {
        task: WorkerTask,
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
        code: String,
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
