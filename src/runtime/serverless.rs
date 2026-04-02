use std::sync::Arc;

use tokio::sync::RwLock;
use v8::{Platform, SharedRef};

use crate::runtime::{Pod, WorkerTask, pod_job};

pub struct Serverless {
    platform: SharedRef<Platform>,
    pods: Vec<Arc<RwLock<Pod>>>,
}

impl Serverless {
    pub fn new(n_threads: usize, n_workers: usize) -> Self {
        // we gotta initialize the platform first
        let platform = {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform.clone());
            v8::V8::initialize();

            platform
        };

        // now, we gotta start those threads
        // i know, this might be a bit not so memory efficient
        for _ in 0..n_threads {
            let pod = Arc::new(RwLock::new(Pod::new(n_workers)));
            std::thread::spawn(|| pod_job(pod));
        }

        Self {
            platform,
            pods: Vec::with_capacity(n_threads),
        }
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

    // #[inline]
    // pub async fn create_worker(&self, task: WorkerTask) -> Option<(usize, usize)> {
    //     let pod_id = self.find_vancancy().await?;
    //     let pod = unsafe { self.pods.get(pod_id).unwrap_unchecked() };

    //     let pod_worker_id = {
    //         let mut pod = pod.write().await;
    //         pod.create_worker(task)
    //     };

    //     Some((pod_id, pod_worker_id))
    // }
}
