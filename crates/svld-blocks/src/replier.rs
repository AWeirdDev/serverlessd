use std::{cell::OnceCell, ptr::NonNull};

use tokio::sync::oneshot;

use crate::Block;

#[derive(Debug, thiserror::Error)]
pub enum ReplyError {
    #[error("The worker timed out.")]
    TimedOut,
}

/// The reply (type) to an HTTP event.
pub type Reply = Result<String, ReplyError>;

/// The replier to an HTTP event.
pub type Replier = oneshot::Sender<Reply>;

/// Some replier or `None`, depending on whether it's consumed.
pub type MaybeReplier = Option<Replier>;
type MaybeReplierNonNull = NonNull<MaybeReplier>;
type MaybeReplierPtr = *mut MaybeReplier;

/// # Intrinsic Extension "Replier"
/// It **must** be added to each worker state at all times.
///
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
pub struct ReplierBlock {
    pub replier: OnceCell<MaybeReplierNonNull>,
}

impl ReplierBlock {
    /// Create the extension.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            replier: OnceCell::new(),
        }
    }

    /// Sets the replier.
    ///
    /// This operation may only be taken once.
    #[inline]
    pub fn set_replier(&self, replier_ptr: MaybeReplierPtr) {
        self.replier
            .set(unsafe { NonNull::new_unchecked(replier_ptr) })
            .ok();
    }
}

impl Block for ReplierBlock {
    fn drop_block_data(slf: Box<dyn std::any::Any>)
    where
        Self: Sized + 'static,
    {
        let slf = unsafe { slf.downcast::<Self>().unwrap_unchecked() };

        let mut maybe_replier = slf.replier.get();

        if let Some(replier) = maybe_replier.as_mut() {
            if unsafe { &*replier.as_ptr() }.is_some() {
                let item = unsafe { &mut *replier.as_ptr() };
                if let Some(item) = item.take() {
                    // if there's nothing, we send blank data
                    item.send(Err(ReplyError::TimedOut)).ok();
                }
            }
        }
    }
}
