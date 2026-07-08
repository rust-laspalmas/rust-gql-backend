use super::{Storage, StorageError};
use crate::config::StorageConfig;
use async_trait::async_trait;

/// Supabase Storage backend over its REST API. Mirrors the bucket the Node
/// backend uploads to via `@supabase/supabase-js`.
pub struct SupabaseStorage {
    client: reqwest::Client,
    base_url: String,
    bucket: String,
    key: String,
}

impl SupabaseStorage {
    pub fn new(config: &StorageConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: config.url.trim_end_matches('/').to_owned(),
            bucket: config.bucket.clone(),
            key: config.key.clone(),
        }
    }
}

#[async_trait]
impl Storage for SupabaseStorage {
    async fn put(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<String, StorageError> {
        let endpoint = format!(
            "{}/storage/v1/object/{}/{}",
            self.base_url, self.bucket, key
        );
        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(&self.key)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(bytes)
            .send()
            .await
            .map_err(|error| StorageError::Transport(error.to_string()))?;

        if response.status().is_success() {
            Ok(format!(
                "{}/storage/v1/object/public/{}/{}",
                self.base_url, self.bucket, key
            ))
        } else {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            Err(StorageError::Rejected { status, message })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SupabaseStorage;
    use crate::config::StorageConfig;
    use crate::storage::Storage;

    /// End-to-end against a real Supabase bucket. Skipped by default (no
    /// credentials guaranteed); run with `GQL_STORAGE__URL`/`GQL_STORAGE__KEY` set
    /// and `cargo test -p api -- --ignored`.
    #[tokio::test]
    #[ignore = "requires a live Supabase project; set GQL_STORAGE__URL and GQL_STORAGE__KEY"]
    async fn put_uploads_to_real_supabase() {
        let config = StorageConfig {
            provider: "supabase".to_owned(),
            bucket: std::env::var("GQL_STORAGE__BUCKET").unwrap_or_else(|_| "media".to_owned()),
            url: std::env::var("GQL_STORAGE__URL").expect("GQL_STORAGE__URL"),
            key: std::env::var("GQL_STORAGE__KEY").expect("GQL_STORAGE__KEY"),
        };
        let storage = SupabaseStorage::new(&config);
        let url = storage
            .put("uploads/probe.txt", b"probe".to_vec(), "text/plain")
            .await
            .expect("upload");
        assert!(url.contains("probe.txt"), "unexpected url: {url}");
    }
}
