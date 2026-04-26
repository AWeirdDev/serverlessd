use std::{cell::RefCell, collections::HashMap, path::PathBuf};

use v8::{
    Function, FunctionCallback, FunctionCallbackArguments, Local, MapFnTo, Object, PinScope,
    ReturnValue,
};

use svld_language::{ThrowException, throw};

use crate::{WorkerState, bindings::Binding, blocks::Block};

/// Key-Value store binding.
#[allow(unused)]
pub struct JsKv {
    contents: RefCell<HashMap<String, String>>,
    storage: PathBuf,
}

impl JsKv {
    #[inline]
    pub fn new<P: Into<PathBuf>>(storage: P) -> Self {
        Self {
            contents: RefCell::new(HashMap::new()),
            storage: storage.into(),
        }
    }

    /// Puts a key from the KV.
    #[inline(always)]
    fn js_put(scope: &mut PinScope, args: FunctionCallbackArguments, _rv: ReturnValue) {
        let inner = move || -> Option<()> {
            let state = WorkerState::get_from_isolate(scope);

            let key = {
                let arg0 = args.get(0);
                if arg0.is_null_or_undefined() {
                    throw(scope, ThrowException::type_error("the key wasn't provided"));
                    return Some(());
                }

                arg0.to_string(scope)?.to_rust_string_lossy(scope)
            };

            let value = {
                let arg1 = args.get(1);
                if arg1.is_null_or_undefined() {
                    throw(
                        scope,
                        ThrowException::type_error(
                            "a value wasn't provided; use delete to delete values instead",
                        ),
                    );
                    return Some(());
                }

                arg1.to_string(scope)?.to_rust_string_lossy(scope)
            };

            state.blocks.with_block::<Self, _>(move |block| {
                let mut contents = block.contents.borrow_mut();
                contents.insert(key, value);
            });

            Some(())
        };

        inner();
    }

    /// Gets a key from the KV.
    #[inline(always)]
    fn js_get(scope: &mut PinScope, args: FunctionCallbackArguments, mut rv: ReturnValue) {
        let inner = move || -> Option<()> {
            let state = WorkerState::get_from_isolate(scope);

            let key = {
                let arg0 = args.get(0);
                if arg0.is_null_or_undefined() {
                    throw(scope, ThrowException::type_error("the key wasn't provided"));
                    return Some(());
                }

                arg0.to_string(scope)?.to_rust_string_lossy(scope)
            };

            state
                .blocks
                .with_block::<Self, _>(move |block| -> Option<()> {
                    let contents = block.contents.borrow();
                    let data = contents.get(&key);

                    if let Some(data) = data {
                        rv.set(v8::String::new(scope, data)?.cast());
                    }

                    Some(())
                });

            Some(())
        };

        inner();
    }
}

impl Block for JsKv {}

impl Binding for JsKv {
    fn get_js_value<'s>(scope: &mut PinScope<'s, '_>) -> Option<Local<'s, v8::Value>> {
        let obj = Object::new(scope);
        add_function(scope, obj, "put", Self::js_put)?;
        add_function(scope, obj, "get", Self::js_get)?;

        Some(obj.cast())
    }
}

#[must_use]
fn add_function<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'s, Object>,
    name: &'static str,
    fnk: impl MapFnTo<FunctionCallback>,
) -> Option<()> {
    let function = Function::new(scope, fnk)?.cast();
    object.set(scope, v8::String::new(scope, name)?.cast(), function);

    Some(())
}
