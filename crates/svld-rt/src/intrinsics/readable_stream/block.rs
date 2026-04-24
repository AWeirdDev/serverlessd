use std::{cell::RefCell, ptr::NonNull};

use crate::blocks::Block;

use crate::intrinsics::readable_stream::state::ReadableStreamState;

/// A tracker for every [`ReadableStreamState`] for safe dropping.
///
/// Register each newly-created stream with [`ReadableStreamBlock::register`].
///
/// # Safety
/// `!Sync`.
pub struct ReadableStreamBlock {
    ptrs: RefCell<Vec<NonNull<ReadableStreamState>>>,
}

impl ReadableStreamBlock {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            ptrs: RefCell::new(vec![]),
        }
    }

    #[inline(always)]
    pub(crate) fn register(&self, ptr: *mut ReadableStreamState) {
        self.ptrs
            .borrow_mut()
            .push(unsafe { NonNull::new_unchecked(ptr) });
    }
}

impl Block for ReadableStreamBlock {
    fn drop_block_data(slf: Box<dyn std::any::Any>)
    where
        Self: Sized + 'static,
    {
        let slf = unsafe { slf.downcast::<Self>().unwrap_unchecked() };

        let mut ptrs = slf.ptrs.borrow_mut();
        ptrs.drain(..).for_each(|ptr| {
            let _ = unsafe { Box::from_raw(ptr.as_ptr()) };
        });
    }
}
