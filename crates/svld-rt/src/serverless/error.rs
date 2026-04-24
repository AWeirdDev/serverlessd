#[derive(Debug, thiserror::Error)]
pub enum CreateWorkerError {
    #[error("unknown worker {0:?}")]
    UnknownWorker(String),

    #[error("cannot create task: {0}")]
    CannotCreateTask(String),
}
