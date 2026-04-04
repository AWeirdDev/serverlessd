use std::{thread, time::Duration};

use tokio::{
    sync::{mpsc, oneshot},
    task::LocalSet,
};
use tokio_util::task::TaskTracker;

use v8::IsolateHandle;

pub enum MonitorTrigger {
    Spawn {
        isolate_handle: IsolateHandle,
        reply: oneshot::Sender<Monitoring>,
    },
}

type MonitorTx = mpsc::Sender<MonitorTrigger>;
type MonitorRx = mpsc::Receiver<MonitorTrigger>;

pub struct Monitor {
    // SAFETY: we don't need to get a stable memory address
    // # of pod workers will not increase
    tracker: TaskTracker,
}

impl Monitor {
    /// Creates a wall time monitor manager.
    ///
    /// To keep things simple, the monitor creates an uninitialized
    /// [`Vec`] of memory with the capacity of `n_workers` assigned.
    /// This is slightly different from a `Pod`, which initializes
    /// only when needed, while allocating the same amount of memory.
    pub fn new() -> Self {
        Self {
            tracker: TaskTracker::new(),
        }
    }

    /// Start monitoring. There is no need to `join()` the thread.
    /// Cancelling does not matter for this context.
    pub fn start(self) -> MonitorHandle {
        let (tx, rx) = mpsc::channel(1);

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

    /// Spawns the monitor for `worker_id`.
    ///
    /// # Safety
    /// Worker of ID `worker_id` must exist.
    #[must_use]
    fn spawn(&mut self, isolate_handle: IsolateHandle) -> Monitoring {
        let (tx, rx) = mpsc::channel(1);

        let mw = MonitoredWorker::new(isolate_handle, rx);
        self.tracker.spawn_local(monitor_worker_task(mw));

        Monitoring::new(tx)
    }
}

#[repr(transparent)]
#[derive(Clone)]
pub struct MonitorHandle {
    tx: MonitorTx,
}

impl MonitorHandle {
    #[inline(always)]
    fn new(tx: MonitorTx) -> Self {
        Self { tx }
    }

    #[must_use]
    pub async fn start_monitoring(&self, isolate_handle: IsolateHandle) -> Option<Monitoring> {
        let (reply, recv) = oneshot::channel();
        self.tx
            .send(MonitorTrigger::Spawn {
                isolate_handle,
                reply,
            })
            .await
            .ok()?;

        recv.await.ok()
    }
}

pub struct MonitoredWorker {
    isolate: IsolateHandle,
    rx: mpsc::Receiver<()>,
}

impl MonitoredWorker {
    #[inline(always)]
    pub fn new(isolate: IsolateHandle, rx: mpsc::Receiver<()>) -> Self {
        Self { isolate, rx }
    }
}

async fn monitor_task(mut monitor: Monitor, mut rx: MonitorRx) {
    while let Some(trigger) = rx.recv().await {
        match trigger {
            MonitorTrigger::Spawn {
                isolate_handle,
                reply,
            } => {
                let monitoring = monitor.spawn(isolate_handle);
                reply.send(monitoring).ok();
            }
        }
    }
}

#[repr(transparent)]
pub struct Monitoring {
    tx: mpsc::Sender<()>,
}

impl Monitoring {
    #[inline(always)]
    fn new(tx: mpsc::Sender<()>) -> Self {
        Self { tx }
    }

    /// Tick. You must tick back when the work is done.
    #[inline(always)]
    pub fn tick(&self) {
        self.tx.try_send(()).ok();
    }
}

async fn monitor_worker_task(mut mw: MonitoredWorker) {
    while let Some(()) = mw.rx.recv().await {
        tracing::info!("i got you");
        tokio::select! {
            // we still like the user at some point
            biased;

            _ = mw.rx.recv() => {
                tracing::info!("you're good this time...");
            }
            _ = tokio::time::sleep(Duration::from_millis(10)) => {
                tracing::error!("time's up bitch");
                if !mw.isolate.terminate_execution() {
                    tracing::error!("failed to terminate isolate when 10ms time's up");
                }
                break;
            }
        };
    }
}
