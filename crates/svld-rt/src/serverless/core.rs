use std::sync::Arc;

use bon::bon;
use bytes::Bytes;

use v8::{Platform, SharedRef};

use crate::{
    bindings::BindingStore,
    models::WorkerConfig,
    pod::PodHandle,
    serverless::{
        code_store::{CodeStore, CodeStoreError},
        space::SpaceState,
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
#[allow(unused)]
pub struct Serverless {
    pub n_pods: usize,
    pub n_workers: usize,
    pub platform: SharedRef<Platform>,
    pub binding_store: Arc<BindingStore>,
    pub code_store: CodeStore,

    pods: Vec<PodHandle>,
    space: SpaceState,
}

#[bon]
impl Serverless {
    /// Create a serverless runtime.
    #[builder]
    pub fn new(
        n_pods: usize,
        n_workers: usize,
        parent: Option<&str>,
        workers_path: Option<&str>,
        binding_store: Arc<BindingStore>,
    ) -> Self {
        // we gotta initialize the platform first
        let platform = {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform.clone());
            v8::V8::initialize();

            platform
        };

        let pods = Vec::with_capacity(n_pods);
        let code_store = CodeStore::new(
            parent.unwrap_or(".serverlessd"),
            workers_path.unwrap_or("workers"),
        );

        let space = SpaceState::new(n_pods, n_workers);

        Self {
            n_pods,
            n_workers,
            code_store,
            platform,
            pods,
            binding_store,
            space,
        }
    }

    /// Gets a clone of the shared reference from [`v8`].
    #[inline(always)]
    pub fn get_platform(&self) -> SharedRef<Platform> {
        self.platform.clone()
    }

    /// Find vacancy.
    ///
    /// # Returns
    /// `Some(((pod_handle, monitor_handke), (pod_id, pod_worker_id)))` if found.
    #[inline]
    pub async fn find_vacancy_and_warmup(&self) -> Option<(PodHandle, usize, usize)> {
        if let Some(pod_id) = self.space.request_use_space() {
            let pod = self.pods.get(pod_id).unwrap();

            let pod_worker_id = pod.create_and_warmup_worker().await.unwrap();
            Some((pod.clone(), pod_id, pod_worker_id))
        } else {
            None
        }
    }

    /// Releases count for a space in a pod.
    pub fn release_pod_space(&self, pod_id: usize) {
        let _ = self.space.release_pod_space(pod_id);
    }

    #[inline(always)]
    pub fn get_pod(&self, id: usize) -> Option<&PodHandle> {
        self.pods.get(id)
    }

    /// Pushes a pod handle to the serverless runtime.
    #[inline(always)]
    pub fn push_pod(&mut self, pod_handle: PodHandle) {
        self.pods.push(pod_handle);
    }

    /// Stops all pods.
    pub async fn kill(&mut self) {
        for pod in self.pods.drain(..) {
            if pod.kill().await.is_err() {
                tracing::error!("failed to halt");
            }
        }
    }

    /// Stops a pod.
    pub async fn kill_pod(&mut self, id: usize) {
        if let Some(pod) = self.pods.get_mut(id) {
            let _ = pod.kill().await;
        }
    }

    #[inline(always)]
    pub async fn upload_worker(
        &mut self,
        code: Bytes,
        config: WorkerConfig,
    ) -> Result<(), CodeStoreError> {
        self.code_store.upload_worker(code, config).await
    }

    #[inline(always)]
    pub async fn remove_worker_code(&mut self, name: &str) {
        self.code_store.remove_worker(name).await;
    }
}
