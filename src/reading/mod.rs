//! Readings: one question, declared as a spec file, answered from klepto's
//! facts into a dated page under `<project>/readings/<name>/`, with every
//! earlier page kept beside it.
//!
//! `reading run <spec.toml>` writes `<stamp>.md` and `<stamp>.json` (the
//! code_passenger.query.v1 receipt inside a custodian.reading.v1 record)
//! and names the latest in the folder's `index.json` (never a copy: each
//! reading is stored once). `reading list` shows the
//! specs and their latest readings. Nothing else in the project is written.

pub mod declared;
pub mod diag;
pub mod facts;
pub mod impact;
pub mod kinds;
pub mod patterns;
pub mod refusal;
pub mod requirements;
pub mod count;
pub mod render;
pub mod scope;
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
    /// A `bundle` reading's bundle bytes, written beside its page (`<stamp>.bundle`);
    /// not part of the record, so not part of its digest.
    #[serde(skip)]
    pub artifact: Option<Vec<u8>>,
    /// A `count` reading's count (`custodian.count.v1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<count::CountReport>,
    /// The finding in a few lines (the duplicates' top crate pairs, an impact's record line).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headline: Vec<String>,
    /// Where the parsed index it read came from: `parsed (<ms> ms)`, `from cache (<ms> ms)`, `from daemon`. Not part
    /// of the reading's digest: the same answer from a cache is the same reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<String>,
    /// A `requirements` reading's report (`custodian.requirements.v1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<requirements::RequirementsReport>,
}

/// What a kind found, before it becomes a report.
#[derive(Debug, Default)]
pub struct Answer {
    pub counts: Vec<Count>,
    pub rows: Vec<CodeQueryResult>,
    pub unresolved: Vec<CodeQueryResult>,
    /// The bytes a kind made (the `bundle` kind's bundle).
    pub artifact: Option<Vec<u8>>,
    /// The `count` kind's count.
    pub count: Option<count::CountReport>,
    /// The finding in a few lines, for a one-line summary (the duplicates' top crate pairs).
    pub headline: Vec<String>,
    /// What the kind did not read, and why.
    pub not_read: Vec<String>,
}

impl Answer {
    pub fn count(&mut self, what: impl Into<String>, n: usize) {
        self.counts.push(Count { what: what.into(), n });
    }
}

/// Whether a reading needs the syntax trees parsed: a kind that walks them, but a patterns reading whose code's shapes
/// are kept already ([`patterns::mined_kept`]) reads its functions from the index and its sources.
fn needs_trees(root: &Path, spec: &ReadingSpec) -> bool {
    spec.kind.walks_trees() && !(spec.kind == ReadingKind::Patterns && patterns::mined_kept(root, spec))
}

/// Answers `spec` over the workspace at `root`; writes nothing.
pub fn read(root: &Path, spec_path: &Path, spec: &ReadingSpec, stamp: &str, how: crate::paths::Resolving) -> Result<ReadingReport> {
    if spec.kind == ReadingKind::Sweep {
        return read_sweep(root, spec_path, spec, stamp, how);
    }
    if spec.kind == ReadingKind::Requirements {
        return requirements::read(root, spec_path, spec, stamp, how);
    }
    // a kind that walks the syntax trees is answered over a parse: read straight from one (no index read back
    // from the cache only to be parsed again, none kept)
    let facts = if needs_trees(root, spec) { Facts::build_parsed(root, &spec.scope, how)? } else { Facts::build(root, &spec.scope, how)? };
    read_facts(&facts, spec_path, spec, stamp)
}

/// Answers `spec` over a scan already made (a workspace held open).
pub fn read_facts(facts: &Facts, spec_path: &Path, spec: &ReadingSpec, stamp: &str) -> Result<ReadingReport> {
    read_facts_over(facts, None, spec_path, spec, stamp)
}

/// [`read_facts`] over a code graph already made from the same scan (`graph_from_klepto` of `facts`), so readings
/// answered one after another over one scan (a batch's preflights) make the graph once, not once each. `None` makes
/// it here, as `read_facts` does; a kind that walks the syntax trees over an index read back from the cache makes its
/// own from the parse it needs.
pub fn read_facts_over(facts: &Facts, graph: Option<&crate::CodeGraph>, spec_path: &Path, spec: &ReadingSpec, stamp: &str) -> Result<ReadingReport> {
    // a kind answered without a scan is answered at the scan's root
    if !spec.kind.needs_scan() {
        return read(&facts.root, spec_path, spec, stamp, facts.how);
    }
    // a kind that walks the syntax trees, over an index read back from the cache (which holds none): parsed for it
    let parsed;
    let reparsed = !facts.trees && needs_trees(&facts.root, spec);
    let facts = if reparsed {
        parsed = Facts::build_parsed(&facts.root, &spec.scope, facts.how)?;
        &parsed
    } else {
        facts
    };
    let spec = &resolved(facts, spec)?;
    let made;
    let graph = match graph.filter(|_| !reparsed) {
        Some(g) => g,
        None => {
            made = crate::graph_from_klepto(&facts.root, &facts.k);
            &made
        }
    };
    let index_key = crate::index_key_of(graph, &spec.query_args()).ok().map(|r| r.key.key_id);
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
    not_read.extend(answer.not_read.iter().cloned());
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
        artifact: answer.artifact,
        count: answer.count,
        headline: answer.headline,
        index: Some(facts.index.line()),
        requirements: None,
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
        artifact: None,
        count: None,
        headline: Vec::new(),
        index: None,
        requirements: None,
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
    /// The folder's readings index, which names the latest (`index.json`; it was a copy, `latest.md`, before 0.3.3).
    pub latest: Option<PathBuf>,
    pub index_key: Option<String>,
    pub items: Vec<String>,
    /// The bundle file a `bundle` reading wrote beside its page.
    #[serde(default)]
    pub bundle: Option<PathBuf>,
    /// The finding in a few lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headline: Vec<String>,
    /// Where the parsed index came from (the reading's `index`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<String>,
}

/// The env key a reading line takes extra changed files from (affected_tests
/// after a git ref's diff), one path per line.
pub const CHANGE_FILES_ENV: &str = "CUSTODIAN_CHANGE_FILES";

/// A spec's readings folder's index (`index.json`): the latest reading and every one, by stem
/// (`<stamp>`, whose `.md` and `.json` sit beside it). It names them; it never copies one.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReadingsIndex {
    pub latest: String,
    pub readings: Vec<String>,
}

/// The readings index's file in a spec's readings folder.
pub const READINGS_INDEX: &str = "index.json";

/// Writes `<base>/<name>/<stamp>.md` and `.json` (and `.bundle`, a bundle
/// reading's bundle), and names the latest in the folder's index
/// ([`READINGS_INDEX`]), each whole. An earlier page is never overwritten: a second reading in the
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
    // the latest is named, never copied: one copy of each reading
    let latest = dir.join(READINGS_INDEX);
    let mut index: ReadingsIndex = std::fs::read_to_string(&latest).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    index.readings.push(stem.clone());
    index.latest = stem;
    crate::store::write_whole(&latest, (serde_json::to_string_pretty(&index)? + "\n").as_bytes())?;
    if let Some(bytes) = &report.artifact {
        crate::store::write_whole(&page.with_extension("bundle"), bytes)?;
    }
    // a patterns reading's swaps as change spec files, one per exact match, beside the reading
    if report.kind == ReadingKind::Patterns {
        let specs = page.with_extension("specs");
        for (name, text) in patterns::spec_files(report) {
            crate::store::write_whole(&specs.join(name), text.as_bytes())?;
        }
    }
    Ok((page, json, latest))
}

/// The reading line's body (`custodian exec-reading`): the spec read and
/// answered; written under `out`, or printed when `out` is `-`; the summary
/// printed last, as one JSON line.
pub fn exec_reading(spec_path: &Path, root: &Path, out: &str, stamp: &str, in_job: bool) -> Result<ReadingSummary> {
    let files = std::env::var(CHANGE_FILES_ENV).ok();
    let how = if in_job { crate::paths::Resolving::Job } else { crate::paths::Resolving::Free };
    answer(spec_path, root, out, stamp, how, None, files.as_deref())
}

/// The reading line's body, whichever runner answers: the spec read (with
/// the change files a git ref's diff named), answered over `ws` when the
/// caller holds the workspace open (in process) or over a scan of `root`
/// (the line), written under `out` or printed when `out` is `-`.
pub fn answer(spec_path: &Path, root: &Path, out: &str, stamp: &str, how: crate::paths::Resolving, ws: Option<&crate::api::Workspace>, change_files: Option<&str>) -> Result<ReadingSummary> {
    let mut spec = ReadingSpec::from_path(spec_path)?;
    if let Some(files) = change_files {
        spec.change.files.extend(files.lines().filter(|l| !l.trim().is_empty()).map(str::to_string));
    }
    let report = match ws {
        Some(ws) if spec.kind.needs_scan() && ws.scope == spec.scope => read_facts(ws.facts(), spec_path, &spec, stamp)?,
        _ => read(root, spec_path, &spec, stamp, how)?,
    };
    let (page, json, latest) = if out == "-" {
        print!("{}", render::page(&report));
        (None, None, None)
    } else {
        let (p, j, l) = write(Path::new(out), &report)?;
        (Some(p), Some(j), Some(l))
    };
    let bundle = page.as_ref().filter(|_| report.artifact.is_some()).map(|p| p.with_extension("bundle"));
    Ok(ReadingSummary { name: report.name, kind: report.kind, target: report.target, stamp: report.stamp, counts: report.counts, page, json, latest, index_key: report.index_key, items: report.items, bundle, headline: report.headline, index: report.index })
}

/// A reading written before, read back.
pub fn load(json: &Path) -> Result<ReadingReport> {
    let text = std::fs::read_to_string(json).map_err(|e| ReadingRefusal::ReadingUnreadable.refuse(format!("{}: {e}", json.display())))?;
    serde_json::from_str(&text).map_err(|e| ReadingRefusal::ReadingUnreadable.refuse(format!("{}: {e}", json.display())))
}

