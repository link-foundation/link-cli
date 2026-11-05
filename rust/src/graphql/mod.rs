//! A Hasura-compatible GraphQL server for the Deep standard (issue #153), so
//! that `DeepClient` and deepcase connect to `clink --serve --graphql`.
//!
//! Each part is a layer over the one below, which it reaches only through
//! [`Triplets`](crate::triplets::Triplets) and
//! [`NamedTypeLinks`](crate::named_type_links::NamedTypeLinks):
//!
//! - [`store`] maps the rows of Deep to triplets, under the permission layer;
//! - [`plan`] translates `where` to substitution queries;
//! - [`select`], [`mutate`] and [`virtual_tables`] execute a document;
//! - [`server`] speaks HTTP and both WebSocket subprotocols.
//!
//! See `docs/GRAPHQL.md` for the mapping and for what is left out.

pub mod crypto;
pub mod document;
pub mod execute;
pub mod json;
pub mod mutate;
pub mod plan;
pub mod select;
pub mod server;
pub mod store;
pub mod tables;
pub mod virtual_tables;

pub use execute::{execute, Auth, Settings};
pub use json::Json;
pub use server::GraphqlServer;
pub use store::DeepStore;
