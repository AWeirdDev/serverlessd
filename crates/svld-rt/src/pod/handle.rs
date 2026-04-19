use tokio::sync::oneshot;

use crate::{
    WorkerTask, WorkerTrigger,
    pod::{PodTrigger, trigger::PodTx},
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
    ///
    /// # Returns
    /// A boolean indicating whether the operation was successful.
    #[must_use]
    pub async fn kill(&self) -> bool {
        let (token, recv) = oneshot::channel();

        if !self.tx.send(PodTrigger::Kill { token }).await.is_ok() {
            tracing::error!("failed to kill pod");
            return false;
        }

        recv.await.is_ok()
    }

    /// Checks whether or not this pod has any vacancies to run
    /// a task.
    pub async fn has_vacancies(&self) -> bool {
        let (reply, recv) = oneshot::channel();
        if !self.trigger(PodTrigger::CheckVacancies { reply }).await {
            return false;
        }

        recv.await.ok().unwrap_or(false)
    }

    /// Creates and warms up a worker.
    ///
    /// # Returns
    /// `Some(worker_id)` if successful.
    pub async fn create_and_warmup_worker(&self) -> Option<usize> {
        let (reply, receive) = oneshot::channel::<usize>();
        if !self.trigger(PodTrigger::WarmUpWorker { reply }).await {
            return None;
        }

        receive.await.ok()
    }

    /// Assigns a worker a task.
    ///
    /// # Returns
    /// A boolean indicating whether the operation was successful.
    #[must_use]
    #[inline]
    pub async fn assign_worker_task(&self, id: usize, task: WorkerTask) -> bool {
        self.trigger(PodTrigger::ToWorker {
            id,
            trigger: WorkerTrigger::StartTask { id, task },
        })
        .await
    }

    /// Marks a worker as "vacant," meaning it is now ready to be
    /// assigned with new tasks.
    #[must_use]
    #[inline]
    pub async fn remove_worker(&self, id: usize) -> bool {
        let success = self.trigger(PodTrigger::RemoveWorker { id }).await;
        if success { true } else { false }
    }

    #[inline(always)]
    #[must_use]
    pub async fn trigger(&self, trigger: PodTrigger) -> bool {
        self.tx.send(trigger).await.is_ok()
    }
}

impl std::fmt::Debug for PodHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PodHandle")
    }
}
