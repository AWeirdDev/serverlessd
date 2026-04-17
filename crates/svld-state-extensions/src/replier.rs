use std::{cell::RefCell, ptr::NonNull};

use tokio::sync::oneshot;

use crate::WorkerStateExtension;

// TODO: change String to something more meaningful and versatile,
// like a Result.
type MaybeReplier = NonNull<Option<oneshot::Sender<String>>>;

/// Safely wrapped replier for replying to HTTP requests from worker requests.
///
/// For example, when a request comes in, we need to reply.
/// In order to reply, we create a channel (replier) using `oneshot::Sender<T>`.
/// However, we may leak memory if the replier is never used, so we need a container
/// to safely drop the replier.
/// `ReplierWorkerExtension` provides an extension interface exactly for this.
///
/// ```no_run
/// let extensions = WorkerStateExtensions::new::<1>()
///     .with_extension(ReplierWorkerExtension::new());
/// ```
///
/// This is `!Sync`.
#[repr(transparent)]
pub struct ReplierWorkerStateExtension {
    pub replier: RefCell<Option<MaybeReplier>>,
}

impl ReplierWorkerStateExtension {
    /// Create the extension.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            replier: RefCell::new(None),
        }
    }

    /// Sets the replier.
    #[inline]
    pub fn set_replier(&self, replier_ptr: *mut Option<oneshot::Sender<String>>) {
        let mut shell = self.replier.borrow_mut();
        let ptr = unsafe { NonNull::new_unchecked(replier_ptr) };
        shell.replace(ptr);
    }
}

impl WorkerStateExtension for ReplierWorkerStateExtension {
    fn drop_extension_data(slf: Box<dyn std::any::Any>)
    where
        Self: Sized + 'static,
    {
        let slf = unsafe { slf.downcast::<Self>().unwrap_unchecked() };
        let mut maybe_replier = slf.replier.borrow_mut();

        if let Some(replier) = maybe_replier.as_mut() {
            if unsafe { &*replier.as_ptr() }.is_some() {
                let item = unsafe { &mut *replier.as_ptr() };
                if let Some(item) = item.take() {
                    // if there's nothing, we send blank data
                    item.send(String::new()).ok();
                }
            }
        }
    }
}
