use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServerlessdConfig {
    /// The port to bind the HTTP server to.
    #[serde(default = "defaults::port")]
    pub port: u16,

    /// The host to bind the HTTP server to.
    #[serde(default = "defaults::host")]
    pub host: String,

    /// The number of pods (threads) for serverless execution.
    #[serde(default = "defaults::pods")]
    pub pods: usize,

    /// The number of workers per pod (thread) for serverless execution.
    #[serde(default = "defaults::workers_per_pod")]
    pub workers_per_pod: usize,

    /// Available bindings the workers can use.
    #[serde(default)]
    pub bindings: Vec<String>,

    /// Maximum amount of memory in bytes.
    #[serde(default = "defaults::max_memory")]
    pub max_memory: usize,

    /// The strategy for determinating what worker the request is looking for.
    #[serde(default = "defaults::determination_strategy")]
    pub determination_strategy: DeterminationStrategy,
}

crate::from_str_impl!(ServerlessdConfig);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeterminationStrategy {
    /// Use the path to determine.
    ///
    /// For example:
    ///
    /// ```text
    /// http://localhost:3000/worker/my-worker
    /// ```
    ///
    /// => `my-worker`
    ///
    Path,

    /// Use the host name to determine.
    ///
    /// For example:
    ///
    /// ```yaml
    /// Host: my-worker.workers.local
    /// ```
    ///
    /// => `my-worker`
    HostName,
}

mod defaults {
    use super::*;

    #[inline(always)]
    pub const fn port() -> u16 {
        3000
    }

    #[inline(always)]
    pub const fn pods() -> usize {
        50
    }

    #[inline(always)]
    pub const fn workers_per_pod() -> usize {
        2
    }

    #[inline(always)]
    pub fn host() -> String {
        "127.0.0.1".to_string()
    }

    #[inline(always)]
    pub const fn max_memory() -> usize {
        // from cloudflare:
        // https://developers.cloudflare.com/workers/platform/limits
        //
        // their hard-coded limit is 128 MB, as of 2026-05-28
        const CF_MAX_HEAP: usize = 128 * 1024 * 1024;
        CF_MAX_HEAP
    }

    #[inline(always)]
    pub const fn determination_strategy() -> DeterminationStrategy {
        DeterminationStrategy::Path
    }
}
