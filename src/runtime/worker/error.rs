use thiserror::Error;

use crate::language::ExceptionDetails;

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("runtime error, details: {0:#?}")]
    RuntimeError(Option<ExceptionDetails>),

    #[error("failed to compile, details: {0:#?}")]
    CompileError(Option<ExceptionDetails>),

    #[error("failed to init module, details: {0:#?}")]
    ModuleInitError(Option<ExceptionDetails>),

    // we need to blacklist these workers immediately
    #[error("no entrypoint is found at all")]
    NoEntrypoint,

    #[error("worker timeout")]
    Timeout,

    #[error("unknown internal error: {0:#?}")]
    Unknown(String),
}
