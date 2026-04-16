use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::{
    KvEntry, KvEntryWithMetadata, KvError, KvKey, ListOptions, ListResult, PutOptions,
    backend::{DEFAULT_LIST_LIMIT, decode_cursor, encode_cursor, now_secs, prefix_upper_bound},
    store::KvStore,
};

// ── Internal entry ────────────────────────────────────────────────────────────

struct MemoryEntry {
    value: Vec<u8>,
    metadata: Option<serde_json::Value>,
    /// Absolute Unix timestamp (seconds). `None` = no expiry.
    expires_at: Option<u64>,
}

impl MemoryEntry {
    fn is_alive(&self) -> bool {
        match self.expires_at {
            Some(ts) => now_secs() <= ts,
            None => true,
        }
    }
}

// ── MemoryKv ──────────────────────────────────────────────────────────────────

/// An in-memory KV store backed by a sorted [`BTreeMap`].
///
/// Using `BTreeMap` keeps keys in lexicographic order, which makes
/// cursor-based `list()` pagination O(log n) and allocation-free per page.
///
/// Data is **not** persisted across process restarts. Clone-sharing is cheap —
/// all clones reference the same underlying data through an `Arc<RwLock<…>>`.
#[derive(Clone, Default)]
pub struct MemoryKv {
    inner: Arc<RwLock<BTreeMap<String, MemoryEntry>>>,
}

impl MemoryKv {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl KvStore for MemoryKv {
    async fn get(&self, key: &str) -> Result<Option<KvEntry>, KvError> {
        let store = self.inner.read().await;
        Ok(store.get(key).and_then(|e| {
            e.is_alive().then(|| KvEntry {
                value: e.value.clone(),
                expiration: e.expires_at,
            })
        }))
    }

    async fn get_with_metadata(&self, key: &str) -> Result<Option<KvEntryWithMetadata>, KvError> {
        let store = self.inner.read().await;
        Ok(store.get(key).and_then(|e| {
            e.is_alive().then(|| KvEntryWithMetadata {
                value: e.value.clone(),
                metadata: e.metadata.clone(),
                expiration: e.expires_at,
            })
        }))
    }

    async fn put(&self, key: &str, value: Vec<u8>, options: PutOptions) -> Result<(), KvError> {
        let expires_at = options
            .expiration
            .or_else(|| options.expiration_ttl.map(|ttl| now_secs() + ttl));

        let mut store = self.inner.write().await;
        store.insert(
            key.to_owned(),
            MemoryEntry {
                value,
                metadata: options.metadata,
                expires_at,
            },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        self.inner.write().await.remove(key);
        Ok(())
    }

    async fn list(&self, options: ListOptions) -> Result<ListResult, KvError> {
        let limit = options.limit.unwrap_or(DEFAULT_LIST_LIMIT) as usize;

        let cursor_key = options
            .cursor
            .as_deref()
            .map(decode_cursor)
            .transpose()?;

        let upper_bound = options.prefix.as_deref().and_then(prefix_upper_bound);

        let store = self.inner.read().await;
        let now = now_secs();

        // BTreeMap::range gives us O(log n) seek directly to the start of the
        // prefix range, then we iterate forward — no full-table scan.
        // Using `&str` bounds avoids the Borrow<str> vs Borrow<String> ambiguity.
        let start: std::ops::Bound<&str> = match (&cursor_key, &options.prefix) {
            (Some(ck), _) => std::ops::Bound::Excluded(ck.as_str()),
            (None, Some(pfx)) => std::ops::Bound::Included(pfx.as_str()),
            (None, None) => std::ops::Bound::Unbounded,
        };
        let end: std::ops::Bound<&str> = match &upper_bound {
            Some(ub) => std::ops::Bound::Excluded(ub.as_str()),
            None => std::ops::Bound::Unbounded,
        };

        // Fetch limit+1 to detect whether there is a next page.
        let mut keys: Vec<KvKey> = Vec::with_capacity(limit + 1);
        for (k, e) in store.range::<str, _>((start, end)) {
            // When a cursor was set, the range already excludes keys <= cursor.
            // When only a prefix was set, double-check (in case cursor_key
            // happened to sort before the prefix).
            if let Some(pfx) = options.prefix.as_deref() {
                if !k.starts_with(pfx) {
                    continue;
                }
            }

            if e.expires_at.is_none() || e.expires_at.is_some_and(|ts| ts > now) {
                keys.push(KvKey {
                    name: k.clone(),
                    expiration: e.expires_at,
                    metadata: e.metadata.clone(),
                });
            }

            if keys.len() == limit + 1 {
                break; // we have enough to know there is a next page
            }
        }

        let list_complete = keys.len() <= limit;
        if !list_complete {
            keys.pop(); // remove the sentinel
        }

        let cursor = (!list_complete).then(|| keys.last().map(|k| encode_cursor(&k.name))).flatten();

        Ok(ListResult { keys, list_complete, cursor })
    }

    async fn purge_expired(&self) -> Result<usize, KvError> {
        let now = now_secs();
        let mut store = self.inner.write().await;
        let before = store.len();
        store.retain(|_, e| e.expires_at.is_none_or(|ts| ts > now));
        Ok(before - store.len())
    }
}
