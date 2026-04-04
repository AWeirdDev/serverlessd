use std::cell::{Cell, RefCell};

use tokio::{
    sync::{mpsc, oneshot},
    task,
};
use tokio_util::task::TaskTracker;

use crate::runtime::{
    Worker, WorkerTask, WorkerTrigger,
    monitor::{Monitor, MonitorHandle},
};

#[derive(Debug)]
pub enum PodTrigger {
    CheckVacancies {
        reply: oneshot::Sender<bool>,
    },

    /// Create a new worker within the pod.
    CreateWorker {
        task: WorkerTask,
        reply: oneshot::Sender<usize>,
    },

    #[allow(unused)]
    ToWorker {
        id: usize,
        trigger: WorkerTrigger,
    },

    /// Stops the pod.
    Halt {
        token: oneshot::Sender<()>,
    },
}

pub type PodTx = mpsc::Sender<PodTrigger>;
type PodRx = mpsc::Receiver<PodTrigger>;

/// A thread containing multiple workers.
pub struct Pod {
    workers: Vec<Option<Worker>>,
    vacancies: Vec<usize>,
    pub(super) monitor: MonitorHandle,
    pub(super) tasks: TaskTracker,
}

impl Pod {
    /// Spawn a dedicated thread for managing workers.
    pub fn start(n_workers: usize) -> (PodHandle, task::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel::<PodTrigger>(64);

        let pod = Self {
            workers: Vec::with_capacity(n_workers),
            vacancies: Vec::with_capacity(n_workers),
            tasks: TaskTracker::new(),
            monitor: {
                let m = Monitor::new();
                m.start()
            },
        };

        let pod_handle = PodHandle::new(tx);
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
    pub const fn has_vacancy(&self) -> bool {
        !self.vacancies.is_empty() || self.workers.len() < self.workers.capacity()
    }

    pub fn get_next_worker_id(&mut self) -> usize {
        self.vacancies.pop().unwrap_or({
            let ln = self.workers.len();
            self.workers.push(None);
            ln
        })
    }

    pub fn put_worker(&mut self, id: usize, worker: Worker) {
        unsafe {
            self.workers.get_mut(id).unwrap_unchecked().replace(worker);
        }
    }

    #[allow(unused)]
    fn remove_worker(&mut self, id: usize) -> bool {
        if let Some(worker) = self.workers.get_mut(id) {
            let _ = unsafe { worker.take().unwrap_unchecked() };
            self.vacancies.push(id);

            true
        } else {
            false
        }
    }

    #[inline]
    fn get_worker(&self, id: usize) -> Option<&Worker> {
        if let Some(worker) = self.workers.get(id) {
            worker.as_ref()
        } else {
            None
        }
    }

    /// Create & start a new worker instance, then return the handle.
    #[inline]
    #[must_use]
    fn create_worker(&mut self, task: WorkerTask) -> usize {
        let worker = Worker::start(self, task);

        let id = self.get_next_worker_id();
        self.put_worker(id, worker);

        id
    }
}

#[repr(transparent)]
pub struct PodHandle {
    tx: PodTx,
}

impl PodHandle {
    #[inline(always)]
    pub fn new(tx: PodTx) -> Self {
        Self { tx }
    }

    /// Stop the pod task.
    ///
    /// Returns `false` if failed.
    #[must_use]
    pub async fn halt(&self) -> bool {
        let (token, recv) = oneshot::channel();

        tracing::info!("waiting for pod to halt...");
        if !self.tx.send(PodTrigger::Halt { token }).await.is_ok() {
            tracing::error!("failed to halt pod");
            return false;
        }

        recv.await.is_ok()
    }

    pub async fn has_vacancies(&self) -> bool {
        let (reply, recv) = oneshot::channel();
        if !self.trigger(PodTrigger::CheckVacancies { reply }).await {
            return false;
        }

        recv.await.ok().unwrap_or(false)
    }

    /// Create a worker. Returns `Some(worker_id)` if successful.
    pub async fn create_worker(&self, task: WorkerTask) -> Option<usize> {
        let (reply, receive) = oneshot::channel::<usize>();
        if !self.trigger(PodTrigger::CreateWorker { task, reply }).await {
            return None;
        }

        receive.await.ok()
    }

    #[inline(always)]
    #[must_use]
    pub async fn trigger(&self, trigger: PodTrigger) -> bool {
        self.tx.send(trigger).await.is_ok()
    }
}

#[tracing::instrument(name = "pod_task", skip_all)]
async fn pod_task(mut pod: Pod, mut rx: PodRx) {
    while let Some(event) = rx.recv().await {
        match event {
            PodTrigger::CheckVacancies { reply } => {
                reply.send(pod.has_vacancy()).ok();
            }

            PodTrigger::CreateWorker { task, reply } => {
                let id = pod.create_worker(task);
                reply.send(id).ok();
            }

            PodTrigger::ToWorker { id, trigger } => {
                if let Some(worker) = pod.get_worker(id) {
                    let _ = worker.trigger(trigger).await;
                }
            }

            PodTrigger::Halt { token } => {
                for worker in pod.workers.drain(..) {
                    tracing::info!("closing workers in this pod...");

                    let (wtoken, recv) = oneshot::channel();

                    if let Some(worker) = worker {
                        let _ = worker.trigger(WorkerTrigger::Halt { token: wtoken }).await;
                    }

                    recv.await.ok();
                }

                tracing::info!("all workers closed in this pod");
                token.send(()).ok();
                break;
            }
        }
    }
}
