use tokio::{sync::mpsc, task};
use tokio_util::task::TaskTracker;

use crate::runtime::{
    Monitor, MonitorHandle, WorkerHandle,
    pod::{PodTrigger, handle::PodHandle, task::pod_task, trigger::PodTx},
};

/// A thread containing multiple workers.
#[derive(Debug)]
pub struct Pod {
    pub tx: PodTx,
    pub monitor: MonitorHandle,
    pub tasks: TaskTracker,
    pub(super) workers: Vec<Option<WorkerHandle>>,
    pub(super) vacancies: Vec<usize>,
}

impl Pod {
    /// Spawn a dedicated thread for managing workers.
    pub fn start(n_workers: usize) -> (PodHandle, task::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel::<PodTrigger>(64);
        let pod_handle = PodHandle::new(tx.clone());

        let pod = Self {
            tx: tx,
            workers: Vec::with_capacity(n_workers),
            vacancies: Vec::with_capacity(n_workers),
            tasks: TaskTracker::new(),
            monitor: {
                let m = Monitor::new(pod_handle.clone());
                m.start()
            },
        };

        let join_handle = {
            task::spawn_blocking(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to create runtime");
                let local = tokio::task::LocalSet::new();
                rt.block_on(local.run_until(pod_task(pod, rx)));
            })
        };

        (pod_handle, join_handle)
    }

    #[inline(always)]
    pub fn has_vacancy(&self) -> bool {
        !self.vacancies.is_empty() || self.workers.len() < self.workers.capacity()
    }

    /// Get the next worker ID.
    /// You **must** check if this pod has a vacancy.
    #[inline(always)]
    pub fn get_next_worker_id(&mut self) -> usize {
        self.vacancies.pop().unwrap_or_else(|| {
            let ln = self.workers.len();
            self.workers.push(None);
            ln
        })
    }

    pub fn put_worker(&mut self, id: usize, worker: WorkerHandle) {
        unsafe {
            self.workers.get_mut(id).unwrap_unchecked().replace(worker);
        }
    }

    pub fn remove_worker(&mut self, id: usize) -> bool {
        if let Some(worker) = self.workers.get_mut(id) {
            let res = worker.take();
            if res.is_some() {
                self.vacancies.push(id);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    #[inline]
    pub(super) fn get_worker(&self, id: usize) -> Option<&WorkerHandle> {
        if let Some(worker) = self.workers.get(id) {
            worker.as_ref()
        } else {
            None
        }
    }

    /// Create & start a new worker instance, then return the handle.
    #[inline]
    #[must_use]
    pub(super) fn create_and_warmup_worker(&mut self) -> usize {
        let worker = WorkerHandle::start(self);

        let id = self.get_next_worker_id();
        self.put_worker(id, worker);

        id
    }
}
