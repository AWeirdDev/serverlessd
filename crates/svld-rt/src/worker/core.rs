use tokio::sync::mpsc;

use crate::{
    pod::Pod,
    triggers::{WorkerTrigger, WorkerTx},
    worker::task::{WarmUpWorkerArgs, create_cancel_safe_task},
};

/// A referenced-counted handle to the serverless worker.
#[derive(Clone)]
#[repr(transparent)]
pub struct Worker {
    pub(super) tx: WorkerTx,
}

impl Worker {
    /// Start a new worker.
    #[inline]
    pub fn start_worker(pod: &Pod) -> Self {
        let (tx, rx) = mpsc::channel::<WorkerTrigger>(1);
        let monitor_handle = pod.monitor.clone();

        pod.tasks.spawn_local(create_cancel_safe_task(
            WarmUpWorkerArgs::builder()
                .pod_tx(pod.tx.clone())
                .worker_tx(tx.clone())
                .worker_rx(rx)
                .monitor_handle(monitor_handle)
                .platform(pod.get_platform())
                .binding_store(pod.binding_store.clone())
                .build(),
        ));

        Self { tx }
    }

    /// Trigger.
    ///
    /// Returns `false` if the channel is closed.
    #[inline(always)]
    #[must_use]
    pub async fn trigger(&self, trigger: WorkerTrigger) -> Result<(), WorkerTriggerError> {
        self.tx
            .send(trigger)
            .await
            .map_err(|_| WorkerTriggerError::ChannelClosed)
    }
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorkerHandle")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerTriggerError {
    #[error("the channel to the worker has closed")]
    ChannelClosed,
}
