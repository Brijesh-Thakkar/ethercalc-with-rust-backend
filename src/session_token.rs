//! Signed, expiring session tokens.
//!
//! A token is `base64url(payload) "." base64url(HMAC-SHA256(payload))`, where
//! the payload carries the user id, email and an absolute expiry. The server
//! never trusts a cookie value that does not verify against `SECRET_KEY`.

use std::sync::Arc;

use anyhow::{bail, Result};
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "tc_session";
pub const SESSION_TTL_SECS: i64 = 7 * 24 * 60 * 60;
const MIN_KEY_LEN: usize = 32;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionClaims {
    pub user_id: Uuid,
    pub email: String,
}

#[derive(Serialize, Deserialize)]
struct Payload {
    uid: Uuid,
    em: String,
    exp: i64,
}

#[derive(Clone)]
pub struct SessionSigner {
    key: Arc<Vec<u8>>,
    secure_cookies: bool,
}

impl SessionSigner {
    pub fn new(key: &[u8], secure_cookies: bool) -> Result<Self> {
        if key.len() < MIN_KEY_LEN {
            bail!("SECRET_KEY must be at least {MIN_KEY_LEN} bytes");
        }
        Ok(Self {
            key: Arc::new(key.to_vec()),
            secure_cookies,
        })
    }

    /// Builds the signer from configuration. `SECRET_KEY` is mandatory unless
    /// the environment is `development` or `test`, where a random per-process
    /// key is used instead (sessions then do not survive a restart).
    pub fn from_config(environment: &str, secret_key: Option<&str>) -> Result<Self> {
        let relaxed = matches!(environment, "development" | "test");
        match secret_key.filter(|k| !k.is_empty()) {
            Some(key) => Self::new(key.as_bytes(), !relaxed),
            None if relaxed => {
                tracing::warn!("SECRET_KEY is not set; using a random key for this process");
                let key: [u8; 32] = rand::rng().random();
                Self::new(&key, false)
            }
            None => bail!("SECRET_KEY must be set when ENVIRONMENT is '{environment}'"),
        }
    }

    pub fn issue(&self, user_id: Uuid, email: &str, now: i64) -> String {
        let payload = Payload {
            uid: user_id,
            em: email.to_string(),
            exp: now + SESSION_TTL_SECS,
        };
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap_or_default());
        let sig = URL_SAFE_NO_PAD.encode(self.mac(body.as_bytes()).finalize().into_bytes());
        format!("{body}.{sig}")
    }

    pub fn verify(&self, token: &str, now: i64) -> Option<SessionClaims> {
        let (body, sig) = token.split_once('.')?;
        let sig = URL_SAFE_NO_PAD.decode(sig).ok()?;
        self.mac(body.as_bytes()).verify_slice(&sig).ok()?;
        let payload: Payload = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).ok()?).ok()?;
        if payload.exp <= now {
            return None;
        }
        Some(SessionClaims {
            user_id: payload.uid,
            email: payload.em,
        })
    }

    pub fn cookie(&self, user_id: Uuid, email: &str, now: i64) -> Cookie<'static> {
        Cookie::build((SESSION_COOKIE, self.issue(user_id, email, now)))
            .path("/")
            .http_only(true)
            .secure(self.secure_cookies)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::seconds(SESSION_TTL_SECS))
            .build()
    }

    fn mac(&self, data: &[u8]) -> HmacSha256 {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC accepts any key length");
        mac.update(data);
        mac
    }
}

pub fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer(byte: u8) -> SessionSigner {
        SessionSigner::new(&[byte; 32], true).unwrap()
    }

    #[test]
    fn round_trip() {
        let s = signer(1);
        let id = Uuid::new_v4();
        let claims = s.verify(&s.issue(id, "a@b.c", 1000), 1001).unwrap();
        assert_eq!(claims.user_id, id);
        assert_eq!(claims.email, "a@b.c");
    }

    #[test]
    fn rejects_bare_uuid_and_garbage() {
        let s = signer(1);
        for bad in [&Uuid::new_v4().to_string(), "", ".", "a.b", "..."] {
            assert!(s.verify(bad, 0).is_none(), "accepted {bad:?}");
        }
    }

    #[test]
    fn rejects_tampered_payload() {
        let s = signer(1);
        let token = s.issue(Uuid::new_v4(), "a@b.c", 1000);
        let (_, sig) = token.split_once('.').unwrap();
        let forged = Payload {
            uid: Uuid::new_v4(),
            em: "victim@x.y".into(),
            exp: i64::MAX,
        };
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&forged).unwrap());
        assert!(s.verify(&format!("{body}.{sig}"), 1001).is_none());
    }

    #[test]
    fn rejects_token_signed_with_other_key() {
        let token = signer(1).issue(Uuid::new_v4(), "a@b.c", 1000);
        assert!(signer(2).verify(&token, 1001).is_none());
    }

    #[test]
    fn rejects_expired_token() {
        let s = signer(1);
        let token = s.issue(Uuid::new_v4(), "a@b.c", 1000);
        assert!(s.verify(&token, 1000 + SESSION_TTL_SECS - 1).is_some());
        assert!(s.verify(&token, 1000 + SESSION_TTL_SECS).is_none());
    }

    #[test]
    fn production_requires_secret_key() {
        assert!(SessionSigner::from_config("production", None).is_err());
        assert!(SessionSigner::from_config("production", Some("")).is_err());
        assert!(SessionSigner::from_config("production", Some("too-short")).is_err());
        assert!(SessionSigner::from_config("production", Some(&"k".repeat(32))).is_ok());
        assert!(SessionSigner::from_config("development", None).is_ok());
    }

    #[test]
    fn cookie_attributes() {
        let c = signer(1).cookie(Uuid::new_v4(), "a@b.c", unix_now());
        assert_eq!(c.http_only(), Some(true));
        assert_eq!(c.secure(), Some(true));
        assert_eq!(c.same_site(), Some(SameSite::Lax));
        assert!(c.max_age().is_some());
        let dev = SessionSigner::from_config("development", None).unwrap();
        assert_eq!(dev.cookie(Uuid::new_v4(), "a@b.c", 0).secure(), Some(false));
    }
}
