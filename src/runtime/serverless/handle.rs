use bytes::Bytes;
use tokio::sync::oneshot;

use crate::runtime::{
    PodTrigger, WorkerTrigger,
    serverless::{
        code_store::CodeStoreError,
        trigger::{ServerlessTrigger, ServerlessTx},
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
    pub async fn create_worker(&self, name: String) -> Option<(usize, usize)> {
        tracing::info!("creating worker for {}", name);

        let (reply, receive) = oneshot::channel();
        self.tx
            .send(ServerlessTrigger::CreateWorker { name, reply })
            .await
            .ok()?;

        let Ok(result) = receive.await else {
            return None;
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
    pub async fn remove_worker(&self, name: String) -> Option<()> {
        self.trigger(ServerlessTrigger::RemoveWorkerCode { name })
            .await
    }

    #[inline]
    #[must_use]
    pub async fn send_http_to_worker(&self, pod: usize, wrk: usize) -> Option<String> {
        let (reply, recv) = oneshot::channel();
        self.trigger(ServerlessTrigger::ToPod {
            id: pod,
            trigger: PodTrigger::ToWorker {
                id: wrk,
                trigger: WorkerTrigger::Http { reply },
            },
        })
        .await?;

        recv.await.ok()
    }

    /// Trigger the serverless runtime.
    #[inline]
    #[must_use]
    pub async fn trigger(&self, trigger: ServerlessTrigger) -> Option<()> {
        self.tx.send(trigger).await.ok()
    }
}
