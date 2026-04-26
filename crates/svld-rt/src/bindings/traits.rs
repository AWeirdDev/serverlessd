use v8::{Local, PinScope, Value};

use crate::blocks::Block;

/// A binding.
///
/// Bindings can extend runtime features.
pub trait BindingClient: Block {
    /// Gets the JavaScript value of this binding.
    ///
    /// # Returns
    /// `Some( value )` if successful, otherwise `None`.
    fn get_js_value<'s>(scope: &mut PinScope<'s, '_>) -> Option<Local<'s, Value>>;
}
