use std::{thread, time::Duration};

use tokio::{
    sync::{mpsc, oneshot},
    task::LocalSet,
    time::Instant,
};
use tokio_util::task::TaskTracker;

use v8::IsolateHandle;

use crate::{PodHandle, WorkerTrigger, worker::WorkerTx};

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
    // SAFETY: we don't need to get a stable memory address
    // # of pod workers will not increase
    tracker: TaskTracker,
    pod_handle: PodHandle,
}

impl Monitor {
    /// Creates a worker wall time monitor that performs
    /// monitoring on a different thread to ensure true parallelism.
    #[inline]
    pub fn new(pod_handle: PodHandle) -> Self {
        Self {
            tracker: TaskTracker::new(),
            pod_handle,
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
        let (tx, rx) = mpsc::unbounded_channel();

        let mw = MonitoredWorker::new(isolate_handle, worker_tx, rx);
        self.tracker
            .spawn_local(monitor_worker_task(mw, self.pod_handle.clone(), worker_id));

        Monitoring::new(tx)
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
    rx: mpsc::UnboundedReceiver<()>,
}

impl MonitoredWorker {
    #[inline(always)]
    pub fn new(
        isolate: IsolateHandle,
        worker_tx: WorkerTx,
        rx: mpsc::UnboundedReceiver<()>,
    ) -> Self {
        Self {
            isolate,
            worker_tx,
            rx,
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

pub struct MonitoredFuture<F> {
    inner: F,
    tx: mpsc::UnboundedSender<()>,
}

impl<F: Future> Future for MonitoredFuture<F> {
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let inner = unsafe { std::pin::Pin::new_unchecked(&mut this.inner) };

        this.tx.send(()).ok();
        let result = inner.poll(cx);
        this.tx.send(()).ok();

        result
    }
}

#[repr(transparent)]
pub struct Monitoring {
    tx: mpsc::UnboundedSender<()>,
}

impl Monitoring {
    #[inline(always)]
    fn new(tx: mpsc::UnboundedSender<()>) -> Self {
        Self { tx }
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
        self.tx.send(()).ok();
    }

    /// Create a monitored future. Ticking is done between task polls.
    #[inline(always)]
    pub fn monitored_future<F: Future>(&self, f: F) -> MonitoredFuture<F> {
        MonitoredFuture {
            inner: f,
            tx: self.tx.clone(),
        }
    }
}

async fn monitor_worker_task(mut mw: MonitoredWorker, pod: PodHandle, worker_id: usize) {
    tracing::info!("monitoring worker {}", worker_id);

    let mut elapsed = Duration::default();

    let walltime_tick = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(walltime_tick);

    tracing::info!("starting to receive!");
    while let Some(()) = mw.rx.recv().await {
        tracing::info!("beep boop");
        if elapsed.as_secs() > 10 {
            tracing::error!("(per worker, 10s) time's up");
            break;
        }

        let start = Instant::now();

        tokio::select! {
            biased;

            _ = &mut walltime_tick => {
                // oopsie daisy, time's up!
                tracing::error!("(per worker, 10s) time's up");
                break;
            }

            _ = mw.rx.recv() => {
                elapsed += start.elapsed();
            }

            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                tracing::error!("(per task, 100ms) time's up");
                break;
            }
        };
    }

    halt(&mw);

    // and then kill the whole isolate
    let (token, recv) = oneshot::channel();
    mw.worker_tx.send(WorkerTrigger::Kill { token }).await.ok();
    recv.await.ok();

    tracing::info!("shutting down, removing worker");
    // after we successfully killed it, we can essentially 'remove' this worker
    let _ = pod.remove_worker(worker_id).await;
}

fn halt(mw: &MonitoredWorker) {
    tracing::info!("terminating v8 execution");

    if !mw.isolate.terminate_execution() {
        tracing::warn!("failed to terminate isolate when time's up (isolate already destroyed)");
    }

    // we then halt the current task
    mw.worker_tx.try_send(WorkerTrigger::HaltTask).ok();
}
