use std::{sync::Arc, thread, time::Duration};

use tokio::{
    sync::{Notify, mpsc, oneshot},
    task::LocalSet,
    time,
};
use tokio_util::task::TaskTracker;

use v8::IsolateHandle;

use crate::triggers::{WorkerTrigger, WorkerTx};

pub enum MonitorTrigger {
    Spawn {
        isolate_handle: IsolateHandle,
        worker_id: usize,
        worker_tx: WorkerTx,
        reply: oneshot::Sender<Monitoring>,
    },
}

type MonitorTx = mpsc::UnboundedSender<MonitorTrigger>;
type MonitorRx = mpsc::UnboundedReceiver<MonitorTrigger>;

/// A monitor attached to a pod, which can be used to monitor threads.
pub struct Monitor {
    tracker: TaskTracker,
}

impl Monitor {
    /// Creates a worker wall time monitor that performs
    /// monitoring on a different thread to ensure true parallelism.
    #[inline]
    pub fn new() -> Self {
        Self {
            tracker: TaskTracker::new(),
        }
    }

    /// Start monitoring a worker. There is no need to `join()` the thread.
    /// Cancelling does not matter for this context.
    pub fn start(self) -> MonitorHandle {
        let (tx, rx) = mpsc::unbounded_channel();

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create runtime for monitoring");

            let local = LocalSet::new();
            rt.block_on(local.run_until(monitor_task(self, rx)));
        });

        MonitorHandle::new(tx)
    }

    /// Spawns the monitor for a worker.
    ///
    /// # Safety
    /// Worker of ID `worker_id` must exist.
    #[must_use]
    fn spawn(
        &mut self,
        isolate_handle: IsolateHandle,
        worker_id: usize,
        worker_tx: WorkerTx,
    ) -> Monitoring {
        let notify = Arc::new(Notify::new());

        let mw = MonitoredWorker::new(isolate_handle, worker_tx, notify.clone());
        self.tracker.spawn_local(monitor_worker_task(mw, worker_id));

        Monitoring::new(notify)
    }
}

/// A monitor handle for communicating with the monitor
/// task. You can request to monitor a worker with this.
#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct MonitorHandle {
    tx: MonitorTx,
}

impl MonitorHandle {
    #[inline(always)]
    fn new(tx: MonitorTx) -> Self {
        Self { tx }
    }

    /// Start monitoring a worker.
    #[must_use]
    pub async fn start_monitoring(
        &self,
        isolate_handle: IsolateHandle,
        worker_id: usize,
        worker_tx: WorkerTx,
    ) -> Option<Monitoring> {
        let (reply, recv) = oneshot::channel();
        self.tx
            .send(MonitorTrigger::Spawn {
                isolate_handle,
                reply,
                worker_id,
                worker_tx,
            })
            .ok()?;

        recv.await.ok()
    }
}

pub struct MonitoredWorker {
    isolate: IsolateHandle,
    worker_tx: WorkerTx,
    notify: Arc<Notify>,
}

impl MonitoredWorker {
    #[inline(always)]
    pub fn new(isolate: IsolateHandle, worker_tx: WorkerTx, notify: Arc<Notify>) -> Self {
        Self {
            isolate,
            worker_tx,
            notify,
        }
    }
}

async fn monitor_task(mut monitor: Monitor, mut rx: MonitorRx) {
    while let Some(trigger) = rx.recv().await {
        match trigger {
            MonitorTrigger::Spawn {
                isolate_handle,
                reply,
                worker_id,
                worker_tx,
            } => {
                let monitoring = monitor.spawn(isolate_handle, worker_id, worker_tx);
                reply.send(monitoring).ok();
            }
        }
    }
}

#[repr(transparent)]
pub struct Monitoring {
    notify: Arc<Notify>,
}

impl Monitoring {
    #[inline(always)]
    fn new(tx: Arc<Notify>) -> Self {
        Self { notify: tx }
    }

    /// Tick.
    ///
    /// You must tick back when the work is done.
    /// If the tick-back isn't received within 30ms, the
    /// associated isolate will be terminated immediately.
    ///
    /// # Example
    /// ```no_run
    /// monitoring.tick();
    ///
    /// do_some_probably_heavy_work();
    ///
    /// monitoring.tick();
    /// ```
    #[inline(always)]
    pub fn tick(&self) {
        self.notify.notify_one();
    }
}

async fn monitor_worker_task(mw: MonitoredWorker, worker_id: usize) {
    tracing::info!("monitoring worker {}", worker_id);

    let mut elapsed = Duration::default();

    let walltime_tick = time::sleep(Duration::from_secs(10));
    tokio::pin!(walltime_tick);

    let deadline = time::sleep(Duration::from_millis(10));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            biased;
            _ = &mut walltime_tick => {
                break;
            }
            msg = mw.notify.notified() => msg,
        };

        let remaining = Duration::from_millis(10).saturating_sub(elapsed);
        if remaining.is_zero() {
            break;
        }

        deadline.as_mut().reset(time::Instant::now() + remaining);

        let start = time::Instant::now();

        tokio::select! {
            biased;
            _ = &mut walltime_tick => {
                break;
            }
            _ = &mut deadline => {
                break;
            }
            msg = mw.notify.notified() => msg,
        };

        elapsed += start.elapsed();
        if elapsed >= Duration::from_millis(10) {
            break;
        }
    }

    halt(&mw);
}

fn halt(mw: &MonitoredWorker) {
    mw.isolate.terminate_execution();

    // we then halt the current task
    mw.worker_tx.try_send(WorkerTrigger::HaltTask).ok();
}
