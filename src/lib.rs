//! custodian_kit — read a Rust workspace as data, through klepto: the code
//! graph, queries, pins, bounded context windows, feature use sites, a plain
//! tantivy index with its versioned registry, and reading specs that answer
//! one question into a dated page.

mod model;
pub use model::*;
mod commands;
pub use commands::*;
mod args;
pub(crate) use args::*;
mod tantivy_index;
pub(crate) use tantivy_index::*;
mod registry;
pub(crate) use registry::*;
mod index_key;
pub(crate) use index_key::*;
mod tantivy_query;
pub(crate) use tantivy_query::*;
mod field_plan;
pub(crate) use field_plan::*;
mod edit_plan;
pub(crate) use edit_plan::*;
mod search_plan;
pub(crate) use search_plan::*;
mod context;
pub(crate) use context::*;
mod query;
pub use query::*;
mod render;
pub use render::*;
mod graph;
pub use graph::*;

pub mod reading;
pub mod digest;
pub mod receipts;
pub mod task_records;
pub mod store;
pub mod exec;
pub mod paths;
pub mod runs;
pub mod jobs;
pub mod diff;
pub mod features;
pub mod decisions;
pub mod testrun;
pub mod check;
pub mod annotate;
pub mod sweep;
pub mod job_bundle;
pub mod bundle;
pub mod api;
pub mod oneo;
pub mod change;
pub mod linkage;

#[cfg(test)]
mod tests;
