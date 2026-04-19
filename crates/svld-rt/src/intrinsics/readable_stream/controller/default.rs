use std::ffi::c_void;

use v8::{
    External, FunctionCallbackArguments, Global, Local, Object, PinScope, ReturnValue, Value,
};

use svld_language::{ThrowException, throw};

use crate::intrinsics::readable_stream::{ReadableStreamState, StreamInternalState};

#[inline(always)]
fn get_state<'a>(args: &'a FunctionCallbackArguments) -> &'a mut ReadableStreamState {
    unsafe { &mut *(args.data().cast::<External>().value() as *mut _) }
}

/// Build `{ value, done }` from a Global.
fn build_gdone_result<'s>(
    scope: &mut PinScope<'s, '_>,
    value: Global<Value>,
    done: bool,
) -> Option<Local<'s, Object>> {
    let val = Local::new(scope, value);
    let obj = Object::new(scope);

    let v_k = v8::String::new(scope, "value")?.cast::<Value>();
    obj.set(scope, v_k, val);

    let d_k = v8::String::new(scope, "done")?.cast::<Value>();
    obj.set(scope, d_k, v8::Boolean::new(scope, done).cast());

    Some(obj)
}

fn build_done_result<'s>(scope: &mut PinScope<'s, '_>) -> Option<Local<'s, Object>> {
    let obj = Object::new(scope);
    let v_k = v8::String::new(scope, "value")?.cast::<Value>();
    obj.set(scope, v_k, v8::undefined(scope).cast());

    let d_k = v8::String::new(scope, "done")?.cast::<Value>();
    obj.set(scope, d_k, v8::Boolean::new(scope, true).cast());

    Some(obj)
}

pub struct JsDefaultController;

impl JsDefaultController {
    /// Implements the core `read()` logic: dequeue a chunk or park the resolver.
    ///
    /// Calls `pull` when parking so push-sources get a chance to produce data
    /// before the microtask checkpoint. Returns the promise to set on `rv`.
    pub fn perform_read<'s>(
        scope: &mut PinScope<'s, '_>,
        state_ptr: *mut ReadableStreamState,
    ) -> Option<Local<'s, v8::Value>> {
        let state = unsafe { &mut *state_ptr };

        let resolver = v8::PromiseResolver::new(scope)?;
        let promise = resolver.get_promise(scope);

        match &state.state {
            StreamInternalState::Errored(r) => {
                let greason = r.clone();
                let reason = Local::new(scope, greason);
                resolver.reject(scope, reason);
            }

            StreamInternalState::Closed => {
                if let Some(gchunk) = state.dequeue() {
                    let result = build_gdone_result(scope, gchunk, false)?;
                    resolver.resolve(scope, result.cast());
                } else {
                    let result = build_done_result(scope)?;
                    resolver.resolve(scope, result.cast());
                }
            }

            StreamInternalState::Readable => {
                if let Some(gchunk) = state.dequeue() {
                    let result = build_gdone_result(scope, gchunk, false)?;
                    resolver.resolve(scope, result.cast());

                    // If close was requested and queue is now drained, close.
                    if state.close_requested && state.queue.is_empty() {
                        state.state = StreamInternalState::Closed;
                        // Any further parked reads are handled on their own calls.
                    }
                } else {
                    // Park the resolver then invite the source to produce data.
                    let gresolve = Global::new(scope, resolver);
                    state.pending_reads.push_back(gresolve);

                    // Call pull(controller) if available — may synchronously enqueue
                    // and thus resolve the resolver we just parked.
                    if let (Some(ref gc), Some(ref gp)) =
                        (state.controller.clone(), state.pull_fn.clone())
                    {
                        let controller = Local::new(scope, gc);
                        let pull = Local::new(scope, gp);
                        let recv = v8::undefined(scope).cast::<Value>();
                        pull.call(scope, recv, &[controller.cast()]);
                    }
                }
            }
        }

        Some(promise.cast())
    }

    pub fn build_controller<'s>(
        scope: &mut PinScope<'s, '_>,
        state_ptr: *mut ReadableStreamState,
    ) -> Option<Local<'s, Object>> {
        let data = External::new(scope, state_ptr as *mut c_void);

        let obj = Object::new(scope);

        macro_rules! set_method {
            ($name:literal, $cb:expr) => {{
                let f = v8::Function::builder($cb).data(data.cast()).build(scope)?;
                let key = v8::String::new(scope, $name).unwrap().cast::<Value>();
                obj.set(scope, key, f.cast());
            }};
        }

        set_method!("enqueue", Self::js_enqueue_callback);
        set_method!("close", Self::js_close_callback);
        set_method!("error", Self::js_error_callback);

        Some(obj)
    }

    /// ```js
    /// controller.enqueue(chunk)
    /// ```
    ///
    /// If a `read()` is parked waiting for data, resolve it immediately.
    /// Otherwise buffer the chunk.
    fn js_enqueue_callback(
        scope: &mut PinScope,
        args: FunctionCallbackArguments,
        _rv: ReturnValue,
    ) {
        let state = get_state(&args);

        if !state.is_readable() {
            throw(
                scope,
                ThrowException::type_error(
                    "Cannot enqueue into a closed or errored ReadableStream",
                ),
            );
            return;
        }

        let gchunk = Global::new(scope, args.get(0));

        if let Some(gresolve) = state.pending_reads.pop_front() {
            let resolver = Local::new(scope, gresolve);

            let Some(result) = build_gdone_result(scope, gchunk, false) else {
                return;
            };

            resolver.resolve(scope, result.cast());
        } else {
            state.enqueue(gchunk);
        }
    }

    /// ```js
    /// controller.close()
    /// ```
    ///
    /// Marks the stream as close-requested. If the queue is already empty,
    /// transitions immediately to `Closed` and resolves any parked readers with
    /// `{ value: undefined, done: true }`.
    fn js_close_callback(scope: &mut PinScope, args: FunctionCallbackArguments, _rv: ReturnValue) {
        let state = get_state(&args);

        if !state.is_readable() || state.close_requested {
            throw(
                scope,
                ThrowException::type_error("Cannot close a ReadableStream that is already closed"),
            );
            return;
        }

        state.close_requested = true;

        if state.queue.is_empty() {
            state.state = StreamInternalState::Closed;
            while let Some(gresolve) = state.pending_reads.pop_front() {
                let resolver = Local::new(scope, gresolve);

                let Some(result) = build_done_result(scope) else {
                    return;
                };

                resolver.resolve(scope, result.cast());
            }
        }

        // otherwise pending_reads will be drained as the queue empties in read().
    }

    /// ```js
    /// controller.error(reason)
    /// ```
    ///
    /// Transitions the stream to the `Errored` state and rejects all parked readers.
    fn js_error_callback(scope: &mut PinScope, args: FunctionCallbackArguments, _rv: ReturnValue) {
        let state = get_state(&args);

        if !state.is_readable() {
            throw(
                scope,
                ThrowException::type_error("Cannot error a ReadableStream that is already closed"),
            );
            return;
        }

        let reason = args.get(0);
        let greason = Global::new(scope, reason);

        state.state = StreamInternalState::Errored(greason);
        state.queue.clear();

        while let Some(gresolve) = state.pending_reads.pop_front() {
            let resolver = Local::new(scope, gresolve);
            resolver.reject(scope, reason);
        }
    }
}
