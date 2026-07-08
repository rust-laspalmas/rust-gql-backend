use crate::db::{DataError, UserRepository};
use async_graphql::dataloader::Loader;
use rust_gql_domain::{User, UserId};
use std::collections::HashMap;
use std::sync::Arc;

/// async-graphql [`Loader`] that batches user lookups. When several resolvers ask
/// for users within one request, async-graphql coalesces the keys and calls
/// [`Loader::load`] once, which issues a single `id = ANY($1)` query — the N+1
/// fix. It is not yet wired to a GraphQL field (the contract has no user-bearing
/// query), but it is the reusable batching unit for when one is added.
pub struct UserLoader {
    repository: UserRepository,
}

impl UserLoader {
    pub fn new(repository: UserRepository) -> Self {
        Self { repository }
    }
}

impl Loader<UserId> for UserLoader {
    type Value = User;
    type Error = Arc<DataError>;

    async fn load(&self, keys: &[UserId]) -> Result<HashMap<UserId, Self::Value>, Self::Error> {
        let users = self.repository.find_by_ids(keys).await.map_err(Arc::new)?;
        Ok(index_by_id(users))
    }
}

/// Index loaded users by their id so async-graphql can match each requested key
/// to its value (missing keys are simply absent from the map).
fn index_by_id(users: Vec<User>) -> HashMap<UserId, User> {
    users
        .into_iter()
        .map(|user| (user.id.clone(), user))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::index_by_id;
    use chrono::DateTime;
    use rust_gql_domain::{Email, User, UserId, UserName};

    fn user(id: &str) -> User {
        User {
            id: UserId::parse(id).expect("valid id"),
            email: Email::parse("ada@example.com").expect("valid email"),
            name: UserName::parse("Ada").expect("valid name"),
            email_verified: true,
            image: None,
            biography: None,
            created_at: DateTime::from_timestamp(0, 0).expect("epoch"),
        }
    }

    #[test]
    fn indexes_users_by_id() {
        let map = index_by_id(vec![user("a"), user("b")]);
        assert_eq!(map.len(), 2);
        let key_a = UserId::parse("a").expect("valid id");
        let key_b = UserId::parse("b").expect("valid id");
        assert_eq!(map[&key_a].id.as_str(), "a");
        assert!(map.contains_key(&key_b));
    }
}
