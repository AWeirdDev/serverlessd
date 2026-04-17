use std::any::Any;

use typeid::ConstTypeId;

type ExtensionMeta = (ConstTypeId, Box<dyn Any>, fn(Box<dyn Any>) -> ());

/// A container for extensions, ensuring memory safety with `drop()`.
///
/// Intrinsic extensions, such as the "replier," must be added.
///
/// # Time complexity
/// Searching is `O(N)`, but it's fine because the number of extensions should be small.
#[repr(transparent)]
pub struct WorkerStateExtensions {
    extensions: Vec<ExtensionMeta>,
}

impl WorkerStateExtensions {
    /// Create a new worker state extension.
    ///
    /// It's **strongly** recommended that you should assign `N` to indicate the
    /// amount of capacity to allocate.
    #[inline(always)]
    pub fn new<const N: usize>() -> Self {
        Self {
            extensions: Vec::with_capacity(N),
        }
    }

    /// Pushes an extension.
    #[inline(always)]
    pub fn with_extension<T: WorkerStateExtension + 'static>(mut self, extension: T) -> Self {
        self.extensions
            .push((T::TYPE, Box::new(extension) as _, T::drop_extension_data));
        self
    }

    /// Gets the extension of type `T`.
    ///
    /// # Returns
    /// Some referenced value (`&T`). If any of the following
    /// is not satisfied, `None` is returned:
    ///
    /// - A value of type `T` is not found.
    #[inline(always)]
    pub fn get_extension<T: 'static>(&self) -> Option<&T> {
        let id = ConstTypeId::of::<T>();

        self.extensions
            .iter()
            .find(|item| item.0 == id)
            .and_then(|item| item.1.downcast_ref())
    }

    /// Gets the extension of type `T` without checking availability.
    #[inline(always)]
    pub unsafe fn get_extension_unchecked<T: 'static>(&self) -> &T {
        unsafe { self.get_extension::<T>().unwrap_unchecked() }
    }
}

impl Drop for WorkerStateExtensions {
    fn drop(&mut self) {
        self.extensions.drain(..).for_each(|item| {
            item.2(item.1);
        });
    }
}

/// A worker state extension.
pub trait WorkerStateExtension {
    const TYPE: ConstTypeId = ConstTypeId::of::<Self>();

    /// Drop the worker state extension.
    fn drop_extension_data(slf: Box<dyn Any>)
    where
        Self: Sized + 'static,
    {
        let _ = unsafe { slf.downcast::<Self>().unwrap_unchecked() };
    }
}
