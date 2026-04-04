use std::ptr::null_mut;

use tokio::sync::{mpsc, oneshot};
use v8::{Global, Isolate, Local, Platform, Promise, SharedRef};

use crate::{
    compile, intrinsics,
    language::{ExceptionDetails, ExceptionDetailsExt, Promised},
    runtime::{Pod, monitor::MonitorHandle, state::WorkerState},
    scope_with_context, try_catch,
};

#[derive(Debug)]
#[allow(unused)]
pub enum WorkerTrigger {
    Http {},
    Halt { token: oneshot::Sender<()> },
}

pub type WorkerTx = mpsc::Sender<WorkerTrigger>;
type WorkerRx = mpsc::Receiver<WorkerTrigger>;

/// A serverless worker.
#[derive(Debug)]
#[repr(transparent)]
pub struct Worker {
    tx: WorkerTx,
}

impl Worker {
    #[inline]
    pub fn start(pod: &Pod, task: WorkerTask) -> Self {
        let (tx, rx) = mpsc::channel::<WorkerTrigger>(64);

        let monitor = pod.monitor.clone();

        pod.tasks.spawn_local(create_task(task, rx, monitor));
        Self { tx }
    }

    /// Trigger.
    ///
    /// Returns `false` if the channel is closed.
    #[inline(always)]
    #[must_use]
    pub async fn trigger(&self, trigger: WorkerTrigger) -> bool {
        self.tx.send(trigger).await.is_ok()
    }
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
///     // name for exception display
///     source_name: "worker.js",
///
///     // the platform
///     platform: serverless.get_platform(),
/// }
/// ```
#[derive(Debug)]
pub struct WorkerTask {
    // TODO: use BTreeMap
    pub source: String,
    pub source_name: String,
    pub platform: SharedRef<Platform>,
}

#[tracing::instrument(skip_all)]
async fn create_task(task: WorkerTask, mut rx: WorkerRx, monitor: MonitorHandle) {
    let WorkerTask {
        source,
        source_name,
        platform,
    } = task;

    let isolate = Box::new(v8::Isolate::new(Default::default()));
    let Some(monitoring) = monitor.start_monitoring(isolate.thread_safe_handle()).await else {
        tracing::error!("failed to start monitoring");
        return;
    };

    let state = WorkerState::new_injected(platform, isolate);

    macro_rules! some {
        ($scope:expr, $k:expr) => {{
            let Some(m) = $k else {
                tracing::error!("errored on worker, force quit");
                close_state($scope).await;
                return;
            };
            m
        }};
    }

    tracing::info!("initializing environment for worker");

    // environment initialization
    let (module, promise) = {
        scope_with_context!(
            isolate: unsafe { state.get_isolate() },
            let &mut scope,
            let context
        );

        let intrinsics_obj = intrinsics::build_intrinsics(&state.platform, scope);
        tracing::info!("built intrinsics");

        // we're gonna put them in the global
        {
            let context_global = context.global(scope);
            intrinsics::extract_intrinsics(scope, context_global, intrinsics_obj);
        }
        tracing::info!("extracted intrinsics");

        let module = compile::compile_module(scope, source, source_name);
        tracing::info!("compiled module");

        monitoring.tick();

        try_catch!(scope: scope, let try_catch);

        tracing::info!("instantiating module");
        // instantiate imports, etc.
        {
            let res = module.instantiate_module(try_catch, compile::resolve_module_callback);
            if res.is_none() {
                tracing::info!(
                    "error while instantiating module, reason: {:#?}",
                    try_catch.exception_details()
                );
                return;
            }
        }
        tracing::info!("instantiating module");

        // instantiate evaluations
        let Some(promise) = module.evaluate(try_catch) else {
            tracing::info!(
                "error while evaluating module, reason: {:#?}",
                try_catch.exception_details()
            );
            close_state(try_catch).await;
            return;
        };

        let promise = promise.cast::<Promise>();

        monitoring.tick();

        (
            Global::new(try_catch, module),
            Global::new(try_catch, promise),
        )
    };

    tracing::info!("resolving promise for worker env init");

    let isolate = unsafe { state.get_isolate() };
    while Platform::pump_message_loop(&state.platform, isolate, false) {}

    tracing::info!("resolved promise for worker env init");

    scope_with_context!(
        isolate: isolate,
        let &mut scope,
        let context
    );

    let module = Local::new(scope, module);
    {
        let promise = Local::new(scope, promise);
        let promised = Promised::new(scope, promise);

        match promised {
            Promised::Rejected(value) => {
                // usually we get an exception
                let exception = some!(scope, ExceptionDetails::from_exception(scope, value));
                tracing::error!("failed to init worker env, reason: {:?}", exception);
                return;
            }
            Promised::Resolved(_) => {
                tracing::info!("worker env initialized")
            }
        }
    }

    let namespace = module.get_module_namespace().cast::<v8::Object>();
    let entrypoint = some!(
        scope,
        namespace.get(
            scope,
            some!(scope, v8::String::new(scope, "default")).cast(),
        )
    );

    if !entrypoint.is_object() || entrypoint.is_null_or_undefined() {
        tracing::error!("error while getting worker entrypoint");
        close_state(scope).await;
        return;
    }

    let entrypoint = entrypoint.cast::<v8::Object>();
    let entrypoint_fetch = {
        let item = some!(
            scope,
            entrypoint.get(scope, some!(scope, v8::String::new(scope, "fetch")).cast())
        );

        if item.is_function() {
            Some(item.cast::<v8::Function>())
        } else {
            None
        }
    };

    while let Some(event) = rx.recv().await {
        scope.perform_microtask_checkpoint();

        match event {
            WorkerTrigger::Halt { token } => {
                // clean up
                tracing::info!("worker clean up");
                close_state(scope).await;

                token.send(()).ok();

                break;
            }

            WorkerTrigger::Http {} => {
                if let Some(fetch) = entrypoint_fetch {
                    monitoring.tick();

                    try_catch!(scope: scope, let try_catch);

                    let result = some!(
                        try_catch,
                        fetch.call(try_catch, v8::undefined(try_catch).cast(), &[])
                    );
                    // if result.is_promise() {
                    //     result.cast::<v8::Promise>();
                    // }

                    monitoring.tick();
                }
            }
        }
    }
}

/// Gracefully closes the worker state, releasing memory.
#[inline]
async fn close_state(scope: &mut Isolate) {
    let state = WorkerState::open_from_isolate(scope);
    state.wait_close().await;
    scope.set_data(0, null_mut() as *mut _);
}
