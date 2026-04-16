/// Errors produced by a [`KvStore`](crate::KvStore).
#[derive(thiserror::Error, Debug)]
pub enum KvError {
    /// The underlying storage engine returned an error.
    #[error("store error: {0}")]
    Store(String),

    /// Metadata or value could not be (de)serialized as JSON.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// A `spawn_blocking` task was cancelled or panicked.
    #[error("background task failed")]
    TaskFailed,

    /// The pagination cursor is malformed or was produced by a different store.
    #[error("invalid cursor")]
    InvalidCursor,
}
