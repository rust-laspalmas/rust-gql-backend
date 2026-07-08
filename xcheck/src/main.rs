use graphql_client::{GraphQLQuery, Response};

/// The Leptos client's operation, generated from the SAME `schema.graphql` and
/// the SAME query file the wasm frontend uses. `build_query` therefore produces a
/// request byte-identical to the one the browser sends — only the transport
/// (reqwest here vs gloo-net there) differs.
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "../schema.graphql",
    query_path = "../../rust-gql-frontend/src/queries/hello.graphql",
    response_derives = "Debug"
)]
struct HelloQuery;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://localhost:4000/graphql".to_owned());

    let request_body = HelloQuery::build_query(hello_query::Variables {});
    let response: Response<hello_query::ResponseData> = reqwest::Client::new()
        .post(&endpoint)
        .json(&request_body)
        .send()
        .await?
        .json()
        .await?;

    let data = response
        .data
        .ok_or_else(|| anyhow::anyhow!("no data in GraphQL response"))?;

    // Emit the response `data` in a form directly comparable to the Node client's.
    println!("{{\"hello\":{:?}}}", data.hello);
    Ok(())
}
