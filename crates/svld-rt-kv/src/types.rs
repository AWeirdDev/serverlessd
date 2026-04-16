use serde::{Deserialize, Serialize};

// ── get / get_with_metadata ──────────────────────────────────────────────────

/// Value returned by [`KvStore::get`].
#[derive(Debug, Clone)]
pub struct KvEntry {
    /// Raw bytes stored for this key.
    pub value: Vec<u8>,
    /// Absolute Unix timestamp (seconds) at which the key expires,
    /// or `None` if the key has no expiry.
    pub expiration: Option<u64>,
}

/// Value returned by [`KvStore::get_with_metadata`].
#[derive(Debug, Clone)]
pub struct KvEntryWithMetadata {
    /// Raw bytes stored for this key.
    pub value: Vec<u8>,
    /// Optional JSON metadata attached during `put`.
    pub metadata: Option<serde_json::Value>,
    /// Absolute Unix timestamp (seconds) at which the key expires.
    pub expiration: Option<u64>,
}

// ── put ──────────────────────────────────────────────────────────────────────

/// Options accepted by [`KvStore::put`].
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    /// Absolute Unix timestamp (seconds) at which the key should expire.
    /// Takes precedence over `expiration_ttl` when both are set.
    pub expiration: Option<u64>,
    /// Relative TTL in seconds from the time of the `put` call.
    pub expiration_ttl: Option<u64>,
    /// Arbitrary JSON metadata to attach to the key.
    /// Retrievable via `get_with_metadata` and `list`.
    pub metadata: Option<serde_json::Value>,
}

// ── list ─────────────────────────────────────────────────────────────────────

/// Options accepted by [`KvStore::list`].
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Only return keys that start with this string.
    pub prefix: Option<String>,
    /// Maximum number of keys to return per page. Defaults to 1000.
    pub limit: Option<u32>,
    /// Opaque cursor from a previous [`ListResult`] for pagination.
    pub cursor: Option<String>,
}

/// A single key entry returned by [`KvStore::list`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvKey {
    /// The key name.
    pub name: String,
    /// Absolute Unix timestamp (seconds) at which the key expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration: Option<u64>,
    /// Metadata attached to the key, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Result from [`KvStore::list`].
#[derive(Debug, Clone)]
pub struct ListResult {
    /// The matching keys for this page.
    pub keys: Vec<KvKey>,
    /// `true` if this is the final page (no more keys after this).
    pub list_complete: bool,
    /// Pass this to the next `list` call as `cursor` to fetch the next page.
    /// `None` when `list_complete` is `true`.
    pub cursor: Option<String>,
}
