//! Object storage behind a `trait Storage`, so the upload mutation is agnostic
//! to the backend. `supabase` (reqwest) is implemented; `s3` is a reserved
//! feature gate. The backend is chosen by `[storage].provider`.

#[cfg(feature = "storage-supabase")]
mod supabase;

use crate::config::StorageConfig;
use std::sync::Arc;
use thiserror::Error;

/// Shared, object-safe storage handle injected into the GraphQL context.
pub type SharedStorage = Arc<dyn Storage>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage transport error: {0}")]
    Transport(String),
    #[error("storage rejected upload ({status}): {message}")]
    Rejected { status: u16, message: String },
}

/// Store a byte payload and return a locator (URL or key) for it.
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    async fn put(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<String, StorageError>;
}

/// Construct the configured storage backend. Returns `None` when the selected
/// provider is not compiled in (its feature is off).
pub fn build_storage(config: &StorageConfig) -> Option<SharedStorage> {
    match config.provider.as_str() {
        #[cfg(feature = "storage-supabase")]
        "supabase" => Some(Arc::new(supabase::SupabaseStorage::new(config))),
        _ => None,
    }
}
