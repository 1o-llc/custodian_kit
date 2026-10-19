//! A route (`crate:<name>`, `decl:<name>`, a declared `path:`, or a job) as
//! one annotated `bundles` file, its contents chosen by a filters word and
//! the items at the redaction level held through `classified`.
//!
//! The signature is the stable surface the CLI binds to (`api::bundle`). Its
//! body is the bundles extension's to fill (an explicit file list into
//! bundles, the filters as a big-bad-bool word); until then it hands the
//! route to the reading kind `bundle` (`crate::job_bundle::bundle_reading`).

use crate::api::{Refusal, Workspace};
use std::path::PathBuf;

/// The bundle filters as one packed word (no tests, no benches, no examples,
/// only src, redact secret, reveal, ...). Its bits are what a receipt keeps
/// and what the same-search key digests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct FiltersWord(pub u32);

/// A bundle written.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Bundle {
    /// The `.bundle` file.
    pub file: PathBuf,
    /// `sha256:<hex>` of its bytes.
    pub digest: String,
    /// Files in it.
    pub files: usize,
    /// Items held redacted.
    pub held: usize,
}

/// `route` as one bundle file under the workspace's readings, filtered by `filters`.
pub fn bundle(ws: &Workspace, route: &str, filters: FiltersWord) -> Result<Bundle, Refusal> {
    let spec = crate::api::ReadingSpec::from_toml(&format!(
        "name = \"bundle\"\nquestion = \"this route as one file\"\nkind = \"bundle\"\ntarget = {}\n",
        toml::Value::String(route.to_string())
    ))?;
    let reading = crate::api::run(ws, &spec)?;
    let out = std::env::temp_dir().join(format!("custodian-bundle-{}-{}.bundle", std::process::id(), filters.0));
    let file = crate::job_bundle::bundle_reading(&ws.root, &reading.report, &out)?;
    let digest = crate::digest::tagged(std::fs::read(&file).unwrap_or_default());
    let held = reading.report.query.results.iter().filter(|r| r.attributes.contains_key("held")).count();
    Ok(Bundle { files: reading.report.query.results.len(), file, digest, held })
}
