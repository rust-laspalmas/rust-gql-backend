use super::{AuthError, AuthProvider, Principal};
use crate::config::SessionAuthConfig;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::percent_decode_str;
use rust_gql_domain::UserId;
use sha2::Sha256;
use sqlx::PgPool;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Reads Better-Auth `sessions` rows from the shared Postgres.
#[derive(Clone)]
pub struct SessionStore {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    user_id: String,
    expires_at: chrono::NaiveDateTime,
}

const SELECT_SESSION: &str = "SELECT user_id, expires_at FROM sessions WHERE token = $1";

impl SessionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn find_by_token(&self, token: &str) -> Result<Option<SessionRow>, sqlx::Error> {
        sqlx::query_as::<_, SessionRow>(SELECT_SESSION)
            .bind(token)
            .fetch_optional(&self.pool)
            .await
    }
}

/// Validates the Better-Auth `better-auth.session_token` cookie. The cookie value
/// is `${token}.${base64(HMAC-SHA256(token, secret))}` (verified against
/// better-auth 1.6.19). When `verify_signature` is set the HMAC is checked in
/// constant time before the DB lookup; otherwise only the token lookup runs.
pub struct SessionAuthProvider {
    config: SessionAuthConfig,
    store: SessionStore,
}

impl SessionAuthProvider {
    pub fn new(config: SessionAuthConfig, store: SessionStore) -> Self {
        Self { config, store }
    }
}

impl AuthProvider for SessionAuthProvider {
    async fn authenticate(
        &self,
        headers: &axum::http::HeaderMap,
    ) -> Result<Option<Principal>, AuthError> {
        let Some(raw) = extract_cookie(headers, &self.config.cookie_name) else {
            return Ok(None);
        };

        let value = percent_decode_str(&raw)
            .decode_utf8()
            .map_err(|_| AuthError::MalformedCookie)?;
        let (token, signature) = value.split_once('.').ok_or(AuthError::MalformedCookie)?;

        if self.config.verify_signature {
            verify_signature(token, signature, &self.config.secret)?;
        }

        let row = self
            .store
            .find_by_token(token)
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))?
            .ok_or(AuthError::InvalidSession)?;

        if row.expires_at.and_utc() <= Utc::now() {
            return Err(AuthError::InvalidSession);
        }

        let user_id = UserId::parse(row.user_id).map_err(|_| AuthError::InvalidSession)?;
        Ok(Some(Principal { user_id }))
    }
}

/// Read a single cookie value from the `Cookie` header.
fn extract_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let cookies = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        (key == name).then(|| value.to_owned())
    })
}

/// Verify `signature` equals `base64_std(HMAC-SHA256(token, secret))` in constant
/// time.
fn verify_signature(token: &str, signature: &str, secret: &str) -> Result<(), AuthError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| AuthError::Backend("invalid hmac key length".to_owned()))?;
    mac.update(token.as_bytes());
    let expected = STANDARD.encode(mac.finalize().into_bytes());

    if expected.as_bytes().ct_eq(signature.as_bytes()).into() {
        Ok(())
    } else {
        Err(AuthError::InvalidSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_cookie, verify_signature};
    use axum::http::{HeaderMap, header};

    // Real vector captured from better-auth 1.6.19 (see review §3.1.2): this
    // cross-language test proves the Rust HMAC reproduces Better-Auth's signature.
    const TOKEN: &str = "Nsho3U4xusoMwYHov6yjQqDdMtlgsRof";
    const SECRET: &str = "probe-secret-0123456789abcdef-32byteslong!!";
    const SIGNATURE: &str = "EgVgsEKhIk7PnyBhvlz9Hayadr83gMMuceM1RQ39r7M=";

    #[test]
    fn reproduces_better_auth_signature() {
        assert!(verify_signature(TOKEN, SIGNATURE, SECRET).is_ok());
    }

    #[test]
    fn rejects_tampered_signature() {
        assert!(verify_signature(TOKEN, "AAAAdifferentsignatureAAAA=", SECRET).is_err());
        assert!(verify_signature("othertoken", SIGNATURE, SECRET).is_err());
    }

    #[test]
    fn extracts_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "other=1; better-auth.session_token=abc.def; last=2"
                .parse()
                .expect("header"),
        );
        assert_eq!(
            extract_cookie(&headers, "better-auth.session_token").as_deref(),
            Some("abc.def")
        );
        assert_eq!(extract_cookie(&headers, "missing"), None);
    }

    #[test]
    fn no_cookie_header_is_none() {
        assert_eq!(extract_cookie(&HeaderMap::new(), "x"), None);
    }
}
