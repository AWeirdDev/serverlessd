mod block;
mod controller;
mod source;
mod state;

use std::ffi::c_void;

use v8::{External, FunctionCallbackArguments, Global, Local, Object, PinScope, ReturnValue, Value};

use svld_language::{ThrowException, throw, type_and_value};

use crate::{
    block::ReadableStreamBlock,
    controller::default::{build_controller, perform_read},
    source::{UnderlyingSource, UnderlyingSourceParseError},
    state::ReadableStreamState,
};

pub use block::ReadableStreamBlock as RsBlock;

// ── ISOLATE SLOT ──────────────────────────────────────────────────────────────
//
// Slot 1 carries a *mut ReadableStreamBlock so the JS constructor can register
// each newly-allocated ReadableStreamState for cleanup.  Slot 0 is already
// used by WorkerState.

const BLOCK_SLOT: u32 = 1;

// ── stream object internal field layout ──────────────────────────────────────
//
// Internal field 0 : *mut ReadableStreamState  (as v8::External)

struct JsReadableStream;

impl JsReadableStream {
    fn build_object<'s>(scope: &mut PinScope<'s, '_>) -> Local<'s, Value> {
        let func_tmpl = v8::FunctionTemplate::new(scope, Self::constructor);

        let name = v8::String::new(scope, "ReadableStream").unwrap();
        func_tmpl.set_class_name(name);

        let inst = func_tmpl.instance_template(scope);
        inst.set_internal_field_count(1);

        // ── prototype methods ─────────────────────────────────────────────────
        let proto = func_tmpl.prototype_template(scope);

        {
            let f = v8::FunctionTemplate::new(scope, get_reader_callback);
            let k = v8::String::new(scope, "getReader").unwrap();
            proto.set(k.cast(), f.cast());
        }

        {
            let f = v8::FunctionTemplate::new(scope, cancel_callback);
            let k = v8::String::new(scope, "cancel").unwrap();
            proto.set(k.cast(), f.cast());
        }

        func_tmpl.get_function(scope).unwrap().cast()
    }

    fn constructor(scope: &mut PinScope, args: FunctionCallbackArguments, _rv: ReturnValue) {
        if !args.is_construct_call() {
            throw(
                scope,
                ThrowException::type_error(
                    "ReadableStream must be called as a constructor (use `new`)",
                ),
            );
            return;
        }

        let source = {
            let arg0 = args.get(0);
            match UnderlyingSource::new(scope, arg0) {
                Ok(src) => Some(src),
                Err(UnderlyingSourceParseError::Undefined) => None,
                Err(UnderlyingSourceParseError::NotObject) => {
                    throw(
                        scope,
                        ThrowException::type_error(format!(
                            "The \"source\" argument must be of type object. Received {}",
                            type_and_value(scope, arg0).format_to_string(scope)
                        )),
                    );
                    return;
                }
            }
        };

        let (start_fn, pull_fn, cancel_fn) = source
            .map(|s| (s.start, s.pull, s.cancel))
            .unwrap_or((None, None, None));

        // Allocate state on the heap; ownership transferred to the block.
        let state = Box::new(ReadableStreamState::new(pull_fn, cancel_fn));
        let state_ptr = Box::into_raw(state);

        // Register with the ReadableStreamBlock so it gets freed on worker drop.
        let block_ptr = scope.get_data(BLOCK_SLOT) as *mut ReadableStreamBlock;
        if !block_ptr.is_null() {
            unsafe { &*block_ptr }.register(state_ptr);
        }

        // Store state pointer in the JS object's internal field.
        let ext = External::new(scope, state_ptr as *mut c_void);
        args.this().set_internal_field(0, ext.cast());

        // Build the controller and cache it in the state.
        let controller = build_controller(scope, state_ptr);
        unsafe { &mut *state_ptr }.controller = Some(Global::new(scope, controller));

        // Invoke underlyingSource.start(controller) if provided.
        if let Some(gstart) = start_fn {
            let start = Local::new(scope, gstart);
            let recv = v8::undefined(scope).cast::<Value>();
            start.call(scope, recv, &[controller.cast()]);
        }
    }
}

// ── stream prototype callbacks ────────────────────────────────────────────────

/// stream.getReader() → ReadableStreamDefaultReader
fn get_reader_callback(
    scope: &mut PinScope,
    args: FunctionCallbackArguments,
    mut rv: ReturnValue,
) {
    let state_ptr = match get_state_ptr(scope, args.this()) {
        Some(p) => p,
        None => {
            throw(scope, ThrowException::type_error("Invalid ReadableStream receiver"));
            return;
        }
    };

    let reader = build_reader(scope, state_ptr);
    rv.set(reader.cast());
}

/// stream.cancel(reason?) → undefined (best-effort)
fn cancel_callback(
    scope: &mut PinScope,
    args: FunctionCallbackArguments,
    _rv: ReturnValue,
) {
    let state_ptr = match get_state_ptr(scope, args.this()) {
        Some(p) => p,
        None => return,
    };
    let state = unsafe { &mut *state_ptr };

    let reason = args.get(0);
    if let Some(gcancel) = &state.cancel_fn {
        let cancel = Local::new(scope, gcancel.clone());
        let recv = v8::undefined(scope).cast::<Value>();
        cancel.call(scope, recv, &[reason]);
    }

    state.queue.clear();
    state.state = crate::state::StreamInternalState::Closed;

    while let Some(gresolve) = state.pending_reads.pop_front() {
        let resolver = Local::new(scope, gresolve);
        let undef = v8::undefined(scope).cast::<Value>();
        let done_result = {
            let obj = Object::new(scope);
            let vk = v8::String::new(scope, "value").unwrap().cast::<Value>();
            let dk = v8::String::new(scope, "done").unwrap().cast::<Value>();
            obj.set(scope, vk, undef);
            obj.set(scope, dk, v8::Boolean::new(scope, true).cast());
            obj
        };
        resolver.resolve(scope, done_result.cast());
    }
}

// ── reader ────────────────────────────────────────────────────────────────────

/// Build a `ReadableStreamDefaultReader` object backed by `state_ptr`.
fn build_reader<'s>(
    scope: &mut PinScope<'s, '_>,
    state_ptr: *mut ReadableStreamState,
) -> Local<'s, Object> {
    let data = External::new(scope, state_ptr as *mut c_void);
    let obj = Object::new(scope);

    macro_rules! set_method {
        ($name:literal, $cb:expr) => {{
            let f = v8::Function::builder($cb)
                .data(data.cast())
                .build(scope)
                .unwrap();
            let key = v8::String::new(scope, $name).unwrap().cast::<Value>();
            obj.set(scope, key, f.cast());
        }};
    }

    set_method!("read", reader_read_callback);
    set_method!("releaseLock", reader_release_lock_callback);
    set_method!("cancel", reader_cancel_callback);

    obj
}

/// reader.read() → Promise<{value, done}>
fn reader_read_callback(
    scope: &mut PinScope,
    args: FunctionCallbackArguments,
    mut rv: ReturnValue,
) {
    let state_ptr = args.data().cast::<External>().value() as *mut ReadableStreamState;
    rv.set(perform_read(scope, state_ptr));
}

/// reader.releaseLock() — no-op (single-reader model)
fn reader_release_lock_callback(
    _scope: &mut PinScope,
    _args: FunctionCallbackArguments,
    _rv: ReturnValue,
) {
}

/// reader.cancel(reason?) — delegates to stream cancel logic
fn reader_cancel_callback(
    scope: &mut PinScope,
    args: FunctionCallbackArguments,
    _rv: ReturnValue,
) {
    let state_ptr = args.data().cast::<External>().value() as *mut ReadableStreamState;
    let state = unsafe { &mut *state_ptr };

    let reason = args.get(0);
    if let Some(gcancel) = &state.cancel_fn {
        let cancel = Local::new(scope, gcancel.clone());
        let recv = v8::undefined(scope).cast::<Value>();
        cancel.call(scope, recv, &[reason]);
    }

    state.queue.clear();
    state.state = crate::state::StreamInternalState::Closed;

    while let Some(gresolve) = state.pending_reads.pop_front() {
        let resolver = Local::new(scope, gresolve);
        let result = {
            let obj = Object::new(scope);
            let vk = v8::String::new(scope, "value").unwrap().cast::<Value>();
            let dk = v8::String::new(scope, "done").unwrap().cast::<Value>();
            obj.set(scope, vk, v8::undefined(scope).cast());
            obj.set(scope, dk, v8::Boolean::new(scope, true).cast());
            obj
        };
        resolver.resolve(scope, result.cast());
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Extract the `*mut ReadableStreamState` from a stream JS object's internal field 0.
#[inline(always)]
fn get_state_ptr(
    scope: &mut PinScope,
    this: Local<Object>,
) -> Option<*mut ReadableStreamState> {
    let ext = this
        .get_internal_field(scope, 0)?
        .cast::<External>();
    Some(ext.value() as *mut ReadableStreamState)
}

// ── public API ────────────────────────────────────────────────────────────────

/// Register `ReadableStream` on the global object.
///
/// Isolate slot [`BLOCK_SLOT`] must already contain a valid
/// `*mut ReadableStreamBlock` (set by the runtime during worker-state
/// initialisation) so the constructor can register its allocations.
pub fn register(scope: &mut PinScope, global: Local<Object>) {
    let ctor = JsReadableStream::build_object(scope);
    let key = v8::String::new(scope, "ReadableStream")
        .unwrap()
        .cast::<Value>();
    global.set(scope, key, ctor);
}
