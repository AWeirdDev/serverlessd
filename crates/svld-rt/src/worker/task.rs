use std::{ffi::c_void, ptr::NonNull, str::FromStr, sync::Arc};

use super::WorkerError;
use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use v8::{
    External, Function, GetPropertyNamesArgs, Global, Local, Module, OwnedIsolate, Platform,
    Promise, SharedRef,
};

use crate::{
    blocks::{MaybeReplier, ReplierBlock},
    intrinsics::{self, JsResponse},
    model::WorkerHttpResponse,
};
use svld_language::{ExceptionDetails, ExceptionDetailsExt, Promised, get_bytes, throw};

use crate::{
    PodTrigger, PodTx, WorkerState, compile, scope_with_context, try_catch,
    worker::{
        MonitorHandle, WorkerTx,
        state::CreateWorkerState,
        trigger::{WorkerRx, WorkerTrigger},
    },
};

/// The worker task.
///
/// Example:
///
/// ```rs
/// let serverless = Serverless::start_one();
/// let task = WorkerTask {
///     // the code
///     source: "export default {}".to_string(),
///
///     // the platform
///     platform: serverless.get_platform(),
/// }
/// ```
#[derive(Debug)]
pub struct WorkerTask {
    // TODO: use BTreeMap
    pub source: String,
    pub platform: SharedRef<Platform>,
}

pub struct WarmUpWorkerArgs {
    pub pod_tx: PodTx,
    pub worker_tx: WorkerTx,
    pub worker_rx: WorkerRx,
    pub monitor_handle: MonitorHandle,
}

pub(super) async fn create_cancel_safe_task(
    WarmUpWorkerArgs {
        pod_tx,
        worker_tx: tx,
        worker_rx: mut rx,
        monitor_handle,
    }: WarmUpWorkerArgs,
) {
    let mut isolate = Box::new(v8::Isolate::new(Default::default()));
    isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);

    let isolate_ptr = unsafe { NonNull::new_unchecked(Box::into_raw(isolate)) };

    let mut roll_id = 0_i32;
    while let Some(msg) = rx.recv().await {
        match msg {
            WorkerTrigger::StartTask { id, task } => {
                tracing::info!("worker is starting task; initializing");

                let mut state_handle = None;

                let result = create_task(
                    &mut rx,
                    InitWorkerArgs {
                        worker_id: id,
                        isolate: isolate_ptr,
                        task,
                        tx: tx.clone(),
                        monitor_handle: monitor_handle.clone(),
                        state_handle: &mut state_handle,
                        roll_id,
                    },
                )
                .await;
                tracing::info!("task stopped/finished, marking worker as sleeping");

                match result {
                    Ok(should_restart) => {
                        state_handle.take().map(|st| close_state(st));

                        if !should_restart {
                            drop_isolate(isolate_ptr);
                            return;
                        }

                        state_handle.take().map(|st| close_state(st));
                    }
                    Err(err) => {
                        tracing::error!("got error on closed handler, {:?}", err);

                        if let Some(st) = state_handle.take() {
                            st.blocks.with_block::<ReplierBlock, _>(move |block| {
                                tracing::error!("the received error was sent to the replier.");
                                if let Some(replier) = block.take_replier() {
                                    replier.send(Err(err)).ok();
                                }
                            });
                            close_state(st);
                        }
                    }
                }

                roll_id += roll_id.wrapping_add(1);
                pod_tx
                    .send(PodTrigger::MarkWorkerAsSleeping { id })
                    .await
                    .ok();
            }

            WorkerTrigger::Kill { token } => {
                tracing::info!("received signal KILL at sleep");
                drop_isolate(isolate_ptr);
                token.send(()).ok();
                return;
            }

            _ => {
                tracing::warn!(
                    "unknown worker trigger event {:?} while in sleeping loop, skipping",
                    msg
                );
            }
        }
    }

    drop_isolate(isolate_ptr);
}

#[inline(always)]
fn drop_isolate(isolate_ptr: NonNull<OwnedIsolate>) {
    tracing::info!("dropping isolate!");
    let _ = unsafe { Box::from_raw(isolate_ptr.as_ptr()) };
    tracing::info!("isolate is shut down.");
}

#[repr(packed)]
struct InitWorkerArgs<'a> {
    worker_id: usize,
    isolate: NonNull<OwnedIsolate>,
    task: WorkerTask,
    tx: WorkerTx,
    monitor_handle: MonitorHandle,
    state_handle: &'a mut Option<Arc<WorkerState>>,
    roll_id: i32,
}

/// Create a task for running this worker.
///
/// # Returns
/// A `bool`, indicating whether to reuse this warmed worker.
#[tracing::instrument(skip_all)]
async fn create_task(rx: &mut WorkerRx, args: InitWorkerArgs<'_>) -> Result<bool, WorkerError> {
    let InitResult {
        state,
        module,
        promise,
    } = {
        match init_worker_for_task(args).await {
            Ok(t) => t,
            Err(e) => {
                return Err(e);
            }
        }
    };

    let (_entrypoint, mut entrypoint_fetch) = {
        let isolate = unsafe { state.get_isolate() };

        scope_with_context!(
            isolate: isolate,
            let &mut scope,
            let context
        );
        try_catch!(scope: scope, let try_catch);

        let module = Local::new(try_catch, module);
        {
            let promise = Local::new(try_catch, promise);
            let promised = Promised::new(try_catch, promise);

            // still pending, meaning there's await on the top level
            // we dont support it for now
            if promised.is_none() {
                return Err(WorkerError::Serverlessd(
                    "Got pending promise on top level.".to_string(),
                ));
            }

            match unsafe { promised.unwrap_unchecked() } {
                Promised::Rejected(value) => {
                    let message = value
                        .to_string(try_catch)
                        .unwrap()
                        .to_rust_string_lossy(try_catch);
                    tracing::error!("failed to initialize worker: {message}");

                    let exception = ExceptionDetails::from_exception(try_catch, value);
                    return Err(WorkerError::ModuleInitError(
                        exception.map(|item| item.to_string()).unwrap_or_else(|| {
                            "while initializing environment, an error occurred".to_string()
                        }),
                    ));
                }
                Promised::Resolved(_) => {}
            }
        }

        let namespace = module.get_module_namespace().cast::<v8::Object>();
        let entrypoint = unwrap_init(
            try_catch,
            namespace.get(try_catch, {
                unwrap_init(try_catch, v8::String::new(try_catch, "default"))?.cast()
            }),
        )?;

        if !entrypoint.is_object() || entrypoint.is_null_or_undefined() {
            tracing::error!("error while getting worker entrypoint");
            return Err(WorkerError::NoEntrypoint);
        }

        let entrypoint = entrypoint.cast::<v8::Object>();
        let entrypoint_fetch = {
            let item = unwrap_init(
                try_catch,
                entrypoint.get(try_catch, {
                    unwrap_init(try_catch, v8::String::new(try_catch, "fetch"))?.cast()
                }),
            )?;

            if item.is_function() {
                Some(item.cast::<v8::Function>())
            } else {
                None
            }
        };

        (
            Global::new(try_catch, entrypoint),
            entrypoint_fetch.map(|item| Global::new(try_catch, item)),
        )
    };

    tracing::info!("worker now started, waiting for events");

    loop {
        let maybe_event_if_trigger = tokio::select! {
            data = rx.recv() => Some(data),
            _ = state.wait_event_loop_tick() => None
        };

        // event loop
        let Some(maybe_event) = maybe_event_if_trigger else {
            tracing::info!("resolving event loop");

            let isolate = unsafe { state.get_isolate() };
            tracing::info!("creating scope with context");
            scope_with_context!(
                isolate: isolate,
                let &mut scope,
                let context
            );
            try_catch!(scope: scope, let try_catch);

            let mut resolutions = state.pending_resolutions.borrow_mut();
            while let Some((gresolver, result)) = resolutions.pop_front() {
                let resolver = Local::new(try_catch, gresolver);
                match result {
                    Ok(callback) => {
                        let cb = callback(try_catch);
                        let value = Local::new(try_catch, cb);

                        resolver.resolve(try_catch, value);
                    }
                    Err(err) => {
                        let err = Local::new(try_catch, throw(try_catch, err));
                        resolver.reject(try_catch, err);
                    }
                }
            }

            tracing::info!("ticking");
            state.tick_monitoring();
            try_catch.perform_microtask_checkpoint();
            state.tick_monitoring();
            tracing::info!("finished ticky tick!");

            continue;
        };

        let Some(event) = maybe_event else {
            // literally nobody is holding shit to us
            // we might as well just say goodbye
            return Ok(false);
        };

        match event {
            // ===== bad events =====
            WorkerTrigger::StartTask { .. } => {
                tracing::error!(
                    "unwanted 'start task' event while in worker loop; should be a bug"
                );
                break;
            }
            WorkerTrigger::Kill { token } => {
                tracing::error!(
                    "unwanted 'kill' event while in worker loop; use WorkerTrigger::HaltTask first. ignoring."
                );
                token.send(()).ok();
                continue;
            }

            // ===== acceptable events =====
            WorkerTrigger::HaltTask => {
                // clean up
                tracing::warn!("worker halting");
                return Ok(true);
            }

            WorkerTrigger::Http { reply } => {
                tracing::info!("worker received http");

                if let Some(gfetch) = entrypoint_fetch.take() {
                    let isolate = unsafe { state.get_isolate() };
                    scope_with_context!(
                        isolate: isolate,
                        let &mut scope,
                        let context
                    );
                    try_catch!(scope: scope, let try_catch);

                    let fetch = Local::new(try_catch, gfetch);

                    let replier_handle = Box::new(Some(reply));

                    let replier_ptr = Box::into_raw(replier_handle);
                    unsafe {
                        state
                            .blocks
                            .with_block_unchecked::<ReplierBlock, _>(move |shell| {
                                shell.set_replier(replier_ptr);
                            })
                    };

                    // next: call
                    state.tick_monitoring();
                    let Some(result) = fetch.call(try_catch, v8::undefined(try_catch).cast(), &[])
                    else {
                        return Err(WorkerError::Timeout);
                    };
                    state.tick_monitoring();

                    if !result.is_promise() {
                        continue;
                    }
                    let promise = result.cast::<v8::Promise>();

                    {
                        // == RESOLVE ==
                        let resolve = Function::builder(
                            |scope: &mut v8::PinScope,
                             args: v8::FunctionCallbackArguments,
                             _rv: v8::ReturnValue| {
                                let inner = |scope: &mut v8::PinScope,
                                             args: &v8::FunctionCallbackArguments|
                                 -> Option<WorkerHttpResponse> {
                                        let result = args.get(0);
                                        let js_resp = JsResponse::retrieve(scope)?;

                                        if result.instance_of(scope, js_resp.cast())? {
                                            let result = result.cast::<v8::Object>();

                                            let data = get_bytes(
                                                scope,
                                                result
                                                    .get(
                                                        scope,
                                                        v8::String::new(scope, "body")?.cast(),
                                                    )?,
                                            )
                                            .unwrap_or(
                                                Bytes::new(), // this won't allocate
                                            );

                                            let headers = {
                                                let mut map = HeaderMap::new();

                                                let jsh = result
                                                    .get(
                                                        scope,
                                                        v8::String::new(scope, "headers")?.cast(),
                                                    )?;
                                                if !jsh.is_null_or_undefined() {
                                                    let jsh = jsh.cast::<v8::Object>();
                                                    let names = jsh
                                                        .get_own_property_names(
                                                            scope,
                                                            GetPropertyNamesArgs::default(),
                                                        )?;

                                                    for idx in 0..names.length() {
                                                        let name = names.get_index(scope, idx)?;
                                                        let item = jsh.get(scope, name)?;

                                                        if let Ok(name) = HeaderName::from_str(
                                                            &name
                                                                .to_string(scope)?
                                                                .to_rust_string_lossy(scope),
                                                        ) &&
                                                        let Ok(value) = HeaderValue::from_str(
                                                            &item
                                                                .to_string(scope)?
                                                                .to_rust_string_lossy(scope),
                                                        ) {
                                                            map.insert(
                                                                name,
                                                                value,
                                                            );
                                                        }
                                                    }
                                                }

                                                map
                                            };

                                            let status_code = {
                                                let c = result
                                                    .get(
                                                        scope,
                                                        v8::String::new(scope, "status")?
                                                            .cast(),
                                                    )?;

                                                if c.is_null_or_undefined() {
                                                    200
                                                } else {
                                                    c.to_number(scope)?.int32_value(scope)? as u16
                                                }
                                            };

                                            Some(
                                                WorkerHttpResponse::builder()
                                                    .body(data)
                                                    .headers(headers)
                                                    .status(
                                                        StatusCode::from_u16(status_code).unwrap_or_default(),
                                                    )
                                                    .build()
                                            )
                                        } else {
                                            let data = result.to_string(scope)?.to_rust_string_lossy(scope);
                                            Some(
                                                WorkerHttpResponse::builder()
                                                    .body(Bytes::from(data))
                                                    .headers(HeaderMap::new())
                                                    .status(StatusCode::ACCEPTED)
                                                    .build()
                                            )
                                        }

                                };

                                let response = inner(scope, &args);

                                // this is a mutable reference, NOT OWNED!!!!!!
                                // IT'S A &mut!!!11!
                                let replier = unsafe {
                                    &mut *(args.data().cast::<External>().value()
                                        as *mut MaybeReplier)
                                };
                                if let Some(replier) = replier.take()
                                {
                                    if let Some(resp) = response {
                                        replier.send(Ok(resp)).ok();
                                    } else {
                                        replier.send(
                                            Err(
                                                WorkerError::RuntimeError("Unknown runtime error: couldn't get response".to_string())
                                            )
                                        ).ok();
                                    }
                                }
                            },
                        )
                        .data(External::new(try_catch, replier_ptr as *mut c_void).cast())
                        .build(try_catch);
                        promise.then(try_catch, unwrap_runtime(try_catch, resolve)?);
                    }

                    {
                        // == REJECT ==
                        let reject = Function::builder(
                            |scope: &mut v8::PinScope,
                             args: v8::FunctionCallbackArguments,
                             _rv: v8::ReturnValue| {
                                let replier = unsafe {
                                    &mut *(args.data().cast::<External>().value()
                                        as *mut MaybeReplier)
                                };

                                if let Some(replier) = replier.take() {
                                    replier
                                        .send(Err(WorkerError::RuntimeError(
                                            args.get(0).to_rust_string_lossy(scope),
                                        )))
                                        .ok();
                                }
                            },
                        )
                        .data(External::new(try_catch, replier_ptr as *mut c_void).cast())
                        .build(try_catch);
                        promise.catch(try_catch, unwrap_runtime(try_catch, reject)?);
                    }

                    state.tick_monitoring();
                    try_catch.perform_microtask_checkpoint();
                    state.tick_monitoring();
                }
            }
        }
    }

    Ok(true)
}

struct InitResult {
    state: Arc<WorkerState>,
    module: Global<Module>,
    promise: Global<Promise>,
}

async fn init_worker_for_task(
    InitWorkerArgs {
        worker_id,
        isolate,
        task,
        tx,
        monitor_handle,
        state_handle,
        roll_id,
    }: InitWorkerArgs<'_>,
) -> Result<InitResult, WorkerError> {
    let WorkerTask { source, platform } = task;

    let Some(state) = WorkerState::create_injected(CreateWorkerState {
        platform,
        isolate,
        worker_id,
        worker_tx: tx,
        monitor_handle,
    })
    .await
    else {
        return Err(WorkerError::Unknown(
            "failed to create worker state".to_string(),
        ));
    };

    // we need to tell create_cancel_safe_task()
    // that we've got a state here, and they can
    // cancel it gracefully
    state_handle.replace(state.clone());

    // environment initialization
    let (module, promise) = {
        scope_with_context!(
            isolate: unsafe { state.get_isolate() },
            let &mut scope,
            let context
        );
        try_catch!(scope: scope, let try_catch);

        let intrinsics_obj = {
            let build_result = intrinsics::build_intrinsics(try_catch);
            unwrap_init(try_catch, build_result)?
        };

        try_catch.set_data(1, intrinsics_obj.clone().into_raw().as_ptr() as *mut c_void);

        // we're gonna put them in the global
        {
            let context_global = context.global(try_catch);
            unwrap_init(
                try_catch,
                intrinsics::extract_intrinsics(try_catch, context_global, intrinsics_obj),
            )?;
        }

        let module = unwrap_compilation(
            try_catch,
            compile::compile_module(try_catch, source, format!("worker.js",), roll_id),
        )?;

        // instantiate imports, etc.
        {
            let res = module.instantiate_module(try_catch, compile::resolve_module_callback);
            if res.is_none() {
                return Err(WorkerError::ModuleInitError(
                    try_catch
                        .exception_details()
                        .map(|item| item.to_string())
                        .unwrap_or_else(|| {
                            "module instantiation (imports, etc.) failed".to_string()
                        }),
                ));
            }
        }

        // instantiate evaluations
        state.tick_monitoring();
        let Some(promise) = module.evaluate(try_catch) else {
            return Err(WorkerError::ModuleInitError(
                try_catch
                    .exception_details()
                    .map(|item| item.to_string())
                    .unwrap_or_else(|| "failed to evaluate module".to_string()),
            ));
        };
        state.tick_monitoring();

        let promise = promise.cast::<Promise>();

        (
            Global::new(try_catch, module),
            Global::new(try_catch, promise),
        )
    };

    state.tick_monitoring();
    // we gotta wait for it to initialize
    {
        let isolate = unsafe { state.get_isolate() };
        while Platform::pump_message_loop(&state.platform, isolate, false) {}
    }
    state.tick_monitoring();

    Ok(InitResult {
        state,
        module,
        promise,
    })
}

/// Gracefully closes the worker state, releasing memory.
///
/// Additionally, this also removes globals.
#[inline]
fn close_state(state: Arc<WorkerState>) {
    // first the worker state
    let isolate = unsafe { &mut *state.isolate.as_ptr() };
    drop(state);

    let state2 = WorkerState::open_from_isolate(isolate);
    state2.close();

    // then the global intrinsics
    let data = isolate.get_data(1);
    if !data.is_null() {
        let _ =
            unsafe { Global::from_raw(isolate, NonNull::new_unchecked(data as *mut v8::Value)) };
    }

    // then all the globals, we gotta erase em
    scope_with_context!(
        isolate: isolate,
        let &mut scope,
        let context
    );
    let global = context.global(scope);
    let own_props = global
        .get_own_property_names(scope, GetPropertyNamesArgs::default())
        .unwrap();

    for i in 0..own_props.length() {
        let key = own_props.get_index(scope, i).unwrap();
        global.delete(scope, key);
    }

    scope.low_memory_notification();

    // at this point, state & state2 gets dropped
    // memory gets freed (hopefully, PLEASE)
}

macro_rules! _simple_unwrap_impl {
    ($name:ident, $p:expr) => {
        #[inline]
        #[doc = "Unwrap `Option<T>` data returning, `Ok(T)` if success, `"]
        #[doc = stringify!($p)]
        #[doc = "` if failed."]
        fn $name<T>(
            scope: &v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
            data: Option<T>,
        ) -> Result<T, WorkerError> {
            let Some(data) = data else {
                return Err($p(scope
                    .exception_details()
                    .map(|item| item.to_string())
                    .unwrap_or_else(|| format!("unknown error"))));
            };
            Ok(data)
        }
    };
}

_simple_unwrap_impl!(unwrap_compilation, WorkerError::CompileError);
_simple_unwrap_impl!(unwrap_init, WorkerError::ModuleInitError);
_simple_unwrap_impl!(unwrap_runtime, WorkerError::RuntimeError);
