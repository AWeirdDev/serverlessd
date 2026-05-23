use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde::Deserialize;

/// An HTTP response from the worker.
#[derive(bon::Builder, Debug)]
pub struct WorkerHttpResponse {
    /// The HTTP status code.
    pub status: StatusCode,

    /// The HTTP headers.
    pub headers: HeaderMap,

    /// The body in bytes.
    /// You can obtain this with `get_bytes()` from the `language` crate.
    pub body: Bytes,
}

#[derive(bon::Builder, Debug)]
pub struct WorkerHttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Bytes,
}

#[derive(bon::Builder, Debug, Deserialize)]
pub struct WorkerConfig {
    pub bindings: Vec<BindingConfig>,
}

#[derive(bon::Builder, Debug, Deserialize)]
pub struct BindingConfig {
    /// The name of the binding. For example: `MY_BINDING`.
    pub name: String,

    /// The type of the binding. For example: `kv`.
    pub type_: String,
}
