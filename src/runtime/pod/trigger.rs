use tokio::sync::{mpsc, oneshot};

use crate::runtime::WorkerTrigger;

#[derive(Debug)]
pub enum PodTrigger {
    CheckVacancies {
        reply: oneshot::Sender<bool>,
    },

    /// Send data to a worker.
    ToWorker {
        id: usize,
        trigger: WorkerTrigger,
    },

    /// Kill all workers in the pod.
    Kill {
        token: oneshot::Sender<()>,
    },

    /// Warm up a worker.
    ///
    /// You can get then get the ID of the warmed worker.
    WarmUpWorker {
        reply: oneshot::Sender<usize>,
    },

    /// Remove a worker.
    ///
    /// At this point, the worker will be removed from the
    /// array, and can no longer be accessed.
    RemoveWorker {
        id: usize,
    },
}

pub type PodTx = mpsc::Sender<PodTrigger>;
pub(super) type PodRx = mpsc::Receiver<PodTrigger>;
