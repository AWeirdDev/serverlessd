use std::{hint, mem};

use tokio::{io, sync::mpsc, task};
use tokio_util::task::TaskTracker;

use crate::{
    Monitor, MonitorHandle, WorkerHandle,
    pod::{PodTrigger, handle::PodHandle, task::pod_task, trigger::PodTx},
};

/// "A pod," conceptually it consists of a thread containing multiple workers
/// and a thread serving as a monitor for monitoring worker execution time.
///
/// This structure contains:
/// - A handle for communicating with the pod task
/// - A handle for communicating with the monitor
/// - A task tracker, for tracking worker tasks
/// - A container with an array of handles for communicating with each worker
///
/// # Vacancy searching performance
/// It should be guaranteed that each pod has a small number of workers,
/// so that despite the action of searching for vacancies is `O(N)`, the
/// `N` is so small that it's generally neglectable, making it `O(1)`.
#[derive(Debug)]
pub struct Pod {
    pub tx: PodTx,
    pub monitor: MonitorHandle,
    pub tasks: TaskTracker,
    pub(super) workers: Vec<StatedWorkerHandle>,
}

impl Pod {
    /// Spawn a dedicated thread for managing workers.
    ///
    /// # Returns
    /// `(PodHandle, JoinHandle)`, where the `PodHandle` is for communicating
    /// with the pod task.
    pub fn start(n_workers: usize) -> (PodHandle, task::JoinHandle<io::Result<()>>) {
        let (tx, rx) = mpsc::channel::<PodTrigger>(n_workers);
        let pod_handle = PodHandle::new(tx.clone());

        let pod = Self {
            tx: tx,
            workers: Vec::with_capacity(n_workers),
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
                    .build()?;
                let local = tokio::task::LocalSet::new();
                rt.block_on(local.run_until(pod_task(pod, rx)));
                Ok(())
            })
        };

        (pod_handle, join_handle)
    }

    /// Checks whether or not there are any vacancies.
    #[inline(always)]
    pub fn has_vacancies(&self) -> bool {
        self.find_available_worker_spot().is_some()
    }

    /// Removes a worker from the pod.
    ///
    /// # Returns
    /// A boolean indicating whether the worker has been removed successfully.
    pub fn remove_worker(&mut self, id: usize) -> bool {
        if let Some(worker) = self.workers.get_mut(id) {
            worker.replace(StatedWorkerHandle::Absent);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn mark_worker_as_sleeping(&mut self, id: usize) -> bool {
        if let Some(worker) = self.workers.get_mut(id) {
            worker.mark_as_sleeping();
            true
        } else {
            false
        }
    }

    /// Gets a worker from the pod by ID, returning the worker handle with state attached.
    #[inline]
    pub(super) fn get_worker(&self, id: usize) -> Option<WorkerHandle> {
        if let Some(worker) = self.workers.get(id) {
            worker.get_handle()
        } else {
            None
        }
    }

    /// Finds a sleeping or absent worker spot.
    ///
    /// # Returns
    /// `Some( ( worker_id, is_sleeping ) )`
    #[inline(always)]
    pub(super) fn find_available_worker_spot(&self) -> Option<(usize, bool)> {
        self.workers
            .iter()
            .enumerate()
            .find(|(_, item)| item.is_sleeping() || item.is_absent())
            .map(|(id, item)| (id, item.is_sleeping()))
    }

    /// Creates and starts a new worker instance (or reuses a sleeping one), returning the ID.
    #[inline]
    #[must_use]
    pub(super) fn create_and_warmup_worker(&mut self) -> Option<usize> {
        let (worker_id, sleeping) = self.find_available_worker_spot()?;

        if !sleeping {
            let handle = WorkerHandle::start(self);
            let stated_handle = unsafe { self.workers.get_mut(worker_id).unwrap_unchecked() };
            stated_handle.replace(StatedWorkerHandle::Running(Some(handle)));
        }

        Some(worker_id)
    }
}

/// A worker handle with a state.
#[derive(Debug)]
pub(super) enum StatedWorkerHandle {
    Absent,
    Sleeping(Option<WorkerHandle>),
    Running(Option<WorkerHandle>),
}

#[allow(unused)]
impl StatedWorkerHandle {
    #[inline(always)]
    pub(super) const fn new_sleeping(handle: WorkerHandle) -> Self {
        Self::Sleeping(Some(handle))
    }

    #[inline(always)]
    pub(super) const fn new_running(handle: WorkerHandle) -> Self {
        Self::Running(Some(handle))
    }

    pub(super) const fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    pub(super) const fn is_sleeping(&self) -> bool {
        matches!(self, Self::Sleeping(_))
    }

    pub(super) const fn is_running(&self) -> bool {
        matches!(self, Self::Running(_))
    }

    pub(super) fn mark_as_absent(&mut self) -> Option<WorkerHandle> {
        if matches!(self, Self::Absent) {
            return None;
        }

        unsafe {
            match mem::replace(self, Self::Absent) {
                Self::Sleeping(mut v) => Some(v.take().unwrap_unchecked()),
                Self::Running(mut v) => Some(v.take().unwrap_unchecked()),
                _ => hint::unreachable_unchecked(),
            }
        }
    }

    #[inline(always)]
    pub(super) fn mark_as_sleeping(&mut self) {
        let khandle @ Some(_) = self.take_handle() else {
            return;
        };
        mem::replace(self, Self::Sleeping(khandle));
    }

    #[inline(always)]
    pub(super) fn mark_as_running(&mut self) {
        let khandle @ Some(_) = self.take_handle() else {
            return;
        };
        mem::replace(self, Self::Running(khandle));
    }

    #[inline(always)]
    pub(super) fn take_handle(&mut self) -> Option<WorkerHandle> {
        match mem::replace(self, Self::Absent) {
            Self::Absent => None,
            Self::Running(v) => v,
            Self::Sleeping(v) => v,
        }
    }

    #[inline(always)]
    pub(super) fn get_handle(&self) -> Option<WorkerHandle> {
        match self {
            Self::Absent => None,
            Self::Running(v) => v.clone(),
            Self::Sleeping(v) => v.clone(),
        }
    }

    #[inline(always)]
    pub(super) fn replace(&mut self, x: StatedWorkerHandle) -> Self {
        mem::replace(self, x)
    }
}
