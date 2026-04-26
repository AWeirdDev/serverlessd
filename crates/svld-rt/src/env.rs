use std::sync::Arc;

use v8::{Local, Object, PinScope, Value};

use crate::{WorkerState, bindings::BindingClient};

/// The JavaScript runtime `env`.
pub struct JsEnv<'s> {
    obj: Local<'s, Object>,
    state: Arc<WorkerState>,
}

impl<'s> JsEnv<'s> {
    #[inline(always)]
    #[must_use]
    pub fn builder(scope: &PinScope<'s, '_>, state: Arc<WorkerState>) -> Self {
        Self {
            obj: Object::new(scope),
            state,
        }
    }

    /// Adds a binding to the `env`.
    #[must_use]
    pub fn add_binding<B: BindingClient + 'static>(
        self,
        scope: &mut PinScope<'s, '_>,
        key: &str,
        binding: B,
    ) -> Option<Self> {
        let value = B::get_js_value(scope)?;

        self.obj
            .set(scope, v8::String::new(scope, key)?.cast(), value);
        self.state.blocks.push_block(binding);

        Some(self)
    }

    /// Builds the env, returning the JavaScript value.
    #[inline(always)]
    #[must_use]
    pub fn build(self) -> Local<'s, Value> {
        self.obj.cast()
    }
}
