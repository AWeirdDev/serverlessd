use std::hint;

use v8::{Local, PinScope, PromiseState};

pub enum Promised<'s> {
    Resolved(Local<'s, v8::Value>),
    Rejected(Local<'s, v8::Value>),
}

impl<'s> Promised<'s> {
    /// Create a new [`Promised`] for better promise state handling.
    ///
    /// # Safety
    /// The promise state **MUST NOT** be pending.
    pub fn new(scope: &PinScope<'s, '_>, promise: Local<'s, v8::Promise>) -> Self {
        match promise.state() {
            PromiseState::Fulfilled => Self::Resolved(promise.result(scope)),
            PromiseState::Rejected => Self::Rejected(promise.result(scope)),
            PromiseState::Pending => unsafe { hint::unreachable_unchecked() },
        }
    }
}
