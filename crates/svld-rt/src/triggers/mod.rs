//! "Triggers" are messages passed to task loops.
//!
//! Communicatation via message passing avoids race conditions or synchronization
//! errors.

mod pod;
mod serverless;
mod worker;

pub use pod::*;
pub use serverless::*;
pub use worker::*;
