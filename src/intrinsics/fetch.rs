use std::str::FromStr;

use reqwest::{
    Method,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use v8::{Global, Local, Object, PromiseResolver};

use crate::{
    language::{ThrowException, throw},
    runtime::WorkerState,
};

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

    state.extensions.add_client();
    let client = unsafe { state.extensions.get_client().unwrap_unchecked() };

    if args.length() == 0 {
        let exc = throw(
            scope,
            ThrowException::type_error("fetch: At least 1 argument required, but only 0 passed"),
        );
        let resolver = v8::PromiseResolver::new(scope).unwrap();
        resolver.reject(scope, exc);
        rv.set(resolver.cast());

        return;
    }

    // 1. get the url
    let url = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

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
        {}
    }

    let resolver = some!(PromiseResolver::new(scope));
    let gresolver = Global::new(scope, resolver);
    let fut = {
        let state2 = state.clone();
        state.monitored_future(async move {
            state2.tick_monitoring();

            let result = rq.send().await;

            match result {
                Ok(_resp) => {
                    state2.schedule_resolution(
                        gresolver,
                        Ok(Box::new(move |scope| {
                            Local::new(scope, v8::null(scope).cast())
                        })),
                    );
                }

                Err(err) => {
                    let details = err.to_string();
                    state2.schedule_resolution(gresolver, Err(ThrowException::Error(details)));
                }
            }
        })
    };
    state.tasks.spawn_local(fut);

    rv.set(resolver.cast());
}
