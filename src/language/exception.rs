use v8::{HandleScope, PinScope, PinnedRef, TryCatch};

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
