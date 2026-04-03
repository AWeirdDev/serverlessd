use v8::{HandleScope, Local, PinScope, PinnedRef, TryCatch};

#[derive(Debug)]
pub struct ExceptionDetails {
    pub name: String,
    pub stack: String,
    pub message: String,
}

impl ExceptionDetails {
    pub fn from_exception(scope: &PinScope, exc: v8::Local<v8::Value>) -> Option<Self> {
        let exc = exc.cast::<v8::Object>();
        let name = exc
            .get(scope, v8::String::new(scope, "name")?.cast())?
            .to_rust_string_lossy(scope);

        let stack = exc
            .get(scope, v8::String::new(scope, "stack")?.cast())?
            .to_rust_string_lossy(scope);

        let message = exc
            .get(scope, v8::String::new(scope, "message")?.cast())?
            .to_rust_string_lossy(scope);

        Some(Self {
            name,
            stack,
            message,
        })
    }
}

pub trait ExceptionDetailsExt {
    /// Gets the exception details for better error-handling support.
    fn exception_details(&self) -> Option<ExceptionDetails>;
}

impl ExceptionDetailsExt for PinnedRef<'_, TryCatch<'_, '_, HandleScope<'_>>> {
    #[inline]
    fn exception_details(&self) -> Option<ExceptionDetails> {
        self.exception()
            .and_then(|item| ExceptionDetails::from_exception(self, item))
    }
}

pub enum ThrowException<K: AsRef<str>> {
    Error(K),
    TypeError(K),
}

impl<K: AsRef<str>> ThrowException<K> {
    fn into_exception<'a>(&self, scope: &'a v8::PinScope) -> v8::Local<'a, v8::Value> {
        macro_rules! bind_to_v8_err {
            (message: $message:expr, exc: $exc:expr) => {
                $exc(
                    scope,
                    v8::String::new(scope, $message.as_ref())
                        .map(|item| item.cast())
                        .unwrap_or_else(|| v8::null(scope).cast()),
                )
            };
        }

        match self {
            Self::Error(message) => {
                bind_to_v8_err!(message: message, exc: v8::Exception::error)
            }

            Self::TypeError(message) => {
                bind_to_v8_err!(message: message, exc: v8::Exception::type_error)
            }
        }
    }
}

/// Throw an exception.
///
/// # Returns
/// The created exception.
#[inline]
pub fn throw<'a, K: AsRef<str>>(
    scope: &'a v8::PinScope,
    exc: ThrowException<K>,
) -> Local<'a, v8::Value> {
    let exc = exc.into_exception(scope);
    scope.throw_exception(exc);
    exc
}
