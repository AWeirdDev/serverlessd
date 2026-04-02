use v8::Local;

pub enum ThrowException<K: AsRef<str>> {
    TypeError(K),
}

impl<K: AsRef<str>> ThrowException<K> {
    fn into_exception<'a>(&self, scope: &'a v8::PinScope) -> v8::Local<'a, v8::Value> {
        match self {
            Self::TypeError(message) => v8::Exception::type_error(
                scope,
                v8::String::new(scope, message.as_ref())
                    .map(|item| item.cast())
                    .unwrap_or_else(|| v8::null(scope).cast()),
            ),
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
