use v8::{Function, Global, Local, Object, PinScope, Value};

#[allow(unused)]
pub struct UnderlyingSource {
    /// Called once on construction.
    /// Set up push sources or enqueue initial chunks here.
    ///
    /// ```ts
    /// declare function start(
    ///     controller: ReadableStreamDefaultController | ReadableByteStreamController
    /// );
    /// ```
    pub start: Option<Global<Function>>,

    /// Called repeatedly when the internal queue is not full.
    /// Returns a `Promise` to pause pull calls until it resolves.
    ///
    /// ```ts
    /// declare function pull(
    ///     controller: ReadableStreamDefaultController | ReadableByteStreamController
    /// );
    /// ```
    pub pull: Option<Global<Function>>,

    /// Called when the consumer cancels the stream.
    /// Use to release underlying resources.
    ///
    /// ```ts
    /// declare function cancel(reason: string);
    /// ```
    pub cancel: Option<Global<Function>>,

    /// Set to "bytes" for a byte stream with BYOB reader support.
    /// Omit for default stream.
    ///
    /// ```ts
    /// "bytes" | undefined
    /// ```
    pub type_: Option<String>,

    /// Byte streams only.
    /// Auto-allocates a buffer of this size for BYOB read requests, enabling zero-copy transfers.
    pub auto_allocate_chunk_size: Option<usize>,
}

impl UnderlyingSource {
    pub fn new<'s>(
        scope: &PinScope<'s, '_>,
        value: Local<'s, Value>,
    ) -> Result<Self, UnderlyingSourceParseError> {
        if value.is_undefined() {
            return Err(UnderlyingSourceParseError::Undefined);
        }

        if !value.is_object() || value.is_null() {
            return Err(UnderlyingSourceParseError::NotObject);
        }

        let underlying_source = value.cast::<Object>();

        let start_fn = get_usrc_function(scope, underlying_source, "start");
        let pull_fn = get_usrc_function(scope, underlying_source, "pull");
        let cancel_fn = get_usrc_function(scope, underlying_source, "cancel");

        let type_ = {
            v8::String::new(scope, "type")
                .map(|item| item.cast::<Value>())
                .and_then(|key_name| underlying_source.get(scope, key_name))
                .and_then(|item| {
                    if !item.is_null_or_undefined() && item.is_string() {
                        Some(item.to_rust_string_lossy(scope))
                    } else {
                        None
                    }
                })
        };

        let auto_allocate_chunk_size = {
            v8::String::new(scope, "autoAllocateChunkSize")
                .map(|item| item.cast::<Value>())
                .and_then(|key_name| underlying_source.get(scope, key_name))
                .and_then(|item| {
                    if !item.is_null_or_undefined() && item.is_number() {
                        Some(item.cast::<v8::Number>())
                    } else {
                        None
                    }
                })
                .and_then(|item| item.uint32_value(scope))
                .map(|item| item as usize)
        };

        Ok(Self {
            start: start_fn.map(|item| Global::new(scope, item)),
            pull: pull_fn.map(|item| Global::new(scope, item)),
            cancel: cancel_fn.map(|item| Global::new(scope, item)),
            type_,
            auto_allocate_chunk_size,
        })
    }
}

/// Get a field value from underlying source.
#[inline(always)]
#[must_use]
fn get_usrc_function<'s>(
    scope: &PinScope<'s, '_>,
    obj: Local<'s, Object>,
    name: &'static str,
) -> Option<Local<'s, Function>> {
    v8::String::new(scope, name)
        .map(|item| item.cast::<Value>())
        .and_then(|key_name| obj.get(scope, key_name))
        .and_then(|item| {
            if !item.is_null_or_undefined() && item.is_function() {
                Some(item.cast::<Function>())
            } else {
                None
            }
        })
}

#[derive(Debug, thiserror::Error)]
pub enum UnderlyingSourceParseError {
    #[error("expected an object")]
    NotObject,

    #[error("received undefined")]
    Undefined,
}
