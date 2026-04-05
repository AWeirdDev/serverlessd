use std::net::SocketAddr;

use tokio::{
    sync::mpsc,
    task::{self, JoinHandle},
};
use v8::{Platform, SharedRef};

use crate::runtime::{
    PodHandle, WorkerTask,
    serverless::{handle::ServerlessHandle, task::serverless_task},
};

/// The serverless runtime.
///
/// Example:
/// ```rs
/// let serverless = Serverless::start(
///     10, // the number of threads you need
///     10, // the number of workers per thread
/// )
/// ```
pub struct Serverless {
    // why the fuck is this super fucking big???
    // like, fucking 16 bytes
    // or whatever, if you're happy with it
    pub(super) platform: SharedRef<Platform>,
    pub(super) pods: Vec<PodHandle>,

    pub(super) n_threads: usize,
    pub(super) n_workers: usize,
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

        Self {
            platform,
            pods,
            n_threads,
            n_workers,
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
    pub fn start(self, addr: SocketAddr) -> (ServerlessHandle, JoinHandle<()>) {
        let (tx, rx) = mpsc::channel(512);
        let handle = task::spawn(serverless_task(self, rx, addr));

        (ServerlessHandle::new(tx), handle)
    }

    /// Get the platform from [`v8`].
    #[inline(always)]
    pub fn get_platform(&self) -> SharedRef<Platform> {
        self.platform.clone()
    }

    #[inline]
    async fn find_vancancy(&self) -> Option<usize> {
        for (idx, pod) in self.pods.iter().enumerate() {
            if pod.has_vacancies().await {
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
    pub(super) async fn halt(&mut self) {
        for pod in self.pods.drain(..) {
            if !pod.halt().await {
                tracing::error!("failed to halt");
            }
        }
    }

    /// Stop a pod.
    #[allow(unused)]
    pub(super) async fn halt_pod(&mut self, id: usize) -> bool {
        if let Some(pod) = self.pods.get_mut(id) {
            pod.halt().await
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
    pub(super) async fn create_worker(&self, task: WorkerTask) -> Option<(usize, usize)> {
        let pod_id = self.find_vancancy().await?;
        let pod = unsafe { self.pods.get(pod_id).unwrap_unchecked() };

        let pod_worker_id = pod.create_worker(task).await?;
        Some((pod_id, pod_worker_id))
    }
}
