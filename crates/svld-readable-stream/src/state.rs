use std::collections::VecDeque;

use v8::{Function, Global, Object, Value};

pub(crate) enum StreamInternalState {
    Readable,
    Closed,
    Errored(Global<Value>),
}

pub(crate) struct ReadableStreamState {
    /// Enqueued chunks waiting to be read.
    pub queue: VecDeque<Global<Value>>,

    /// Current stream state.
    pub state: StreamInternalState,

    /// Set when `controller.close()` is called; stream closes once queue drains.
    pub close_requested: bool,

    /// Cached controller object so `pull` can be re-invoked with it.
    pub controller: Option<Global<Object>>,

    /// `underlyingSource.pull` — called when queue drops below capacity.
    pub pull_fn: Option<Global<Function>>,

    /// `underlyingSource.cancel` — called when the consumer cancels the stream.
    pub cancel_fn: Option<Global<Function>>,
}

impl ReadableStreamState {
    #[inline(always)]
    pub fn new(pull_fn: Option<Global<Function>>, cancel_fn: Option<Global<Function>>) -> Self {
        Self {
            queue: VecDeque::new(),
            state: StreamInternalState::Readable,
            close_requested: false,
            controller: None,
            pull_fn,
            cancel_fn,
        }
    }

    /// Returns true if the stream is in the readable state.
    #[inline(always)]
    pub fn is_readable(&self) -> bool {
        matches!(self.state, StreamInternalState::Readable)
    }

    #[inline(always)]
    pub fn enqueue(&mut self, value: Global<Value>) {
        self.queue.push_back(value);
    }

    #[inline(always)]
    pub fn dequeue(&mut self) -> Option<Global<Value>> {
        self.queue.pop_front()
    }
}
