use crate::{
    intrinsics::utils::{ThrowException, throw},
    runtime::WorkerState,
};

/// Fetch API for serverless.
pub fn fetch(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let state = WorkerState::get_from_isolate(scope);
    state.add_client();
    let client = unsafe { state.get_client().unwrap_unchecked() };

    if args.length() == 0 {
        let exc = throw(
            scope,
            ThrowException::TypeError("fetch: At least 1 argument required, but only 0 passed"),
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

    println!("{url:?} {client:#?}");
}
