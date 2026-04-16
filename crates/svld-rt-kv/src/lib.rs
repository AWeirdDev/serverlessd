//! # svld-rt-kv
//!
//! Serverlessd runtime KV namespace — the storage backend for the Workers KV
//! binding (`env.KV`), intentionally free of V8 / runtime dependencies.
//!
//! ## Architecture
//!
//! ```text
//!   ┌─────────────────────────────────────────────┐
//!   │              serverlessd (main crate)         │
//!   │                                               │
//!   │  src/bindings/kv.rs  ←──── builds V8 object  │
//!   │       │                    with get/put/…     │
//!   │       │  Arc<dyn KvStore>                     │
//!   └───────┼───────────────────────────────────────┘
//!           │
//!   ┌───────▼──────────────────────────────────────┐
//!   │                svld-rt-kv (this crate)        │
//!   │                                               │
//!   │  KvStore trait                                │
//!   │    ├── MemoryKv   (BTreeMap + RwLock)         │
//!   │    └── SqliteKv   (rusqlite + spawn_blocking) │
//!   └──────────────────────────────────────────────-┘
//! ```
//!
//! ## Cloudflare KV API surface
//!
//! | JS method | Rust method |
//! |-----------|-------------|
//! | `kv.get(key)` | [`KvStore::get`] |
//! | `kv.getWithMetadata(key)` | [`KvStore::get_with_metadata`] |
//! | `kv.put(key, value, opts)` | [`KvStore::put`] |
//! | `kv.delete(key)` | [`KvStore::delete`] |
//! | `kv.list(opts)` | [`KvStore::list`] |
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use svld_rt_kv::{SqliteKv, KvStore, PutOptions, ListOptions};
//!
//! # #[tokio::main] async fn main() -> Result<(), svld_rt_kv::KvError> {
//! let kv = SqliteKv::open(".serverlessd/kv/my-namespace.db")?;
//!
//! kv.put("hello", b"world".to_vec(), PutOptions {
//!     expiration_ttl: Some(3600), // expires in 1 hour
//!     ..Default::default()
//! }).await?;
//!
//! if let Some(entry) = kv.get("hello").await? {
//!     println!("{}", String::from_utf8_lossy(&entry.value)); // "world"
//! }
//!
//! let page = kv.list(ListOptions { prefix: Some("hel".into()), ..Default::default() }).await?;
//! println!("{} key(s), complete={}", page.keys.len(), page.list_complete);
//! # Ok(()) }
//! ```

mod error;
mod store;
mod types;

pub mod backend;

pub use error::KvError;
pub use store::KvStore;
pub use types::{KvEntry, KvEntryWithMetadata, KvKey, ListOptions, ListResult, PutOptions};

// Convenience re-exports — users can write `svld_rt_kv::MemoryKv` directly.
pub use backend::{MemoryKv, SqliteKv};
