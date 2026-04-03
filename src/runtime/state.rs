use std::{
    cell::OnceCell,
    ffi::c_void,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::Arc,
};

use reqwest::Client;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tokio_util::task::TaskTracker;
use v8::{Isolate, Platform, SharedRef};

/// The worker state.
///
/// Internally, the state data should be stored in the isolate
/// using [`WorkerState::inject_to_isolate`].
pub struct WorkerState {
    client: OnceCell<Client>,
    pub tasks: TaskTracker,
    pub ctx_scope: ObscuredContextScope,
    pub platform: SharedRef<Platform>,
}

impl WorkerState {
    /// Create a new worker state, then inject state data to the isolate.
    #[inline(always)]
    pub fn new_injected(
        platform: SharedRef<Platform>,
        ctx_scope: Box<v8::ContextScope<'_, '_, v8::HandleScope<'_>>>,
    ) -> Arc<Self> {
        let slf = Arc::new(Self {
            client: OnceCell::new(),
            ctx_scope: ObscuredContextScope::new(ctx_scope),
            tasks: TaskTracker::new(),
            platform,
        });
        let item = Arc::clone(&slf);
        item.ctx_scope.get_mut_static().set_data(0, slf.into_raw());

        item
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

pub struct ObscuredContextScope {
    ptr: NonNull<c_void>,
    lock: RwLock<()>,
}

unsafe impl Send for ObscuredContextScope {}
unsafe impl Sync for ObscuredContextScope {}

impl ObscuredContextScope {
    /// Create an obscured context scope.
    ///
    /// # Safety
    /// Needs to live as long as the context scope.
    /// Otherwise, segmentation fault is expected.
    #[inline(always)]
    pub fn new(ctx_scope: Box<v8::ContextScope<'_, '_, v8::HandleScope<'_>>>) -> Self {
        Self {
            ptr: unsafe { NonNull::new_unchecked(Box::into_raw(ctx_scope) as _) },
            lock: RwLock::new(()),
        }
    }

    /// Gets the context scope statically.
    ///
    /// Borrow checking would be disabled, and the
    /// lock is immediately freed after returning.
    #[inline(always)]
    pub fn get_static(
        &self,
    ) -> &'static v8::ContextScope<'static, 'static, v8::HandleScope<'static>> {
        unsafe { &*(self.ptr.as_ptr() as *mut v8::ContextScope<'_, '_, v8::HandleScope<'_>>) }
    }

    /// Gets the context scope statically, with mutability.
    ///
    /// Borrow checking would be disabled, and the
    /// lock is immediately freed after returning.
    #[inline(always)]
    pub fn get_mut_static(
        &self,
    ) -> &'static mut v8::ContextScope<'static, 'static, v8::HandleScope<'static>> {
        unsafe { &mut *(self.ptr.as_ptr() as *mut v8::ContextScope<'_, '_, v8::HandleScope<'_>>) }
    }

    /// Gets the context scope.
    #[inline(always)]
    pub async fn get<'a>(&'a self) -> ContextScopeGuard<'a> {
        let ptr_lock = self.lock.read().await;
        ContextScopeGuard((self.ptr, ptr_lock))
    }

    /// Gets the context scope as a mutable reference.
    #[inline(always)]
    pub async fn get_mut<'a>(&'a self) -> ContextScopeMutGuard<'a> {
        let ptr_lock = self.lock.write().await;
        ContextScopeMutGuard((self.ptr, ptr_lock))
    }
}

#[repr(transparent)]
pub struct ContextScopeGuard<'a>((NonNull<c_void>, RwLockReadGuard<'a, ()>));

impl<'a> Deref for ContextScopeGuard<'a> {
    type Target = v8::ContextScope<'a, 'a, v8::HandleScope<'a>>;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.0.0.as_ptr() as *mut v8::ContextScope<'_, '_, v8::HandleScope<'_>>) }
    }
}

#[repr(transparent)]
pub struct ContextScopeMutGuard<'a>((NonNull<c_void>, RwLockWriteGuard<'a, ()>));

impl<'a> Deref for ContextScopeMutGuard<'a> {
    type Target = v8::ContextScope<'a, 'a, v8::HandleScope<'a>>;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.0.0.as_ptr() as *mut v8::ContextScope<'_, '_, v8::HandleScope<'_>>) }
    }
}

impl<'a> DerefMut for ContextScopeMutGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self.0.0.as_ptr() as *mut v8::ContextScope<'_, '_, v8::HandleScope<'_>>) }
    }
}
