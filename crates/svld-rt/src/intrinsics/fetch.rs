use std::str::FromStr;

use crate::blocks::HttpClientBlock;
use reqwest::{
    Method,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use v8::{Global, Local, Object, PromiseResolver};

use svld_language::{ThrowException, throw};

use crate::worker::WorkerState;

use super::response::JsResponse;

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
    let state = WorkerState::get_from_isolate(scope);

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

    let mut rq = unsafe {
        state
            .blocks
            .with_block_unchecked::<HttpClientBlock, _>(move |block| {
                block.add_client();
                block.get_client().unwrap().request(method, url)
            })
    };

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

    {
        let state2 = state.clone();
        state.tasks.spawn_local(async move {
            let resp = match rq.send().await {
                Ok(r) => r,
                Err(err) => {
                    state2.schedule_resolution_and_tick(
                        gresolver,
                        Err(ThrowException::Error(err.to_string())),
                    );
                    return;
                }
            };

            // capture metadata before consuming the response body
            let status = resp.status().as_u16();
            let final_url = resp.url().to_string();
            let resp_headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
                .collect();

            let bytes = match resp.bytes().await {
                Ok(b) => b,
                Err(err) => {
                    tracing::error!("errored on fetch(), ticking event loop");
                    state2.schedule_resolution_and_tick(
                        gresolver,
                        Err(ThrowException::Error(err.to_string())),
                    );
                    return;
                }
            };

            tracing::info!("ok on fetch(), ticking event loop");
            state2.schedule_resolution_and_tick(
                gresolver,
                Ok(Box::new(move |scope| {
                    build_fetch_response(scope, status, &final_url, &resp_headers, &bytes)
                        .map(|o| o.cast())
                        .unwrap_or_else(|| v8::undefined(scope).cast())
                })),
            );
        })
    };

    rv.set(resolver.cast());
}

fn build_fetch_response<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    status: u16,
    url: &str,
    headers: &[(String, String)],
    bytes: &[u8],
) -> Option<Local<'s, v8::Object>> {
    tracing::info!("building Response");
    let headers_obj = v8::Object::new(scope);
    for (k, v) in headers {
        let kv = v8::String::new(scope, k)?.cast::<v8::Value>();
        let vv = v8::String::new(scope, v)?.cast::<v8::Value>();
        headers_obj.set(scope, kv, vv);
    }

    JsResponse::builder(scope)
        .status(scope, status)?
        .url(scope, url)?
        .type_(scope, "basic")?
        .headers(scope, headers_obj)?
        .body_bytes(scope, bytes)?
        .build(scope)
}
