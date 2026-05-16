use std::{
    ptr::null_mut,
    sync::atomic::{self, AtomicPtr, AtomicU64},
};

/// Represents the states of available spaces of all pods.
///
/// It is a thin wrapper around `Vec<PodSpaceState>`.
/// See [`PodSpaceState`] for more information on its concurrency model.
#[derive(Debug)]
pub struct SpaceState {
    states: Vec<PodSpaceState>,
    available: PodIndexStack,
}

unsafe impl Sync for SpaceState {}
unsafe impl Send for SpaceState {}

impl SpaceState {
    #[inline(always)]
    pub fn new(n_pods: usize, n_workers: usize) -> Self {
        let mut states = Vec::with_capacity(n_pods);
        let available = PodIndexStack::new();

        for idx in 0..n_pods {
            states.push(PodSpaceState::new_blank(n_workers));
            available.prepend(idx);
        }

        Self { states, available }
    }

    /// Gets the space information of the given pod.
    #[inline(always)]
    pub fn get_pod_space(&self, pod_id: usize) -> Option<&PodSpaceState> {
        self.states.get(pod_id)
    }

    /// Indicates a release of pod space, updating the counts.
    ///
    /// If the pod was not found, an error (`PodSpaceError::PodNotFound`) is returned.
    #[inline]
    pub fn release_pod_space(&self, pod_id: usize) -> Result<(), PodSpaceError> {
        if let Some(space) = self.get_pod_space(pod_id) {
            let should_add_to_stack = space.release_one();
            if should_add_to_stack {
                self.available.prepend(pod_id);
            }
            Ok(())
        } else {
            Err(PodSpaceError::PodNotFound)
        }
    }

    /// Marks a use of space, updating the counts and returning the ID of the pod.
    ///
    /// If no space is available, `None` is returned.
    pub fn request_use_space(&self) -> Option<usize> {
        while let Some(pod_id) = self.available.pop() {
            let space = self.get_pod_space(pod_id).unwrap();
            match space.try_use_one() {
                Ok(()) => {
                    return Some(pod_id);
                }
                Err(_) => continue,
            }
        }

        None
    }
}

/// Represents the current state of available spaces (workers) in a pod.
///
/// Packs (absent: u16, sleeping: u16, running: u16) into a u64.
///
/// This is done to avoid [TOCTOU (time-of-check to time-of-use)](https://en.wikipedia.org/wiki/Time-of-check_to_time-of-use)
/// race condition. Under the hood, it can be treated as:
///
/// ```no_run
/// #[repr(C)]
/// struct PodSpaceStateRepr {
///     running: u16,
///     sleeping: u16,
///     absent: u16,
///     _pad: [u8; 16], // zeros
/// }
/// ```
///
/// # General layout
/// ```python
/// [    48-63    |      47-32      |       31-16       |      15-00      ]
/// [PAD: 16 bits] [absent: 16 bits] [sleeping: 16 bits] [running: 16 bits]
/// ```
///
/// Then, the data
#[derive(Debug)]
pub struct PodSpaceState {
    packed: AtomicU64,
}

unsafe impl Sync for PodSpaceState {}
unsafe impl Send for PodSpaceState {}

impl PodSpaceState {
    const SHIFT_ABSENT: u64 = 32;
    const SHIFT_SLEEPING: u64 = 16;
    const SHIFT_RUNNING: u64 = 0;
    const IN_STACK_BIT: u64 = 1 << 63;
    const MASK: u64 = 0xFFFF;

    #[inline(always)]
    const fn pack(absent: u64, sleeping: u64, running: u64) -> u64 {
        (absent << Self::SHIFT_ABSENT)
            | (sleeping << Self::SHIFT_SLEEPING)
            | (running << Self::SHIFT_RUNNING)
    }

    /// Unpacks the data.
    ///
    /// # Returns
    /// `(in_stack, absent, sleeping, running)`
    #[inline(always)]
    const fn unpack(packed: u64) -> (bool, u64, u64, u64) {
        (
            (packed & Self::IN_STACK_BIT != 0),
            (packed >> Self::SHIFT_ABSENT) & Self::MASK,
            (packed >> Self::SHIFT_SLEEPING) & Self::MASK,
            (packed >> Self::SHIFT_RUNNING) & Self::MASK,
        )
    }

    /// Creates a new blank state.
    #[inline(always)]
    pub const fn new_blank(n_workers: usize) -> Self {
        Self {
            packed: AtomicU64::new(Self::pack(n_workers as u64, 0, 0)),
        }
    }

    /// Attempts to use a vacant space, updating the counts.
    pub fn try_use_one(&self) -> Result<(), PodSpaceError> {
        let mut current = self.packed.load(atomic::Ordering::Acquire);

        loop {
            let (_, absent, sleeping, running) = Self::unpack(current);

            let new_packed = {
                if absent > 0 {
                    Self::pack(absent - 1, sleeping, running + 1)
                } else if sleeping > 0 {
                    Self::pack(absent, sleeping - 1, running + 1)
                } else {
                    return Err(PodSpaceError::NoVacancies);
                }
            };

            match self.packed.compare_exchange_weak(
                current,
                new_packed,
                atomic::Ordering::AcqRel,
                atomic::Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(n) => current = n,
            }
        }
    }

    /// Releases a vacant space (from a running one), updating the counts.
    ///
    /// # Returns
    /// A boolean indicating whether to add to the stack.
    #[must_use]
    fn release_one(&self) -> bool {
        let mut current = self.packed.load(atomic::Ordering::Acquire);

        loop {
            let (in_stack, absent, sleeping, running) = Self::unpack(current);

            let new_packed = Self::pack(absent, sleeping + 1, running - 1);

            match self.packed.compare_exchange_weak(
                current,
                new_packed | Self::IN_STACK_BIT,
                atomic::Ordering::AcqRel,
                atomic::Ordering::Acquire,
            ) {
                Ok(_) => break !in_stack,
                Err(n) => current = n,
            }
        }
    }
}

/// An atomic stack of pod indices.
///
/// The implementation carries a known ABA problem, which generally can be ignored with
/// [`PodSpaceState::try_use_one`] in place as it already avoids race conditions.
#[derive(Debug)]
struct PodIndexStack {
    head: AtomicPtr<StackItem>,
}

impl PodIndexStack {
    /// Creates an empty stack.
    const fn new() -> Self {
        Self {
            head: AtomicPtr::new(null_mut()),
        }
    }

    /// Prepends a pod index to the stack.
    fn prepend(&self, x: usize) {
        let item = Box::into_raw(Box::new(StackItem {
            data: x,
            next: null_mut(),
        }));

        loop {
            let head = self.head.load(atomic::Ordering::Acquire);
            unsafe { (*item).next = head };

            match self.head.compare_exchange_weak(
                head,
                item,
                atomic::Ordering::AcqRel,
                atomic::Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(_) => continue,
            }
        }
    }

    /// Pops a pod index from the stack.
    #[inline]
    fn pop(&self) -> Option<usize> {
        loop {
            let item_ptr = self.head.load(atomic::Ordering::Acquire);
            if item_ptr.is_null() {
                break None;
            } else {
                let item = unsafe { &*item_ptr };
                match self.head.compare_exchange_weak(
                    item_ptr,
                    unsafe { (&*item_ptr).next },
                    atomic::Ordering::AcqRel,
                    atomic::Ordering::Relaxed,
                ) {
                    Ok(_) => break Some(item.data),
                    Err(_) => continue,
                }
            }
        }
    }
}

impl Drop for PodIndexStack {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}

#[derive(Debug)]
struct StackItem {
    data: usize,
    next: *mut StackItem,
}

unsafe impl Sync for StackItem {}
unsafe impl Send for StackItem {}

#[derive(Debug, thiserror::Error)]
pub enum PodSpaceError {
    #[error("no vacancies available")]
    NoVacancies,

    #[error("the pod was not found")]
    PodNotFound,
}
