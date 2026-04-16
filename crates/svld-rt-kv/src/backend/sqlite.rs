use std::{path::Path, sync::{Arc, Mutex}};

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    KvEntry, KvEntryWithMetadata, KvError, KvKey, ListOptions, ListResult, PutOptions,
    backend::{DEFAULT_LIST_LIMIT, decode_cursor, encode_cursor, now_secs, prefix_upper_bound},
    store::KvStore,
};

// ── Schema ────────────────────────────────────────────────────────────────────

const INIT_SQL: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous  = NORMAL;

    CREATE TABLE IF NOT EXISTS kv (
        key        TEXT    PRIMARY KEY NOT NULL,
        value      BLOB    NOT NULL,
        metadata   TEXT,
        expires_at INTEGER
    ) STRICT;

    CREATE INDEX IF NOT EXISTS idx_kv_expires
        ON kv (expires_at)
        WHERE expires_at IS NOT NULL;
";

// ── SqliteKv ──────────────────────────────────────────────────────────────────

/// A persistent KV store backed by a single SQLite database file (WAL mode).
///
/// All blocking I/O runs inside `tokio::task::spawn_blocking` so the async
/// executor is never stalled. Multiple workers can share one `SqliteKv` via
/// `Arc<SqliteKv>` — operations are serialized through a `Mutex`, which is
/// acceptable because SQLite in WAL mode already limits concurrent writers.
///
/// # Naming convention
/// Each KV **namespace** maps to one database file. Create one `SqliteKv`
/// per namespace and hand it to every worker that needs that namespace.
///
/// ```rust,no_run
/// # use svld_rt_kv::SqliteKv;
/// let users_kv = SqliteKv::open(".serverlessd/kv/users.db")?;
/// let config_kv = SqliteKv::open(".serverlessd/kv/config.db")?;
/// # Ok::<(), svld_rt_kv::KvError>(())
/// ```
#[derive(Clone)]
pub struct SqliteKv {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteKv {
    /// Open (or create) a SQLite KV namespace at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, KvError> {
        let conn = Connection::open(path).map_err(|e| KvError::Store(e.to_string()))?;
        conn.execute_batch(INIT_SQL).map_err(|e| KvError::Store(e.to_string()))?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Create an ephemeral in-memory SQLite KV namespace.
    /// Useful for tests or single-request workers.
    pub fn in_memory() -> Result<Self, KvError> {
        Self::open(":memory:")
    }
}

// ── KvStore impl ──────────────────────────────────────────────────────────────

#[async_trait]
impl KvStore for SqliteKv {
    async fn get(&self, key: &str) -> Result<Option<KvEntry>, KvError> {
        let key = key.to_owned();
        let conn = self.conn.clone();
        let now = now_secs() as i64;

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.query_row(
                "SELECT value, expires_at FROM kv
                  WHERE key = ?1
                    AND (expires_at IS NULL OR expires_at > ?2)",
                params![key, now],
                |row| {
                    Ok(KvEntry {
                        value: row.get(0)?,
                        expiration: row.get::<_, Option<i64>>(1)?.map(|v| v as u64),
                    })
                },
            )
            .optional()
            .map_err(|e| KvError::Store(e.to_string()))
        })
        .await
        .map_err(|_| KvError::TaskFailed)?
    }

    async fn get_with_metadata(&self, key: &str) -> Result<Option<KvEntryWithMetadata>, KvError> {
        let key = key.to_owned();
        let conn = self.conn.clone();
        let now = now_secs() as i64;

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();

            conn.query_row(
                "SELECT value, metadata, expires_at FROM kv
                  WHERE key = ?1
                    AND (expires_at IS NULL OR expires_at > ?2)",
                params![key, now],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                    ))
                },
            )
            .optional()
            .map_err(|e| KvError::Store(e.to_string()))?
            .map(|(value, meta_json, expiration)| {
                let metadata = meta_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(KvError::Serialization)?;
                Ok(KvEntryWithMetadata { value, metadata, expiration })
            })
            .transpose()
        })
        .await
        .map_err(|_| KvError::TaskFailed)?
    }

    async fn put(&self, key: &str, value: Vec<u8>, options: PutOptions) -> Result<(), KvError> {
        let key = key.to_owned();
        let conn = self.conn.clone();

        let expires_at = options
            .expiration
            .or_else(|| options.expiration_ttl.map(|ttl| now_secs() + ttl))
            .map(|ts| ts as i64);
        let metadata_json = options
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO kv (key, value, metadata, expires_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(key) DO UPDATE SET
                     value      = excluded.value,
                     metadata   = excluded.metadata,
                     expires_at = excluded.expires_at",
                params![key, value, metadata_json, expires_at],
            )
            .map(|_| ())
            .map_err(|e| KvError::Store(e.to_string()))
        })
        .await
        .map_err(|_| KvError::TaskFailed)?
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        let key = key.to_owned();
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute("DELETE FROM kv WHERE key = ?1", params![key])
                .map(|_| ())
                .map_err(|e| KvError::Store(e.to_string()))
        })
        .await
        .map_err(|_| KvError::TaskFailed)?
    }

    async fn list(&self, options: ListOptions) -> Result<ListResult, KvError> {
        let limit = options.limit.unwrap_or(DEFAULT_LIST_LIMIT) as usize;
        let fetch_limit = (limit + 1) as i64; // fetch one extra to detect next page

        let cursor_key = options.cursor.as_deref().map(decode_cursor).transpose()?;
        let prefix_lower = options.prefix.clone();
        let prefix_upper = options.prefix.as_deref().and_then(prefix_upper_bound);
        let conn = self.conn.clone();
        let now = now_secs() as i64;

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();

            // We use a single parameterized query that handles both "with prefix"
            // and "without prefix" cases through nullable parameters.
            // The index on `key` covers the range scan; the `expires_at` index
            // covers the expiry filter on filtered rows.
            let mut stmt = conn
                .prepare(
                    "SELECT key, metadata, expires_at FROM kv
                      WHERE (expires_at IS NULL OR expires_at > ?1)
                        AND (?2 IS NULL OR key >= ?2)
                        AND (?3 IS NULL OR key <  ?3)
                        AND (?4 IS NULL OR key >  ?4)
                      ORDER BY key ASC
                      LIMIT ?5",
                )
                .map_err(|e| KvError::Store(e.to_string()))?;

            let rows = stmt
                .query_map(
                    params![
                        now,
                        prefix_lower.as_deref(),
                        prefix_upper.as_deref(),
                        cursor_key.as_deref(),
                        fetch_limit,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                        ))
                    },
                )
                .map_err(|e| KvError::Store(e.to_string()))?;

            let mut keys: Vec<KvKey> = Vec::with_capacity(limit + 1);
            for row in rows {
                let (name, meta_json, expiration) =
                    row.map_err(|e| KvError::Store(e.to_string()))?;
                let metadata = meta_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(KvError::Serialization)?;
                keys.push(KvKey { name, expiration, metadata });
            }

            let list_complete = keys.len() <= limit;
            if !list_complete {
                keys.pop();
            }

            let cursor = (!list_complete)
                .then(|| keys.last().map(|k| encode_cursor(&k.name)))
                .flatten();

            Ok(ListResult { keys, list_complete, cursor })
        })
        .await
        .map_err(|_| KvError::TaskFailed)?
    }

    async fn purge_expired(&self) -> Result<usize, KvError> {
        let conn = self.conn.clone();
        let now = now_secs() as i64;

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "DELETE FROM kv WHERE expires_at IS NOT NULL AND expires_at <= ?1",
                params![now],
            )
            .map_err(|e| KvError::Store(e.to_string()))
        })
        .await
        .map_err(|_| KvError::TaskFailed)?
    }
}
