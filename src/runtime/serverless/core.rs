use std::net::SocketAddr;

use bytes::Bytes;
use tokio::{
    sync::mpsc,
    task::{self, JoinHandle},
};
use v8::{Platform, SharedRef};

use crate::runtime::{
    PodHandle, WorkerTask,
    serverless::{
        code_store::{CodeStore, CodeStoreError},
        handle::ServerlessHandle,
        task::serverless_task,
    },
};

/// The serverless runtime, as an application.
///
/// Example:
/// ```rs
/// let serverless = Serverless::new(
///     10, // the number of threads you need
///     10, // the number of workers per thread
/// );
/// ```
#[derive(Debug)]
pub struct Serverless {
    pub(super) n_threads: usize,
    pub(super) n_workers: usize,

    pub(super) code_store: CodeStore,

    // why the fuck is this super fucking big???
    // like, fucking 16 bytes
    // or whatever, if you're happy with it
    pub(super) platform: SharedRef<Platform>,
    pub(super) pods: Vec<PodHandle>,
}

impl Serverless {
    /// Create a serverless runtime.
    pub fn new(n_threads: usize, n_workers: usize) -> Self {
        // we gotta initialize the platform first
        let platform = {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform.clone());
            v8::V8::initialize();

            platform
        };

        let pods = Vec::with_capacity(n_threads);
        let code_store = CodeStore::new();

        Self {
            n_threads,
            n_workers,
            code_store,
            platform,
            pods,
        }
    }

    /// Create a serverless runtime for one worker only.
    #[inline]
    pub fn new_one() -> Self {
        Self::new(1, 1)
    }

    /// Starts the serverless runtime.
    #[inline]
    #[must_use]
    pub fn start(self, addr: SocketAddr, secret: String) -> (ServerlessHandle, JoinHandle<()>) {
        let (tx, rx) = mpsc::channel(512);
        let handle = task::spawn(serverless_task(
            self,
            rx,
            addr,
            ServerlessHandle::new(tx.clone()),
            secret,
        ));

        (ServerlessHandle::new(tx), handle)
    }

    /// Gets a clone of the shared reference from [`v8`].
    #[inline(always)]
    pub fn get_platform(&self) -> SharedRef<Platform> {
        self.platform.clone()
    }

    #[inline]
    async fn find_vacancy(&self) -> Option<usize> {
        for (idx, pod) in self.pods.iter().enumerate() {
            if pod.has_vacancies().await {
                tracing::info!("found pod {} has a vacancy!", idx);
                return Some(idx);
            }
        }
        None
    }

    #[inline(always)]
    pub(super) fn get_pod(&self, id: usize) -> Option<&PodHandle> {
        self.pods.get(id)
    }

    /// Stop all pods.
    pub(super) async fn kill(&mut self) {
        for pod in self.pods.drain(..) {
            if !pod.kill().await {
                tracing::error!("failed to halt");
            }
        }
    }

    /// Stop a pod.
    #[allow(unused)]
    pub(super) async fn kill_pod(&mut self, id: usize) -> bool {
        if let Some(pod) = self.pods.get_mut(id) {
            pod.kill().await
        } else {
            false
        }
    }

    /// Finds vacancies from pods, then create a worker
    /// within the pod, eventually returning `Some()` tuple containing:
    ///
    /// `(pod_id: usize, pod_worker_id: usize)`
    ///
    /// Under one of these conditions, `None` is returned:
    /// - No vacancies available
    /// - Failed to trigger pod
    /// - Failed to receive worker id under the designated pod
    #[must_use]
    pub(super) async fn create_worker_task(&self, task: WorkerTask) -> Option<(usize, usize)> {
        tracing::info!("finding vacancy");
        let pod_id = self.find_vacancy().await?;
        tracing::info!("found vacancy! {pod_id}");

        let pod = unsafe { self.pods.get(pod_id).unwrap_unchecked() };
        let pod_worker_id = pod.create_and_warmup_worker().await?;

        let success = pod.assign_worker_task(pod_worker_id, task).await;
        if success {
            Some((pod_id, pod_worker_id))
        } else {
            None
        }
    }

    #[inline]
    pub(super) async fn upload_worker_code(
        &mut self,
        name: String,
        code: Bytes,
    ) -> Result<(), CodeStoreError> {
        self.code_store.upload_worker_code(name, code).await
    }

    #[inline]
    pub(super) async fn remove_worker_code(&mut self, name: &str) {
        self.code_store.remove_worker_code(name).await;
    }
}
