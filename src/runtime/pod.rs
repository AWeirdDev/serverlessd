use std::{sync::Arc, thread};

use tokio::sync::{RwLock, mpsc, oneshot};
use tokio_util::task::TaskTracker;

use crate::runtime::{Worker, WorkerTask, WorkerTrigger};

#[derive(Debug)]
pub enum PodTrigger {
    /// Create a new worker within the pod.
    CreateWorker {
        task: WorkerTask,
        reply: oneshot::Sender<usize>,
    },

    ToWorker {
        id: usize,
        trigger: WorkerTrigger,
    },

    /// Stops the pod.
    Halt,
}

pub type PodTx = mpsc::Sender<PodTrigger>;
type PodRx = mpsc::Receiver<PodTrigger>;

/// A thread containing multiple workers.
#[derive(Debug)]
pub struct Pod {
    workers: Vec<Option<Worker>>,
    vacancies: Vec<usize>,
    tx: PodTx,
    pub(super) tasks: TaskTracker,
}

impl Pod {
    /// Spawn a dedicated thread for managing workers.
    pub fn start(n_workers: usize) -> Arc<RwLock<Self>> {
        let (tx, rx) = mpsc::channel::<PodTrigger>(64);

        let pod = Arc::new(RwLock::new(Self {
            workers: Vec::with_capacity(n_workers),
            vacancies: Vec::with_capacity(n_workers),
            tasks: TaskTracker::new(),
            tx,
        }));

        {
            let pod2 = pod.clone();
            thread::spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to create runtime");
                let local = tokio::task::LocalSet::new();
                rt.block_on(local.run_until(pod_task(pod2, rx)));
            });
        }

        pod
    }

    #[inline(always)]
    pub fn start_one() -> Arc<RwLock<Self>> {
        Self::start(1)
    }

    #[inline(always)]
    pub const fn has_vacancy(&self) -> bool {
        !self.vacancies.is_empty() || self.workers.len() < self.workers.capacity()
    }

    /// Find vacancies (or create a new one), then put the
    /// worker instance there.
    pub fn put_worker(&mut self, worker: Worker) -> usize {
        // we'll get a id
        let id = {
            self.vacancies.pop().unwrap_or({
                let ln = self.workers.len();
                self.workers.push(None);
                ln
            })
        };
        unsafe {
            self.workers.get_mut(id).unwrap_unchecked().replace(worker);
        }

        id
    }

    pub fn remove_worker(&mut self, id: usize) -> bool {
        if let Some(worker) = self.workers.get_mut(id) {
            let _ = unsafe { worker.take().unwrap_unchecked() };
            self.vacancies.push(id);

            true
        } else {
            false
        }
    }

    #[inline]
    pub fn get_worker(&self, id: usize) -> Option<&Worker> {
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
        self.put_worker(worker)
    }

    #[inline(always)]
    #[must_use]
    pub async fn trigger(&self, trigger: PodTrigger) -> bool {
        self.tx.send(trigger).await.is_ok()
    }

    /// Stop the pod task.
    ///
    /// Requires explicit access.
    #[inline(always)]
    #[must_use]
    pub async fn halt(&mut self) -> bool {
        self.tx.send(PodTrigger::Halt).await.is_ok()
    }
}

async fn pod_task(pod: Arc<RwLock<Pod>>, mut rx: PodRx) {
    while let Some(event) = rx.recv().await {
        match event {
            PodTrigger::CreateWorker { task, reply } => {
                let mut pod = pod.write().await;
                let id = pod.create_worker(task);
                reply.send(id).ok();
            }

            PodTrigger::ToWorker { id, trigger } => {
                let pod = pod.read().await;
                if let Some(worker) = pod.get_worker(id) {
                    let _ = worker.trigger(trigger).await;
                }
            }

            PodTrigger::Halt => {
                // we need to hold the explicit &mut
                // otherwise other instances might still
                // want to find empty workers to use, which is bad
                let mut pod = pod.write().await;
                for worker in pod.workers.drain(..) {
                    if let Some(worker) = worker {
                        let _ = worker.trigger(WorkerTrigger::Halt).await;
                    }
                }

                break;
            }
        }
    }
}
