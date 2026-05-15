#[allow(unused)]
mod _priv {
    use std::sync::Arc;

    use v8::{Local, Object, PinScope, Value};

    use crate::worker::WorkerState;

    /// The JavaScript runtime `env`.
    pub struct JsEnv<'s> {
        obj: Local<'s, Object>,
        state: Arc<WorkerState>,
    }

    impl<'s> JsEnv<'s> {
        #[inline(always)]
        #[must_use]
        pub fn new(scope: &PinScope<'s, '_>, state: Arc<WorkerState>) -> Self {
            Self {
                obj: Object::new(scope),
                state,
            }
        }

        /// Builds the env, returning the JavaScript value.
        #[inline(always)]
        #[must_use]
        pub fn get_js_value(self) -> Local<'s, Value> {
            self.obj.cast()
        }
    }
}
