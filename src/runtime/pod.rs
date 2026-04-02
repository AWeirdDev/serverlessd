use std::sync::Arc;

use tokio::{sync::RwLock, sync::mpsc};
use tokio_util::task::TaskTracker;

use crate::runtime::{Worker, WorkerTask};

/// A thread containing multiple workers.
pub struct Pod {
    workers: Vec<Option<Worker>>,
    vacancies: Vec<usize>,
    pub(super) tasks: TaskTracker,
}

impl Pod {
    pub fn new(n_workers: usize) -> Self {
        Self {
            workers: Vec::with_capacity(n_workers),
            vacancies: Vec::with_capacity(n_workers),
            tasks: TaskTracker::new(),
        }
    }

    pub fn new_one() -> Self {
        Self {
            workers: Vec::with_capacity(1),
            vacancies: Vec::new(), // no allocation
            tasks: TaskTracker::new(),
        }
    }

    #[inline(always)]
    pub const fn has_vacancy(&self) -> bool {
        !self.vacancies.is_empty() || self.workers.len() < self.workers.capacity()
    }

    /// Find vacancies (or create a new one), then put the
    /// worker instance there.
    pub fn put_worker(&mut self, worker: Worker) -> usize {
        // we'll get a spot
        let spot = {
            self.vacancies.pop().unwrap_or({
                let ln = self.workers.len();
                self.workers.push(None);
                ln
            })
        };
        unsafe {
            self.workers
                .get_mut(spot)
                .unwrap_unchecked()
                .replace(worker);
        }

        spot
    }

    pub fn remove_worker(&mut self, spot: usize) -> bool {
        if let Some(worker) = self.workers.get_mut(spot) {
            let _ = unsafe { worker.take().unwrap_unchecked() };
            self.vacancies.push(spot);

            true
        } else {
            false
        }
    }

    /// Create a new worker instance, returning the handle.
    #[inline]
    pub fn create_worker(&mut self, task: WorkerTask) -> usize {
        let worker = Worker::start(self, task);
        self.put_worker(worker)
    }
}

pub fn pod_job(pod: Arc<RwLock<Pod>>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create runtime");

    let local = tokio::task::LocalSet::new();
    rt.block_on(local.run_until(pod_task(pod)));
}

async fn pod_task(pod: Arc<RwLock<Pod>>) {}
