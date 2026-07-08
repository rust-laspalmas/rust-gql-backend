//! GofiGeeks GQL backend library: runtime configuration and the code-first
//! GraphQL schema. Both the `gql-api` binary and the workspace `xtask` build on
//! this crate — the schema is defined once here and emitted from `xtask` as the
//! SDL contract.

pub mod auth;
pub mod config;
pub mod db;
pub mod graphql;
pub mod http;
pub mod loader;
pub mod storage;

pub use graphql::{ApiSchema, build_schema};
