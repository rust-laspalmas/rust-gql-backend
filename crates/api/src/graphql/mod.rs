mod model;
mod mutation;
mod query;
mod scalar;
mod subscription;

pub use model::User;
pub use mutation::MutationRoot;
pub use query::QueryRoot;
pub use scalar::DateTimeIso;
pub use subscription::SubscriptionRoot;

use async_graphql::Schema;
use rust_gql_domain::Role;

/// Executable schema type for the backend: query, mutation and subscription
/// roots.
pub type ApiSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

/// Build the executable schema. This is the single definition consumed by the
/// HTTP transport (later task) and by `xtask emit-schema` to produce the SDL
/// contract.
///
/// `User` and `Role` are registered explicitly: the Node contract declares them
/// in `user.gql`/`role.gql` even though no query returns them yet, and a
/// code-first schema only includes reachable types. Registering keeps the
/// emitted SDL aligned with Node.
pub fn build_schema() -> ApiSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .register_output_type::<User>()
        .register_output_type::<Role>()
        .finish()
}

#[cfg(test)]
mod tests {
    use super::build_schema;

    #[tokio::test]
    async fn hello_matches_node_resolver() {
        let response = build_schema().execute("{ hello }").await;
        assert!(response.errors.is_empty(), "errors: {:?}", response.errors);
        assert!(format!("{:?}", response.data).contains("Hello World!"));
    }

    #[tokio::test]
    async fn subscription_count_streams_values() {
        use async_graphql::futures_util::StreamExt;
        let schema = build_schema();
        let mut stream = schema.execute_stream("subscription { count(to: 3) }");
        let mut emitted = Vec::new();
        while let Some(response) = stream.next().await {
            emitted.push(format!("{:?}", response.data));
        }
        assert_eq!(emitted.len(), 3);
        assert!(emitted[0].contains('0'));
        assert!(emitted[2].contains('2'));
    }

    #[test]
    fn sdl_mirrors_node_contract() {
        let sdl = build_schema().sdl();
        for needle in [
            "type Query {",
            "hello: String!",
            "type User",
            "id: String!",
            "email: String!",
            "emailVerified: Boolean!",
            "image: String",
            "createdAt: DateTimeISO!",
            "scalar DateTimeISO",
            "enum Role",
            "ADMIN",
            "USER",
        ] {
            assert!(
                sdl.contains(needle),
                "emitted SDL is missing `{needle}`:\n{sdl}"
            );
        }
    }
}
