use tokio::sync::{mpsc, oneshot};

use crate::runtime::{WorkerTask, WorkerTrigger};

#[derive(Debug)]
pub enum PodTrigger {
    CheckVacancies {
        reply: oneshot::Sender<bool>,
    },

    /// Create a new worker within the pod.
    CreateWorker {
        task: WorkerTask,
        reply: oneshot::Sender<usize>,
    },

    #[allow(unused)]
    ToWorker {
        id: usize,
        trigger: WorkerTrigger,
    },

    /// Stops the pod.
    Halt {
        token: oneshot::Sender<()>,
    },
}

pub type PodTx = mpsc::Sender<PodTrigger>;
pub(super) type PodRx = mpsc::Receiver<PodTrigger>;
