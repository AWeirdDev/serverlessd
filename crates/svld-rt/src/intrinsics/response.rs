use std::{mem, ptr::NonNull};

use svld_language::{ThrowException, throw};
use v8::{Global, Local, Value};

use crate::try_catch;

pub struct JsResponse;

#[allow(unused)]
impl JsResponse {
    #[inline(always)]
    #[must_use]
    pub fn builder<'s>(scope: &mut v8::PinScope<'s, '_>) -> ResponseBuilder<'s> {
        ResponseBuilder::new(scope)
    }

    pub fn get_new_fn<'s>(scope: &v8::PinScope<'s, '_>) -> Option<Local<'s, Value>> {
        let function_template = v8::FunctionTemplate::new(scope, Self::js_constructor);
        let function = function_template.get_function(scope)?;

        Some(function.cast())
    }

    fn js_constructor(
        scope: &mut v8::PinScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        fn inner(
            scope: &mut v8::PinScope,
            args: v8::FunctionCallbackArguments,
            mut rv: v8::ReturnValue,
        ) -> Option<()> {
            let this = args.this();

            let body = args.get(0);
            let options = {
                let arg = args.get(1);
                if arg.is_object() && !arg.is_null_or_undefined() {
                    Some(arg.cast::<v8::Object>())
                } else {
                    None
                }
            };

            this.set(scope, v8::String::new(scope, "body")?.cast(), body);

            rv.set(this.cast());

            Some(())
        }

        inner(scope, args, rv);
    }

    /// `json()` static method.
    fn static_json(
        scope: &mut v8::PinScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        fn inner(
            scope: &mut v8::PinScope,
            args: v8::FunctionCallbackArguments,
            mut rv: v8::ReturnValue,
        ) -> Option<()> {
            let data = args.get(0);
            let options = args.get(1);

            if data.is_null_or_undefined() && options.is_null_or_undefined() {
                throw(
                    scope,
                    ThrowException::type_error(
                        "Response.json: At least 1 argument required, but only 0 passed",
                    ),
                );
                return None;
            }

            let json_text = {
                try_catch!(scope: scope, let try_catch);

                let Some(result) = v8::json::stringify(try_catch, data) else {
                    if let Some(exc) = try_catch.exception() {
                        rv.set(exc);
                    }
                    return None;
                };

                result
            };

            let resp = JsResponse::builder(scope)
                .body(scope, json_text.cast())?
                .build(scope)?;
            rv.set(resp.cast());

            Some(())
        }

        inner(scope, args, rv);
    }

    fn instance_json(
        scope: &mut v8::PinScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
    }
}

#[repr(transparent)]
pub struct ResponseBuilder<'s> {
    this: Local<'s, v8::Object>,
}

#[allow(unused)]
impl<'s> ResponseBuilder<'s> {
    /// Creates a new response builder.
    #[inline]
    #[must_use]
    pub fn new(scope: &v8::PinScope<'s, '_>) -> Self {
        Self {
            this: v8::Object::new(scope),
        }
    }

    #[must_use]
    pub fn status(self, scope: &v8::PinScope<'s, '_>, code: u16) -> Option<Self> {
        let code_k = v8::String::new(scope, "status")?;
        self.this.set(
            scope,
            code_k.cast(),
            v8::Number::new(scope, code as f64).cast(),
        );

        Some(self)
    }

    #[must_use]
    pub fn url<K: AsRef<str>>(self, scope: &v8::PinScope<'s, '_>, url: K) -> Option<Self> {
        let url_k = v8::String::new(scope, "url")?;

        let url = v8::String::new(scope, url.as_ref())?;
        self.this.set(scope, url_k.cast(), url.cast());

        Some(self)
    }

    #[must_use]
    pub fn type_<K: AsRef<str>>(self, scope: &v8::PinScope<'s, '_>, name: K) -> Option<Self> {
        let type_k = v8::String::new(scope, "type")?;

        let url = v8::String::new(scope, name.as_ref())?;
        self.this.set(scope, type_k.cast(), url.cast());

        Some(self)
    }

    #[must_use]
    pub fn redirected(self, scope: &v8::PinScope<'s, '_>, redirected: bool) -> Option<Self> {
        let redir_k = v8::String::new(scope, "redirected")?;

        self.this.set(
            scope,
            redir_k.cast(),
            v8::Boolean::new(scope, redirected).cast(),
        );

        Some(self)
    }

    pub fn body(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        data: Local<'s, v8::Value>,
    ) -> Option<Self> {
        let body_k = v8::String::new(scope, "body")?;

        let rs = {
            let inner = scope.get_data(1);
            if inner.is_null() {
                return None;
            }

            let gintrinsics =
                unsafe { Global::from_raw(scope, NonNull::new_unchecked(inner as *mut v8::Value)) };

            let intrinsics = Local::new(scope, gintrinsics);
            mem::forget(Global::new(scope, intrinsics));
            intrinsics
                .cast::<v8::Object>()
                .get(scope, v8::String::new(scope, "ReadableStream")?.cast())?
                .cast::<v8::Function>()
        };

        self.this.set(
            scope,
            body_k.cast(),
            rs.call(scope, v8::undefined(scope).cast(), &[data])?,
        );

        Some(self)
    }

    #[must_use]
    pub fn build(self, scope: &v8::PinScope<'s, '_>) -> Option<Local<'s, v8::Object>> {
        // derived values
        // .ok
        {
            let status = self
                .this
                .get(scope, v8::String::new(scope, "status")?.cast())?;

            self.this.set(scope, v8::String::new(scope, "ok")?.cast(), {
                if status.is_null_or_undefined() {
                    v8::Number::new(scope, 200 as f64).cast()
                } else {
                    status
                }
            });
        }

        // state values
        {
            self.this.set(
                scope,
                v8::String::new(scope, "bodyUsed")?.cast(),
                v8::Boolean::new(scope, false).cast(),
            );
        }

        Some(self.this)
    }
}
