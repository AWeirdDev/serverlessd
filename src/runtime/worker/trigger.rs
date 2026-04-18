use svld_blocks::Replier;

use tokio::sync::{mpsc, oneshot};

use crate::runtime::WorkerTask;

#[derive(Debug)]
#[allow(unused)]
pub enum WorkerTrigger {
    /// Start a worker task.
    StartTask {
        id: usize,
        task: WorkerTask,
    },

    /// Stop a task from running.
    HaltTask,

    Http {
        reply: Replier,
    },

    /// Request an isolate refresh for later use.
    Refresh,

    /// Kill the isolate completely.
    ///
    /// # Warning
    /// You may not kill the worker if it's not in
    /// sleeping state. Use `WorkerTrigger::HaltTask` first.
    Kill {
        token: oneshot::Sender<()>,
    },
}

pub type WorkerTx = mpsc::Sender<WorkerTrigger>;
pub(super) type WorkerRx = mpsc::Receiver<WorkerTrigger>;
