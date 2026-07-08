use crate::storage::SharedStorage;
use async_graphql::{Context, Error, Object, Result, Upload};
use std::io::Read;

/// GraphQL root mutation. `uploadMedia` receives a file through the GraphQL
/// multipart request spec (jaydenseric — the format apollo-upload-client sends)
/// and stores it via the configured [`SharedStorage`], returning the object URL.
///
/// This surface is additive to the Node contract, whose `mutation.gql.disabled`
/// keeps `Mutation`/`Upload` inert; the existing SPA is unaffected.
pub struct MutationRoot;

#[Object(name = "Mutation")]
impl MutationRoot {
    async fn upload_media(&self, ctx: &Context<'_>, file: Upload) -> Result<String> {
        let storage = ctx.data::<SharedStorage>()?;
        let value = file.value(ctx)?;
        let content_type = value
            .content_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_owned());
        let key = format!("uploads/{}", value.filename);

        let mut bytes = Vec::new();
        value
            .into_read()
            .read_to_end(&mut bytes)
            .map_err(|error| Error::new(error.to_string()))?;

        storage
            .put(&key, bytes, &content_type)
            .await
            .map_err(|error| Error::new(error.to_string()))
    }
}
