use std::{cell::RefCell, collections::VecDeque, ffi::c_void, ptr::NonNull, sync::Arc};

use tokio_util::task::TaskTracker;
use v8::{Global, Isolate, OwnedIsolate, Platform, PromiseResolver, SharedRef};

use svld_language::ThrowException;
use svld_state_extensions::{
    HttpClientWorkerExtension, ReplierWorkerStateExtension, WorkerStateExtensions,
};

use crate::runtime::worker::{MonitorHandle, MonitoredFuture, Monitoring, WorkerTx};

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
}

/// Parameters for creating a worker state.
#[repr(packed)]
pub struct CreateWorkerState {
    /// The platform the worker is on.
    /// You can obtain this when initializing the platform with `v8`.
    pub platform: SharedRef<Platform>,

    /// The isolate pointer.
    pub isolate: NonNull<OwnedIsolate>,

    /// The ID of the worker.
    pub worker_id: usize,

    /// A event dispatcher for the worker.
    pub worker_tx: WorkerTx,

    /// The monitor handle.
    pub monitor_handle: MonitorHandle,
}

impl WorkerState {
    /// Create a new worker state, then inject state data to the isolate.
    ///
    /// # Safety
    /// `isolate` must exist.
    #[inline(always)]
    pub async fn create_injected(
        CreateWorkerState {
            platform,
            isolate,
            worker_id,
            worker_tx,
            monitor_handle,
        }: CreateWorkerState,
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
            extensions: {
                WorkerStateExtensions::new::<2>() // IMPORTANT: put the exact amount here!
                    .with_extension(HttpClientWorkerExtension::new())
                    .with_extension(ReplierWorkerStateExtension::new())
            },
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

    /// Schedule promise resolution.
    #[inline]
    pub fn schedule_resolution(&self, resolver: Global<PromiseResolver>, result: ResolutionResult) {
        self.pending_resolutions
            .borrow_mut()
            .push_back((resolver, result));
    }

    #[inline(always)]
    pub fn get_extension<T: Sized + 'static>(&self) -> Option<&T> {
        self.extensions.get_extension()
    }
}
