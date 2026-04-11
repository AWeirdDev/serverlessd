use std::{mem, num::NonZeroUsize, ptr::NonNull};

use tokio::sync::mpsc;

use crate::runtime::{
    Pod,
    worker::{
        task::{WarmUpWorkerArgs, create_cancel_safe_task},
        trigger::{WorkerTrigger, WorkerTx},
    },
};

/// A referenced-counted handle to the serverless worker.
#[derive(Clone)]
#[repr(transparent)]
pub struct WorkerHandle {
    pub(super) tx: WorkerTx,
}

impl WorkerHandle {
    /// Start a new worker.
    #[inline]
    pub fn start(pod: &Pod) -> Self {
        let (tx, rx) = mpsc::channel::<WorkerTrigger>(1);

        let monitor_handle = pod.monitor.clone();

        pod.tasks
            .spawn_local(create_cancel_safe_task(WarmUpWorkerArgs {
                pod_tx: pod.tx.clone(),
                worker_tx: tx.clone(),
                worker_rx: rx,
                monitor_handle,
            }));

        Self { tx }
    }

    /// Trigger.
    ///
    /// Returns `false` if the channel is closed.
    #[inline(always)]
    #[must_use]
    pub async fn trigger(&self, trigger: WorkerTrigger) -> bool {
        self.tx.send(trigger).await.is_ok()
    }
}

impl std::fmt::Debug for WorkerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorkerHandle")
    }
}

/// Represents the state of a non-destroyed worker, either sleeping or working.
#[repr(usize)]
#[derive(Debug, PartialEq, Eq)]
pub enum WorkerState {
    Sleeping = 0,
    Working = 1,
}

/// A worker handle with its state attached.
pub struct WorkerHandleWithState(NonNull<WorkerHandle>);

impl WorkerHandleWithState {
    /// Creates a worker handle with the worker state attached.
    ///
    /// # Mechanism
    /// Under the hood, we can take use heap allocation characteristics
    /// and use pointer tagging to make things even more compact.
    pub fn new(handle: Box<WorkerHandle>, state: WorkerState) -> Self {
        let ptr = Box::into_raw(handle);

        let mut me = Self(unsafe {
            NonNull::new_unchecked(ptr as *mut _) // it doesn't really matter if we did this
        });
        me.set_state(state);

        me
    }

    /// Sets the state representation of the worker.
    #[inline(always)]
    pub fn set_state(&mut self, state: WorkerState) {
        self.0 = self.0.map_addr(|addr| {
            let raw = addr.get();
            let tagged = (raw & !1) | (state as usize);
            unsafe { NonZeroUsize::new(tagged).unwrap_unchecked() }
        });
    }

    #[inline(always)]
    pub fn get_state(&self) -> WorkerState {
        unsafe { mem::transmute(self.0.addr().get() & 1) }
    }

    #[inline(always)]
    fn get_ptr(&self) -> *mut WorkerHandle {
        self.0
            .map_addr(|addr| {
                let raw = addr.get() & !1;
                unsafe { NonZeroUsize::new(raw).unwrap_unchecked() }
            })
            .as_ptr()
    }
}

impl Drop for WorkerHandleWithState {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.get_ptr()) };
    }
}

impl AsRef<WorkerHandle> for WorkerHandleWithState {
    #[inline(always)]
    fn as_ref(&self) -> &WorkerHandle {
        unsafe { &*self.get_ptr() }
    }
}

impl std::ops::Deref for WorkerHandleWithState {
    type Target = WorkerHandle;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl std::fmt::Debug for WorkerHandleWithState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorkerHandle({:?})", self.get_state())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_handle_with_state() {
        let (tx, _) = mpsc::channel(1);
        let mut handle =
            WorkerHandleWithState::new(Box::new(WorkerHandle { tx }), WorkerState::Working);

        assert_eq!(handle.get_state(), WorkerState::Working);

        handle.set_state(WorkerState::Sleeping);
        assert_eq!(handle.get_state(), WorkerState::Sleeping);

        let worker = handle.as_ref();
        let _ = worker.clone();
    }
}
