use std::str::FromStr;

use reqwest::{
    Method,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use svld_state_extensions::HttpClientWorkerExtension;
use v8::{Global, Local, Object, PromiseResolver};

use svld_language::{ThrowException, throw};

use crate::{intrinsics::response::Response, runtime::WorkerState};

macro_rules! some {
    ($k:expr) => {{
        let Some(m) = $k else {
            return;
        };
        m
    }};

    ($k:expr, else ($scope:expr, $rv:expr) => $b:block) => {{
        let Some(m) = $k else {
            let rej = $b;
            let resolver = v8::PromiseResolver::new($scope).unwrap();
            resolver.reject($scope, rej.cast());
            $rv.set(resolver.cast());
            return;
        };
        m
    }};
}

macro_rules! ok {
    ($k:expr) => {{
        let Ok(m) = $k else {
            return;
        };
        m
    }};

    ($k:expr, else ($scope:expr, $rv:expr) => $b:block) => {{
        let Ok(m) = $k else {
            let rej = $b;
            let resolver = v8::PromiseResolver::new($scope).unwrap();
            resolver.reject($scope, rej.cast());
            $rv.set(resolver.cast());
            return;
        };
        m
    }};
}

/// Fetch API for serverless.
pub fn fetch(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    tracing::info!("fetch()");
    let state = WorkerState::get_from_isolate(scope);

    let client_extension = unsafe {
        state
            .extensions
            .get_extension_unchecked::<HttpClientWorkerExtension>()
    };

    client_extension.add_client();
    let client = unsafe { client_extension.get_client().unwrap_unchecked() };

    if args.length() == 0 {
        let exc = throw(
            scope,
            ThrowException::type_error("fetch: At least 1 argument required, but only 0 passed"),
        );
        let resolver = some!(v8::PromiseResolver::new(scope));
        resolver.reject(scope, exc);
        rv.set(resolver.cast());

        return;
    }

    // get the url
    let url = some!(args.get(0).to_string(scope)).to_rust_string_lossy(scope);

    let options = args.get(1);
    let has_options = options.is_object() && !options.is_null_or_undefined();

    let method = if has_options {
        let options = options.cast::<Object>();

        // method
        let meth_name = options
            .get(scope, some!(v8::String::new(scope, "method")).cast())
            .map(|item| item.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "GET".to_string());

        // NOTE: custom behavior
        // this is to align with Rust reqwest's behaviors
        // fuck it
        ok!(Method::from_str(&meth_name), else (scope, rv) => {
            throw(scope, ThrowException::type_error("fetch: Invalid method"))
        })
    } else {
        Method::GET
    };

    let mut rq = client.request(method, url);

    // we gotta parse some fucking options now
    if has_options {
        let options = options.cast::<Object>();

        // headers
        {
            let headers_k =
                some!(options.get(scope, some!(v8::String::new(scope, "headers")).cast()));

            if headers_k.is_object() && !headers_k.is_null_or_undefined() {
                let headers_obj = headers_k.cast::<Object>();
                let header_names =
                    some!(headers_obj.get_own_property_names(scope, Default::default()));

                let mut rq_headers = HeaderMap::new();

                for idx in 0..header_names.length() {
                    if let Some(key) = header_names.get_index(scope, idx) {
                        let key_str = some!(key.to_string(scope)).to_rust_string_lossy(scope);
                        let val = some!(headers_obj.get(scope, key));
                        let val_str = some!(val.to_string(scope)).to_rust_string_lossy(scope);
                        rq_headers.insert(
                            ok!(HeaderName::from_str(&key_str)),
                            ok!(HeaderValue::from_str(&val_str)),
                        );
                    }
                }

                rq = rq.headers(rq_headers);
            }
        }

        // body
        'body: {
            let body_k = some!(options.get(scope, some!(v8::String::new(scope, "body")).cast()));

            if body_k.is_null_or_undefined() {
                break 'body;
            }

            if body_k.is_string() {
                let s = some!(body_k.to_string(scope)).to_rust_string_lossy(scope);
                rq = rq.body(s);
            } else if body_k.is_array_buffer_view() {
                let view = body_k.cast::<v8::ArrayBufferView>();
                let mut buf = vec![0u8; view.byte_length()];
                view.copy_contents(&mut buf);
                rq = rq.body(buf);
            } else if body_k.is_array_buffer() {
                let ab = body_k.cast::<v8::ArrayBuffer>();
                let store = ab.get_backing_store();
                let slice = unsafe {
                    std::slice::from_raw_parts(
                        some!(store.data()).as_ptr() as *const u8,
                        store.byte_length(),
                    )
                };
                rq = rq.body(slice.to_vec());
            }
        }
    }

    let resolver = some!(PromiseResolver::new(scope));
    let gresolver = Global::new(scope, resolver);
    let fut = {
        let state2 = state.clone();
        state.monitored_future(async move {
            state2.tick_monitoring();

            let result = rq.send().await;

            match result {
                Ok(resp) => {
                    state2.schedule_resolution_and_tick(
                        gresolver,
                        Ok(Box::new(move |scope| {
                            let Some(jsresp) = Response::builder(scope)
                                .type_("basic")
                                .status(resp.status().as_u16())
                                .url(resp.url().as_str())
                                .build()
                            else {
                                return throw(scope, ThrowException::error("unknown error"));
                            };

                            // .headers — a Headers-like object { get(name), entries(), ... }

                            // Store raw body bytes in an ArrayBuffer
                            // let body_ab = bytes_to_array_buffer(scope, &resp.body);

                            // .arrayBuffer() → Promise<ArrayBuffer>
                            // let ab_clone = v8::Global::new(scope, body_ab);
                            // let ab_fn =
                            //     make_body_method(scope, ab_clone, BodyKind::ArrayBuffer);
                            // set_prop(scope, obj, "arrayBuffer", ab_fn.into());

                            // .text() → Promise<string>
                            // let ab_clone2 = v8::Global::new(scope, body_ab);
                            // let text_fn = make_body_method(scope, ab_clone2, BodyKind::Text);
                            // set_prop(scope, obj, "text", text_fn.into());

                            // .json() → Promise<any>
                            // let ab_clone3 = v8::Global::new(scope, body_ab);
                            // let json_fn = make_body_method(scope, ab_clone3, BodyKind::Json);
                            // set_prop(scope, obj, "json", json_fn.into());

                            // .blob() — returns a minimal object with {size, type, arrayBuffer()}
                            // let ab_clone4 = v8::Global::new(scope, body_ab);
                            // let blob_fn = make_body_method(scope, ab_clone4, BodyKind::Blob);
                            // set_prop(scope, obj, "blob", blob_fn.into());

                            Local::new(scope, jsresp.cast())
                        })),
                    );
                }

                Err(err) => {
                    let details = err.to_string();
                    state2.schedule_resolution_and_tick(
                        gresolver,
                        Err(ThrowException::Error(details)),
                    );
                }
            }
        })
    };
    state.tasks.spawn_local(fut);

    rv.set(resolver.cast());
}
