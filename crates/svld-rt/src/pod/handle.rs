use tokio::sync::oneshot;

use crate::{
    WorkerTask,
    triggers::{PodTrigger, PodTx, WorkerTrigger},
};

/// A handle for interacting with the `Pod` via message passing.
#[repr(transparent)]
#[derive(Clone)]
pub struct PodHandle {
    tx: PodTx,
}

impl PodHandle {
    #[inline(always)]
    pub fn new(tx: PodTx) -> Self {
        Self { tx }
    }

    /// Kills the pod.
    ///
    /// When sent, the pod management thread will do the following
    /// for each worker within it:
    ///
    /// 1. Halts the existing task (if not at sleeping state)
    /// 2. Kills the worker
    ///
    /// This enables graceful canceling.
    pub async fn kill(&self) -> Result<(), PodTriggerError> {
        let (token, recv) = oneshot::channel();

        if let Err(err) = self.trigger(PodTrigger::Kill { token }).await {
            tracing::error!("failed to kill pod");
            return Err(err);
        }

        recv.await.map_err(|_| PodTriggerError::ChannelClosed)
    }

    /// Checks whether or not this pod has any vacancies to run a task.
    pub async fn has_vacancies(&self) -> bool {
        let (reply, recv) = oneshot::channel();
        if self
            .trigger(PodTrigger::CheckVacancies { reply })
            .await
            .is_err()
        {
            return false;
        }

        recv.await.ok().unwrap_or(false)
    }

    /// Creates and warms up a worker.
    pub async fn create_and_warmup_worker(&self) -> Result<usize, PodTriggerError> {
        let (reply, receive) = oneshot::channel::<usize>();
        self.trigger(PodTrigger::WarmUpWorker { reply }).await?;

        receive.await.map_err(|_| PodTriggerError::ChannelClosed)
    }

    /// Assigns a task to a worker.
    #[inline]
    pub async fn assign_worker_task(&self, id: usize, task: WorkerTask) {
        let _ = self
            .trigger(PodTrigger::ToWorker {
                id,
                trigger: WorkerTrigger::StartTask { id, task },
            })
            .await;
    }

    /// Marks a worker as "vacant," meaning it is now ready to be
    /// assigned with new tasks.
    #[inline]
    pub async fn remove_worker(&self, id: usize) -> Result<(), PodTriggerError> {
        self.trigger(PodTrigger::RemoveWorker { id }).await
    }

    #[inline]
    pub async fn trigger(&self, trigger: PodTrigger) -> Result<(), PodTriggerError> {
        self.tx
            .send(trigger)
            .await
            .map_err(|_| PodTriggerError::ChannelClosed)
    }
}

impl std::fmt::Debug for PodHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PodHandle")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PodTriggerError {
    #[error("the pod channel has closed.")]
    ChannelClosed,
}
