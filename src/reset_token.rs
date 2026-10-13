//! Single-use, expiring password-reset tokens.
//!
//! The token itself is only ever sent to the user. What gets stored (in the
//! existing `users.dongle` column) is `sha256_hex(token) ":" expiry_unix`, so a
//! database read does not reveal a usable token.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};

pub const RESET_TOKEN_TTL_SECS: i64 = 60 * 60;

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Returns `(token_to_send, value_to_store)`.
pub fn generate(now: i64) -> (String, String) {
    let bytes: [u8; 32] = rand::rng().random();
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let stored = format!("{}:{}", hash(&token), now + RESET_TOKEN_TTL_SECS);
    (token, stored)
}

/// True if `token` matches the stored value and has not expired.
pub fn matches(stored: &str, token: &str, now: i64) -> bool {
    let Some((stored_hash, exp)) = stored.split_once(':') else {
        return false;
    };
    let Ok(exp) = exp.parse::<i64>() else {
        return false;
    };
    if exp <= now || token.is_empty() {
        return false;
    }
    constant_time_eq(stored_hash.as_bytes(), hash(token).as_bytes())
}

fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_token_matches_until_expiry() {
        let (token, stored) = generate(1000);
        assert!(matches(&stored, &token, 1001));
        assert!(matches(&stored, &token, 1000 + RESET_TOKEN_TTL_SECS - 1));
        assert!(!matches(&stored, &token, 1000 + RESET_TOKEN_TTL_SECS));
    }

    #[test]
    fn wrong_empty_and_malformed_tokens_do_not_match() {
        let (token, stored) = generate(1000);
        assert!(!matches(&stored, "", 1001));
        assert!(!matches(&stored, "not-the-token", 1001));
        assert!(!matches(&stored, &format!("{token}x"), 1001));
        assert!(!matches("", &token, 1001));
        assert!(!matches("garbage", &token, 1001));
        assert!(!matches(&format!("{}:abc", hash(&token)), &token, 1001));
    }

    #[test]
    fn tokens_are_unique_and_not_stored_in_clear() {
        let (t1, s1) = generate(0);
        let (t2, _) = generate(0);
        assert_ne!(t1, t2);
        assert!(!s1.contains(&t1));
        assert!(t1.len() >= 43);
    }
}
