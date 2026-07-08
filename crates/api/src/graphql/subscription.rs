use async_graphql::Subscription;
use async_graphql::futures_util::{Stream, stream};

/// GraphQL root subscription. `count` emits the integers `0..to` as a
/// deterministic stream used to exercise both subscription transports (SSE and
/// graphql-ws). Additive to the Node contract, whose `subscription.gql.disabled`
/// keeps `Subscription` inert.
pub struct SubscriptionRoot;

#[Subscription(name = "Subscription")]
impl SubscriptionRoot {
    async fn count(&self, #[graphql(default = 3)] to: i32) -> impl Stream<Item = i32> {
        stream::iter(0..to.max(0))
    }
}
