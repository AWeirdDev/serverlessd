use std::sync::Arc;

use tokio::sync::{RwLock, oneshot};
use v8::{Platform, SharedRef};

use crate::runtime::{Pod, WorkerTask, pod::PodTrigger};

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
    platform: SharedRef<Platform>,
    pods: Vec<Arc<RwLock<Pod>>>,
}

impl Serverless {
    /// Start the serverless runtime with configuration.
    pub fn start(n_threads: usize, n_workers: usize) -> Self {
        // we gotta initialize the platform first
        let platform = {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform.clone());
            v8::V8::initialize();

            platform
        };

        let mut pods = Vec::with_capacity(n_threads);

        // now, we gotta start those threads
        // i know, this might be a bit not so memory efficient
        for _ in 0..n_threads {
            let pod = Pod::start(n_workers);
            pods.push(pod);
        }

        Self { platform, pods }
    }

    /// Get the platform from [`v8`].
    #[inline(always)]
    pub fn get_platform(&self) -> SharedRef<Platform> {
        self.platform.clone()
    }

    /// Start the serverless runtime for one worker only.
    #[inline]
    pub fn start_one() -> Self {
        Self::start(1, 1)
    }

    #[inline]
    pub async fn find_vancancy(&self) -> Option<usize> {
        for (idx, pod) in self.pods.iter().enumerate() {
            let pod = pod.read().await;
            if pod.has_vacancy() {
                return Some(idx);
            }
        }
        None
    }

    /// Stop all pods.
    pub async fn halt(&mut self) {
        for pod in self.pods.drain(..) {
            let mut pod = pod.write().await;
            let _ = pod.halt();
        }
    }

    /// Stop a pod.
    pub async fn halt_pod(&mut self, id: usize) -> bool {
        if let Some(pod) = self.pods.get_mut(id) {
            let mut pod = pod.write().await;
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
    pub async fn create_worker(&self, task: WorkerTask) -> Option<(usize, usize)> {
        let pod_id = self.find_vancancy().await?;
        let pod = unsafe { self.pods.get(pod_id).unwrap_unchecked() };

        let receive = {
            // NOTE:
            // DO NOT remove this block!
            // if you do, we're prone to dead locks.
            //
            // essentially, we trigger it, and immediately say fuh nawh,
            // you can have the handle
            // since the pod wants to create a worker within
            let pod = pod.read().await;
            let (reply, receive) = oneshot::channel::<usize>();
            if !pod.trigger(PodTrigger::CreateWorker { task, reply }).await {
                return None;
            }
            receive
        };

        let pod_worker_id = receive.await.ok()?;
        Some((pod_id, pod_worker_id))
    }
}
