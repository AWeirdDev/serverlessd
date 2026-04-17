use std::{ffi::c_void, ptr::NonNull, sync::Arc};

use tokio::sync::oneshot;
use v8::{External, Function, Global, Local, Module, OwnedIsolate, Platform, Promise, SharedRef};

use svld_language::{ExceptionDetails, ExceptionDetailsExt, Promised, throw};

use crate::{
    compile, intrinsics,
    runtime::{
        PodTrigger, PodTx, WorkerState,
        worker::{
            MonitorHandle, WorkerTx,
            error::WorkerError,
            trigger::{WorkerRx, WorkerTrigger},
        },
    },
    scope_with_context, try_catch,
};

/// Unwrap.
///
/// # Option<T>
/// ```no_run
/// // this should be used within create_task()
/// let a = Some(1);
/// let b = unwrap!(
///     try_catch_scope,
///     some init a.map(|k| k + 1)
/// );
/// assert!(b == 2);
/// ```
macro_rules! unwrap {
    ($try_catch:expr, some $p:expr => $k:expr) => {{
        let Some(k) = $k else {
            return Err($p($try_catch.exception_details()));
        };
        k
    }};

    ($try_catch:expr, some compile $k:expr) => {
        unwrap!($try_catch, some WorkerError::CompileError => $k)
    };

    ($try_catch:expr, some init $k:expr) => {
        unwrap!($try_catch, some WorkerError::ModuleInitError => $k)
    };

    ($try_catch:expr, some runtime $k:expr) => {
        unwrap!($try_catch, some WorkerError::RuntimeError => $k)
    };
}

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
    isolate.set_microtasks_policy(v8::MicrotasksPolicy::Auto);

    let isolate_ptr = unsafe { NonNull::new_unchecked(Box::into_raw(isolate)) };

    while let Some(msg) = rx.recv().await {
        match msg {
            WorkerTrigger::StartTask { id, task } => {
                let mut state_handle = None;

                let result = create_task(
                    id,
                    isolate_ptr,
                    task,
                    tx.clone(),
                    &mut rx,
                    monitor_handle.clone(),
                    &mut state_handle,
                )
                .await;
                tracing::info!("task finished, marking worker as vacant");

                // at this point, the work is done
                pod_tx
                    .send(PodTrigger::MarkWorkerAsSleeping { id })
                    .await
                    .ok();

                match result {
                    Ok(should_restart) => {
                        if !should_restart {
                            drop_isolate(isolate_ptr);
                            break;
                        }

                        if let Some(state) = state_handle {
                            tracing::info!("closing state");
                            close_state(state).await;
                        }
                    }
                    Err(err) => {
                        tracing::error!("got error on closed handler, {:?}", err);
                    }
                }
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
}

#[inline(always)]
fn drop_isolate(isolate_ptr: NonNull<OwnedIsolate>) {
    tracing::info!("dropping isolate!");
    let _ = unsafe { Box::from_raw(isolate_ptr.as_ptr()) };
    tracing::info!("isolate is shut down.");
}

/// Create a task for running this worker.
///
/// # Returns
/// A `bool`, indicating whether to reuse this warmed worker.
#[tracing::instrument(skip_all)]
async fn create_task(
    worker_id: usize,
    isolate_ptr: NonNull<OwnedIsolate>,
    task: WorkerTask,
    tx: WorkerTx,
    rx: &mut WorkerRx,
    monitor_handle: MonitorHandle,
    state_handle: &mut Option<Arc<WorkerState>>,
) -> Result<bool, WorkerError> {
    let InitResult {
        state,
        module,
        promise,
    } = {
        match init_worker_for_task(
            worker_id,
            isolate_ptr,
            task,
            tx,
            monitor_handle,
            state_handle,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                return Err(e);
            }
        }
    };
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

        match promised {
            Promised::Rejected(value) => {
                let exception = ExceptionDetails::from_exception(try_catch, value);
                return Err(WorkerError::ModuleInitError(exception));
            }
            Promised::Resolved(_) => {
                tracing::info!("worker env initialized");
            }
        }
    }

    let namespace = module.get_module_namespace().cast::<v8::Object>();
    let entrypoint = unwrap!(
        try_catch,
        some init namespace.get(try_catch, {
            unwrap!(try_catch, some init v8::String::new(try_catch, "default")).cast()
        })
    );

    if !entrypoint.is_object() || entrypoint.is_null_or_undefined() {
        tracing::error!("error while getting worker entrypoint");
        return Err(WorkerError::NoEntrypoint);
    }

    let entrypoint = entrypoint.cast::<v8::Object>();
    let entrypoint_fetch = {
        let item = unwrap!(
            try_catch,
            some init entrypoint.get(try_catch, {
                unwrap!(try_catch, some init v8::String::new(try_catch, "fetch")).cast()
            })
        );

        if item.is_function() {
            Some(item.cast::<v8::Function>())
        } else {
            None
        }
    };

    tracing::info!("worker now started, waiting for events");
    while let Some(event) = rx.recv().await {
        {
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
        }
        try_catch.perform_microtask_checkpoint();

        state.cleanup_dead_repliers();

        match event {
            // ===== bad events =====
            WorkerTrigger::StartTask { .. } => {
                tracing::error!("unwanted 'start task' event while in worker loop");
                break;
            }
            WorkerTrigger::Kill { .. } => {
                tracing::error!(
                    "unwanted 'kill' event while in worker loop; use WorkerTrigger::HaltTask first. ignoring."
                );
                continue;
            }

            // ===== acceptable events =====
            WorkerTrigger::HaltTask => {
                // clean up
                tracing::warn!("worker halting");
                break;
            }

            WorkerTrigger::Http { reply } => {
                tracing::info!("worker received http");
                if let Some(fetch) = entrypoint_fetch {
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

                    // replier still exists
                    let replier_handle = Box::new(Some(reply));

                    let replier_ptr = Box::into_raw(replier_handle);
                    let idx = state.get_next_replier_idx();
                    state.add_replier(idx, replier_ptr);

                    {
                        let resolve = Function::builder(
                            |scope: &mut v8::PinScope,
                             args: v8::FunctionCallbackArguments,
                             _rv: v8::ReturnValue| {
                                let state = WorkerState::get_from_isolate(scope);
                                state.tick_monitoring(); // the cpu task is done. nice!

                                let replier = unsafe {
                                    &mut *(args.data().cast::<External>().value()
                                        as *mut Option<oneshot::Sender<String>>)
                                };

                                if let Some(replier) = replier.take() {
                                    replier.send(args.get(0).to_rust_string_lossy(scope)).ok();
                                }
                            },
                        )
                        .data(External::new(try_catch, replier_ptr as *mut c_void).cast())
                        .build(try_catch);
                        promise.then(
                            try_catch,
                            unwrap!(
                                try_catch,
                                some runtime resolve
                            ),
                        );
                    }

                    {
                        let reject = Function::builder(
                            |scope: &mut v8::PinScope,
                             args: v8::FunctionCallbackArguments,
                             _rv: v8::ReturnValue| {
                                let state = WorkerState::get_from_isolate(scope);
                                state.tick_monitoring();

                                let replier = unsafe {
                                    &mut *(args.data().cast::<External>().value()
                                        as *mut Option<oneshot::Sender<String>>)
                                };

                                if let Some(replier) = replier.take() {
                                    replier.send(args.get(0).to_rust_string_lossy(scope)).ok();
                                }
                            },
                        )
                        .data(External::new(try_catch, replier_ptr as *mut c_void).cast())
                        .build(try_catch);
                        promise.catch(try_catch, unwrap!(try_catch, some runtime reject));
                    }

                    try_catch.perform_microtask_checkpoint();
                }
            }

            WorkerTrigger::Refresh => {
                // before this session dies out, we need to remove the global first
                // let global = context.global(try_catch);

                return Ok(true);
            }
        }
    }

    Ok(false)
}

struct InitResult {
    state: Arc<WorkerState>,
    module: Global<Module>,
    promise: Global<Promise>,
}

async fn init_worker_for_task(
    worker_id: usize,
    isolate: NonNull<OwnedIsolate>,
    task: WorkerTask,
    tx: WorkerTx,
    monitor_handle: MonitorHandle,
    state_handle: &mut Option<Arc<WorkerState>>,
) -> Result<InitResult, WorkerError> {
    let WorkerTask { source, platform } = task;

    let Some(state) =
        WorkerState::create_injected(platform, isolate, worker_id, tx, monitor_handle).await
    else {
        return Err(WorkerError::Unknown(
            "failed to create worker state".to_string(),
        ));
    };

    // we need to tell create_cancel_safe_task()
    // that we've got a state here, and they can
    // cancel it gracefully
    state_handle.replace(state.clone());

    tracing::info!("initializing environment for worker");

    // environment initialization
    let (module, promise) = {
        scope_with_context!(
            isolate: unsafe { state.get_isolate() },
            let &mut scope,
            let context
        );
        try_catch!(scope: scope, let try_catch);

        let intrinsics_obj =
            unwrap!(try_catch, some init intrinsics::build_intrinsics(&state.platform, try_catch));

        // we're gonna put them in the global
        {
            let context_global = context.global(try_catch);
            unwrap!(
                try_catch,
                some init intrinsics::extract_intrinsics(try_catch, context_global, intrinsics_obj)
            );
        }

        let module = unwrap!(try_catch, some compile compile::compile_module(try_catch, source, "worker.js"));

        state.tick_monitoring();

        // instantiate imports, etc.
        {
            let res = module.instantiate_module(try_catch, compile::resolve_module_callback);
            if res.is_none() {
                return Err(WorkerError::ModuleInitError(try_catch.exception_details()));
            }
        }

        // instantiate evaluations
        let Some(promise) = module.evaluate(try_catch) else {
            return Err(WorkerError::ModuleInitError(try_catch.exception_details()));
        };
        let promise = promise.cast::<Promise>();

        state.tick_monitoring();

        (
            Global::new(try_catch, module),
            Global::new(try_catch, promise),
        )
    };

    // we gotta wait for it to initialize
    {
        let isolate = unsafe { state.get_isolate() };
        while Platform::pump_message_loop(&state.platform, isolate, false) {}
    }

    Ok(InitResult {
        state,
        module,
        promise,
    })
}

/// Gracefully closes the worker state, releasing memory.
#[inline]
async fn close_state(state: Arc<WorkerState>) {
    let isolate = unsafe { &mut *state.isolate.as_ptr() };
    let state2 = WorkerState::open_from_isolate(isolate);
    state2.wait_close().await;

    // at this point, state & state2 gets dropped
    // memory gets freed (hopefully, PLEASE)
}
