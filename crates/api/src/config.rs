use figment::providers::{Env, Format, Toml};
use figment::{Error, Figment};
use serde::Deserialize;
use thiserror::Error as ThisError;

const AUTH_PROVIDERS: &[&str] = &["session", "jwt", "oidc"];
const STORAGE_PROVIDERS: &[&str] = &["supabase", "s3"];
const SUBSCRIPTION_TRANSPORTS: &[&str] = &["sse", "ws"];

/// Fully resolved runtime configuration. Every value comes from `config.toml`
/// (non-secret defaults) layered under `GQL_`-prefixed environment variables
/// (secrets and overrides). No network address, credential, or strategy
/// selection is hardcoded in the binary (review §5).
#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub auth: AuthConfig,
    pub subscriptions: SubscriptionsConfig,
    pub cors: CorsConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub base_url: String,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    /// Injected via `GQL_DATABASE__URL`; intentionally absent from `config.toml`.
    pub url: String,
    pub case: String,
    pub log: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub provider: String,
    pub bucket: String,
    /// Supabase project URL (e.g. `https://xyz.supabase.co`). Not secret, but
    /// injected via `GQL_STORAGE__URL` alongside the key for convenience.
    #[serde(default)]
    pub url: String,
    /// Storage service key. Secret; supplied via `GQL_STORAGE__KEY`.
    #[serde(default)]
    pub key: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    pub provider: String,
    pub session: SessionAuthConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionAuthConfig {
    /// Cookie carrying the Better-Auth session token. Default
    /// `better-auth.session_token` (verified against better-auth 1.6.19).
    pub cookie_name: String,
    /// When true, the HMAC-SHA256 signature is verified before the DB lookup;
    /// when false, only the DB token lookup runs. Both paths are config-selected.
    pub verify_signature: bool,
    /// Shared BETTER_AUTH_SECRET, required only when `verify_signature` is true.
    /// Supplied via `GQL_AUTH__SESSION__SECRET`, never committed.
    #[serde(default)]
    pub secret: String,
}

#[derive(Debug, Deserialize)]
pub struct SubscriptionsConfig {
    pub transport: String,
}

#[derive(Debug, Deserialize)]
pub struct CorsConfig {
    pub origins: Vec<String>,
}

impl AppConfig {
    /// Load and validate configuration. Fails fast when a required secret (such
    /// as the database url) is missing rather than starting in a half-configured
    /// state.
    pub fn load() -> Result<Self, Box<Error>> {
        Figment::new()
            .merge(Toml::file("config.toml"))
            .merge(Env::prefixed("GQL_").split("__"))
            .extract()
            .map_err(Box::new)
    }

    /// Reject a configuration that parsed but is not operable: missing secrets
    /// or a strategy string outside the set the binary can dispatch on. Runs at
    /// boot so the failure names the exact field.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.database.url.trim().is_empty() {
            return Err(ConfigError::MissingSecret("database.url"));
        }
        if self.storage.bucket.trim().is_empty() {
            return Err(ConfigError::MissingSecret("storage.bucket"));
        }
        check_one_of("auth.provider", &self.auth.provider, AUTH_PROVIDERS)?;
        if self.auth.provider == "session"
            && self.auth.session.verify_signature
            && self.auth.session.secret.trim().is_empty()
        {
            return Err(ConfigError::MissingSecret("auth.session.secret"));
        }
        check_one_of(
            "storage.provider",
            &self.storage.provider,
            STORAGE_PROVIDERS,
        )?;
        if self.storage.provider == "supabase"
            && (self.storage.url.trim().is_empty() || self.storage.key.trim().is_empty())
        {
            return Err(ConfigError::MissingSecret("storage.url / storage.key"));
        }
        check_one_of(
            "subscriptions.transport",
            &self.subscriptions.transport,
            SUBSCRIPTION_TRANSPORTS,
        )?;
        Ok(())
    }
}

fn check_one_of(field: &'static str, value: &str, allowed: &[&str]) -> Result<(), ConfigError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(ConfigError::InvalidValue {
            field,
            value: value.to_owned(),
            allowed: allowed.join(", "),
        })
    }
}

/// Configuration validation failures, surfaced at boot instead of as a later
/// runtime error.
#[derive(Debug, ThisError)]
pub enum ConfigError {
    #[error("missing required secret: {0} (provide it via the matching GQL_ env var)")]
    MissingSecret(&'static str),
    #[error("invalid value for {field}: `{value}` (expected one of: {allowed})")]
    InvalidValue {
        field: &'static str,
        value: String,
        allowed: String,
    },
}
