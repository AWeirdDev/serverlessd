use std::cell::RefCell;

use svld_blocks::Block;

use crate::state::ReadableStreamState;

/// Tracks every [`ReadableStreamState`] heap allocation so they are freed when
/// the worker state (and its [`Blocks`]) is dropped.
///
/// Register each newly-created stream with [`ReadableStreamBlock::register`].
///
/// The block is `!Send` because raw pointers and `RefCell` are `!Send`, which
/// is fine: V8 isolates are single-threaded and live on a `LocalSet`.
pub struct ReadableStreamBlock {
    ptrs: RefCell<Vec<*mut ReadableStreamState>>,
}

impl ReadableStreamBlock {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            ptrs: RefCell::new(Vec::new()),
        }
    }

    /// Register a stream state pointer for cleanup on worker drop.
    #[inline(always)]
    pub(crate) fn register(&self, ptr: *mut ReadableStreamState) {
        self.ptrs.borrow_mut().push(ptr);
    }
}

impl Block for ReadableStreamBlock {
    fn drop_block_data(slf: Box<dyn std::any::Any>)
    where
        Self: Sized + 'static,
    {
        let slf = unsafe { slf.downcast::<Self>().unwrap_unchecked() };
        for ptr in slf.ptrs.borrow_mut().drain(..) {
            // SAFETY: every pointer was produced by Box::into_raw and has not
            // been freed yet (stream lives for the duration of the isolate).
            let _ = unsafe { Box::from_raw(ptr) };
        }
    }
}
