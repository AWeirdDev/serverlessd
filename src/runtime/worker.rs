use tokio::{sync::mpsc, task::JoinHandle};
use v8::{Global, Local, Platform, Promise, SharedRef};

use crate::{
    compile, intrinsics,
    language::{ExceptionDetails, Promised},
    runtime::{Pod, state::WorkerState},
    scope_with_context,
};

#[derive(Debug)]
pub enum WorkerTrigger {
    Halt,
}

pub type WorkerTx = mpsc::Sender<WorkerTrigger>;
type WorkerRx = mpsc::Receiver<WorkerTrigger>;

#[derive(Debug)]
pub struct Worker {
    tx: WorkerTx,
    handle: JoinHandle<Option<()>>,
}

impl Worker {
    #[inline]
    pub fn start(pod: &Pod, task: WorkerTask) -> Self {
        let (tx, rx) = mpsc::channel::<WorkerTrigger>(64);
        let handle = pod.tasks.spawn_local(create_task(task, rx));
        Self { tx, handle }
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

async fn create_task(task: WorkerTask, mut rx: WorkerRx) -> Option<()> {
    let WorkerTask {
        source,
        source_name,
        platform,
    } = task;

    let isolate = Box::new(v8::Isolate::new(Default::default()));
    let state = WorkerState::new_injected(platform, isolate);

    // environment initialization
    let (module, promise) = {
        scope_with_context!(
            isolate: unsafe { state.get_isolate() },
            let &mut scope,
            let context
        );

        let intrinsics_obj = intrinsics::build_intrinsics(&state.platform, scope);

        // we're gonna put them in the global
        {
            let context_global = context.global(scope);
            intrinsics::extract_intrinsics(scope, context_global, intrinsics_obj);
        }

        let module = compile::compile_module(scope, source, source_name);
        module
            .instantiate_module(scope, compile::resolve_module_callback)
            .expect("instantiation failed");

        let promise = module
            .evaluate(scope)
            .expect("failed to evaluate")
            .cast::<Promise>();

        (Global::new(scope, module), Global::new(scope, promise))
    };

    let isolate = unsafe { state.get_isolate() };
    while Platform::pump_message_loop(&state.platform, isolate, false) {}

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
                let exception = ExceptionDetails::from_exception(scope, value)?;
                println!("{:#?}", exception);
                return None;
            }
            Promised::Resolved(value) => {
                println!("{}", value.to_rust_string_lossy(scope));
            }
        }
    }

    let namespace = module.get_module_namespace().cast::<v8::Object>();
    let _entrypoint = namespace.get(scope, v8::String::new(scope, "default")?.cast())?;

    while let Some(event) = rx.recv().await {
        match event {
            WorkerTrigger::Halt => break,
        }
    }

    // clean up
    {
        let state = WorkerState::open_from_isolate(scope);
        state.wait_close().await;
    }

    Some(())
}
