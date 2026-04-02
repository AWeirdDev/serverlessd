use std::{cell::OnceCell, ffi::c_void};

use reqwest::Client;
use tokio_util::task::TaskTracker;
use v8::{Isolate, Platform, SharedRef};

pub struct WorkerState {
    client: OnceCell<Client>,
    pub tasks: TaskTracker,
    pub platform: SharedRef<Platform>,
}

impl WorkerState {
    #[inline(always)]
    pub fn new(platform: SharedRef<Platform>) -> Box<Self> {
        Box::new(Self {
            client: OnceCell::new(),
            tasks: TaskTracker::new(),
            platform,
        })
    }

    #[inline(always)]
    pub fn into_raw(self: Box<Self>) -> *mut c_void {
        Box::into_raw(self) as *mut _
    }

    #[inline(always)]
    pub fn inject_to_isolate<'a>(self: Box<Self>, isolate: &mut Isolate) -> &'a WorkerState {
        let item = self.into_raw();
        isolate.set_data(0, item);
        unsafe { &*(item as *mut WorkerState) }
    }

    #[inline(always)]
    pub fn get_from_isolate<'a>(isolate: &'a Isolate) -> &'a WorkerState {
        unsafe { &*(isolate.get_data(0) as *mut WorkerState) }
    }

    #[inline(always)]
    pub fn open_from_isolate<'a>(isolate: &'a Isolate) -> Box<WorkerState> {
        unsafe { Box::from_raw(isolate.get_data(0) as *mut WorkerState) }
    }

    /// Wait until the runtime has closed.
    #[inline]
    pub async fn wait_close(self: Box<Self>) {
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

    #[inline(always)]
    pub fn get_client(&self) -> Option<&Client> {
        self.client.get()
    }
}
