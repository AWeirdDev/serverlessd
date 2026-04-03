//! Some helpers (utilities) for interacting with the JavaScript language
//! and the v8 engine itself.

mod exception;
mod promise;

pub use exception::{ExceptionDetails, ExceptionDetailsExt, ThrowException, throw};
pub use promise::Promised;
