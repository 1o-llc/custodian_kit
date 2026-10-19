//! Readings: one question, declared as a spec file, answered from klepto's
//! facts into a dated page under `<project>/readings/<name>/`, with every
//! earlier page kept beside it.
//!
//! `reading run <spec.toml>` writes `<stamp>.md` and `<stamp>.json` (the
//! code_passenger.query.v1 receipt inside a custodian.reading.v1 record)
//! and copies them to `latest.md` / `latest.json`. `reading list` shows the
//! specs and their latest readings. Nothing else in the project is written.

pub mod declared;
pub mod diag;
pub mod facts;
pub mod kinds;
pub mod refusal;
pub mod render;
pub mod spec;

use crate::{CodeGraphSummary, CodeQueryReport, CodeQueryResult, Result};
use facts::Facts;
use refusal::ReadingRefusal;
use serde::{Deserialize, Serialize};
use spec::{ReadingKind, ReadingSpec};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const READING_SCHEMA: &str = "custodian.reading.v1";

/// One count of a reading, in the order the page shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Count {
    pub what: String,
    pub n: usize,
}

/// One reading: the spec it answers, when, and the facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingReport {
    pub schema: String,
    pub name: String,
    pub question: String,
    pub kind: ReadingKind,
    pub target: String,
    /// The spec file, relative to the root.
    pub spec: String,
    /// UTC, `YYYYMMDDTHHMMSSZ`.
    pub stamp: String,
    pub root: String,
    pub counts: Vec<Count>,
    /// The sites as code_passenger.query.v1 results.
    pub query: CodeQueryReport,
    /// What klepto could not resolve, as it reports it.
    pub unresolved: Vec<CodeQueryResult>,
    /// Every crate the results name, with the binaries that link it.
    pub binaries: BTreeMap<String, Vec<String>>,
    /// Files klepto skipped and why; why dependency macros were not read.
    pub not_read: Vec<String>,
    /// The registry's polymorphic index key of this reading's query (`kidx_...`).
    #[serde(default)]
    pub index_key: Option<String>,
    /// Every item the results name (a job's touched items).
    #[serde(default)]
    pub items: Vec<String>,
}

/// What a kind found, before it becomes a report.
#[derive(Debug, Default)]
pub struct Answer {
    pub counts: Vec<Count>,
    pub rows: Vec<CodeQueryResult>,
    pub unresolved: Vec<CodeQueryResult>,
}

impl Answer {
    pub fn count(&mut self, what: impl Into<String>, n: usize) {
        self.counts.push(Count { what: what.into(), n });
    }
}

/// Answers `spec` over the workspace at `root`; writes nothing.
pub fn read(root: &Path, spec_path: &Path, spec: &ReadingSpec, stamp: &str, how: crate::paths::Resolving) -> Result<ReadingReport> {
    if spec.kind == ReadingKind::Sweep {
        return read_sweep(root, spec_path, spec, stamp, how);
    }
    let facts = Facts::build(root, &spec.scope, how)?;
    read_facts(&facts, spec_path, spec, stamp)
}

/// Answers `spec` over a scan already made (a workspace held open).
pub fn read_facts(facts: &Facts, spec_path: &Path, spec: &ReadingSpec, stamp: &str) -> Result<ReadingReport> {
    let facts = facts;
    let spec = &resolved(facts, spec)?;
    let graph = crate::graph_from_klepto(&facts.root, &facts.k);
    let index_key = crate::index_key_of(&graph, &spec.query_args()).ok().map(|r| r.key.key_id);
    let answer = kinds::answer(facts, spec)?;

    let mut binaries = BTreeMap::new();
    for r in answer.rows.iter().chain(&answer.unresolved) {
        if let Some(c) = &r.crate_name {
            binaries.entry(c.clone()).or_insert_with(|| facts.bins_of(c));
        }
    }
    let mut not_read: Vec<String> = facts.k.skipped_files.iter().map(|(p, e)| format!("{}: {e}", facts.rel(p))).collect();
    if let Some(n) = &facts.k.dep_macro_note {
        not_read.push(n.clone());
    }
    let summary = CodeGraphSummary {
        crate_count: facts.packages.len(),
        file_count: facts.k.files.len(),
        feature_count: facts.k.features.values().map(|m| m.features.len()).sum(),
        dependency_count: facts.k.features.values().map(|m| m.dependencies.len()).sum(),
        optional_dependency_count: facts.k.features.values().flat_map(|m| &m.dependencies).filter(|d| d.optional).count(),
        function_count: facts.k.functions.len(),
        import_count: facts.k.imports.len(),
        use_site_count: facts.k.paths.len() + facts.k.calls.len() + facts.k.macros_inv.len(),
        finding_count: answer.unresolved.len(),
    };
    let items: Vec<String> = answer
        .rows
        .iter()
        .chain(&answer.unresolved)
        .filter_map(|r| r.attributes.get("item").cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut query_spec = crate::parse_code_query_spec(&spec.query_args());
    query_spec.limit = usize::MAX;
    let query = CodeQueryReport {
        schema: "code_passenger.query.v1".to_string(),
        graph_schema: "klepto.receipt.v1".to_string(),
        root: facts.root.display().to_string(),
        root_exists: true,
        max_depth: 0,
        query: query_spec,
        result_count: answer.rows.len(),
        truncated: false,
        results: answer.rows,
        summary,
        findings: Vec::new(),
    };
    Ok(ReadingReport {
        schema: READING_SCHEMA.to_string(),
        name: spec.name.clone(),
        question: spec.question.clone(),
        kind: spec.kind,
        target: spec.target.clone(),
        spec: facts.rel(&spec_path.canonicalize().unwrap_or_else(|_| spec_path.to_path_buf())),
        stamp: stamp.to_string(),
        root: facts.root.display().to_string(),
        counts: answer.counts,
        query,
        unresolved: answer.unresolved,
        binaries,
        not_read,
        index_key,
        items,
    })
}

/// The sweep's reading: no scan, no route; refused inside a job.
fn read_sweep(root: &Path, spec_path: &Path, spec: &ReadingSpec, stamp: &str, how: crate::paths::Resolving) -> Result<ReadingReport> {
    if how == crate::paths::Resolving::Job {
        return Err(ReadingRefusal::WalkedPathInJob.refuse(format!("{}: a sweep walks an undeclared tree, which a job does not", spec.name)));
    }
    let root = root.canonicalize().map_err(|e| ReadingRefusal::NoWorkspace.refuse(e))?;
    let store = crate::store::Store::find(&root);
    let table = crate::paths::PathsTable::load(&root, store.as_ref()).ok();
    let swept = crate::sweep::sweep(&root, Path::new(spec.target.trim_start_matches("path:")), table.as_ref())?;
    let mut answer = Answer::default();
    answer.count("files", swept.files);
    answer.count("crates (a Cargo.toml)", swept.crates.len());
    answer.count("directories with TOML files", swept.toml_dirs.len());
    answer.count("routes offered", swept.offered.len());
    let row = |kind: &str, label: String, attrs: Vec<(&str, String)>| CodeQueryResult {
        id: crate::stable_id(kind, &label),
        kind: kind.into(),
        label,
        crate_name: None,
        path: None,
        line: None,
        column: None,
        score_reason: vec!["swept".into()],
        attributes: attrs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    };
    answer.rows.extend(swept.by_extension.iter().map(|(e, n)| row("swept-extension", e.clone(), vec![("files", n.to_string())])));
    answer.rows.extend(swept.crates.iter().map(|c| row("swept-crate", c.clone(), vec![])));
    answer.rows.extend(swept.offered.iter().map(|o| row("route-offered", o.clone(), vec![])));
    let query_spec = crate::parse_code_query_spec(&spec.query_args());
    Ok(ReadingReport {
        schema: READING_SCHEMA.to_string(),
        name: spec.name.clone(),
        question: spec.question.clone(),
        kind: spec.kind,
        target: spec.target.clone(),
        spec: spec_path.display().to_string(),
        stamp: stamp.to_string(),
        root: root.display().to_string(),
        counts: answer.counts,
        query: CodeQueryReport {
            schema: "code_passenger.query.v1".to_string(),
            graph_schema: "custodian.sweep.v1".to_string(),
            root: root.display().to_string(),
            root_exists: true,
            max_depth: 0,
            query: query_spec,
            result_count: answer.rows.len(),
            truncated: false,
            results: answer.rows,
            summary: CodeGraphSummary::default(),
            findings: Vec::new(),
        },
        unresolved: Vec::new(),
        binaries: BTreeMap::new(),
        not_read: Vec::new(),
        index_key: None,
        items: Vec::new(),
    })
}

/// The spec with its target resolved through the declared routes: a
/// `crate:` route becomes the crate's name or the file's path, a `path:`
/// route its raw path (free mode, or declared). In a job, a target spelled
/// as a path that is not a declared route is refused.
fn resolved(facts: &Facts, spec: &ReadingSpec) -> Result<ReadingSpec> {
    let mut spec = spec.clone();
    if crate::paths::names_a_place(&spec.target) && spec.kind != ReadingKind::DeclaredIn {
        let route = facts.paths.resolve(&spec.target, facts.how)?;
        spec.target = route
            .crate_name()
            .map(str::to_string)
            .or_else(|| route.crate_file(&facts.root))
            .unwrap_or_else(|| route.name().strip_prefix("path:").unwrap_or(route.name()).to_string());
    }
    Ok(spec)
}

/// What a reading line prints last, for the run that started it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingSummary {
    pub name: String,
    pub kind: ReadingKind,
    pub target: String,
    pub stamp: String,
    pub counts: Vec<Count>,
    /// Where the page and its JSON were written (`None` in free mode).
    pub page: Option<PathBuf>,
    pub json: Option<PathBuf>,
    pub latest: Option<PathBuf>,
    pub index_key: Option<String>,
    pub items: Vec<String>,
    /// The bundle file a `bundle` reading wrote beside its page.
    #[serde(default)]
    pub bundle: Option<PathBuf>,
}

/// The env key a reading line takes extra changed files from (affected_tests
/// after a git ref's diff), one path per line.
pub const CHANGE_FILES_ENV: &str = "CUSTODIAN_CHANGE_FILES";

/// Writes `<base>/<name>/<stamp>.md` and `.json`, and their `latest` copies,
/// each whole. An earlier page is never overwritten: a second reading in the
/// same second is `<stamp>-2`.
pub fn write(base: &Path, report: &ReadingReport) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let dir = base.join(&report.name);
    let mut stem = report.stamp.clone();
    let mut n = 2;
    while dir.join(format!("{stem}.md")).exists() || dir.join(format!("{stem}.json")).exists() {
        stem = format!("{}-{n}", report.stamp);
        n += 1;
    }
    let page = dir.join(format!("{stem}.md"));
    let json = dir.join(format!("{stem}.json"));
    let md = render::page(report);
    let text = serde_json::to_string_pretty(report)? + "\n";
    crate::store::write_whole(&page, md.as_bytes())?;
    crate::store::write_whole(&json, text.as_bytes())?;
    let latest = dir.join("latest.md");
    crate::store::write_whole(&latest, md.as_bytes())?;
    crate::store::write_whole(&dir.join("latest.json"), text.as_bytes())?;
    Ok((page, json, latest))
}

/// The reading line's body (`custodian exec-reading`): the spec read and
/// answered; written under `out`, or printed when `out` is `-`; the summary
/// printed last, as one JSON line.
pub fn exec_reading(spec_path: &Path, root: &Path, out: &str, stamp: &str, in_job: bool) -> Result<ReadingSummary> {
    let mut spec = ReadingSpec::from_path(spec_path)?;
    if let Ok(files) = std::env::var(CHANGE_FILES_ENV) {
        spec.change.files.extend(files.lines().filter(|l| !l.trim().is_empty()).map(str::to_string));
    }
    let how = if in_job { crate::paths::Resolving::Job } else { crate::paths::Resolving::Free };
    let report = read(root, spec_path, &spec, stamp, how)?;
    let (page, json, latest) = if out == "-" {
        print!("{}", render::page(&report));
        (None, None, None)
    } else {
        let (p, j, l) = write(Path::new(out), &report)?;
        (Some(p), Some(j), Some(l))
    };
    let bundle = match (report.kind, &page) {
        (ReadingKind::Bundle, Some(p)) => Some(crate::job_bundle::bundle_reading(root, &report, &p.with_extension("bundle"))?),
        _ => None,
    };
    Ok(ReadingSummary { name: report.name, kind: report.kind, target: report.target, stamp: report.stamp, counts: report.counts, page, json, latest, index_key: report.index_key, items: report.items, bundle })
}

/// A reading written before, read back.
pub fn load(json: &Path) -> Result<ReadingReport> {
    let text = std::fs::read_to_string(json).map_err(|e| ReadingRefusal::ReadingUnreadable.refuse(format!("{}: {e}", json.display())))?;
    serde_json::from_str(&text).map_err(|e| ReadingRefusal::ReadingUnreadable.refuse(format!("{}: {e}", json.display())))
}

