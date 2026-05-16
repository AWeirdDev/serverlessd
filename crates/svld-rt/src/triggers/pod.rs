use tokio::sync::{mpsc, oneshot};

use super::worker::WorkerTrigger;

#[derive(Debug)]
pub enum PodTrigger {
    /// Sends data to a worker.
    ToWorker { id: usize, trigger: WorkerTrigger },

    /// Kills all workers in the pod.
    Kill { token: oneshot::Sender<()> },

    /// Warms up a worker.
    ///
    /// You can get then get the ID of the warmed worker.
    ///
    /// # Safety
    /// You **must** check if there are any vacancies available first.
    WarmUpWorker { reply: oneshot::Sender<usize> },

    /// Mark a worker as sleeping.
    MarkWorkerAsSleeping { id: usize },

    /// Remove a worker.
    ///
    /// At this point, the worker will be removed from the
    /// array, and can no longer be accessed.
    RemoveWorker { id: usize },
}

pub type PodTx = mpsc::Sender<PodTrigger>;
pub type PodRx = mpsc::Receiver<PodTrigger>;
