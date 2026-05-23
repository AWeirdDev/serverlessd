// REALLY IMPORTANT NOTE
// this shit is fucking implemented by claude
// i know you might be mad
// but i dont have time to do this

use svld_language::{ThrowException, throw};
use v8::{FunctionTemplate, Local, Value};

use crate::{
    intrinsics::{readable_stream::JsReadableStream, retrieve::retrieve_intrinsic},
    try_catch,
};

pub struct JsRequest;

#[allow(unused)]
impl JsRequest {
    #[inline(always)]
    #[must_use]
    pub fn builder<'s>(scope: &mut v8::PinScope<'s, '_>, url: &str) -> Option<RequestBuilder<'s>> {
        RequestBuilder::new(scope, url)
    }

    pub fn get_fn_template<'s>(
        scope: &v8::PinScope<'s, '_>,
    ) -> Option<Local<'s, FunctionTemplate>> {
        let function_template = v8::FunctionTemplate::new(scope, Self::js_constructor);

        let name = v8::String::new(scope, "Request")?;
        function_template.set_class_name(name);

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
            {
                let f = v8::FunctionTemplate::new(scope, Self::js_instance_clone);
                let k = v8::String::new(scope, "clone")?;
                proto.set(k.cast(), f.cast());
            }
        }

        Some(function_template)
    }

    /// Retrieves the `Request` constructor from the intrinsics object stored in data slot 1.
    pub fn retrieve<'s>(scope: &mut v8::PinScope<'s, '_>) -> Option<Local<'s, v8::Function>> {
        retrieve_intrinsic(scope, "Request").map(|k| k.cast())
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

            // --- resource (first argument) ---
            // May be a URL string or another Request object.
            let resource = args.get(0);
            if resource.is_null_or_undefined() {
                throw(
                    scope,
                    ThrowException::type_error(
                        "Request: At least 1 argument required, but only 0 passed",
                    ),
                );
                return None;
            }

            let options = {
                let arg = args.get(1);
                if arg.is_object() && !arg.is_null_or_undefined() {
                    Some(arg.cast::<v8::Object>())
                } else {
                    None
                }
            };

            // Resolve URL and optionally inherit fields from an existing Request.
            let (url_str, inherited_method, inherited_headers, inherited_body) =
                if resource.is_string() {
                    let url = resource
                        .to_string(scope)
                        .unwrap_or_else(|| v8::String::new(scope, "").unwrap());
                    (url, None, None, None)
                } else if resource.is_object() {
                    // Treat as an existing Request — extract its fields.
                    let req = resource.cast::<v8::Object>();
                    let url = req
                        .get(scope, v8::String::new(scope, "url")?.cast())
                        .filter(|v| v.is_string())
                        .and_then(|v| v.to_string(scope))
                        .unwrap_or_else(|| v8::String::new(scope, "").unwrap());

                    let method = req
                        .get(scope, v8::String::new(scope, "method")?.cast())
                        .filter(|v| v.is_string());

                    let headers = req
                        .get(scope, v8::String::new(scope, "headers")?.cast())
                        .filter(|v| v.is_object() && !v.is_null_or_undefined())
                        .map(|v| v.cast::<v8::Object>());

                    let body = req
                        .get(scope, v8::String::new(scope, "body")?.cast())
                        .filter(|v| !v.is_null_or_undefined());

                    (url, method, headers, body)
                } else {
                    throw(
                        scope,
                        ThrowException::type_error(
                            "Request: resource must be a string URL or a Request object",
                        ),
                    );
                    return None;
                };

            this.set(scope, v8::String::new(scope, "url")?.cast(), url_str.cast());

            // --- method ---
            // init.method > inherited method > "GET"
            let method = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "method")?.cast()))
                .filter(|v| v.is_string())
                .and_then(|v| v.to_string(scope))
                .or(inherited_method.and_then(|v| v.to_string(scope)))
                .unwrap_or_else(|| v8::String::new(scope, "GET").unwrap());
            this.set(
                scope,
                v8::String::new(scope, "method")?.cast(),
                method.cast(),
            );

            // --- headers ---
            let headers = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "headers")?.cast()))
                .filter(|v| v.is_object() && !v.is_null_or_undefined())
                .map(|v| v.cast::<v8::Object>())
                .or(inherited_headers)
                .unwrap_or_else(|| v8::Object::new(scope));
            this.set(
                scope,
                v8::String::new(scope, "headers")?.cast(),
                headers.cast(),
            );

            // --- body ---
            // Bodies are forbidden on GET / HEAD per the Fetch spec.
            let method_str = method.to_rust_string_lossy(scope);
            let body = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "body")?.cast()))
                .filter(|v| !v.is_null_or_undefined())
                .or(inherited_body);

            if let Some(b) = body {
                if method_str.eq_ignore_ascii_case("GET") || method_str.eq_ignore_ascii_case("HEAD")
                {
                    throw(
                        scope,
                        ThrowException::type_error(
                            "Request with GET/HEAD method cannot have a body",
                        ),
                    );
                    return None;
                }
                this.set(scope, v8::String::new(scope, "body")?.cast(), b);
                if b.is_string() {
                    this.set(scope, v8::String::new(scope, "__bodyText__")?.cast(), b);
                }
            }

            // --- mode ---
            let mode = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "mode")?.cast()))
                .filter(|v| v.is_string())
                .and_then(|v| v.to_string(scope))
                .unwrap_or_else(|| v8::String::new(scope, "cors").unwrap());
            this.set(scope, v8::String::new(scope, "mode")?.cast(), mode.cast());

            // --- credentials ---
            let credentials = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "credentials")?.cast()))
                .filter(|v| v.is_string())
                .and_then(|v| v.to_string(scope))
                .unwrap_or_else(|| v8::String::new(scope, "same-origin").unwrap());
            this.set(
                scope,
                v8::String::new(scope, "credentials")?.cast(),
                credentials.cast(),
            );

            // --- cache ---
            let cache = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "cache")?.cast()))
                .filter(|v| v.is_string())
                .and_then(|v| v.to_string(scope))
                .unwrap_or_else(|| v8::String::new(scope, "default").unwrap());
            this.set(scope, v8::String::new(scope, "cache")?.cast(), cache.cast());

            // --- redirect ---
            let redirect = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "redirect")?.cast()))
                .filter(|v| v.is_string())
                .and_then(|v| v.to_string(scope))
                .unwrap_or_else(|| v8::String::new(scope, "follow").unwrap());
            this.set(
                scope,
                v8::String::new(scope, "redirect")?.cast(),
                redirect.cast(),
            );

            // --- referrer ---
            let referrer = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "referrer")?.cast()))
                .filter(|v| v.is_string())
                .and_then(|v| v.to_string(scope))
                .unwrap_or_else(|| v8::String::new(scope, "about:client").unwrap());
            this.set(
                scope,
                v8::String::new(scope, "referrer")?.cast(),
                referrer.cast(),
            );

            // --- integrity ---
            let integrity = options
                .and_then(|opt| opt.get(scope, v8::String::new(scope, "integrity")?.cast()))
                .filter(|v| v.is_string())
                .and_then(|v| v.to_string(scope))
                .unwrap_or_else(|| v8::String::new(scope, "").unwrap());
            this.set(
                scope,
                v8::String::new(scope, "integrity")?.cast(),
                integrity.cast(),
            );

            // --- bodyUsed ---
            this.set(
                scope,
                v8::String::new(scope, "bodyUsed")?.cast(),
                v8::Boolean::new(scope, false).cast(),
            );

            rv.set(this.cast());
            Some(())
        }

        inner(scope, args, rv);
    }

    /// `request.text()` — resolves with the body as a UTF-8 string.
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

    /// `request.arrayBuffer()` — resolves with the raw ArrayBuffer.
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

    /// `request.json()` — resolves with the body parsed as JSON.
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

    /// `request.clone()` — returns a new Request with the same fields.
    /// Per the Fetch spec, cloning a consumed request is a TypeError.
    fn js_instance_clone(
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

            // Guard: bodyUsed
            let body_used = this
                .get(scope, v8::String::new(scope, "bodyUsed")?.cast())
                .filter(|v| v.is_boolean())
                .map(|v| v.boolean_value(scope))
                .unwrap_or(false);

            if body_used {
                throw(
                    scope,
                    ThrowException::type_error("Cannot clone a Request whose body has been used"),
                );
                return None;
            }

            // Pass `this` as the resource argument — the constructor copies
            // url, method, headers, and body from an existing Request object.
            let ctor = JsRequest::retrieve(scope)?;
            let undefined = v8::undefined(scope).cast::<Value>();
            let clone = ctor.new_instance(scope, &[this.cast(), undefined])?;

            rv.set(clone.cast());
            Some(())
        }

        inner(scope, args, rv);
    }
}

// ---------------------------------------------------------------------------
// RequestBuilder
// ---------------------------------------------------------------------------

#[repr(transparent)]
#[must_use]
pub struct RequestBuilder<'s> {
    this: Local<'s, v8::Object>,
}

#[allow(unused)]
impl<'s> RequestBuilder<'s> {
    /// Creates a new builder pre-populated with the URL and sensible Fetch-spec
    /// defaults (method = "GET", mode = "cors", credentials = "same-origin",
    /// cache = "default", redirect = "follow").
    #[must_use]
    pub fn new(scope: &mut v8::PinScope<'s, '_>, url: &str) -> Option<Self> {
        let this = v8::Object::new(scope);

        let url_v = v8::String::new(scope, url)?;
        this.set(scope, v8::String::new(scope, "url")?.cast(), url_v.cast());

        // Fetch-spec defaults
        macro_rules! set_str {
            ($key:expr, $val:expr) => {{
                let k = v8::String::new(scope, $key)?.cast::<Value>();
                let v = v8::String::new(scope, $val)?.cast::<Value>();
                this.set(scope, k, v);
            }};
        }

        set_str!("method", "GET");
        set_str!("mode", "cors");
        set_str!("credentials", "same-origin");
        set_str!("cache", "default");
        set_str!("redirect", "follow");
        set_str!("referrer", "about:client");
        set_str!("integrity", "");

        Some(Self { this })
    }

    #[must_use]
    pub fn method<S: AsRef<str>>(self, scope: &v8::PinScope<'s, '_>, method: S) -> Option<Self> {
        let k = v8::String::new(scope, "method")?;
        let v = v8::String::new(scope, method.as_ref())?;
        self.this.set(scope, k.cast(), v.cast());
        Some(self)
    }

    #[must_use]
    pub fn headers(
        self,
        scope: &v8::PinScope<'s, '_>,
        headers: Local<'s, v8::Object>,
    ) -> Option<Self> {
        let k = v8::String::new(scope, "headers")?;
        self.this.set(scope, k.cast(), headers.cast());
        Some(self)
    }

    #[must_use]
    pub fn mode<S: AsRef<str>>(self, scope: &v8::PinScope<'s, '_>, mode: S) -> Option<Self> {
        let k = v8::String::new(scope, "mode")?;
        let v = v8::String::new(scope, mode.as_ref())?;
        self.this.set(scope, k.cast(), v.cast());
        Some(self)
    }

    #[must_use]
    pub fn credentials<S: AsRef<str>>(
        self,
        scope: &v8::PinScope<'s, '_>,
        credentials: S,
    ) -> Option<Self> {
        let k = v8::String::new(scope, "credentials")?;
        let v = v8::String::new(scope, credentials.as_ref())?;
        self.this.set(scope, k.cast(), v.cast());
        Some(self)
    }

    #[must_use]
    pub fn cache<S: AsRef<str>>(self, scope: &v8::PinScope<'s, '_>, cache: S) -> Option<Self> {
        let k = v8::String::new(scope, "cache")?;
        let v = v8::String::new(scope, cache.as_ref())?;
        self.this.set(scope, k.cast(), v.cast());
        Some(self)
    }

    #[must_use]
    pub fn redirect<S: AsRef<str>>(
        self,
        scope: &v8::PinScope<'s, '_>,
        redirect: S,
    ) -> Option<Self> {
        let k = v8::String::new(scope, "redirect")?;
        let v = v8::String::new(scope, redirect.as_ref())?;
        self.this.set(scope, k.cast(), v.cast());
        Some(self)
    }

    #[must_use]
    pub fn referrer<S: AsRef<str>>(
        self,
        scope: &v8::PinScope<'s, '_>,
        referrer: S,
    ) -> Option<Self> {
        let k = v8::String::new(scope, "referrer")?;
        let v = v8::String::new(scope, referrer.as_ref())?;
        self.this.set(scope, k.cast(), v.cast());
        Some(self)
    }

    #[must_use]
    pub fn integrity<S: AsRef<str>>(
        self,
        scope: &v8::PinScope<'s, '_>,
        integrity: S,
    ) -> Option<Self> {
        let k = v8::String::new(scope, "integrity")?;
        let v = v8::String::new(scope, integrity.as_ref())?;
        self.this.set(scope, k.cast(), v.cast());
        Some(self)
    }

    /// Sets the body from a V8 value. Wraps data in a ReadableStream and
    /// caches `__bodyText__` (string) or `__bodyAB__` (ArrayBuffer) for the
    /// body consumer methods. Rejects GET/HEAD as per the Fetch spec.
    pub fn body(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        data: Local<'s, v8::Value>,
    ) -> Option<Self> {
        // Guard: GET/HEAD bodies are forbidden.
        let method_str = self
            .this
            .get(scope, v8::String::new(scope, "method")?.cast())
            .filter(|v| v.is_string())
            .and_then(|v| v.to_string(scope))
            .map(|s| s.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "GET".to_string());

        if method_str.eq_ignore_ascii_case("GET") || method_str.eq_ignore_ascii_case("HEAD") {
            throw(
                scope,
                ThrowException::type_error("Request with GET/HEAD method cannot have a body"),
            );
            return None;
        }

        let body_k = v8::String::new(scope, "body")?;

        let rs_fn = JsReadableStream::retrieve(scope)?;
        let stream = JsReadableStream::new_with_chunk(scope, rs_fn, data)?;

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

    /// Sets the body from raw bytes. Creates a Uint8Array chunk in the
    /// ReadableStream and caches both the ArrayBuffer and UTF-8 text for the
    /// body consumer methods.
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
        // bodyUsed
        self.this.set(
            scope,
            v8::String::new(scope, "bodyUsed")?.cast(),
            v8::Boolean::new(scope, false).cast(),
        );

        // Body consumer methods (added directly since builder creates plain
        // objects, not class instances with a Request prototype).
        {
            let f = v8::Function::new(scope, JsRequest::js_instance_text)?;
            let k = v8::String::new(scope, "text")?.cast::<Value>();
            self.this.set(scope, k, f.cast());
        }
        {
            let f = v8::Function::new(scope, JsRequest::js_instance_json)?;
            let k = v8::String::new(scope, "json")?.cast::<Value>();
            self.this.set(scope, k, f.cast());
        }
        {
            let f = v8::Function::new(scope, JsRequest::js_instance_array_buffer)?;
            let k = v8::String::new(scope, "arrayBuffer")?.cast::<Value>();
            self.this.set(scope, k, f.cast());
        }
        {
            let f = v8::Function::new(scope, JsRequest::js_instance_clone)?;
            let k = v8::String::new(scope, "clone")?.cast::<Value>();
            self.this.set(scope, k, f.cast());
        }

        Some(self.this)
    }
}
