use bytes::Bytes;
use svld_state_extensions::Reply;
use tokio::sync::oneshot;

use crate::runtime::{
    PodTrigger, WorkerTrigger,
    serverless::{
        code_store::CodeStoreError,
        trigger::{CreateWorkerError, ServerlessTrigger, ServerlessTx},
    },
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
    #[must_use]
    pub async fn create_worker(&self, name: String) -> Result<(usize, usize), CreateWorkerError> {
        tracing::info!("GET worker/{}", name);

        let (reply, receive) = oneshot::channel();
        self.tx
            .send(ServerlessTrigger::CreateWorker { name, reply })
            .await
            .map_err(|_| CreateWorkerError::CannotCreateTask)?;

        let Ok(result) = receive.await else {
            return Err(CreateWorkerError::CannotCreateTask);
        };

        result
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

        // we need to turn it into a sleeping state first
        let result = recv.await.ok()?;
        self.trigger(ServerlessTrigger::ToPod {
            id: pod,
            trigger: PodTrigger::ToWorker {
                id: wrk,
                trigger: WorkerTrigger::HaltTask,
            },
        })
        .await?;

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
