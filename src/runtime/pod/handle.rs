use tokio::sync::oneshot;

use crate::runtime::{
    WorkerTask, WorkerTrigger,
    pod::{PodTrigger, trigger::PodTx},
};

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

    /// Stop the pod task.
    ///
    /// Returns `false` if failed.
    #[must_use]
    pub async fn halt(&self) -> bool {
        let (token, recv) = oneshot::channel();

        tracing::info!("waiting for pod to be killed...");
        if !self.tx.send(PodTrigger::Kill { token }).await.is_ok() {
            tracing::error!("failed to kill pod");
            return false;
        }

        recv.await.is_ok()
    }

    pub async fn has_vacancies(&self) -> bool {
        let (reply, recv) = oneshot::channel();
        if !self.trigger(PodTrigger::CheckVacancies { reply }).await {
            return false;
        }

        recv.await.ok().unwrap_or(false)
    }

    /// Create and warm up a worker. Returns `Some(worker_id)` if successful.
    pub async fn create_worker(&self) -> Option<usize> {
        let (reply, receive) = oneshot::channel::<usize>();
        if !self.trigger(PodTrigger::WarmUpWorker { reply }).await {
            return None;
        }

        receive.await.ok()
    }

    /// Assign a worker a task. Returns `true` if successful.
    #[must_use]
    pub async fn assign_worker_task(&self, id: usize, task: WorkerTask) -> bool {
        let success = self
            .trigger(PodTrigger::ToWorker {
                id,
                trigger: WorkerTrigger::StartTask { id, task },
            })
            .await;
        if success { true } else { false }
    }

    #[must_use]
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
