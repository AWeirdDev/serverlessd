use crate::blocks::Reply;
use bytes::Bytes;
use tokio::sync::oneshot;

use crate::{
    PodTrigger, WorkerTrigger,
    serverless::{
        code_store::CodeStoreError,
        error::CreateWorkerError,
        trigger::{ServerlessTrigger, ServerlessTx},
    },
};

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
    #[must_use]
    pub async fn create_worker(&self, name: String) -> Result<(usize, usize), CreateWorkerError> {
        tracing::info!("GET worker/{}", name);

        let (reply, receive) = oneshot::channel();
        self.tx
            .send(ServerlessTrigger::CreateWorker { name, reply })
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
    #[must_use]
    pub async fn halt_task(&self, pod_id: usize, worker_id: usize) -> bool {
        self.trigger(ServerlessTrigger::ToPod {
            id: pod_id,
            trigger: PodTrigger::ToWorker {
                id: worker_id,
                trigger: WorkerTrigger::HaltTask,
            },
        })
        .await
        .is_some()
    }

    /// Upload worker code.
    #[inline]
    #[must_use]
    pub async fn upload_worker(&self, name: String, code: Bytes) -> Option<CodeStoreError> {
        let (reply, recv) = oneshot::channel();
        self.trigger(ServerlessTrigger::UploadWorkerCode { name, code, reply })
            .await?;

        recv.await.ok()?
    }

    /// Remove worker code.
    #[inline]
    pub async fn remove_worker_code(&self, name: String) -> Option<()> {
        self.trigger(ServerlessTrigger::RemoveWorkerCode { name })
            .await
    }

    #[inline]
    #[must_use]
    pub async fn send_http_to_worker(&self, pod: usize, wrk: usize) -> Option<Reply> {
        let (reply, recv) = oneshot::channel();
        self.trigger(ServerlessTrigger::ToPod {
            id: pod,
            trigger: PodTrigger::ToWorker {
                id: wrk,
                trigger: WorkerTrigger::Http { reply },
            },
        })
        .await?;

        let result = recv.await.ok()?;

        Some(result)
    }

    /// Trigger the serverless runtime.
    #[inline]
    #[must_use]
    pub async fn trigger(&self, trigger: ServerlessTrigger) -> Option<()> {
        self.tx.send(trigger).await.ok()
    }
}

impl std::fmt::Debug for ServerlessHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ServerlessHandle")
    }
}
