//! Data ingestion and caching

pub mod ingest;
pub mod canonicalize;
pub mod cache;
pub mod schema;
pub mod universe;

pub use ingest::{DataIngestor, DataError};
pub use canonicalize::Canonicalizer;
pub use cache::{DataCache, CacheMetadata};
pub use schema::BarSchema;
pub use universe::{Universe, UniverseSet};
