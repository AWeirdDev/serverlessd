//! "Triggers" are messages passed to task loops.
//!
//! Communicatation via message passing avoids race conditions or synchronization
//! errors.

mod pod;
mod worker;

pub use pod::*;
pub use worker::*;
