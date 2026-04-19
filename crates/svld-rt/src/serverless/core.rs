use bytes::Bytes;

use v8::{Platform, SharedRef};

use crate::{
    PodHandle,
    serverless::code_store::{CodeStore, CodeStoreError},
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
    pub n_threads: usize,
    pub n_workers: usize,

    pub code_store: CodeStore,

    // why the fuck is this super fucking big???
    // like, fucking 16 bytes
    // or whatever, if you're happy with it
    pub platform: SharedRef<Platform>,
    pub pods: Vec<PodHandle>,
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
        for (pod_id, pod) in self.pods.iter().enumerate() {
            if pod.has_vacancies().await {
                tracing::info!("found pod {} has a vacancy!", pod_id);

                let pod_worker_id = pod.create_and_warmup_worker().await?;

                return Some((pod.clone(), pod_id, pod_worker_id));
            }
        }
        None
    }

    #[inline(always)]
    pub fn get_pod(&self, id: usize) -> Option<&PodHandle> {
        self.pods.get(id)
    }

    /// Push a pod handle to the serverless runtime.
    #[inline(always)]
    pub fn push_pod(&mut self, pod_handle: PodHandle) {
        self.pods.push(pod_handle);
    }

    /// Stop all pods.
    pub async fn kill(&mut self) {
        for pod in self.pods.drain(..) {
            if !pod.kill().await {
                tracing::error!("failed to halt");
            }
        }
    }

    /// Stop a pod.
    #[allow(unused)]
    pub async fn kill_pod(&mut self, id: usize) -> bool {
        if let Some(pod) = self.pods.get_mut(id) {
            pod.kill().await
        } else {
            false
        }
    }

    #[inline]
    pub async fn upload_worker_code(
        &mut self,
        name: String,
        code: Bytes,
    ) -> Result<(), CodeStoreError> {
        self.code_store.upload_worker_code(name, code).await
    }

    #[inline]
    pub async fn remove_worker_code(&mut self, name: &str) {
        self.code_store.remove_worker_code(name).await;
    }
}
