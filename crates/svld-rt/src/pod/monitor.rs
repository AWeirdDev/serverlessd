use std::{sync::Arc, thread, time::Duration};

use tokio::{
    sync::{mpsc, oneshot},
    task::LocalSet,
    time,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use v8::IsolateHandle;

use crate::triggers::{WorkerTrigger, WorkerTx};

enum MonitorTrigger {
    Spawn {
        isolate_handle: IsolateHandle,
        worker_tx: WorkerTx,
        reply: oneshot::Sender<MonitorHandle>,
    },
}

type MonitorTx = mpsc::UnboundedSender<MonitorTrigger>;
type MonitorRx = mpsc::UnboundedReceiver<MonitorTrigger>;

/// A monitor attached to a pod, which can be used to monitor threads.
pub struct PodAsideMonitor {
    tracker: TaskTracker,
}

impl PodAsideMonitor {
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
    pub fn start(self) -> PodAsideMonitorHandle {
        let (tx, rx) = mpsc::unbounded_channel();

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create runtime for monitoring");

            let local = LocalSet::new();
            rt.block_on(local.run_until(monitor_task(self, rx)));
        });

        PodAsideMonitorHandle::new(tx)
    }

    /// Spawns the monitor for a worker.
    ///
    /// # Safety
    /// Worker of ID `worker_id` must exist.
    #[must_use]
    fn spawn(&mut self, isolate_handle: IsolateHandle, worker_tx: WorkerTx) -> MonitorHandle {
        let (tx, rx) = mpsc::unbounded_channel();
        let notify = Arc::new(tx);
        let cancel = CancellationToken::new();

        let mw = MonitoredWorker::builder()
            .isolate(isolate_handle)
            .worker_tx(worker_tx)
            .cancel(cancel.clone())
            .build();
        self.tracker.spawn_local(monitor_worker_task(mw, rx));

        MonitorHandle { notify, cancel }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct PodAsideMonitorHandle {
    tx: MonitorTx,
}

impl PodAsideMonitorHandle {
    #[inline(always)]
    fn new(tx: MonitorTx) -> Self {
        Self { tx }
    }

    /// Start monitoring a worker.
    #[must_use]
    pub async fn start_monitoring(
        &self,
        isolate_handle: IsolateHandle,
        worker_tx: WorkerTx,
    ) -> Option<MonitorHandle> {
        let (reply, recv) = oneshot::channel();
        self.tx
            .send(MonitorTrigger::Spawn {
                isolate_handle,
                reply,
                worker_tx,
            })
            .ok()?;

        recv.await.ok()
    }
}

#[derive(Debug, bon::Builder)]
struct MonitoredWorker {
    isolate: IsolateHandle,
    worker_tx: WorkerTx,
    cancel: CancellationToken,
}

async fn monitor_task(mut monitor: PodAsideMonitor, mut rx: MonitorRx) {
    while let Some(trigger) = rx.recv().await {
        match trigger {
            MonitorTrigger::Spawn {
                isolate_handle,
                reply,
                worker_tx,
            } => {
                let handle = monitor.spawn(isolate_handle, worker_tx);
                reply.send(handle).ok();
            }
        }
    }
}

pub struct MonitorHandle {
    notify: Arc<mpsc::UnboundedSender<()>>,
    cancel: CancellationToken,
}

impl MonitorHandle {
    /// Ticks.
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
        let _ = self.notify.send(());
    }

    /// Stops the monitoring.
    ///
    /// This can be called when the task is finished which
    /// requires no supervision from then on.
    #[inline(always)]
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

// Monitor — now notifications are never lost:
async fn monitor_worker_task(mw: MonitoredWorker, mut rx: mpsc::UnboundedReceiver<()>) {
    let mut elapsed = Duration::default();

    let walltime_tick = time::sleep(Duration::from_secs(10));
    tokio::pin!(walltime_tick);

    let deadline = time::sleep(Duration::MAX);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            biased;
            _ = mw.cancel.cancelled() => {
                tracing::warn!("monitor: cancelled");
                break;
            }
            _ = &mut walltime_tick => {
                tracing::warn!("monitor: walltime");
                break;
            }
            _ = rx.recv() => {}
        };

        let remaining = Duration::from_millis(10).saturating_sub(elapsed);
        tracing::info!("remaining time: {remaining:?}");
        if remaining.is_zero() {
            tracing::warn!("monitor: no time remaining");
            break;
        }

        deadline.as_mut().reset(time::Instant::now() + remaining);

        let start = time::Instant::now();

        tokio::select! {
            biased;
            _ = &mut walltime_tick => {
                tracing::warn!("monitor: walltime");
                break;
            }
            _ = rx.recv() => {
                deadline.as_mut().reset(
                    time::Instant::now() + Duration::from_secs(86400 * 365 * 30)
                );
            }
            _ = &mut deadline => {
                tracing::warn!("monitor: deadline reached");
                break;
            }
        };

        elapsed += start.elapsed();
        if elapsed >= Duration::from_millis(10) {
            tracing::warn!("monitor: takes more than 10ms between ticks");
            break;
        }
    }

    // halt
    mw.isolate.terminate_execution();

    // we then halt the current task
    mw.worker_tx.try_send(WorkerTrigger::HaltTask).ok();
}
