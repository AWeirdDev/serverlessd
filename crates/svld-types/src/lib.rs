use thiserror::Error;

use svld_language::ExceptionDetails;

/// Represents a worker error.
#[derive(Debug, Error)]
pub enum WorkerError {
    /// Error at runtime.
    #[error("Runtime error, details: {0:?}")]
    RuntimeError(String),

    /// Failed to compile.
    #[error("Failed to compile, details: {0:?}")]
    CompileError(Option<ExceptionDetails>),

    /// Error on initialization.
    #[error("Failed to init module, details: {0:?}")]
    ModuleInitError(Option<ExceptionDetails>),

    // we need to blacklist these workers immediately
    /// Entrypoint is not found.
    #[error("No entrypoint is found at all. Your worker is blacklisted until updated.")]
    NoEntrypoint,

    /// The worker has timed out.
    #[error("Tick, tock! The worker timed out.")]
    Timeout,

    /// An internal error.
    #[error("Unknown internal error.")]
    Unknown(String),

    /// An error from serverlessd.
    #[error("{0}")]
    Serverlessd(String),
}
