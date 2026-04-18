use std::any::Any;

use typeid::ConstTypeId;

type BlockMeta = (ConstTypeId, Box<dyn Any>, fn(Box<dyn Any>) -> ());

/// A container for blocks, ensuring memory safety with `drop()`.
///
/// Intrinsic blocks, such as the "replier," must be added.
///
/// # Time complexity
/// Searching is `O(N)`, but it's fine because the number of blocks should be small.
#[repr(transparent)]
pub struct Blocks {
    blocks: Vec<BlockMeta>,
}

impl Blocks {
    /// Create a new block.
    ///
    /// It's **strongly** recommended that you should assign `N` to indicate the
    /// amount of capacity to allocate.
    #[inline(always)]
    pub fn new<const N: usize>() -> Self {
        Self {
            blocks: Vec::with_capacity(N),
        }
    }

    /// Pushes an block.
    #[inline(always)]
    pub fn with_block<T: Block + 'static>(mut self, block: T) -> Self {
        self.blocks
            .push((T::TYPE, Box::new(block) as _, T::drop_block_data));
        self
    }

    /// Gets the block of type `T`.
    ///
    /// # Returns
    /// Some referenced value (`&T`). If any of the following
    /// is not satisfied, `None` is returned:
    ///
    /// - A value of type `T` is not found.
    #[inline(always)]
    pub fn get_block<T: 'static>(&self) -> Option<&T> {
        let id = ConstTypeId::of::<T>();

        self.blocks
            .iter()
            .find(|item| item.0 == id)
            .and_then(|item| item.1.downcast_ref())
    }

    /// Gets the block of type `T` without checking availability.
    #[inline(always)]
    pub unsafe fn get_block_unchecked<T: 'static>(&self) -> &T {
        unsafe { self.get_block::<T>().unwrap_unchecked() }
    }
}

impl Drop for Blocks {
    fn drop(&mut self) {
        self.blocks.drain(..).for_each(|item| {
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
