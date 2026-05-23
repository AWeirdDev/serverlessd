use std::sync::Arc;

use v8::{Local, Object, PinScope};

use crate::{models::BindingConfig, worker::WorkerState};

/// Creates a JavaScript env (binding) interface.
#[inline(always)]
pub fn create_js_env<'s>(
    scope: &PinScope<'s, '_>,
    state: Arc<WorkerState>,
    mut bindings: Vec<BindingConfig>,
) -> Option<Local<'s, v8::Value>> {
    let obj = Object::new(scope);
    for BindingConfig { name, type_ } in bindings.drain(..) {
        tracing::info!("js env got name={name:?}, type={type_:?}");
        let binding = state.binding_store.get_binding(&type_)?;

        let client = binding.create_client(&name);

        let interface = client.create_interface(scope)?;

        obj.set(scope, v8::String::new(scope, &name)?.cast(), interface);
    }

    Some(obj.cast())
}
