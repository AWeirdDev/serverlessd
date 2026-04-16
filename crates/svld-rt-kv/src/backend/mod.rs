pub mod memory;
pub mod sqlite;

pub use memory::MemoryKv;
pub use sqlite::SqliteKv;

// ── Shared cursor helpers ─────────────────────────────────────────────────────

use base64::Engine as _;
use crate::KvError;

const ENGINE: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD_NO_PAD;

/// Encode a key name into an opaque pagination cursor.
pub(super) fn encode_cursor(key: &str) -> String {
    ENGINE.encode(key.as_bytes())
}

/// Decode an opaque cursor back into a key name.
pub(super) fn decode_cursor(cursor: &str) -> Result<String, KvError> {
    let bytes = ENGINE.decode(cursor).map_err(|_| KvError::InvalidCursor)?;
    String::from_utf8(bytes).map_err(|_| KvError::InvalidCursor)
}

/// Return the first string that is lexicographically greater than all strings
/// starting with `prefix`, for use in range queries. Returns `None` when
/// `prefix` is empty or all bytes would overflow.
pub(super) fn prefix_upper_bound(prefix: &str) -> Option<String> {
    let mut bytes = prefix.as_bytes().to_vec();
    loop {
        match bytes.last_mut() {
            Some(b) if *b < 0xFF => {
                *b += 1;
                return String::from_utf8(bytes).ok();
            }
            Some(_) => {
                bytes.pop(); // overflow this byte, carry to previous
            }
            None => return None, // empty or all-0xFF — no upper bound
        }
    }
}

// ── Shared expiry helper ──────────────────────────────────────────────────────

use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) const DEFAULT_LIST_LIMIT: u32 = 1000;
