//! The stable surface for today: what the custodian binary and the bundles
//! work bind to. Everything here is a re-export or a thin type over the
//! modules that do the work; a change behind it does not change it.
//!
//! - [`ReadingSpec`]: the TOML form (kinds in [`ReadingKind`], target, scope, routes).
//! - [`Workspace::open`]: the parsed klepto model of a project, held open for many readings.
//! - [`run`]: one spec answered over an open workspace, as a [`Reading`].
//! - [`Reading`]: counts, rows, unresolved, and its markdown and JSON.
//! - [`Receipt`]: a run as the journal keeps it.
//! - [`IndexKey::of`]: the code's index key (the digest of the canonical form of the files a scan reads).
//! - [`Refusal`]: a liaise code (`CUST0001`…) and its sentence.
//! - [`bundle()`]: a route as one annotated bundles file (`crate::bundle`).

use crate::reading::facts::Facts;
use crate::CustodianError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub use crate::bundle::{bundle, Bundle, FiltersWord};
pub use crate::paths::{PathsTable, Resolving, Route};
pub use crate::reading::refusal::ReadingRefusal as RefusalCode;
pub use crate::reading::scope::Unavailable;
pub use crate::reading::spec::{BundleOptions, Change, DeclaredRole, DeclaredSource, Depth, DuplicateOptions, ReadingKind, ReadingSpec, Readers, Scope};
pub use crate::reading::{Count, ReadingReport};
pub use crate::store::{Receipt, RECEIPT_SCHEMA};
pub use crate::CodeQueryResult as Row;

/// A refusal: its liaise code and what it says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    /// `CUST0004`.
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for Refusal {}

impl From<CustodianError> for Refusal {
    fn from(e: CustodianError) -> Self {
        match e {
            CustodianError::Reading { code, message } => Refusal { code, message },
            other => Refusal { code: "CUST0000".into(), message: other.to_string() },
        }
    }
}

impl From<Refusal> for CustodianError {
    fn from(r: Refusal) -> Self {
        CustodianError::Reading { code: r.code, message: r.message }
    }
}

/// A project's parsed klepto model, held open: one scan, many readings.
pub struct Workspace {
    pub root: PathBuf,
    /// What the scan read (a spec with another scope needs another workspace).
    pub scope: Scope,
    /// Free, or inside a job (declared routes only).
    pub how: Resolving,
    facts: Facts,
}

impl Workspace {
    /// The workspace at `root`, every member's `src/` read, free mode.
    pub fn open(root: &Path) -> Result<Workspace, Refusal> {
        Self::open_with(root, &Scope::default(), Resolving::Free)
    }

    /// The workspace with a scope (tests read, globs left out) and a mode.
    pub fn open_with(root: &Path, scope: &Scope, how: Resolving) -> Result<Workspace, Refusal> {
        let facts = Facts::build(root, scope, how)?;
        Ok(Workspace { root: facts.root.clone(), scope: scope.clone(), how, facts })
    }

    /// The workspace a spec needs (its scope; tests read for the test kinds).
    pub fn open_for(root: &Path, spec: &ReadingSpec, how: Resolving) -> Result<Workspace, Refusal> {
        Self::open_with(root, &spec.scope, how)
    }

    /// The klepto scan behind it.
    pub fn klepto(&self) -> &klepto::Klepto {
        &self.facts.k
    }

    /// The code graph (code_passenger.graph.v1) of the open scan.
    pub fn graph(&self) -> crate::CodeGraph {
        crate::graph_from_klepto(&self.root, &self.facts.k)
    }

    /// The declared routes.
    pub fn paths(&self) -> &PathsTable {
        &self.facts.paths
    }

    /// The facts the readings answer from.
    pub fn facts(&self) -> &Facts {
        &self.facts
    }
}

/// The code's index key: the digest of the canonical form of every file the
/// scan reads (relative path and content digest) with the reading schema.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IndexKey(pub String);

impl IndexKey {
    /// The key of what an open workspace read.
    pub fn of(ws: &Workspace) -> IndexKey {
        Self::of_root(&ws.root, &ws.scope).unwrap_or_else(|_| IndexKey(String::new()))
    }

    /// The key of what a scan of `root` with `scope` would read, without parsing.
    pub fn of_root(root: &Path, scope: &Scope) -> Result<IndexKey, Refusal> {
        let code = crate::code_digest(root, &scope.scan_options()).map_err(|e| Refusal::from(RefusalCode::Scan.refuse(e)))?;
        Ok(IndexKey(crate::digest::of_value(&(crate::reading::READING_SCHEMA, code))))
    }
}

/// One spec answered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reading {
    pub report: ReadingReport,
}

impl Reading {
    pub fn counts(&self) -> &[Count] {
        &self.report.counts
    }

    pub fn rows(&self) -> &[Row] {
        &self.report.query.results
    }

    pub fn unresolved(&self) -> &[Row] {
        &self.report.unresolved
    }

    /// The facts page.
    pub fn markdown(&self) -> String {
        crate::reading::render::page(&self.report)
    }

    /// The custodian.reading.v1 record (the code_passenger.query.v1 receipt inside).
    pub fn json(&self) -> String {
        serde_json::to_string_pretty(&self.report).unwrap_or_default()
    }

    /// `sha256:<hex>` of the reading's canonical form, its stamp left out.
    pub fn digest(&self) -> String {
        let mut r = self.report.clone();
        r.stamp.clear();
        crate::digest::of_value(&r)
    }

    /// Writes `<base>/<name>/<stamp>.md|.json` and the latest copies.
    pub fn write(&self, base: &Path) -> Result<(PathBuf, PathBuf, PathBuf), Refusal> {
        Ok(crate::reading::write(base, &self.report)?)
    }
}

/// `spec` answered over `ws`. A spec whose scope is not the workspace's is
/// refused (open the workspace for it: [`Workspace::open_for`]).
pub fn run(ws: &Workspace, spec: &ReadingSpec) -> Result<Reading, Refusal> {
    if spec.kind != ReadingKind::Sweep && spec.scope != ws.scope {
        return Err(RefusalCode::SpecInvalid.refuse(format!("{}: its scope is not the open workspace's; open one for it", spec.name)).into());
    }
    let stamp = crate::runs::stamp();
    let report = if spec.kind == ReadingKind::Sweep {
        crate::reading::read(&ws.root, Path::new(&spec.name), spec, &stamp, ws.how)?
    } else {
        crate::reading::read_facts(ws.facts(), Path::new(&spec.name), spec, &stamp)?
    };
    Ok(Reading { report })
}
