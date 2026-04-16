use async_trait::async_trait;

use crate::{KvEntry, KvEntryWithMetadata, KvError, ListOptions, ListResult, PutOptions};

/// The core KV storage interface — mirrors the Cloudflare Workers KV API.
///
/// # Implementations
/// | Type | Persistence | Use case |
/// |------|-------------|----------|
/// | [`MemoryKv`](crate::MemoryKv) | None (in-process) | Development, tests |
/// | [`SqliteKv`](crate::SqliteKv) | Disk (SQLite WAL) | Local / self-hosted |
///
/// # Example (in the V8 binding layer)
/// ```rust,ignore
/// // Create a shared namespace and hand it to every worker.
/// let kv: Arc<dyn KvStore> = Arc::new(SqliteKv::open("data/my-namespace.db")?);
///
/// // In the worker state, call these from a spawn_blocking / scheduled resolution:
/// let entry = kv.get("hello").await?;
/// kv.put("hello", b"world".to_vec(), PutOptions::default()).await?;
/// ```
#[async_trait]
pub trait KvStore: Send + Sync + 'static {
    /// Return the value for `key`, or `None` if the key does not exist
    /// or has expired.
    async fn get(&self, key: &str) -> Result<Option<KvEntry>, KvError>;

    /// Return the value **and** its metadata for `key`, or `None` if the
    /// key does not exist or has expired.
    async fn get_with_metadata(&self, key: &str) -> Result<Option<KvEntryWithMetadata>, KvError>;

    /// Store `value` under `key`, replacing any previous entry.
    async fn put(&self, key: &str, value: Vec<u8>, options: PutOptions) -> Result<(), KvError>;

    /// Delete `key`. Silently succeeds when the key does not exist.
    async fn delete(&self, key: &str) -> Result<(), KvError>;

    /// List keys, with optional prefix filtering and cursor-based pagination.
    async fn list(&self, options: ListOptions) -> Result<ListResult, KvError>;

    /// Delete all expired keys from the store and return how many were removed.
    ///
    /// The two built-in backends perform lazy expiry on reads, so calling
    /// `purge_expired` periodically reclaims space. Defaults to a no-op.
    async fn purge_expired(&self) -> Result<usize, KvError> {
        Ok(0)
    }
}
