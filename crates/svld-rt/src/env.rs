use std::sync::Arc;

use v8::{Local, Object, PinScope};

use crate::worker::WorkerState;

/// Creates a JavaScript env (binding) interface.
#[inline(always)]
pub fn create_js_env<'s>(
    scope: &PinScope<'s, '_>,
    state: Arc<WorkerState>,
) -> Option<Local<'s, v8::Value>> {
    let obj = Object::new(scope);
    for (name, binding) in state.binding_store.list() {
        let client = binding.create_client(name);
        obj.set(
            scope,
            v8::String::new(scope, name)?.cast(),
            client.create_interface(scope)?,
        );
    }

    Some(obj.cast())
}
