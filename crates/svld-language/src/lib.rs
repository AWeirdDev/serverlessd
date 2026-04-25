//! Some helpers (utilities) for interacting with the JavaScript language
//! and the v8 engine itself.

mod bytes;
mod exception;
mod promise;
mod type_and_value;

pub use bytes::get_bytes;
pub use exception::{ExceptionDetails, ExceptionDetailsExt, ThrowException, throw};
pub use promise::Promised;
pub use type_and_value::{TypeAndValue, type_and_value};
