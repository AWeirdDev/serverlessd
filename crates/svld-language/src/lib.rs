//! Some helpers (utilities) for interacting with the JavaScript language
//! and the v8 engine itself.

mod exception;
mod promise;
mod type_and_value;

pub use exception::{ExceptionDetails, ExceptionDetailsExt, ThrowException, throw};
pub use promise::Promised;
pub use type_and_value::{TypeAndValue, type_and_value};
