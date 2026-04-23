use std::collections::HashSet;

use tokio::{sync::mpsc, task};
use tokio_util::task::TaskTracker;

use crate::{
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
    pub(super) vacancies: HashSet<usize>,
}

impl Pod {
    /// Spawn a dedicated thread for managing workers.
    pub fn start(n_workers: usize) -> (PodHandle, task::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel::<PodTrigger>(n_workers);
        let pod_handle = PodHandle::new(tx.clone());

        let pod = Self {
            tx: tx,
            workers: Vec::with_capacity(n_workers),
            vacancies: HashSet::with_capacity(n_workers),
            tasks: TaskTracker::new(),
            monitor: {
                let m = Monitor::new();
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

    /// Checks whether or not there are any vacancies.
    #[inline(always)]
    pub fn has_vacancies(&self) -> bool {
        !self.vacancies.is_empty() || self.workers.len() < self.workers.capacity()
    }

    /// Puts a worker in the pod.
    ///
    /// # Safety
    /// The worker of ID `id` must exist.
    #[inline(always)]
    pub fn put_worker(&mut self, id: usize, handle: WorkerHandle) {
        unsafe {
            self.workers.get_mut(id).unwrap_unchecked().replace(handle);
        }
    }

    /// Removes a worker from the pod.
    ///
    /// # Returns
    /// A boolean indicating whether the worker has been removed successfully.
    pub fn remove_worker(&mut self, id: usize) -> bool {
        if let Some(worker) = self.workers.get_mut(id) {
            let res = worker.take();

            if res.is_none() {
                return false;
            }

            if res.is_some() {
                self.vacancies.insert(id);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    #[inline]
    pub fn mark_worker_as_sleeping(&mut self, id: usize) -> bool {
        if self.workers.get_mut(id).is_some() {
            self.vacancies.insert(id);
            true
        } else {
            false
        }
    }

    /// Gets a worker from the pod by ID, returning the worker handle with state attached.
    #[inline]
    pub(super) fn get_worker(&self, id: usize) -> Option<&WorkerHandle> {
        if let Some(worker) = self.workers.get(id) {
            worker.as_ref()
        } else {
            None
        }
    }

    /// Creates and starts a new worker instance (or reuses a sleeping one), returning the ID.
    #[inline]
    #[must_use]
    pub(super) fn create_and_warmup_worker(&mut self) -> usize {
        if let Some(&id) = self.vacancies.iter().next() {
            self.vacancies.remove(&id);

            if self.workers.get(id).map_or(false, |w| w.is_some()) {
                return id;
            }

            let handle = WorkerHandle::start(self);
            if let Some(slot) = self.workers.get_mut(id) {
                slot.replace(handle);
            }
            return id;
        }

        let handle = WorkerHandle::start(self);
        let id = self.workers.len();
        self.workers.push(Some(handle));
        id
    }
}
