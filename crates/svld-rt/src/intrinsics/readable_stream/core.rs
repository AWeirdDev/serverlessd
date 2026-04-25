use std::ffi::c_void;

use v8::{
    External, FunctionCallbackArguments, Global, Local, Object, PinScope, ReturnValue, Value,
};

use svld_language::{ThrowException, throw, type_and_value};

use crate::{
    WorkerState,
    intrinsics::{
        readable_stream::{
            block::ReadableStreamBlock,
            controller::JsDefaultController,
            source::{UnderlyingSource, UnderlyingSourceParseError},
            state::{ReadableStreamState, StreamInternalState},
        },
        retrieve::retrieve_intrinsic,
    },
};

/// Represents the JavaScript `ReadableStream` API.
///
/// # Internal fields
/// - `0` : `*mut ReadableStreamState`
pub struct JsReadableStream;

impl JsReadableStream {
    /// Creates a ReadableStream pre-loaded with a single chunk and immediately closed.
    pub fn new_with_chunk<'s>(
        scope: &PinScope<'s, '_>,
        rs_constructor: Local<'s, v8::Function>,
        chunk: Local<'s, v8::Value>,
    ) -> Option<Local<'s, v8::Object>> {
        let stream = rs_constructor.new_instance(scope, &[])?;
        let ext = stream.get_internal_field(scope, 0)?.cast::<External>();
        let state_ptr = ext.value() as *mut ReadableStreamState;
        let state = unsafe { &mut *state_ptr };
        state.enqueue(Global::new(scope, chunk));
        state.close_requested = true;
        Some(stream)
    }

    pub fn get_new_fn<'s>(scope: &mut PinScope<'s, '_>) -> Option<Local<'s, Value>> {
        let func_tmpl = v8::FunctionTemplate::new(scope, Self::js_constructor);

        let name = v8::String::new(scope, "ReadableStream")?;
        func_tmpl.set_class_name(name);

        let inst = func_tmpl.instance_template(scope);
        inst.set_internal_field_count(1);

        // prototype
        {
            let proto = func_tmpl.prototype_template(scope);
            {
                let f = v8::FunctionTemplate::new(scope, Self::js_get_reader_callback);
                let k = v8::String::new(scope, "getReader")?;
                proto.set(k.cast(), f.cast());
            }

            {
                let f = v8::FunctionTemplate::new(scope, Self::js_cancel_callback);
                let k = v8::String::new(scope, "cancel")?;
                proto.set(k.cast(), f.cast());
            }
        }

        func_tmpl.get_function(scope).map(|func| func.cast())
    }

    /// Retrieves the `ReadableStream` constructor from the intrinsics object stored in data slot 1.
    #[inline]
    pub fn retrieve<'s>(scope: &mut v8::PinScope<'s, '_>) -> Option<Local<'s, v8::Function>> {
        retrieve_intrinsic(scope, "ReadableStream").map(|k| k.cast())
    }

    fn js_constructor(scope: &mut PinScope, args: FunctionCallbackArguments, _rv: ReturnValue) {
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

        let state = Box::new(ReadableStreamState::new(pull_fn, cancel_fn));
        let state_ptr = Box::into_raw(state);

        // register this to the worker state
        {
            let worker_state = WorkerState::get_from_isolate(scope);
            if !worker_state.blocks.has_block::<ReadableStreamBlock>() {
                worker_state.blocks.push_block(ReadableStreamBlock::new());
            }

            // we'll register this
            worker_state
                .blocks
                .with_block::<ReadableStreamBlock, _>(|block| {
                    block.register(state_ptr);
                });
        }

        let controller = JsDefaultController::build_controller(scope, state_ptr).unwrap();
        unsafe { &mut *state_ptr }
            .controller
            .replace(Global::new(scope, controller));

        args.this()
            .set_internal_field(0, External::new(scope, state_ptr as *mut c_void).cast());

        // invoke underlyingSource.start(controller), if any
        if let Some(gstart) = start_fn {
            let start = Local::new(scope, gstart);
            start.call(scope, v8::undefined(scope).cast(), &[controller.cast()]);
        }
    }

    /// ```ts
    /// stream.getReader(): ReadableStreamDefaultReader
    /// ```
    fn js_get_reader_callback(
        scope: &mut PinScope,
        args: FunctionCallbackArguments,
        mut rv: ReturnValue,
    ) {
        let state_ptr = match get_state_ptr(scope, args.this()) {
            Some(p) => p,
            None => {
                throw(
                    scope,
                    ThrowException::type_error("Invalid ReadableStream receiver"),
                );
                return;
            }
        };

        let reader = JsReadableStreamReader::build_object(scope, state_ptr);
        rv.set(reader.cast());
    }

    /// ```js
    /// stream.cancel(reason?)
    /// ```
    fn js_cancel_callback(scope: &mut PinScope, args: FunctionCallbackArguments, _rv: ReturnValue) {
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
        state.state = StreamInternalState::Closed;

        while let Some(gresolve) = state.pending_reads.pop_front() {
            let resolver = Local::new(scope, gresolve);
            let done_result = {
                let obj = Object::new(scope);

                let vk = v8::String::new(scope, "value").unwrap().cast::<Value>();
                let dk = v8::String::new(scope, "done").unwrap().cast::<Value>();

                obj.set(scope, vk, v8::undefined(scope).cast());
                obj.set(scope, dk, v8::Boolean::new(scope, true).cast());
                obj
            };
            resolver.resolve(scope, done_result.cast());
        }
    }
}

struct JsReadableStreamReader;

impl JsReadableStreamReader {
    /// Build a `ReadableStreamDefaultReader` object backed by `state_ptr`.
    fn build_object<'s>(
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

        set_method!("read", Self::js_read_callback);
        set_method!("releaseLock", Self::js_release_lock_callback);
        set_method!("cancel", Self::js_cancel_callback);

        obj
    }

    /// ```ts
    /// reader.read(): Promise<{ value, done }>
    /// ```
    fn js_read_callback(
        scope: &mut PinScope,
        args: FunctionCallbackArguments,
        mut rv: ReturnValue,
    ) {
        let state_ptr = args.data().cast::<External>().value() as *mut ReadableStreamState;
        let Some(res) = JsDefaultController::perform_read(scope, state_ptr) else {
            return;
        };
        rv.set(res);
    }

    /// ```js
    /// reader.releaseLock()
    /// ```
    ///
    /// No-op. Single-reader model.
    fn js_release_lock_callback(
        _scope: &mut PinScope,
        _args: FunctionCallbackArguments,
        _rv: ReturnValue,
    ) {
    }

    /// ```ts
    /// reader.cancel(reason?)
    /// ```
    ///
    /// Delegates to stream cancel logic.
    fn js_cancel_callback(scope: &mut PinScope, args: FunctionCallbackArguments, _rv: ReturnValue) {
        let state_ptr = args.data().cast::<External>().value() as *mut ReadableStreamState;
        let state = unsafe { &mut *state_ptr };

        let reason = args.get(0);
        if let Some(gcancel) = &state.cancel_fn {
            let cancel = Local::new(scope, gcancel.clone());
            let recv = v8::undefined(scope).cast::<Value>();
            cancel.call(scope, recv, &[reason]);
        }

        state.queue.clear();
        state.state = StreamInternalState::Closed;

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
}

#[inline(always)]
fn get_state_ptr(scope: &mut PinScope, this: Local<Object>) -> Option<*mut ReadableStreamState> {
    let ext = this.get_internal_field(scope, 0)?.cast::<External>();
    Some(ext.value() as *mut ReadableStreamState)
}
