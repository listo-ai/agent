//! Links search scope — materialised link rows with resolved endpoint
//! paths, queryable via RSQL.

mod dto;
mod schema;
mod scope;

pub use dto::{EndpointDto, LinkDto};
pub use schema::link_query_schema;
pub use scope::{LinksQuery, LinksScope};
