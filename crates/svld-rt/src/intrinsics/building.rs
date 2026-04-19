use v8::{GetPropertyNamesArgs, Global, Isolate, Local, PinScope};

use crate::{intrinsics, scope_with_context};

fn add_to_scope<'s>(
    scope: &PinScope<'s, '_>,
    obj: Local<'s, v8::Object>,
    name: &'static str,
    value: Local<'s, v8::Value>,
) -> Option<()> {
    let k = v8::String::new(scope, name)?;
    obj.set(scope, k.into(), value.into());

    Some(())
}

/// Build intrinsics and store them in a [`Global`]-sealed [`v8::Value`].
#[must_use]
pub fn build_intrinsics(isolate: &mut Isolate) -> Option<Global<v8::Value>> {
    scope_with_context!(
        isolate: isolate,
        let &mut scope,
        let context
    );

    let intrinsics_obj = v8::Object::new(scope);

    // fetch()
    {
        let f = v8::Function::new(scope, intrinsics::fetch)?;
        add_to_scope(scope, intrinsics_obj, "fetch", f.cast());
    }

    // ReadableStream
    {
        let rs = intrinsics::JsReadableStream::get_new_fn(scope)?;
        add_to_scope(scope, intrinsics_obj, "ReadableStream", rs.cast());
    }

    // Response
    {
        let rs = intrinsics::JsResponse::get_new_fn(scope)?;
        add_to_scope(scope, intrinsics_obj, "Response", rs.cast());
    }

    // dev only
    if cfg!(debug_assertions) {
        let f = v8::Function::new(scope, intrinsics::point)?.cast();
        add_to_scope(scope, intrinsics_obj, "point", f);
    }

    Some(Global::new(scope, intrinsics_obj.cast()))
}

/// Extract intrinsics to the scope so it can be used by the user.
pub fn extract_intrinsics(
    scope: &PinScope,
    context_global: Local<v8::Object>,
    intrinsics: Global<v8::Value>,
) -> Option<()> {
    let intrinsics = Local::new(scope, intrinsics).cast::<v8::Object>();
    let names = intrinsics.get_own_property_names(scope, GetPropertyNamesArgs::default())?;

    for idx in 0..names.length() {
        let name = names.get_index(scope, idx)?;
        let item = intrinsics.get(scope, name)?;
        context_global.set(scope, name, item);
    }

    Some(())
}
