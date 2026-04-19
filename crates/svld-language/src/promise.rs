use v8::{Local, PinScope, PromiseState};

pub enum Promised<'s> {
    Resolved(Local<'s, v8::Value>),
    Rejected(Local<'s, v8::Value>),
}

impl<'s> Promised<'s> {
    /// Create a new [`Promised`] for better promise state handling.
    ///
    /// Returns
    /// `Some(Promised)`. `None` if still pending.
    pub fn new(scope: &PinScope<'s, '_>, promise: Local<'s, v8::Promise>) -> Option<Self> {
        match promise.state() {
            PromiseState::Fulfilled => Some(Self::Resolved(promise.result(scope))),
            PromiseState::Rejected => Some(Self::Rejected(promise.result(scope))),
            PromiseState::Pending => None,
        }
    }
}
