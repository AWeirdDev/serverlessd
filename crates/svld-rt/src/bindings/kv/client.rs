use tokio::sync::oneshot;
use v8::{
    Function, FunctionCallback, FunctionCallbackArguments, Global, Local, MapFnTo, Object,
    PinScope, PromiseResolver, ReturnValue,
};

use svld_language::{ThrowException, throw};

use crate::{
    WorkerState,
    bindings::{BindingBackendMessage, BindingBackendTx, BindingClient, kv::backend::KvPayload},
    blocks::Block,
};

/// Key-Value store binding.
#[repr(transparent)]
pub struct JsKv {
    tx: BindingBackendTx,
}

impl JsKv {
    /// Creates a new JavaScript KV binding client.
    #[inline]
    pub fn new(tx: BindingBackendTx) -> Self {
        Self { tx }
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

            let resolver = PromiseResolver::new(scope)?;
            let gresolver = Global::new(scope, resolver);

            let tx = state
                .blocks
                .with_block::<Self, _>(move |block| block.tx.clone())
                .unwrap();

            let Ok(data) = ijson::to_value(KvPayload::Put { key, value }) else {
                throw(
                    scope,
                    ThrowException::error("failed to create payload internally"),
                );
                return None;
            };

            let state2 = state.clone();
            state.tasks.spawn_local(async move {
                let (reply, recv) = oneshot::channel();
                let message = BindingBackendMessage::builder()
                    .worker("whatever".to_string())
                    .data(data)
                    .replier(reply)
                    .build();

                tx.send(message).ok();
                recv.await.ok();
                state2.schedule_resolution_and_tick(
                    gresolver,
                    Ok(Box::new(|scope| v8::undefined(scope).cast())),
                );
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

            let tx = state
                .blocks
                .with_block::<Self, _>(move |block| block.tx.clone())
                .unwrap();

            Some(())
        };

        inner();
    }
}

impl Block for JsKv {}

impl BindingClient for JsKv {
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
