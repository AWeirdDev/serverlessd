use v8::{Local, PinScope, Value};

/// A container for a type (`Local<v8::String>`) and value (`Local<v8::Value>`).
pub struct TypeAndValue<'s>(pub String, pub Option<Local<'s, v8::Value>>);

impl<'s> TypeAndValue<'s> {
    /// Format to a string.
    #[inline]
    pub fn format_to_string(&self, scope: &PinScope<'s, '_>) -> String {
        format!(
            "{}{}",
            self.0,
            self.1
                .and_then(|item| item.to_string(scope))
                .map(|item| format!(" ({})", item.to_rust_string_lossy(scope)))
                .unwrap_or_else(|| String::new())
        )
    }
}

/// Gets the type of the value, then returns the type and the given value.
///
/// ```rs
/// pub fn type_and_value(scope, value) -> TypeAndValue
/// ```
#[inline]
pub fn type_and_value<'s>(scope: &PinScope<'s, '_>, value: Local<'s, Value>) -> TypeAndValue<'s> {
    if value.is_null() {
        TypeAndValue("null".to_string(), None)
    } else if value.is_undefined() {
        TypeAndValue("undefined".to_string(), None)
    } else {
        let ty = value.type_of(scope).to_rust_string_lossy(scope);
        TypeAndValue(ty, Some(value))
    }
}
