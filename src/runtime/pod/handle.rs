use tokio::sync::oneshot;

use crate::runtime::{
    WorkerTask,
    pod::{PodTrigger, trigger::PodTx},
};

#[repr(transparent)]
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

        tracing::info!("waiting for pod to halt...");
        if !self.tx.send(PodTrigger::Halt { token }).await.is_ok() {
            tracing::error!("failed to halt pod");
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

    /// Create a worker. Returns `Some(worker_id)` if successful.
    pub async fn create_worker(&self, task: WorkerTask) -> Option<usize> {
        let (reply, receive) = oneshot::channel::<usize>();
        if !self.trigger(PodTrigger::CreateWorker { task, reply }).await {
            return None;
        }

        receive.await.ok()
    }

    #[inline(always)]
    #[must_use]
    pub async fn trigger(&self, trigger: PodTrigger) -> bool {
        self.tx.send(trigger).await.is_ok()
    }
}
