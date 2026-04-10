use std::{
    cell::{OnceCell, RefCell},
    collections::VecDeque,
    ffi::c_void,
    ptr::NonNull,
    sync::Arc,
};

use reqwest::Client;
use tokio::sync::oneshot;
use tokio_util::task::TaskTracker;
use v8::{Global, Isolate, OwnedIsolate, Platform, PromiseResolver, SharedRef};

use crate::{
    language::ThrowException,
    runtime::worker::{MonitorHandle, MonitoredFuture, Monitoring, WorkerTx},
};

type MaybeReplier = NonNull<Option<oneshot::Sender<String>>>;

type ResolutionCallback =
    Box<dyn for<'s> FnOnce(&mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value>>;
type ResolutionResult = Result<ResolutionCallback, ThrowException>;
type PendingResolution = (Global<PromiseResolver>, ResolutionResult);

/// The interior worker state.
///
/// Internally, the state data should be stored in the isolate.
pub struct WorkerState {
    pub tasks: TaskTracker,
    pub pending_resolutions: RefCell<VecDeque<PendingResolution>>,

    pub isolate: NonNull<OwnedIsolate>,
    pub platform: SharedRef<Platform>,
    pub monitoring: Monitoring,
    pub extensions: WorkerStateExtensions,
    pub repliers: RefCell<Vec<Option<MaybeReplier>>>,
}

impl WorkerState {
    /// Create a new worker state, then inject state data to the isolate.
    ///
    /// # Safety
    /// `isolate` must exist.
    #[inline(always)]
    pub async fn create_injected(
        platform: SharedRef<Platform>,
        isolate: NonNull<OwnedIsolate>,
        worker_id: usize,
        worker_tx: WorkerTx,
        monitor_handle: MonitorHandle,
    ) -> Option<Arc<Self>> {
        let isolate_handle = unsafe { isolate.as_ref() }.thread_safe_handle();

        let slf = Arc::new(Self {
            tasks: TaskTracker::new(),
            pending_resolutions: RefCell::new(VecDeque::new()),

            isolate,
            platform,
            monitoring: monitor_handle
                .start_monitoring(isolate_handle, worker_id, worker_tx)
                .await?,
            extensions: WorkerStateExtensions::default(),
            repliers: RefCell::new(vec![]),
        });

        let item = Arc::clone(&slf);

        unsafe {
            item.get_isolate().set_data(0, slf.into_raw());
        };

        Some(item)
    }

    /// Gets the isolate.
    ///
    /// # Safety
    /// There's no gurantee that there's only one holder, and thus it's `unsafe`.
    /// A general approach is not to hold a scope open across an `.await` point.
    #[inline(always)]
    pub unsafe fn get_isolate(&self) -> &mut OwnedIsolate {
        unsafe { &mut *self.isolate.as_ptr() }
    }

    #[inline(always)]
    pub fn into_raw(self: Arc<Self>) -> *mut c_void {
        Arc::into_raw(self) as *mut _
    }

    /// Gets a reference-counted handle of the worker state from an isolate.
    /// It's guranteed that the internal `WorkerState` will never drop.
    #[inline(always)]
    pub fn get_from_isolate(isolate: &Isolate) -> Arc<WorkerState> {
        let ptr = isolate.get_data(0) as *const WorkerState;
        unsafe {
            // this is really fucking important
            // if this is gone, we get ub
            // because the count is never incremented
            Arc::increment_strong_count(ptr);

            Arc::from_raw(ptr)
        }
    }

    /// Opens the original reference-counted handle of the worker state.
    /// It's guranteed that the internal `WorkerState` will drop when no
    /// one's carrying the `Arc`, and returned handle is also dropped.
    #[inline(always)]
    pub fn open_from_isolate<'a>(isolate: &'a Isolate) -> Arc<WorkerState> {
        let ptr = isolate.get_data(0) as *const WorkerState;
        unsafe { Arc::from_raw(ptr) }
    }

    /// Wait until the runtime has closed.
    #[inline]
    pub async fn wait_close(self: Arc<Self>) {
        self.tasks.close();
        self.tasks.wait().await;
    }

    /// Ticks the [`Monitoring`].
    pub fn tick_monitoring(&self) {
        self.monitoring.tick();
    }

    #[inline(always)]
    pub fn monitored_future<F: Future>(&self, f: F) -> MonitoredFuture<F> {
        self.monitoring.monitored_future(f)
    }

    /// Clean up dead repliers.
    #[inline]
    pub fn cleanup_dead_repliers(&self) {
        let mut repliers = self.repliers.borrow_mut();
        for maybe_replier in repliers.iter_mut() {
            if let Some(replier) = maybe_replier {
                if unsafe { &*replier.as_ptr() }.is_none() {
                    maybe_replier.take();
                }
            }
        }
    }

    /// Gets the next replier index to use for safe dropping.
    pub fn get_next_replier_idx(&self) -> usize {
        self.cleanup_dead_repliers();
        let mut repliers = self.repliers.borrow_mut();
        let idx = {
            let maybe_idx = repliers
                .iter()
                .enumerate()
                .find(|(_, k)| k.is_none())
                .map(|(idx, _)| idx);

            match maybe_idx {
                Some(idx) => idx,
                None => {
                    let length = repliers.len();
                    repliers.push(None);
                    length
                }
            }
        };

        idx
    }

    /// Add a replier to the `idx`.
    ///
    /// # Safety
    /// Index of `idx` must exist. You can use `get_next_replier_idx()`.
    pub fn add_replier(&self, idx: usize, replier: *mut Option<oneshot::Sender<String>>) {
        let mut repliers = self.repliers.borrow_mut();
        let shell = unsafe { repliers.get_mut(idx).unwrap_unchecked() };
        let ptr = unsafe { NonNull::new_unchecked(replier) };
        shell.replace(ptr);
    }

    /// Schedule promise resolution.
    #[inline]
    pub fn schedule_resolution(&self, resolver: Global<PromiseResolver>, result: ResolutionResult) {
        self.pending_resolutions
            .borrow_mut()
            .push_back((resolver, result));
    }
}

impl Drop for WorkerState {
    fn drop(&mut self) {
        // we need to clean **all** repliers now
        let mut repliers = self.repliers.borrow_mut();
        for maybe_replier in repliers.drain(..) {
            if let Some(replier) = maybe_replier {
                if unsafe { &*replier.as_ptr() }.is_some() {
                    let item = unsafe { &mut *replier.as_ptr() };
                    if let Some(item) = item.take() {
                        item.send(String::new()).ok();
                    }
                }
            }
        }
    }
}

// ====== state extensions =====
#[derive(Default)]
pub struct WorkerStateExtensions {
    client: OnceCell<Client>,
}

impl WorkerStateExtensions {
    /// Adds an HTTP client to the state, ignoring errors.
    pub fn add_client(&self) {
        self.client
            .set(
                Client::builder()
                    .tls_backend_rustls()
                    .default_headers({
                        use reqwest::header::{HeaderMap, HeaderValue};

                        let mut headers = HeaderMap::new();
                        headers.append("User-Agent", HeaderValue::from_static("Serverless"));

                        headers
                    })
                    .build()
                    .expect("reqwest: cannot initialize tls backend or resolver error"),
            )
            .ok();
    }

    /// Gets the HTTP client, if exists.
    #[inline(always)]
    pub fn get_client(&self) -> Option<&Client> {
        self.client.get()
    }
}
