//! Worker state extensions attached to a worker state for safe attached isolate data dropping.

mod client;
mod core;
mod replier;

pub use client::HttpClientWorkerExtension;
pub use core::{WorkerStateExtension, WorkerStateExtensions};
pub use replier::ReplierWorkerStateExtension;
