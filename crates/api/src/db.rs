use chrono::NaiveDateTime;
use rust_gql_domain::{Email, User, UserId, UserName};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use thiserror::Error;

/// Data-layer error. Wraps sqlx failures and rejects rows whose stored values do
/// not satisfy the domain invariants (e.g. a malformed email in legacy data).
#[derive(Debug, Error)]
pub enum DataError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid stored user row: {0}")]
    Projection(String),
}

/// A row of the Better-Auth `users` table, projected to the columns the wire
/// contract (`type User`) needs — the table also has `role`, `banned`,
/// `updated_at`, etc. that are not part of the contract and are deliberately not
/// selected. `created_at` is `timestamp` (no zone) in the schema, hence
/// `NaiveDateTime`.
#[derive(Debug, sqlx::FromRow)]
struct UserRow {
    id: String,
    email: String,
    name: String,
    email_verified: bool,
    image: Option<String>,
    biography: Option<String>,
    created_at: NaiveDateTime,
}

/// Turn any domain validation failure into a `Projection` error without naming
/// the validator's error type, so this crate needs no direct `garde` dependency.
fn projection_error(error: impl std::fmt::Display) -> DataError {
    DataError::Projection(error.to_string())
}

impl TryFrom<UserRow> for User {
    type Error = DataError;

    fn try_from(row: UserRow) -> Result<Self, Self::Error> {
        Ok(User {
            id: UserId::parse(row.id).map_err(projection_error)?,
            email: Email::parse(row.email).map_err(projection_error)?,
            name: UserName::parse(row.name).map_err(projection_error)?,
            email_verified: row.email_verified,
            image: row.image,
            biography: row.biography,
            created_at: row.created_at.and_utc(),
        })
    }
}

/// Open a connection pool against the existing Postgres database (the same one
/// Drizzle migrates in the Node backend), establishing the first connection
/// eagerly.
pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new().connect(url).await
}

/// Build a pool that connects on first use. Startup is instant and never blocks
/// on the database, so a request that touches no table (e.g. an anonymous
/// `{ hello }`) is served even when Postgres is down; the connection is
/// attempted only when a query actually runs. Only the url syntax is validated.
pub fn connect_lazy(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new().connect_lazy(url)
}

/// Reads users from the shared Postgres database.
#[derive(Clone)]
pub struct UserRepository {
    pool: PgPool,
}

// sqlx 0.9 requires query strings to be `&'static str` (the `SqlSafeStr` bound),
// so these are const literals rather than a formatted string. Values are always
// bound as parameters, never interpolated.
const FIND_USERS_BY_IDS: &str = "SELECT id, email, name, email_verified, image, biography, created_at \
     FROM users WHERE id = ANY($1)";
const LIST_USERS: &str = "SELECT id, email, name, email_verified, image, biography, created_at \
     FROM users ORDER BY created_at";

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Batch-load users by id in a **single** query (`id = ANY($1)`). This is the
    /// primitive the DataLoader uses to collapse N per-key lookups into one round
    /// trip, avoiding the N+1 problem.
    pub async fn find_by_ids(&self, ids: &[UserId]) -> Result<Vec<User>, DataError> {
        let raw: Vec<String> = ids.iter().map(|id| id.as_str().to_owned()).collect();
        let rows = sqlx::query_as::<_, UserRow>(FIND_USERS_BY_IDS)
            .bind(&raw)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(User::try_from).collect()
    }

    /// List all users. Ordered by creation time for stable output.
    pub async fn list(&self) -> Result<Vec<User>, DataError> {
        let rows = sqlx::query_as::<_, UserRow>(LIST_USERS)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(User::try_from).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::UserRow;
    use chrono::NaiveDate;
    use rust_gql_domain::User;

    fn row(email: &str) -> UserRow {
        UserRow {
            id: "usr_1".to_owned(),
            email: email.to_owned(),
            name: "Ada".to_owned(),
            email_verified: true,
            image: None,
            biography: None,
            created_at: NaiveDate::from_ymd_opt(2026, 7, 8)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .expect("valid timestamp"),
        }
    }

    #[test]
    fn projects_valid_row_to_domain_user() {
        let user = User::try_from(row("ada@example.com")).expect("projection");
        assert_eq!(user.id.as_str(), "usr_1");
        assert_eq!(user.email.as_str(), "ada@example.com");
        assert!(user.email_verified);
    }

    #[test]
    fn rejects_row_with_invalid_email() {
        assert!(User::try_from(row("not-an-email")).is_err());
    }

    /// End-to-end against a real Postgres carrying the Better-Auth schema. Skipped
    /// by default (review §7.5: no DB is guaranteed); run with
    /// `GQL_DATABASE__URL=... cargo test -p api -- --ignored`.
    #[tokio::test]
    #[ignore = "requires a live Postgres with the Better-Auth schema; set GQL_DATABASE__URL"]
    async fn find_by_ids_batches_against_real_db() {
        let url = std::env::var("GQL_DATABASE__URL").expect("GQL_DATABASE__URL");
        let pool = super::connect(&url).await.expect("connect");
        let repository = super::UserRepository::new(pool);

        let all = repository.list().await.expect("list users");
        let ids: Vec<_> = all.iter().map(|user| user.id.clone()).collect();
        let batched = repository.find_by_ids(&ids).await.expect("batch load");

        assert_eq!(batched.len(), all.len());
    }
}
