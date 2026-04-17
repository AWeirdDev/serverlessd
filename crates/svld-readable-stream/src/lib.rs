mod controller;
mod source;
mod state;

use v8::{FunctionCallbackArguments, Global, Local, Object, PinScope, ReturnValue};

use svld_language::{ThrowException, throw, type_and_value};

use crate::{
    source::{UnderlyingSource, UnderlyingSourceParseError},
    state::ReadableStreamState,
};

struct JsReadableStream;

impl JsReadableStream {
    fn build_object<'s>(scope: &mut PinScope<'s, '_>) -> Local<'s, v8::Value> {
        let func_tmpl = v8::FunctionTemplate::new(scope, Self::constructor);

        let name = v8::String::new(scope, "ReadableStream").unwrap();
        func_tmpl.set_class_name(name);

        let instance_tmpl = func_tmpl.instance_template(scope);
        instance_tmpl.set_internal_field_count(1);

        let constructor = func_tmpl.get_function(scope).unwrap();

        constructor.cast()
    }

    fn constructor(scope: &mut PinScope, args: FunctionCallbackArguments, _rv: ReturnValue) {
        let source: Option<UnderlyingSource> = {
            let arg0 = args.get(0);
            match UnderlyingSource::new(scope, arg0) {
                Ok(src) => Some(src),
                Err(UnderlyingSourceParseError::Undefined) => None,
                Err(UnderlyingSourceParseError::NotObject) => {
                    throw(
                        scope,
                        ThrowException::type_error(format!(
                            "The \"source\" argument must be of type object. Received {}",
                            type_and_value(scope, arg0).format_to_string(scope)
                        )),
                    );
                    return;
                }
            }
        };

        // unpack callbacks
        let (start_fn, pull_fn, cancel_fn) = source
            .map(|s| (s.start, s.pull, s.cancel))
            .unwrap_or((None, None, None));

        let state = Box::new(ReadableStreamState::new(pull_fn, cancel_fn));
        let state_ptr = Box::into_raw(state);

        let ext = v8::External::new(scope, state_ptr as *mut std::ffi::c_void);
        args.this().set_internal_field(0, ext.cast());

        let gcontroller = Global::new(scope, controller);
        unsafe { &mut *state_ptr }.controller = Some(gcontroller);

        if let Some(start_fn) = start_fn {
            let start_local = Local::new(scope, &start_fn);
            let recv = v8::undefined(scope).cast::<v8::Value>();
            start_local.call(scope, recv, &[controller.cast()]);
        }
    }
}

// ── public API ────────────────────────────────────────────────────────────────
//
// Note: ReadableStreamState is intentionally not freed on JS GC — it lives for
// the duration of the isolate (workers are destroyed as a unit), so this is
// safe. Proper weak-ref cleanup can be added later via v8::Weak<T>.

/// Register `ReadableStream` as a property on `global`.
///
/// Call this during isolate setup alongside the other intrinsics.
pub fn register(scope: &mut PinScope, global: Local<Object>) {
    let ctor = JsReadableStream::build_object(scope);
    let key = v8::String::new(scope, "ReadableStream")
        .unwrap()
        .cast::<v8::Value>();
    global.set(scope, key, ctor);
}
