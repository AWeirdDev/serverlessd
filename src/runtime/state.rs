use std::{cell::OnceCell, ffi::c_void, mem, ptr::NonNull, sync::Arc};

use reqwest::Client;
use tokio_util::task::TaskTracker;
use v8::{Isolate, OwnedIsolate, Platform, SharedRef};

/// The worker state.
///
/// Internally, the state data should be stored in the isolate
/// using [`WorkerState::inject_to_isolate`].
pub struct WorkerState {
    client: OnceCell<Client>,
    pub tasks: TaskTracker,
    pub isolate: NonNull<OwnedIsolate>,
    pub platform: SharedRef<Platform>,
}

impl WorkerState {
    /// Create a new worker state, then inject state data to the isolate.
    #[inline(always)]
    pub fn new_injected(platform: SharedRef<Platform>, isolate: Box<OwnedIsolate>) -> Arc<Self> {
        let slf = Arc::new(Self {
            client: OnceCell::new(),
            tasks: TaskTracker::new(),
            isolate: unsafe { NonNull::new_unchecked(Box::into_raw(isolate)) },
            platform,
        });

        let item = Arc::clone(&slf);

        unsafe {
            item.get_isolate().set_data(0, slf.into_raw());
        };

        item
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
