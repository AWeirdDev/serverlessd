use tokio::sync::mpsc;

use crate::runtime::{
    Pod,
    worker::{
        task::{WarmUpWorkerArgs, create_cancel_safe_task},
        trigger::{WorkerTrigger, WorkerTx},
    },
};

/// A handle to the serverless worker.
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct WorkerHandle {
    tx: WorkerTx,
}

impl WorkerHandle {
    /// Start a new worker.
    #[inline]
    pub fn start(pod: &Pod) -> Self {
        let (tx, rx) = mpsc::channel::<WorkerTrigger>(64);

        let monitor_handle = pod.monitor.clone();

        pod.tasks
            .spawn_local(create_cancel_safe_task(WarmUpWorkerArgs {
                pod_tx: pod.tx.clone(),
                worker_tx: tx.clone(),
                worker_rx: rx,
                monitor_handle,
            }));

        Self { tx }
    }

    /// Trigger.
    ///
    /// Returns `false` if the channel is closed.
    #[inline(always)]
    #[must_use]
    pub async fn trigger(&self, trigger: WorkerTrigger) -> bool {
        self.tx.send(trigger).await.is_ok()
    }
}
