use async_graphql::Object;

/// GraphQL root query. Mirrors `type Query { hello: String! }` from the Node
/// backend's `query.gql`; `hello` returns the same constant as the Node resolver
/// in `resolvers.ts`.
pub struct QueryRoot;

#[Object(name = "Query")]
impl QueryRoot {
    async fn hello(&self) -> &'static str {
        "Hello World!"
    }
}
