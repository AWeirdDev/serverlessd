use std::{ffi::c_void, ptr::NonNull};

use svld_language::{ThrowException, throw};
use v8::{Global, Local, Value};

use crate::{intrinsics::readable_stream::JsReadableStream, try_catch};

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

        let name = v8::String::new(scope, "Response")?;
        function_template.set_class_name(name);

        {
            let k = v8::String::new(scope, "json")?;
            let f = v8::FunctionTemplate::new(scope, Self::js_static_json);
            function_template.set(k.cast(), f.cast());
        }

        {
            let proto = function_template.prototype_template(scope);
            {
                let f = v8::FunctionTemplate::new(scope, Self::js_instance_text);
                let k = v8::String::new(scope, "text")?;
                proto.set(k.cast(), f.cast());
            }
            {
                let f = v8::FunctionTemplate::new(scope, Self::js_instance_json);
                let k = v8::String::new(scope, "json")?;
                proto.set(k.cast(), f.cast());
            }
            {
                let f = v8::FunctionTemplate::new(scope, Self::js_instance_array_buffer);
                let k = v8::String::new(scope, "arrayBuffer")?;
                proto.set(k.cast(), f.cast());
            }
        }

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

            let status = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "status")?.cast()))
                .filter(|s| s.is_number())
                .and_then(|s| s.number_value(scope))
                .map(|n| n as u16)
                .unwrap_or(200u16);
            this.set(
                scope,
                v8::String::new(scope, "status")?.cast(),
                v8::Number::new(scope, status as f64).cast(),
            );

            let status_text = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "statusText")?.cast()))
                .filter(|s| s.is_string())
                .and_then(|s| s.to_string(scope))
                .unwrap_or_else(|| v8::String::new(scope, "").unwrap());
            this.set(
                scope,
                v8::String::new(scope, "statusText")?.cast(),
                status_text.cast(),
            );

            // derived value
            this.set(
                scope,
                v8::String::new(scope, "ok")?.cast(),
                v8::Boolean::new(scope, status >= 200 && status < 300).cast(),
            );

            // state value
            this.set(
                scope,
                v8::String::new(scope, "bodyUsed")?.cast(),
                v8::Boolean::new(scope, false).cast(),
            );

            if !body.is_null_or_undefined() {
                this.set(scope, v8::String::new(scope, "body")?.cast(), body);
                if body.is_string() {
                    this.set(scope, v8::String::new(scope, "__bodyText__")?.cast(), body);
                }
            }

            rv.set(this.cast());
            Some(())
        }

        inner(scope, args, rv);
    }

    /// `Response.json()` static method.
    fn js_static_json(
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
                .status(scope, 200)?
                .body(scope, json_text.cast())?
                .build(scope)?;
            rv.set(resp.cast());

            Some(())
        }

        inner(scope, args, rv);
    }

    /// ```js
    /// response.text()
    /// ```
    ///
    /// Resolves with the body as a UTF-8 string.
    fn js_instance_text(
        scope: &mut v8::PinScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let this = args.this();
        let text_v: Local<Value> = v8::String::new(scope, "__bodyText__")
            .and_then(|k| this.get(scope, k.cast()))
            .filter(|v| v.is_string())
            .unwrap_or_else(|| v8::String::new(scope, "").unwrap().cast());
        let Some(resolver) = v8::PromiseResolver::new(scope) else {
            return;
        };
        resolver.resolve(scope, text_v);
        rv.set(resolver.get_promise(scope).cast());
    }

    /// `response.arrayBuffer()` — resolves with the raw ArrayBuffer.
    fn js_instance_array_buffer(
        scope: &mut v8::PinScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let this = args.this();
        let ab_v: Local<Value> = v8::String::new(scope, "__bodyAB__")
            .and_then(|k| this.get(scope, k.cast()))
            .filter(|v| v.is_array_buffer())
            .unwrap_or_else(|| v8::ArrayBuffer::new(scope, 0).cast());
        let Some(resolver) = v8::PromiseResolver::new(scope) else {
            return;
        };
        resolver.resolve(scope, ab_v);
        rv.set(resolver.get_promise(scope).cast());
    }

    /// `response.json()` — resolves with the body parsed as JSON.
    fn js_instance_json(
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
            let text_str: v8::Local<v8::String> = v8::String::new(scope, "__bodyText__")
                .and_then(|k| this.get(scope, k.cast()))
                .filter(|v| v.is_string())
                .map(|v| v.cast())
                .unwrap_or_else(|| v8::String::new(scope, "null").unwrap());

            let parsed = {
                try_catch!(scope: scope, let tc);
                match v8::json::parse(tc, text_str) {
                    Some(v) => v,
                    None => {
                        let exc = tc.exception().unwrap_or_else(|| {
                            v8::String::new(tc, "JSON parse error").unwrap().cast()
                        });
                        let resolver = v8::PromiseResolver::new(tc)?;
                        resolver.reject(tc, exc);
                        rv.set(resolver.get_promise(tc).cast());
                        return Some(());
                    }
                }
            };

            let resolver = v8::PromiseResolver::new(scope)?;
            resolver.resolve(scope, parsed);
            rv.set(resolver.get_promise(scope).cast());
            Some(())
        }

        inner(scope, args, rv);
    }
}

#[repr(transparent)]
pub struct ResponseBuilder<'s> {
    this: Local<'s, v8::Object>,
}

#[allow(unused)]
impl<'s> ResponseBuilder<'s> {
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
    pub fn status_text<K: AsRef<str>>(self, scope: &v8::PinScope<'s, '_>, text: K) -> Option<Self> {
        let k = v8::String::new(scope, "statusText")?;
        let v = v8::String::new(scope, text.as_ref())?;
        self.this.set(scope, k.cast(), v.cast());
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
        let val = v8::String::new(scope, name.as_ref())?;
        self.this.set(scope, type_k.cast(), val.cast());
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

    #[must_use]
    pub fn headers(
        self,
        scope: &v8::PinScope<'s, '_>,
        headers: Local<'s, v8::Object>,
    ) -> Option<Self> {
        let h_k = v8::String::new(scope, "headers")?;
        self.this.set(scope, h_k.cast(), headers.cast());
        Some(self)
    }

    /// Sets the body from a V8 value. Wraps data in a ReadableStream.
    /// Also caches `__bodyText__` (if string) or `__bodyAB__` (if ArrayBuffer)
    /// for use by `text()` / `json()` / `arrayBuffer()`.
    pub fn body(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        data: Local<'s, v8::Value>,
    ) -> Option<Self> {
        let body_k = v8::String::new(scope, "body")?;

        let rs_fn = get_rs_constructor(scope)?;
        let stream = JsReadableStream::new_with_chunk(scope, rs_fn, data)?;

        // Cache raw data for body consumer methods
        if data.is_string() {
            let k = v8::String::new(scope, "__bodyText__")?.cast::<Value>();
            self.this.set(scope, k, data);
        } else if data.is_array_buffer() {
            let k = v8::String::new(scope, "__bodyAB__")?.cast::<Value>();
            self.this.set(scope, k, data);
        }

        self.this.set(scope, body_k.cast(), stream.cast());
        Some(self)
    }

    /// Sets the body from raw bytes. Creates a Uint8Array chunk in the ReadableStream
    /// and caches the ArrayBuffer and UTF-8 text for `arrayBuffer()` / `text()` / `json()`.
    pub fn body_bytes(self, scope: &mut v8::PinScope<'s, '_>, bytes: &[u8]) -> Option<Self> {
        let ab = v8::ArrayBuffer::new(scope, bytes.len());
        if !bytes.is_empty() {
            let store = ab.get_backing_store();
            if let Some(ptr) = store.data() {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        ptr.as_ptr() as *mut u8,
                        bytes.len(),
                    );
                }
            }
        }

        // Cache for arrayBuffer()
        {
            let k = v8::String::new(scope, "__bodyAB__")?.cast::<Value>();
            self.this.set(scope, k, ab.cast());
        }

        // Cache for text() / json()
        if let Ok(text) = std::str::from_utf8(bytes) {
            let tv = v8::String::new(scope, text)?;
            let k = v8::String::new(scope, "__bodyText__")?.cast::<Value>();
            self.this.set(scope, k, tv.cast());
        }

        let uint8 = v8::Uint8Array::new(scope, ab, 0, bytes.len())?;
        self.body(scope, uint8.cast())
    }

    #[must_use]
    pub fn build(self, scope: &mut v8::PinScope<'s, '_>) -> Option<Local<'s, v8::Object>> {
        // .ok — boolean derived from status
        {
            let status = self
                .this
                .get(scope, v8::String::new(scope, "status")?.cast())?;

            let ok = if status.is_null_or_undefined() {
                true
            } else {
                let code = status.number_value(scope).unwrap_or(200.0) as u16;
                code >= 200 && code < 300
            };

            self.this.set(
                scope,
                v8::String::new(scope, "ok")?.cast(),
                v8::Boolean::new(scope, ok).cast(),
            );
        }

        // .bodyUsed
        self.this.set(
            scope,
            v8::String::new(scope, "bodyUsed")?.cast(),
            v8::Boolean::new(scope, false).cast(),
        );

        // Body consumer methods (added directly since builder creates plain objects,
        // not class instances with a Response prototype)
        {
            let f = v8::Function::new(scope, JsResponse::js_instance_text)?;
            let k = v8::String::new(scope, "text")?.cast::<Value>();
            self.this.set(scope, k, f.cast());
        }
        {
            let f = v8::Function::new(scope, JsResponse::js_instance_json)?;
            let k = v8::String::new(scope, "json")?.cast::<Value>();
            self.this.set(scope, k, f.cast());
        }
        {
            let f = v8::Function::new(scope, JsResponse::js_instance_array_buffer)?;
            let k = v8::String::new(scope, "arrayBuffer")?.cast::<Value>();
            self.this.set(scope, k, f.cast());
        }

        Some(self.this)
    }
}

/// Retrieves the `ReadableStream` constructor from the intrinsics object stored in data slot 1.
fn get_rs_constructor<'s>(scope: &mut v8::PinScope<'s, '_>) -> Option<Local<'s, v8::Function>> {
    let inner = scope.get_data(1);
    if inner.is_null() {
        return None;
    }

    let ptr = unsafe { NonNull::new_unchecked(inner as *mut v8::Value) };
    let gintrinsics = unsafe { Global::from_raw(scope, ptr) };

    let preserved = gintrinsics.clone();
    let intrinsics = Local::new(scope, gintrinsics);

    scope.set_data(1, preserved.into_raw().as_ptr() as *mut c_void);
    Some(
        intrinsics
            .cast::<v8::Object>()
            .get(scope, v8::String::new(scope, "ReadableStream")?.cast())?
            .cast::<v8::Function>(),
    )
}
