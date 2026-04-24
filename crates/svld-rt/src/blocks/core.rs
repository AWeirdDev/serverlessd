use std::{any::Any, cell::RefCell};

use typeid::ConstTypeId;

type BlockMeta = (ConstTypeId, Box<dyn Any>, fn(Box<dyn Any>) -> ());

/// A container for blocks, ensuring memory safety with `drop()`.
///
/// Intrinsic blocks, such as the "replier," must be added.
///
/// # Time complexity
/// Searching is `O(N)`, but it's fine because the number of blocks should be small.
///
/// # Safety
#[repr(transparent)]
pub struct Blocks {
    blocks: RefCell<Vec<BlockMeta>>,
}

impl Blocks {
    /// Create a new block.
    ///
    /// It's **strongly** recommended that you should assign `N` to indicate the
    /// amount of capacity to allocate.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            blocks: RefCell::new(Vec::with_capacity(4)), // simple constant base
        }
    }

    /// Adds a block.
    #[inline(always)]
    pub fn add_block<T: Block + 'static>(self, block: T) -> Self {
        self.push_block(block);
        self
    }

    /// Pushes a block.
    #[inline(always)]
    pub fn push_block<T: Block + 'static>(&self, block: T) {
        let mut blocks = self.blocks.borrow_mut();
        blocks.push((T::TYPE, Box::new(block) as _, T::drop_block_data));
    }

    /// Runs callback on the block of type `T`.
    ///
    /// # Returns
    /// If any of the following is not satisfied, `None` is returned:
    /// - A value of type `T` is not found.
    /// - Currently borrowed mutably.
    ///
    /// # Safety
    /// `!Sync`.
    #[inline(always)]
    pub fn with_block<T: 'static, R>(&self, callback: impl FnOnce(&T) -> R) -> Option<R> {
        let id = ConstTypeId::of::<T>();

        let Ok(blocks) = self.blocks.try_borrow() else {
            return None;
        };
        blocks
            .iter()
            .find(|item| item.0 == id)
            .and_then(|item| item.1.downcast_ref::<T>())
            .map(|item| callback(item))
    }

    /// Runs callback on the block of type `T` without checking availability.
    #[inline(always)]
    pub unsafe fn with_block_unchecked<T: 'static, R>(&self, callback: impl FnOnce(&T) -> R) -> R {
        unsafe { self.with_block::<T, R>(callback).unwrap_unchecked() }
    }

    /// Checks if a block exists.
    pub fn has_block<T: 'static>(&self) -> bool {
        let id = ConstTypeId::of::<T>();
        let blocks = self.blocks.borrow();

        blocks.iter().find(|block| block.0 == id).is_some()
    }
}

impl Drop for Blocks {
    fn drop(&mut self) {
        let blocks = self.blocks.get_mut();
        blocks.drain(..).for_each(|item| {
            item.2(item.1);
        });
    }
}

/// A block.
pub trait Block {
    const TYPE: ConstTypeId = ConstTypeId::of::<Self>();

    /// Drop the block.
    fn drop_block_data(slf: Box<dyn Any>)
    where
        Self: Sized + 'static,
    {
        let _ = unsafe { slf.downcast::<Self>().unwrap_unchecked() };
    }
}
