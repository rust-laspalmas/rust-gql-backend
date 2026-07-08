//! Pluggable authentication. `session` (Better-Auth cookie) is implemented;
//! `jwt`/`oidc` are reserved feature gates for later tasks. Which strategy runs
//! is chosen by `[auth].provider`; the validation depth of the session strategy
//! by `[auth.session].verify_signature` — both config-driven.

#[cfg(not(feature = "auth-session"))]
compile_error!(
    "at least one auth strategy feature must be enabled (currently: `auth-session`); \
	 a backend with no way to authenticate is not a valid configuration"
);

#[cfg(feature = "auth-session")]
mod session;

#[cfg(feature = "auth-session")]
pub use session::{SessionAuthProvider, SessionStore};

use rust_gql_domain::UserId;
use thiserror::Error;

/// The authenticated identity attached to a request. Absence of a `Principal`
/// (represented as `Option<Principal>` in the request context) means anonymous.
#[derive(Debug, Clone)]
pub struct Principal {
    pub user_id: UserId,
}

/// Authentication failure. `authenticate` returns `Ok(None)` for an anonymous
/// request; these variants are for credentials that are present but rejected, or
/// a backend that failed.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("malformed session cookie")]
    MalformedCookie,
    #[error("invalid session signature")]
    InvalidSignature,
    #[error("session expired or unknown")]
    InvalidSession,
    #[error("auth backend unavailable: {0}")]
    Backend(String),
}

/// A strategy that turns request headers into an optional [`Principal`].
pub trait AuthProvider: Send + Sync {
    fn authenticate(
        &self,
        headers: &axum::http::HeaderMap,
    ) -> impl std::future::Future<Output = Result<Option<Principal>, AuthError>> + Send;
}

/// Runtime dispatch over the configured strategy. Selected from `[auth].provider`
/// at boot; new strategies become new variants without touching call sites.
pub enum Authenticator {
    #[cfg(feature = "auth-session")]
    Session(SessionAuthProvider),
}

impl Authenticator {
    pub async fn authenticate(
        &self,
        headers: &axum::http::HeaderMap,
    ) -> Result<Option<Principal>, AuthError> {
        match self {
            #[cfg(feature = "auth-session")]
            Authenticator::Session(provider) => provider.authenticate(headers).await,
        }
    }
}
