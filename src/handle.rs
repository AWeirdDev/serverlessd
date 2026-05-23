use bytes::Bytes;
use tokio::sync::oneshot;

use svld_rt::{
    blocks::Reply,
    models::WorkerHttpRequest,
    serverless::{CodeStoreError, CreateWorkerError},
    triggers::{PodTrigger, WorkerTrigger},
};

use crate::trigger::{ServerlessTrigger, ServerlessTx};

#[repr(transparent)]
pub struct ServerlessHandle {
    tx: ServerlessTx,
}

impl ServerlessHandle {
    #[inline(always)]
    pub fn new(tx: ServerlessTx) -> Self {
        Self { tx }
    }

    /// Notifies the serverless runtime to create a worker.
    pub async fn create_worker_task(
        &self,
        name: String,
    ) -> Result<(usize, usize), CreateWorkerError> {
        let (reply, receive) = oneshot::channel();
        self.trigger(ServerlessTrigger::CreateWorkerTask { name, reply })
            .await
            .map_err(|_| {
                CreateWorkerError::CannotCreateTask(
                    "cannot notify serverless task loop to create worker".to_string(),
                )
            })?;

        let Ok(result) = receive.await else {
            return Err(CreateWorkerError::CannotCreateTask(
                "cannot receive from serverless worker; the channel has possibly closed"
                    .to_string(),
            ));
        };

        result
    }

    /// Halts a task for a worker in a pod.
    ///
    /// After this, the worker will mark itself as "sleeping."
    #[inline]
    pub async fn halt_task_and_clear_space(
        &self,
        pod_id: usize,
        worker_id: usize,
    ) -> Result<(), ServerlessTriggerError> {
        self.trigger(ServerlessTrigger::ToPod {
            id: pod_id,
            trigger: PodTrigger::ToWorker {
                id: worker_id,
                trigger: WorkerTrigger::HaltTask,
            },
        })
        .await
    }

    /// Upload worker code.
    #[inline]
    pub async fn upload_worker(&self, name: String, code: Bytes) -> Result<(), UploadWorkerError> {
        let (reply, recv) = oneshot::channel();
        self.trigger(ServerlessTrigger::UploadWorkerCode { name, code, reply })
            .await?;

        recv.await
            .map_err(|_| UploadWorkerError::Trigger(ServerlessTriggerError::ChannelClosed))??;
        Ok(())
    }

    #[inline]
    pub async fn send_http_to_worker(
        &self,
        pod: usize,
        wrk: usize,
        request: WorkerHttpRequest,
    ) -> Result<Reply, ServerlessTriggerError> {
        let (reply, recv) = oneshot::channel();
        self.trigger(ServerlessTrigger::ToPod {
            id: pod,
            trigger: PodTrigger::ToWorker {
                id: wrk,
                trigger: WorkerTrigger::Http { reply, request },
            },
        })
        .await?;

        let result = recv
            .await
            .map_err(|_| ServerlessTriggerError::ChannelClosed)?;

        Ok(result)
    }

    /// Trigger the serverless runtime.
    #[inline]
    #[must_use]
    pub async fn trigger(&self, trigger: ServerlessTrigger) -> Result<(), ServerlessTriggerError> {
        self.tx
            .send(trigger)
            .await
            .map_err(|_| ServerlessTriggerError::ChannelClosed)
    }
}

impl std::fmt::Debug for ServerlessHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ServerlessHandle")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerlessTriggerError {
    #[error("the channel to the serverless runtime has closed")]
    ChannelClosed,
}

#[derive(Debug, thiserror::Error)]
pub enum UploadWorkerError {
    #[error(transparent)]
    Trigger(#[from] ServerlessTriggerError),

    #[error(transparent)]
    CodeStore(#[from] CodeStoreError),
}
