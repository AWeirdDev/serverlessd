use tokio::sync::oneshot;

use crate::runtime::{
    PodTrigger, WorkerTask, WorkerTrigger,
    serverless::trigger::{ServerlessTrigger, ServerlessTx},
};

#[repr(transparent)]
pub struct ServerlessHandle {
    tx: ServerlessTx,
}

impl ServerlessHandle {
    #[inline(always)]
    pub(super) fn new(tx: ServerlessTx) -> Self {
        Self { tx }
    }

    /// Notifies the serverless runtime to create a worker.
    pub async fn create_worker(&self, task: WorkerTask) -> Option<(usize, usize)> {
        let (reply, receive) = oneshot::channel();
        self.tx
            .send(ServerlessTrigger::CreateWorker { task, reply })
            .await
            .ok()?;

        let Ok(result) = receive.await else {
            return None;
        };

        result
    }

    /// Helper for triggering worker.
    pub async fn trigger_worker(
        &self,
        pod_id: usize,
        worker_id: usize,
        trigger: WorkerTrigger,
    ) -> Option<()> {
        self.tx
            .send(ServerlessTrigger::ToPod {
                id: pod_id,
                trigger: PodTrigger::ToWorker {
                    id: worker_id,
                    trigger,
                },
            })
            .await
            .ok()
    }
}
