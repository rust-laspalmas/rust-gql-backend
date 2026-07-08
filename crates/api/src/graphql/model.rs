use super::scalar::DateTimeIso;
use async_graphql::SimpleObject;
use rust_gql_domain::User as DomainUser;

/// GraphQL projection of the domain [`DomainUser`]. Field names and types mirror
/// `type User` in the Node backend's `user.gql` exactly: async-graphql renames
/// snake_case fields to camelCase (`emailVerified`, `createdAt`), and the domain
/// newtypes are unwrapped to `String` on the wire. The validation lives in the
/// domain; the wire type stays a plain `String`, so the contract is identical to
/// Node's.
#[derive(SimpleObject)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub email_verified: bool,
    pub image: Option<String>,
    pub biography: Option<String>,
    pub created_at: DateTimeIso,
}

impl From<DomainUser> for User {
    fn from(user: DomainUser) -> Self {
        Self {
            id: user.id.as_str().to_owned(),
            email: user.email.as_str().to_owned(),
            name: user.name.as_str().to_owned(),
            email_verified: user.email_verified,
            image: user.image,
            biography: user.biography,
            created_at: user.created_at.into(),
        }
    }
}
