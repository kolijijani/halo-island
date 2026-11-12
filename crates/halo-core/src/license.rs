//! Halo Pro: one permanent key, checked on this PC alone. The key typed in
//! Settings › Halo Pro is hashed (SHA-256) and compared with [`KEY_SHA256`].
//! No server, no machine fingerprint, no device limit, no monitor id.
//!
//! To change the key: put the new key's `sha256sum` here
//! (`printf '%s' 'NEW-KEY' | sha256sum`). Copies already activated with the
//! old key go back to Free until the new one is typed.

/// SHA-256 of the Halo Pro key, lowercase hex.
pub const KEY_SHA256: &str = "ca7c4d170fc8ce338cfc526a260d6ed7997e7ffc6aac1b2688ee47661f2c96e5";

/// The key as people type it: spaces dropped, letters upper-cased, so
/// "halo-2026-koliji-9f4c-77b1 " still counts.
pub fn normalize(key: &str) -> String {
    key.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase()
}

/// Whether `key` unlocks Pro.
pub fn valid(key: &str) -> bool {
    let k = normalize(key);
    !k.is_empty() && crate::sha256::hex(&crate::sha256::sha256(k.as_bytes())) == KEY_SHA256
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_unlocks_and_nothing_else_does() {
        assert!(valid("HALO-2026-KOLIJI-9F4C-77B1"));
        assert!(valid(" halo-2026-koliji-9f4c-77b1\n"));
        assert!(!valid(""));
        assert!(!valid("HALO-2026-KOLIJI-9F4C-77B2"));
        assert!(!valid(KEY_SHA256), "the hash itself is no key");
    }
}
