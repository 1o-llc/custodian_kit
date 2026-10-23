//! One function per reading kind, each answering from `Facts` into
//! code_passenger.query.v1 result rows. use_sites goes through the kit's own
//! query engine (`run_code_query` with `--kind use-site --item`) over a code
//! graph built from klepto's resolved occurrences; the other kinds have no
//! counterpart there and write their rows directly, in the same shape. A row
//! that sits in an item carries the item's `#[sensitivity]` and `#[owner]`
//! when it has them.

use super::declared::{read_declared, Declared};
use super::diag;
use super::facts::{Facts, Link};
use super::refusal::ReadingRefusal;
use super::spec::{matches_target, DeclaredRole, Depth, ReadingKind, ReadingSpec};
use super::Answer;
use crate::{
    path_names_item, run_code_query, stable_id, CodeGraph, CodeGraphSummary, CodeQueryResult, CrateNode, Result, UseSiteKind, UseSiteNode,
};
use klepto::graph::NodeKind;
use klepto::{MiningOptions, PathRole, Resolution, SHAPE_LEVELS};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

/// Set (by `--reveal`) for a reading line to write a secret item's lines as they are.
pub const REVEAL_ENV: &str = "CUSTODIAN_REVEAL";

pub fn answer(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    match spec.kind {
        ReadingKind::UseSites => use_sites(f, spec),
        ReadingKind::ReadersOfEnv => readers_of_env(f, spec),
        ReadingKind::Duplicates => duplicates(f, spec),
        ReadingKind::Dependents => dependents(f, spec),
        ReadingKind::DeclaredIn => declared_in(f, spec),
        ReadingKind::Sockets => sockets(f, spec),
        ReadingKind::Reach => reach(f, spec),
        ReadingKind::Constructs => constructs(f, spec),
        ReadingKind::MatchesOn => matches_on(f, spec),
        ReadingKind::PublicApi => public_api(f, spec),
        ReadingKind::TestsCovering => tests_covering(f, spec),
        ReadingKind::AffectedTests => affected_tests(f, spec),
        ReadingKind::Sensitivity => sensitivity(f, spec),
        ReadingKind::Diagnostics => diagnostics(f, spec),
        ReadingKind::Count => super::count::rows(f, spec),
        ReadingKind::Bundle => bundle_rows(f, spec),
        ReadingKind::Sweep => Err(ReadingRefusal::SpecInvalid.refuse("a sweep is read without a scan")),
        ReadingKind::Impact => super::impact::answer(f, spec),
        ReadingKind::Linkage => crate::linkage::answer(f, spec),
        ReadingKind::Patterns => super::patterns::answer(f, spec),
        ReadingKind::Requirements => Err(ReadingRefusal::SpecInvalid.refuse("a requirements reading is read without a scan")),
    }
}

// ---------------------------------------------------------------------------
// rows

/// A row at a place in the source, with its crate, the line as context, and
/// the enclosing item's marks.
#[allow(clippy::too_many_arguments)]
pub(crate) fn site(f: &Facts, kind: &str, label: impl Into<String>, file: &Path, line: u32, col: Option<u32>, item: Option<&str>, reason: &str, attrs: Vec<(&str, String)>) -> CodeQueryResult {
    let label = label.into();
    let rel = f.rel(file);
    let mut a = BTreeMap::new();
    a.insert("context".to_string(), f.context(file, line));
    if let Some(item) = item {
        a.insert("item".to_string(), item.to_string());
        if let Some(m) = f.marks_of(item) {
            if let Some(s) = &m.sensitivity {
                a.insert("sensitivity".to_string(), s.clone());
                // a secret item's line is held through classified, and written as the
                // bundles redaction marker (the one marker) unless revealed
                if f.held(s) {
                    a.insert("held".to_string(), "true".to_string());
                    let held = classified::ClassifiedString::try_new(f.context(file, line), 1 << 16).ok();
                    let shown = match (&held, std::env::var_os(REVEAL_ENV).is_some()) {
                        (Some(h), true) => h.expose(|t| t.to_string()),
                        (Some(h), false) => h.expose(|t| bundles::redaction::RedactionMarker::over(s, bundles::redaction::Span::Lines { start: line as usize, end: line as usize }, t.as_bytes()).render(crate::bundle::PREFIX)),
                        // too long for a container: the marker over the line itself
                        (None, _) => bundles::redaction::RedactionMarker::over(s, bundles::redaction::Span::Lines { start: line as usize, end: line as usize }, f.context(file, line).as_bytes()).render(crate::bundle::PREFIX),
                    };
                    a.insert("context".to_string(), shown);
                }
            }
            if let Some(o) = &m.owner {
                a.insert("owner".to_string(), o.clone());
            }
            if let Some(mech) = &m.mechanism {
                a.insert("mechanism".to_string(), mech.clone());
            }
            if let Some(b) = &m.boundary {
                a.insert("boundary".to_string(), b.join(", "));
            }
            if let Some(sup) = &m.supersedes {
                a.insert("supersedes".to_string(), sup.clone());
            }
        }
    }
    for (k, v) in attrs {
        a.insert(k.to_string(), v);
    }
    CodeQueryResult {
        id: stable_id(kind, &format!("{rel}:{line}:{}:{label}", col.unwrap_or(0))),
        kind: kind.to_string(),
        label,
        crate_name: f.package_of(file).map(str::to_string),
        path: Some(rel),
        line: Some(line),
        column: col,
        score_reason: vec![reason.to_string()],
        attributes: a,
    }
}

/// A row at an item's own place.
pub(crate) fn item_row(f: &Facts, kind: &str, id: &str, reason: &str, attrs: Vec<(&str, String)>) -> CodeQueryResult {
    match f.item(id).and_then(|d| Some((d, d.location.line?))) {
        Some((d, line)) => site(f, kind, id, &d.location.path, line, d.location.column, Some(id), reason, attrs),
        None => {
            let mut r = named(kind, id, None, reason, attrs);
            if let Some(m) = f.marks_of(id) {
                if let Some(s) = &m.sensitivity {
                    r.attributes.insert("sensitivity".to_string(), s.clone());
                }
            }
            r
        }
    }
}

/// A row with no source place (a crate, a binary, a key, a pair of crates).
/// A row with no site, for kinds outside this file.
pub(crate) fn named_row(kind: &str, label: &str, crate_name: Option<String>, reason: &str) -> CodeQueryResult {
    named(kind, label.to_string(), crate_name, reason, vec![])
}

pub(crate) fn named(kind: &str, label: impl Into<String>, crate_name: Option<String>, reason: &str, attrs: Vec<(&str, String)>) -> CodeQueryResult {
    let label = label.into();
    CodeQueryResult {
        id: stable_id(kind, &label),
        kind: kind.to_string(),
        label,
        crate_name,
        path: None,
        line: None,
        column: None,
        score_reason: vec![reason.to_string()],
        attributes: attrs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    }
}

/// A row at a line of a declaration file.
fn declared_row(f: &Facts, kind: &str, d: &Declared, reason: &str) -> CodeQueryResult {
    let file = f.root.join(&d.file);
    let mut r = site(f, kind, d.value.clone(), &file, d.line, None, None, reason, vec![("entry", d.entry.clone()), ("key", d.at.clone()), ("role", d.role.as_str().to_string())]);
    r.crate_name = None;
    r
}

pub(crate) fn canonical(r: &Resolution) -> Option<&str> {
    Facts::canonical(r)
}

fn unresolved_text(r: &Resolution) -> Option<(String, String)> {
    match r {
        Resolution::Unresolved { reason, candidates, .. } => Some((format!("{reason:?}"), candidates.join(", "))),
        _ => None,
    }
}

pub(crate) fn crates_and_bins(f: &Facts, rows: &[CodeQueryResult]) -> (usize, usize) {
    let crates: BTreeSet<&str> = rows.iter().filter_map(|r| r.crate_name.as_deref()).collect();
    let bins: BTreeSet<String> = crates.iter().flat_map(|c| f.bins_of(c)).collect();
    (crates.len(), bins.len())
}

pub(crate) fn sort_rows(rows: &mut [CodeQueryResult]) {
    rows.sort_by(|a, b| (&a.kind, &a.path, a.line, a.column, &a.label).cmp(&(&b.kind, &b.path, b.line, b.column, &b.label)));
}

fn tail(target: &str) -> &str {
    target.rsplit("::").next().unwrap_or(target)
}

/// Items among `ids` whose level is at least `confidential` in the level table.
fn sensitive_among<'a>(f: &Facts, ids: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    let floor = f.level_rank("confidential");
    ids.into_iter()
        .filter(|id| f.marks_of(id).and_then(|m| m.sensitivity.as_deref()).and_then(|l| f.level_rank(l)).zip(floor).is_some_and(|(p, fl)| p >= fl))
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------
// use_sites

pub(crate) fn use_sites(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    let name = tail(target);
    let k = &f.k;
    let mut ans = Answer::default();

    let mut defs: Vec<CodeQueryResult> = k
        .items
        .iter()
        .filter(|d| path_names_item(&d.id, target))
        .filter_map(|d| {
            let line = d.location.line?;
            Some(site(f, "definition", d.id.clone(), &d.location.path, line, d.location.column, Some(&d.id), "klepto item", vec![("item_kind", format!("{:?}", d.kind).to_lowercase())]))
        })
        .collect();
    sort_rows(&mut defs);

    // klepto's resolved occurrences ending in the name, as the kit's use-site nodes
    let mut sites: Vec<UseSiteNode> = Vec::new();
    let mut items_at: BTreeMap<(String, u32, u32), String> = BTreeMap::new();
    let mut seen: HashSet<(String, u32, u32, String)> = HashSet::new();
    let mut add = |loc: &klepto::FileLocation, res: Option<&Resolution>, kind: UseSiteKind, scope: Option<&String>, item: Option<&String>| {
        let (Some(line), Some(col), Some(full)) = (loc.line, loc.column, res.and_then(canonical)) else { return };
        if full.rsplit("::").next() != Some(name) {
            return;
        }
        let file = f.rel(&loc.path);
        if !seen.insert((file.clone(), line, col, full.to_string())) {
            return;
        }
        if let Some(i) = item {
            items_at.insert((file.clone(), line, col), i.clone());
        }
        let package = f.package_of(&loc.path).unwrap_or("").to_string();
        let mut segs = full.split("::");
        let dep = segs.next().unwrap_or("").to_string();
        let head = segs.next().unwrap_or("*").to_string();
        let kind = match (kind, res) {
            (UseSiteKind::Path, Some(Resolution::Local(_))) => UseSiteKind::InternalPath,
            (k, _) => k,
        };
        sites.push(UseSiteNode {
            id: stable_id("site", &format!("{file}:{line}:{col}:{full}")),
            crate_id: stable_id("crate", &package),
            crate_name: package,
            path: file,
            dep,
            full_path: full.to_string(),
            head,
            kind,
            line,
            column: col,
            scope: scope.cloned().unwrap_or_else(|| "file".to_string()),
            feature_candidates: Vec::new(),
            optional_dep: false,
        });
    };
    for c in &k.calls {
        add(&c.location, c.resolution.as_ref(), UseSiteKind::Path, c.enclosing_fn.as_ref(), c.enclosing_item.as_ref());
    }
    for p in &k.paths {
        add(&p.location, p.resolution.as_ref(), UseSiteKind::Path, p.enclosing_fn.as_ref(), p.enclosing_item.as_ref());
    }
    for m in &k.macros_inv {
        add(&m.location, m.resolution.as_ref(), UseSiteKind::MacroCall, m.enclosing_fn.as_ref(), m.enclosing_item.as_ref());
    }
    for i in &k.imports {
        add(&i.location, i.resolution.as_ref(), UseSiteKind::UseStmt, i.enclosing_fn.as_ref(), i.enclosing_item.as_ref());
    }
    for a in &k.attrs {
        for r in &a.arg_resolutions {
            add(&a.location, Some(r), UseSiteKind::Path, a.enclosing_fn.as_ref(), a.enclosing_item.as_ref());
        }
    }
    let crates: Vec<CrateNode> = f
        .packages
        .values()
        .map(|p| CrateNode {
            id: stable_id("crate", &p.name),
            name: p.name.clone(),
            crate_root: p.dir.clone(),
            manifest_path: format!("{}/Cargo.toml", p.dir),
            src_root: None,
        })
        .collect();
    let mut graph = CodeGraph {
        schema: "code_passenger.graph.v1".to_string(),
        root: f.root.display().to_string(),
        root_exists: true,
        max_depth: 0,
        crates,
        files: Vec::new(),
        features: Vec::new(),
        deps: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        use_sites: sites,
        findings: Vec::new(),
        summary: CodeGraphSummary::default(),
    };
    crate::finish_summary(&mut graph);
    let mut q = crate::parse_code_query_spec(&spec.query_args());
    q.limit = usize::MAX;
    let report = run_code_query(&graph, q);
    let mut uses: Vec<CodeQueryResult> = report
        .results
        .into_iter()
        .map(|mut r| {
            if let (Some(p), Some(l)) = (&r.path, r.line) {
                r.attributes.insert("context".to_string(), f.context(&f.root.join(p), l));
                if let Some(item) = items_at.get(&(p.clone(), l, r.column.unwrap_or(0))) {
                    r.attributes.insert("item".to_string(), item.clone());
                    if let Some(m) = f.marks_of(item) {
                        if let Some(s) = &m.sensitivity {
                            r.attributes.insert("sensitivity".to_string(), s.clone());
                        }
                        if let Some(o) = &m.owner {
                            r.attributes.insert("owner".to_string(), o.clone());
                        }
                    }
                }
            }
            r.label = r.attributes.get("full_path").cloned().unwrap_or(r.label);
            r
        })
        .collect();
    sort_rows(&mut uses);

    // what klepto could not resolve that could be the name
    let mut unresolved = Vec::new();
    let mut useen: HashSet<(String, u32, u32)> = HashSet::new();
    for u in &k.graph.unresolved {
        let last = tail(&u.raw);
        let cand = matches!(&u.resolution, Resolution::Unresolved { candidates, .. } if candidates.iter().any(|c| path_names_item(c, target)));
        let (Some(line), true) = (u.location.line, last == name || cand) else { continue };
        if !useen.insert((f.rel(&u.location.path), line, u.location.column.unwrap_or(0))) {
            continue;
        }
        let (reason, cands) = unresolved_text(&u.resolution).unwrap_or_default();
        unresolved.push(site(f, "unresolved", u.raw.clone(), &u.location.path, line, u.location.column, Some(&u.from), "klepto: unresolved", vec![("reason", reason), ("candidates", cands)]));
    }
    for c in k.calls.iter().filter(|c| c.is_method && c.callee == name) {
        let Some(r) = &c.resolution else { continue };
        let (Some((reason, cands)), Some(line)) = (unresolved_text(r), c.location.line) else { continue };
        if !useen.insert((f.rel(&c.location.path), line, c.location.column.unwrap_or(0))) {
            continue;
        }
        unresolved.push(site(f, "unresolved", format!(".{}()", c.callee), &c.location.path, line, c.location.column, c.enclosing_item.as_deref(), "klepto: method receiver not typed", vec![("reason", reason), ("candidates", cands)]));
    }
    let mut in_macros = 0;
    for m in &k.macros_inv {
        let named_here: Vec<&String> = m.token_paths.iter().filter(|p| tail(p) == name).collect();
        let Some(line) = m.location.line else { continue };
        for p in named_here {
            in_macros += 1;
            unresolved.push(site(
                f,
                "macro-token",
                format!("{p} in {}!", m.path.as_deref().unwrap_or(&m.name)),
                &m.location.path,
                line,
                m.location.column,
                m.enclosing_item.as_deref(),
                "named inside macro tokens; klepto resolves no paths there",
                vec![("macro", m.name.clone())],
            ));
        }
    }
    sort_rows(&mut unresolved);

    let (crates_n, bins_n) = crates_and_bins(f, &uses);
    ans.count("definitions", defs.len());
    ans.count("use sites (resolved)", uses.len());
    ans.count("crates using it", crates_n);
    ans.count("binaries linking those crates", bins_n);
    ans.count("unresolved that could be it (klepto)", unresolved.len() - in_macros);
    ans.count("named inside macro tokens", in_macros);
    ans.rows = defs.into_iter().chain(uses).collect();
    ans.unresolved = unresolved;
    Ok(ans)
}

// ---------------------------------------------------------------------------
// env reads (readers_of_env, and the socket keys of sockets)

#[derive(Default)]
struct EnvReads {
    /// key → rows
    reads: BTreeMap<String, Vec<CodeQueryResult>>,
    named: BTreeMap<String, Vec<CodeQueryResult>>,
    consts: BTreeMap<String, Vec<CodeQueryResult>>,
    unresolved: Vec<CodeQueryResult>,
}

/// The reader fns: those the spec names, and (with `follow`) every fn or
/// method klepto's item graph shows reaching one of them.
fn reader_fns(f: &Facts, spec: &ReadingSpec) -> BTreeSet<String> {
    let base: BTreeSet<String> = spec.readers.fns.iter().cloned().collect();
    let mut readers = base.clone();
    if spec.readers.follow {
        let g = &f.k.graph;
        readers.extend(f.closure_of(base.iter().map(String::as_str)).into_iter().filter(|id| g.node(id).is_some_and(|n| matches!(n.kind, NodeKind::Fn | NodeKind::Method))));
    }
    readers
}

fn is_const_item(f: &Facts, id: &str) -> bool {
    f.item(id).is_some_and(|d| matches!(d.kind, NodeKind::Const | NodeKind::Static))
}

fn env_reads(f: &Facts, spec: &ReadingSpec, target: &str, prefix: &str) -> EnvReads {
    let g = &f.k.graph;
    let readers = reader_fns(f, spec);
    let is_reader = |id: &str| readers.contains(id) || g.resolve_id(id).is_some_and(|r| readers.contains(r));
    let macro_reads = |name: &str| spec.readers.macros.iter().any(|m| m == name || m == tail(name));

    let mut out = EnvReads::default();
    // constants that hold a key: const id → key
    let mut key_consts: BTreeMap<String, String> = BTreeMap::new();
    for l in &f.k.literals {
        let (Some(item), None, None) = (&l.enclosing_item, &l.arg_of, &l.in_macro) else { continue };
        if !is_env_key(target, &l.value) || !is_const_item(f, item) {
            continue;
        }
        let Some(line) = l.location.line else { continue };
        key_consts.insert(item.clone(), l.value.clone());
        out.consts.entry(l.value.clone()).or_default().push(site(f, &format!("{prefix}const"), item.clone(), &l.location.path, line, l.location.column, Some(item), "a constant holding the key", vec![("key", l.value.clone())]));
    }

    // a key written at a place, read or not by the call it is an argument of
    let decide = |out: &mut EnvReads, key: String, via: String, loc: &klepto::FileLocation, item: Option<&str>, arg_of: Option<&klepto::ArgOf>, in_macro: Option<&str>| {
        let Some(line) = loc.line else { return };
        let mut attrs = vec![("key", key.clone()), ("via", via)];
        if let Some(m) = in_macro.filter(|m| macro_reads(m)) {
            attrs.push(("reader", format!("{m}!")));
            out.reads.entry(key.clone()).or_default().push(site(f, &format!("{prefix}read"), key, &loc.path, line, loc.column, item, "read by a macro", attrs));
            return;
        }
        let Some(call) = arg_of else {
            if let Some(m) = in_macro {
                attrs.push(("in", format!("{m}!")));
            }
            out.named.entry(key.clone()).or_default().push(site(f, &format!("{prefix}named"), key, &loc.path, line, loc.column, item, "named, not in a call", attrs));
            return;
        };
        if let Some(m) = in_macro {
            attrs.push(("built_by", format!("{m}!")));
        }
        match call.resolution.as_ref() {
            Some(r) => match canonical(r) {
                Some(id) if is_reader(id) => {
                    attrs.push(("reader", id.to_string()));
                    out.reads.entry(key.clone()).or_default().push(site(f, &format!("{prefix}read"), key, &loc.path, line, loc.column, item, "an argument of a reader", attrs));
                }
                Some(id) => {
                    attrs.push(("in", id.to_string()));
                    out.named.entry(key.clone()).or_default().push(site(f, &format!("{prefix}named"), key, &loc.path, line, loc.column, item, "an argument of a call that does not read the environment", attrs));
                }
                None => {
                    let (reason, cands) = unresolved_text(r).unwrap_or_default();
                    attrs.extend([("call", call.callee.clone()), ("reason", reason), ("candidates", cands)]);
                    out.unresolved.push(site(f, &format!("{prefix}unresolved"), key, &loc.path, line, loc.column, item, "klepto: the call is unresolved", attrs));
                }
            },
            None => {
                attrs.push(("call", call.callee.clone()));
                out.unresolved.push(site(f, &format!("{prefix}unresolved"), key, &loc.path, line, loc.column, item, "klepto recorded no resolution for the call", attrs));
            }
        }
    };
    for l in &f.k.literals {
        let initialises_const = l.arg_of.is_none() && l.in_macro.is_none() && l.enclosing_item.as_deref().is_some_and(|i| is_const_item(f, i));
        if initialises_const || !is_env_key(target, &l.value) {
            continue;
        }
        decide(&mut out, l.value.clone(), "literal".into(), &l.location, l.enclosing_item.as_deref(), l.arg_of.as_ref(), l.in_macro.as_deref());
    }
    // the key constants wherever they are used
    for p in &f.k.paths {
        let Some(id) = p.resolution.as_ref().and_then(canonical) else { continue };
        let Some(key) = key_consts.get(id) else { continue };
        decide(&mut out, key.clone(), format!("const {id}"), &p.location, p.enclosing_item.as_deref(), p.arg_of.as_ref(), None);
    }
    out
}

pub(crate) fn declared(f: &Facts, spec: &ReadingSpec) -> Result<Vec<Declared>> {
    match sources(f, spec)? {
        Some(s) => read_declared(&f.root, &s, &f.paths, f.how),
        None => Ok(Vec::new()),
    }
}

/// The declaration sources a reading reads: the spec's own `declared_in`, else the workspace's one declared
/// default (`.custodian/config/declared.toml`), else none (the reading says so; it never counts zeros as if it had
/// read declarations).
pub(crate) fn sources(f: &Facts, spec: &ReadingSpec) -> Result<Option<Vec<super::spec::DeclaredSource>>> {
    if !spec.declared_in.is_empty() {
        return Ok(Some(spec.declared_in.clone()));
    }
    let Some(store) = crate::store::Store::find(&f.root) else { return Ok(None) };
    let file = store.path(&crate::store::named::THRESHOLDS, &[("name", DECLARED_DEFAULT)])?;
    let Ok(text) = std::fs::read_to_string(&file) else { return Ok(None) };
    let d: DeclaredDefault = toml::from_str(&text).map_err(|e| ReadingRefusal::SpecInvalid.refuse(format!("{}: {e}", file.display())))?;
    Ok((!d.declared_in.is_empty()).then_some(d.declared_in))
}

/// The workspace's default declaration sources: `config/declared.toml`, `[[declared_in]]` blocks as a spec writes them.
pub const DECLARED_DEFAULT: &str = "declared";

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredDefault {
    #[serde(default)]
    pub declared_in: Vec<super::spec::DeclaredSource>,
}

/// The note a reading carries when it read no declarations.
fn no_sources_note() -> String {
    format!("declarations were not read: the spec names no declared_in source and the workspace declares no default (.custodian/config/{DECLARED_DEFAULT}.toml)")
}

fn readers_of_env(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    let reads = env_reads(f, spec, target, "env-");
    let decls: Vec<Declared> = declared(f, spec)?.into_iter().filter(|d| d.role == DeclaredRole::Env && matches_target(target, &d.value)).collect();

    let mut keys: BTreeSet<String> = BTreeSet::new();
    keys.extend(reads.reads.keys().cloned());
    keys.extend(reads.named.keys().cloned());
    keys.extend(reads.consts.keys().cloned());
    keys.extend(decls.iter().map(|d| d.value.clone()));

    let mut ans = Answer::default();
    let mut key_rows = Vec::new();
    let (mut read_undeclared, mut declared_unread) = (0, 0);
    for key in &keys {
        let r = reads.reads.get(key).map(Vec::len).unwrap_or(0);
        let n = reads.named.get(key).map(Vec::len).unwrap_or(0);
        let entries: BTreeSet<&str> = decls.iter().filter(|d| &d.value == key).map(|d| d.entry.as_str()).collect();
        let readers: BTreeSet<&str> = reads.reads.get(key).into_iter().flatten().filter_map(|r| r.crate_name.as_deref()).collect();
        if !spec.declared_in.is_empty() {
            if r > 0 && entries.is_empty() {
                read_undeclared += 1;
            }
            if r == 0 && !entries.is_empty() {
                declared_unread += 1;
            }
        }
        key_rows.push(named(
            "env-key",
            key.clone(),
            None,
            "one key",
            vec![
                ("reads", r.to_string()),
                ("named", n.to_string()),
                ("read_by", readers.into_iter().collect::<Vec<_>>().join(", ")),
                ("declared_in", if spec.declared_in.is_empty() { "(no declared_in in the spec)".to_string() } else { entries.into_iter().collect::<Vec<_>>().join(", ") }),
            ],
        ));
    }
    let mut read_rows: Vec<CodeQueryResult> = reads.reads.into_values().flatten().collect();
    let mut named_rows: Vec<CodeQueryResult> = reads.named.into_values().flatten().collect();
    let mut const_rows: Vec<CodeQueryResult> = reads.consts.into_values().flatten().collect();
    let mut decl_rows: Vec<CodeQueryResult> = decls.iter().map(|d| declared_row(f, "env-declared", d, "declared")).collect();
    for v in [&mut read_rows, &mut named_rows, &mut const_rows, &mut decl_rows] {
        sort_rows(v);
    }
    let (crates_n, bins_n) = crates_and_bins(f, &read_rows);
    ans.count("keys", keys.len());
    ans.count("reads", read_rows.len());
    ans.count("crates reading", crates_n);
    ans.count("binaries linking those crates", bins_n);
    ans.count("constants holding a key", const_rows.len());
    ans.count("places naming a key without reading it", named_rows.len());
    if !spec.declared_in.is_empty() {
        if sources(f, spec)?.is_some() {
            ans.count("declarations", decl_rows.len());
        } else {
            ans.not_read.push(no_sources_note());
        }
        ans.count("keys read but declared nowhere", read_undeclared);
        ans.count("keys declared but read nowhere", declared_unread);
    }
    ans.count("unresolved (klepto)", reads.unresolved.len());
    ans.rows = key_rows.into_iter().chain(read_rows).chain(const_rows).chain(decl_rows).chain(named_rows).collect();
    ans.unresolved = reads.unresolved;
    sort_rows(&mut ans.unresolved);
    Ok(ans)
}

// ---------------------------------------------------------------------------
// duplicates

fn duplicates(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let level = SHAPE_LEVELS
        .iter()
        .copied()
        .find(|l| l.name() == spec.duplicates.level)
        .ok_or_else(|| ReadingRefusal::SpecInvalid.refuse(format!("duplicates.level `{}`: one of exact, renamed, literals, types, callees", spec.duplicates.level)))?;
    let only: Option<BTreeSet<String>> = (spec.target.trim() != "*").then(|| spec.target.split(',').map(|s| s.trim().replace('-', "_")).collect());
    let report = klepto::patterns::mine(&f.k, &MiningOptions { min_tokens: spec.duplicates.min_tokens, min_members: 2 });
    let fns: BTreeMap<&str, &klepto::CapturedFn> = f.k.functions.iter().map(|c| (c.fq_name.as_str(), c)).collect();
    let mut per_crate: BTreeMap<String, usize> = BTreeMap::new();
    for c in &f.k.functions {
        if let Some(p) = f.package_of(&c.location.path) {
            *per_crate.entry(p.to_string()).or_default() += 1;
        }
    }

    let mut pairs: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut rows = Vec::new();
    let mut clusters = 0;
    let mut members_n = 0;
    for c in report.clusters.iter().filter(|c| c.level == level) {
        let placed: Vec<(&klepto::CapturedFn, String)> =
            c.members.iter().filter_map(|m| fns.get(m.as_str())).filter_map(|cf| Some((*cf, f.package_of(&cf.location.path)?.to_string()))).collect();
        let crates: BTreeSet<&str> = placed.iter().map(|(_, p)| p.as_str()).collect();
        if crates.len() < 2 || only.as_ref().is_some_and(|o| !crates.iter().any(|c| o.contains(*c))) {
            continue;
        }
        clusters += 1;
        let list: Vec<&str> = crates.iter().copied().collect();
        for i in 0..list.len() {
            for j in i + 1..list.len() {
                *pairs.entry((list[i].to_string(), list[j].to_string())).or_default() += 1;
            }
        }
        for (cf, _) in &placed {
            let Some(line) = cf.location.line else { continue };
            members_n += 1;
            rows.push(site(
                f,
                "duplicate",
                cf.fq_name.clone(),
                &cf.location.path,
                line,
                cf.location.column,
                Some(&cf.fq_name),
                "same body as the others in its cluster",
                vec![("cluster", c.hash.chars().take(12).collect()), ("members", c.members.len().to_string()), ("crates", list.join(", "))],
            ));
        }
    }
    let mut pair_rows: Vec<(usize, CodeQueryResult)> = pairs
        .into_iter()
        .map(|((a, b), n)| {
            let (na, nb) = (per_crate.get(&a).copied().unwrap_or(0), per_crate.get(&b).copied().unwrap_or(0));
            (
                n,
                named(
                    "crate-pair",
                    format!("{a} ↔ {b}"),
                    None,
                    "crates sharing bodies",
                    vec![("shared_bodies", n.to_string()), ("a_functions", format!("{a}: {na}")), ("b_functions", format!("{b}: {nb}"))],
                ),
            )
        })
        .collect();
    pair_rows.sort_by(|(x, a), (y, b)| y.cmp(x).then(a.label.cmp(&b.label)));
    rows.sort_by(|a, b| (&a.attributes.get("crates"), &a.attributes.get("cluster"), &a.path, a.line).cmp(&(&b.attributes.get("crates"), &b.attributes.get("cluster"), &b.path, b.line)));
    let mut ans = Answer::default();
    ans.count(format!("clusters of same bodies across crates ({} level)", level.name()), clusters);
    ans.count("functions in them", members_n);
    ans.count("crate pairs sharing bodies", pair_rows.len());
    ans.count("functions shaped", report.shapes.len());
    // the finding in one line: the crate pairs sharing the most bodies
    ans.headline = pair_rows.iter().take(3).map(|(n, r)| format!("{} ({n} shared bodies)", r.label)).collect();
    ans.rows = pair_rows.into_iter().map(|(_, r)| r).chain(rows).collect();
    Ok(ans)
}

// ---------------------------------------------------------------------------
// dependents

pub(crate) fn dependents(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    let mut ans = Answer::default();
    if let Some(p) = f.package_named(target) {
        let direct = f.direct_dependents(&p.name);
        let linked = f.linked_dependents(&p.name);
        let mut rows = Vec::new();
        for (d, l) in &direct {
            rows.push(named("dependent", d.clone(), Some(d.clone()), "depends on it directly", vec![("how", format!("direct, {}", l.as_str())), ("dir", f.packages[d].dir.clone())]));
        }
        let direct_names: BTreeSet<&String> = direct.iter().map(|(d, _)| d).collect();
        for d in linked.iter().filter(|d| !direct_names.contains(d)) {
            rows.push(named("dependent", d.clone(), Some(d.clone()), "depends on it through others", vec![("how", "transitive".to_string()), ("dir", f.packages[d].dir.clone())]));
        }
        let dev_only = direct.iter().filter(|(d, l)| *l == Link::Dev && !linked.contains(d)).count();
        ans.count("direct dependents", direct_names.len());
        ans.count("dependents through normal and build dependencies (transitive)", linked.len());
        ans.count("dev-only dependents (tests)", dev_only);
        ans.count("binaries linking it", f.bins_of(&p.name).len());
        ans.rows = rows;
        return Ok(ans);
    }
    let id = item_id(f, target)?;
    let direct: BTreeSet<String> = f.referrers_of(&id).iter().cloned().collect();
    let all = f.closure_of([id.as_str()]);
    let mut rows: Vec<CodeQueryResult> = all.iter().map(|d| item_row(f, "dependent", d, "uses it", vec![("how", if direct.contains(d) { "direct" } else { "transitive" }.to_string())])).collect();
    sort_rows(&mut rows);
    let (crates_n, bins_n) = crates_and_bins(f, &rows);
    ans.count(format!("direct dependents of {id}"), direct.len());
    ans.count("dependents, transitive", all.len());
    ans.count("crates", crates_n);
    ans.count("binaries linking those crates", bins_n);
    ans.rows = rows;
    Ok(ans)
}

/// The graph node a target names: exactly, or by its trailing segments.
pub(crate) fn item_id(f: &Facts, target: &str) -> Result<String> {
    let g = &f.k.graph;
    g.resolve_id(target)
        .map(str::to_string)
        .or_else(|| g.nodes.keys().find(|k| path_names_item(k, target) && g.nodes[*k].kind != NodeKind::External).cloned())
        .ok_or_else(|| ReadingRefusal::TargetUnknown.refuse(target))
}

// ---------------------------------------------------------------------------
// declared_in

fn declared_in(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    if spec.declared_in.is_empty() {
        return Err(ReadingRefusal::SpecInvalid.refuse("declared_in needs at least one [[declared_in]] source"));
    }
    let hits: Vec<Declared> = declared(f, spec)?.into_iter().filter(|d| matches_target(spec.target.trim(), &d.value)).collect();
    let files: BTreeSet<&str> = hits.iter().map(|d| d.file.as_str()).collect();
    let entries: BTreeSet<(&str, &str)> = hits.iter().map(|d| (d.file.as_str(), d.entry.as_str())).collect();
    let mut ans = Answer::default();
    ans.count("declarations", hits.len());
    ans.count("files", files.len());
    ans.count("entries", entries.len());
    ans.rows = hits.iter().map(|d| declared_row(f, "declared", d, "declared")).collect();
    Ok(ans)
}

// ---------------------------------------------------------------------------
// sockets

fn sockets(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    if sources(f, spec)?.is_none() {
        return Err(ReadingRefusal::NoDeclaredSource.refuse(format!("sockets {target}: a binary's sockets are declared in its entries; name a [[declared_in]] source (binary, socket, socket_env roles) in the spec or in .custodian/config/{DECLARED_DEFAULT}.toml")));
    }
    let decls = declared(f, spec)?;
    let own: BTreeSet<(&str, &str)> = decls.iter().filter(|d| d.role == DeclaredRole::Binary && d.value == target).map(|d| (d.file.as_str(), d.entry.as_str())).collect();
    let is_own = |d: &Declared| own.contains(&(d.file.as_str(), d.entry.as_str()));
    let paths: BTreeSet<&str> = decls.iter().filter(|d| is_own(d) && d.role == DeclaredRole::Socket).map(|d| d.value.as_str()).collect();
    let keys: BTreeSet<&str> = decls.iter().filter(|d| is_own(d) && d.role == DeclaredRole::SocketEnv).map(|d| d.value.as_str()).collect();

    let mut rows: Vec<CodeQueryResult> = decls.iter().filter(|d| is_own(d) && matches!(d.role, DeclaredRole::Socket | DeclaredRole::SocketEnv)).map(|d| declared_row(f, "socket", d, "the binary's entry declares it")).collect();
    let mut unresolved = Vec::new();
    let mut dial_reads = 0;
    for key in &keys {
        let r = env_reads(f, spec, key, "dial-env-");
        dial_reads += r.reads.values().map(Vec::len).sum::<usize>();
        rows.extend(r.reads.into_values().flatten());
        rows.extend(r.consts.into_values().flatten());
        rows.extend(r.named.into_values().flatten());
        unresolved.extend(r.unresolved);
    }
    // the socket path written in code: a constant holding it and its uses, or the literal itself
    let mut path_consts: BTreeMap<String, String> = BTreeMap::new();
    let mut literals = 0;
    for l in &f.k.literals {
        if !paths.contains(l.value.as_str()) {
            continue;
        }
        let Some(line) = l.location.line else { continue };
        match l.enclosing_item.as_deref().filter(|i| l.arg_of.is_none() && l.in_macro.is_none() && is_const_item(f, i)) {
            Some(item) => {
                path_consts.insert(item.to_string(), l.value.clone());
                rows.push(site(f, "dial-const", item, &l.location.path, line, l.location.column, Some(item), "a constant holding the socket path", vec![("socket", l.value.clone())]));
            }
            None => {
                literals += 1;
                let mut attrs = vec![("socket", l.value.clone())];
                if let Some(a) = &l.arg_of {
                    attrs.push(("in", a.resolution.as_ref().and_then(canonical).unwrap_or(&a.callee).to_string()));
                }
                if let Some(m) = &l.in_macro {
                    attrs.push(("macro", format!("{m}!")));
                }
                rows.push(site(f, "dial-literal", l.value.clone(), &l.location.path, line, l.location.column, l.enclosing_item.as_deref(), "the socket path written in code", attrs));
            }
        }
    }
    let mut const_uses = 0;
    for p in &f.k.paths {
        let (Some(id), Some(line)) = (p.resolution.as_ref().and_then(canonical), p.location.line) else { continue };
        if let Some(sock) = path_consts.get(id) {
            const_uses += 1;
            rows.push(site(f, "dial-const-use", id.to_string(), &p.location.path, line, p.location.column, p.enclosing_item.as_deref(), "uses the socket path's constant", vec![("socket", sock.clone())]));
        }
    }
    // other entries naming the path or its key
    let others: Vec<&Declared> = decls.iter().filter(|d| !is_own(d) && (paths.contains(d.value.as_str()) || keys.contains(d.value.as_str()))).collect();
    rows.extend(others.iter().map(|d| declared_row(f, "dial-entry", d, "another entry names the socket or its key")));
    sort_rows(&mut rows);
    sort_rows(&mut unresolved);
    let mut ans = Answer::default();
    ans.count("entries running the binary", own.len());
    ans.count("sockets declared", paths.len());
    ans.count("socket env keys declared", keys.len());
    ans.count("reads of those keys in code", dial_reads);
    ans.count("socket path literals in code", literals);
    ans.count("constants holding a socket path", path_consts.len());
    ans.count("uses of those constants", const_uses);
    ans.count("other entries naming a socket or its key", others.len());
    ans.count("unresolved (klepto)", unresolved.len());
    ans.rows = rows;
    ans.unresolved = unresolved;
    Ok(ans)
}

// ---------------------------------------------------------------------------
// reach

/// What a target names for reach: a crate (by name or directory), a file
/// (relative to the root), or an item.
enum Changed {
    Crate(String),
    Items { seeds: BTreeSet<String>, crates: BTreeSet<String> },
}

fn changed(f: &Facts, target: &str) -> Result<Changed> {
    if let Some(p) = f.package_named(target) {
        return Ok(Changed::Crate(p.name.clone()));
    }
    let as_file = f.root.join(target.trim_start_matches("./"));
    if as_file.is_file() {
        let seeds: BTreeSet<String> = f.k.items.iter().filter(|d| d.location.path == as_file).map(|d| d.id.clone()).collect();
        let crates = f.package_of(&as_file).map(|p| BTreeSet::from([p.to_string()])).or_else(|| f.package_of_rel(target).map(|p| BTreeSet::from([p.name.clone()]))).unwrap_or_default();
        return Ok(Changed::Items { seeds, crates });
    }
    let id = item_id(f, target)?;
    let crates = f.item(&id).map(|d| BTreeSet::from([d.package.clone()])).unwrap_or_default();
    Ok(Changed::Items { seeds: BTreeSet::from([id]), crates })
}

/// The codes the reached items raise (their paths naming a code variant).
fn codes_touched(f: &Facts, items: &BTreeSet<String>) -> Vec<(String, String)> {
    let (codes, _) = diag::tables(f);
    let by_variant: BTreeMap<&str, &diag::Declared> = codes.iter().map(|d| (d.variant.as_str(), d)).collect();
    let mut out = BTreeSet::new();
    for p in &f.k.paths {
        let (Some(item), Some(id)) = (&p.enclosing_item, p.resolution.as_ref().and_then(canonical)) else { continue };
        if items.contains(item) {
            if let Some(d) = by_variant.get(id) {
                out.insert((d.code.clone().unwrap_or_else(|| d.variant.clone()), d.message.clone()));
            }
        }
    }
    out.into_iter().collect()
}

pub(crate) fn reach(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    let mut rows = Vec::new();
    // a changed crate reaches every binary linking it; changed items reach the
    // binaries whose own crate root holds an item that reaches them
    let mut item_bins: Option<BTreeSet<String>> = None;
    // with the change gone: how deep it sits in each crate using it
    let mut scopes: Vec<crate::reading::scope::Unavailable> = Vec::new();
    let (crates, items): (BTreeSet<String>, BTreeSet<String>) = match changed(f, target)? {
        Changed::Crate(c) => {
            let items: BTreeSet<String> = f.k.items.iter().filter(|d| d.package == c).map(|d| d.id.clone()).collect();
            rows.push(named("reach-crate", c.clone(), Some(c.clone()), "the changed crate", vec![("how", "changed".into()), ("dir", f.packages[&c].dir.clone())]));
            let mut crates = BTreeSet::from([c.clone()]);
            let direct: BTreeSet<String> = f.direct_dependents(&c).into_iter().filter(|(_, l)| *l != Link::Dev).map(|(d, _)| d).collect();
            for d in f.linked_dependents(&c) {
                let how = if direct.contains(&d) { "direct" } else { "transitive" };
                rows.push(named("reach-crate", d.clone(), Some(d.clone()), "links the changed crate", vec![("how", how.into()), ("dir", f.packages[&d].dir.clone())]));
                crates.insert(d);
            }
            for d in &direct {
                scopes.push(crate::reading::scope::of_dependency(f, d, &c));
            }
            for d in f.direct_dependents(&c).into_iter().filter(|(d, l)| *l == Link::Dev && !crates.contains(d)).map(|(d, _)| d) {
                rows.push(named("reach-test-only", d.clone(), Some(d.clone()), "a dev-dependency: its tests build against the changed crate", vec![("dir", f.packages[&d].dir.clone())]));
            }
            (crates, items)
        }
        Changed::Items { seeds, crates: seed_crates } => {
            for s in &seeds {
                rows.push(item_row(f, "reach-changed", s, "the changed item", vec![]));
            }
            let reached = f.closure_of(seeds.iter().map(String::as_str));
            for r in &reached {
                rows.push(item_row(f, "reach-item", r, "reaches the change through its references", vec![]));
            }
            let mut crates = seed_crates;
            crates.extend(reached.iter().filter_map(|id| f.item(id).map(|d| d.package.clone())));
            for c in &crates {
                rows.push(named("reach-crate", c.clone(), Some(c.clone()), "holds the change or an item reaching it", vec![("dir", f.packages.get(c).map(|p| p.dir.clone()).unwrap_or_default())]));
            }
            let mut by_pkg: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for id in &seeds {
                if let Some(d) = f.item(id) {
                    by_pkg.entry(d.package.clone()).or_default().insert(id.clone());
                }
            }
            for (p, ids) in &by_pkg {
                scopes.push(crate::reading::scope::unavailable(f, p, target, ids));
            }
            let mut all = seeds;
            all.extend(reached);
            item_bins = Some(f.packages.values().flat_map(|p| p.bins.iter()).filter(|b| {
                let root = format!("{}::", b.replace('-', "_"));
                all.iter().any(|id| id.starts_with(&root))
            }).cloned().collect());
            (crates, all)
        }
    };
    let bins: BTreeSet<String> = item_bins.unwrap_or_else(|| crates.iter().flat_map(|c| f.bins_of(c)).collect());
    for b in &bins {
        let from: Vec<&str> = f.packages.values().filter(|q| q.bins.contains(b)).map(|q| q.name.as_str()).collect();
        rows.push(named("reach-binary", b.clone(), from.first().map(|s| s.to_string()), "a binary linking what changed", vec![("built_by", from.join(", "))]));
    }
    let files: BTreeSet<String> = items.iter().filter_map(|id| f.item(id).map(|d| f.rel(&d.location.path))).collect();
    let decls = declared(f, spec)?;
    let entries: Vec<&Declared> = decls.iter().filter(|d| d.role == DeclaredRole::Binary && bins.contains(&d.value)).collect();
    rows.extend(entries.iter().map(|d| declared_row(f, "reach-entry", d, "names a binary that links what changed")));
    let sensitive = sensitive_among(f, items.iter().map(String::as_str));
    for s in &sensitive {
        rows.push(item_row(f, "reach-sensitive", s, "a reached item at level confidential or above", vec![]));
    }
    let codes = codes_touched(f, &items);
    for (code, msg) in &codes {
        rows.push(named("reach-code", code.clone(), None, "a liaise code the reached items raise", vec![("message", msg.clone())]));
    }
    // the env keys the reached items read, as their `reads_env` marks declare them (the maintenance registry is the
    // declaration; no second source)
    let env = env_read_by(f, &items);
    for (key, by) in &env {
        rows.push(named("reach-env", key.clone(), None, "an env key a reached item reads (its reads_env mark)", vec![("read_by", by.iter().cloned().collect::<Vec<_>>().join(", "))]));
    }
    for u in &scopes {
        rows.extend(scope_rows(u));
    }
    let entry_names: BTreeSet<(&str, &str)> = entries.iter().map(|d| (d.file.as_str(), d.entry.as_str())).collect();
    let mut ans = Answer::default();
    ans.count("items touched", items.len());
    ans.count("files", files.len());
    ans.count("crates", crates.len());
    ans.count("binaries", bins.len());
    let read_sources = sources(f, spec)?.is_some();
    if read_sources {
        ans.count("entries naming those binaries", entry_names.len());
        ans.count("declarations naming those binaries", entries.len());
    } else {
        ans.not_read.push(no_sources_note());
    }
    ans.count("items at level confidential or above", sensitive.len());
    ans.count("liaise codes raised there", codes.len());
    ans.count("env keys read there (reads_env)", env.len());
    ans.count("test-only dependents", rows.iter().filter(|r| r.kind == "reach-test-only").count());
    ans.count("items unavailable with it off", scopes.iter().map(|u| u.items.len()).sum());
    ans.count("files wholly behind it", scopes.iter().map(|u| u.units.iter().filter(|x| x.depth == Depth::File).count()).sum());
    ans.count("callers in other crates losing items", scopes.iter().map(|u| u.callers_elsewhere.len()).sum());
    ans.rows = rows;
    Ok(ans)
}

/// Env key → the items among `items` whose `reads_env` marks name it.
pub(crate) fn env_read_by(f: &Facts, items: &BTreeSet<String>) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for id in items {
        if let Some(m) = f.marks.get(id) {
            for k in &m.reads_env {
                out.entry(k.clone()).or_default().insert(id.clone());
            }
        }
    }
    out
}

/// A scope as rows: one `scope` row per crate (its depth), one
/// `unavailable-unit` per unit, one `loses` per caller elsewhere.
/// A literal that is an env key the target names: the target matches it and
/// it is spelled as a key (letters, digits, `_`; not a sentence or `K=V`).
fn is_env_key(target: &str, value: &str) -> bool {
    matches_target(target, value) && !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !value.starts_with(|c: char| c.is_ascii_digit())
}

pub fn scope_rows(u: &crate::reading::scope::Unavailable) -> Vec<CodeQueryResult> {
    let depth = u.depth.map(Depth::as_str).unwrap_or("none").to_string();
    let mut rows = vec![named(
        "scope",
        format!("{} without {}", u.package, u.cause),
        Some(u.package.clone()),
        "how deep it sits, and what goes with it off",
        vec![("depth", depth), ("cause", u.cause.clone()), ("direct", u.direct.join(", ")), ("unavailable", u.items.len().to_string()), ("header", u.header().join("\n"))],
    )];
    for x in &u.units {
        rows.push(named("unavailable-unit", x.name.clone(), Some(u.package.clone()), "unavailable with it off", vec![("depth", x.depth.as_str().to_string()), ("items", x.items.to_string()), ("cause", u.cause.clone())]));
    }
    for l in &u.callers_elsewhere {
        rows.push(named("loses", l.caller.clone(), Some(l.package.clone()), "a caller in another crate that loses these", vec![("loses", l.loses.join(", ")), ("cause", u.cause.clone())]));
    }
    rows
}

// ---------------------------------------------------------------------------
// constructs

pub(crate) fn types_named<'f>(f: &'f Facts, target: &str) -> Vec<&'f klepto::TypeDecl> {
    f.k.types.iter().filter(|t| path_names_item(&t.id, target)).collect()
}

pub(crate) fn constructs(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    let types = types_named(f, target);
    if types.is_empty() {
        return Err(ReadingRefusal::TargetUnknown.refuse(target));
    }
    let mut rows = Vec::new();
    let mut ctor_fns: BTreeSet<String> = BTreeSet::new();
    for t in &types {
        let names_it = |id: &str| id == t.id || id.strip_prefix(&format!("{}::", t.id)).is_some_and(|rest| t.variants.iter().any(|v| v.name == rest));
        for p in &f.k.paths {
            let (Some(id), Some(line)) = (p.resolution.as_ref().and_then(canonical), p.location.line) else { continue };
            if !names_it(id) {
                continue;
            }
            let how = match (p.role, p.ns) {
                (PathRole::StructLiteral, _) => "struct literal",
                (PathRole::Other, klepto::Namespace::Value) => "constructor or unit value",
                _ => continue,
            };
            rows.push(site(f, "construct", id, &p.location.path, line, p.location.column, p.enclosing_item.as_deref(), how, vec![("how", how.to_string())]));
        }
        // its own associated fns returning Self or the type
        for func in &f.k.functions {
            let klepto::FnKind::ImplMethod { self_ty, trait_ty: None } = &func.kind else { continue };
            if tail(self_ty) != t.name {
                continue;
            }
            let ret = func.return_ty.as_deref().unwrap_or("");
            if ret == "Self" || ret == t.name || ret.ends_with(&format!("::{}", t.name)) {
                ctor_fns.insert(func.fq_name.clone());
            }
        }
    }
    for c in &f.k.calls {
        let (Some(id), Some(line)) = (c.resolution.as_ref().and_then(canonical), c.location.line) else { continue };
        if ctor_fns.contains(id) {
            rows.push(site(f, "construct", id, &c.location.path, line, c.location.column, c.enclosing_item.as_deref(), "a call of its own constructor", vec![("how", "constructor fn".into())]));
        }
    }
    sort_rows(&mut rows);
    rows.dedup_by(|a, b| a.path == b.path && a.line == b.line && a.column == b.column && a.label == b.label);
    let (crates_n, bins_n) = crates_and_bins(f, &rows);
    let mut ans = Answer::default();
    ans.count("types named", types.len());
    ans.count("places building it", rows.len());
    ans.count("constructor fns returning it", ctor_fns.len());
    ans.count("crates", crates_n);
    ans.count("binaries linking those crates", bins_n);
    ans.rows = rows;
    Ok(ans)
}

// ---------------------------------------------------------------------------
// matches_on

pub(crate) fn matches_on(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    let enums: Vec<&klepto::TypeDecl> = types_named(f, target).into_iter().filter(|t| !t.variants.is_empty()).collect();
    if enums.is_empty() {
        return Err(ReadingRefusal::TargetUnknown.refuse(format!("{target}: no enum of that name")));
    }
    let mut rows = Vec::new();
    let mut missing_total = 0;
    for e in &enums {
        let variants: BTreeSet<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
        for m in &f.k.matches {
            let named: BTreeSet<&str> = m
                .arms
                .iter()
                .flat_map(|a| a.paths.iter())
                .filter_map(|p| p.resolution.as_ref().and_then(canonical))
                .filter_map(|id| id.strip_prefix(&format!("{}::", e.id)))
                .filter(|v| variants.contains(v))
                .collect();
            if named.is_empty() {
                continue;
            }
            let Some(line) = m.location.line else { continue };
            let catch_all = m.arms.iter().any(|a| a.catch_all);
            let missing: Vec<&str> = variants.difference(&named).copied().collect();
            if !catch_all {
                missing_total += missing.len();
            }
            rows.push(site(
                f,
                "match",
                format!("match {}", m.scrutinee),
                &m.location.path,
                line,
                m.location.column,
                m.enclosing_item.as_deref(),
                "a match naming the enum's variants",
                vec![
                    ("enum", e.id.clone()),
                    ("arms", m.arms.len().to_string()),
                    ("names", named.iter().copied().collect::<Vec<_>>().join(", ")),
                    ("not_named", missing.join(", ")),
                    ("catch_all", catch_all.to_string()),
                ],
            ));
        }
    }
    sort_rows(&mut rows);
    let (crates_n, _) = crates_and_bins(f, &rows);
    let mut ans = Answer::default();
    ans.count("enums named", enums.len());
    ans.count("matches on it", rows.len());
    ans.count("matches with a catch-all arm", rows.iter().filter(|r| r.attributes.get("catch_all").is_some_and(|c| c == "true")).count());
    ans.count("variants left to a catch-all or unnamed, in matches without one", missing_total);
    ans.count("crates", crates_n);
    ans.rows = rows;
    Ok(ans)
}

// ---------------------------------------------------------------------------
// public_api

pub(crate) fn public_api(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    let p = f.package_named(target).ok_or_else(|| ReadingRefusal::TargetUnknown.refuse(target))?;
    let mut rows = Vec::new();
    let mut used = 0;
    let mut uses_total = 0;
    let public: Vec<&klepto::ItemDecl> = f.k.items.iter().filter(|d| d.package == p.name && d.is_public && d.kind != NodeKind::Module).collect();
    for d in &public {
        let users: BTreeSet<&String> = f.referrers_of(&d.id).iter().filter(|r| f.item(r).is_some_and(|u| u.package != p.name)).collect();
        let crates: BTreeSet<String> = users.iter().filter_map(|u| f.item(u).map(|i| i.package.clone())).collect();
        if !users.is_empty() {
            used += 1;
        }
        uses_total += users.len();
        rows.push(item_row(
            f,
            "public-item",
            &d.id,
            "public in the crate",
            vec![("item_kind", format!("{:?}", d.kind).to_lowercase()), ("users", users.len().to_string()), ("used_by_crates", crates.into_iter().collect::<Vec<_>>().join(", "))],
        ));
        for u in users {
            rows.push(item_row(f, "public-use", u, "uses it from another crate", vec![("uses", d.id.clone())]));
        }
    }
    sort_rows(&mut rows);
    let mut ans = Answer::default();
    ans.count("public items", public.len());
    ans.count("used from other crates", used);
    ans.count("never used from another crate", public.len() - used);
    ans.count("uses from other crates (items)", uses_total);
    ans.rows = rows;
    Ok(ans)
}

// ---------------------------------------------------------------------------
// tests (tests_covering, affected_tests)

/// One test, as cargo runs it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TestCase {
    pub package: String,
    /// `lib`, `bin:<name>` or `test:<name>`: the target that holds it.
    pub target: String,
    /// The test's path inside its target, as libtest names it.
    pub filter: String,
    pub id: String,
}

/// The cargo test lines that run exactly `cases`: one per package and
/// target, filters exact.
pub fn test_lines(cases: &[TestCase]) -> Vec<(String, String, Vec<String>)> {
    let mut by: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for c in cases {
        by.entry((c.package.clone(), c.target.clone())).or_default().push(c.filter.clone());
    }
    by.into_iter().map(|((p, t), fs)| (p, t, fs)).collect()
}

/// The cargo flag that selects a target.
pub fn target_flag(target: &str) -> String {
    match target.split_once(':') {
        Some(("bin", n)) => format!("--bin {n}"),
        Some(("test", n)) => format!("--test {n}"),
        _ => "--lib".to_string(),
    }
}

fn test_case(f: &Facts, id: &str) -> Option<TestCase> {
    let d = f.item(id)?;
    let pkg = f.k.packages.get(&d.package)?;
    let mut segs: Vec<&str> = id.split("::").collect();
    let root = segs.remove(0).to_string();
    let target = if root == d.package && pkg.targets.iter().any(|t| t.kind == "lib" || t.kind == "proc-macro") {
        "lib".to_string()
    } else if root == d.package {
        // a package with no library: the binary named as the package is the root
        let b = pkg.targets.iter().find(|t| t.kind == "bin" && t.name.replace('-', "_") == root)?;
        format!("bin:{}", b.name)
    } else {
        // the target whose crate-root name this is (klepto names a root after its target)
        let t = pkg.targets.iter().find(|t| {
            let n = t.name.replace('-', "_");
            n == root || format!("{}_{}", d.package, t.kind) == root
        })?;
        match t.kind.as_str() {
            "test" => format!("test:{}", t.name),
            "bin" => format!("bin:{}", t.name),
            other => format!("{other}:{}", t.name),
        }
    };
    Some(TestCase { package: d.package.clone(), target, filter: segs.join("::"), id: id.to_string() })
}

pub(crate) fn tests_reaching(f: &Facts, seeds: &BTreeSet<String>) -> Vec<TestCase> {
    let tests = f.tests();
    let reached = f.closure_of(seeds.iter().map(String::as_str));
    let mut cases: Vec<TestCase> = tests.iter().filter(|t| reached.contains(*t) || seeds.contains(*t)).filter_map(|t| test_case(f, t)).collect();
    cases.sort();
    cases
}

/// Every test of the named crates (their unit tests, their integration tests, their binaries' tests).
pub(crate) fn tests_of_crates(f: &Facts, crates: &BTreeSet<String>) -> Vec<TestCase> {
    let mut cases: Vec<TestCase> = f.tests().iter().filter(|t| f.item(t).is_some_and(|d| crates.contains(&d.package))).filter_map(|t| test_case(f, t)).collect();
    cases.sort();
    cases
}

pub(crate) fn test_rows(f: &Facts, cases: &[TestCase], reason: &str) -> Vec<CodeQueryResult> {
    let mut rows: Vec<CodeQueryResult> = cases.iter().map(|c| item_row(f, "test", &c.id, reason, vec![("filter", c.filter.clone()), ("test_target", c.target.clone())])).collect();
    for (p, t, filters) in test_lines(cases) {
        let line = format!("cargo test -p {p} {} -- {} --exact", target_flag(&t), filters.join(" "));
        rows.push(named("test-line", line, Some(p), "runs exactly these tests", vec![("tests", filters.len().to_string()), ("test_target", t)]));
    }
    rows
}

fn tests_covering(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let id = item_id(f, spec.target.trim())?;
    let cases = tests_reaching(f, &BTreeSet::from([id.clone()]));
    let rows = test_rows(f, &cases, "a test whose calls reach the item");
    let crates: BTreeSet<&str> = cases.iter().map(|c| c.package.as_str()).collect();
    let mut ans = Answer::default();
    ans.count(format!("tests reaching {id}"), cases.len());
    ans.count("crates", crates.len());
    ans.count("cargo test lines", test_lines(&cases).len());
    ans.count("tests in the scan", f.tests().len());
    ans.rows = rows;
    Ok(ans)
}

/// The items a change touches: those located in the changed files, and the
/// items named.
pub fn changed_items(f: &Facts, files: &[String], items: &[String]) -> BTreeSet<String> {
    let abs: BTreeSet<std::path::PathBuf> = files.iter().map(|p| f.root.join(p.trim_start_matches("./"))).collect();
    let mut out: BTreeSet<String> = f.k.items.iter().filter(|d| abs.contains(&d.location.path)).map(|d| d.id.clone()).collect();
    for i in items {
        if let Ok(id) = item_id(f, i) {
            out.insert(id);
        }
    }
    out
}

fn affected_tests(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    if spec.change.files.is_empty() && spec.change.items.is_empty() {
        return Err(ReadingRefusal::NoChange.refuse(&spec.name));
    }
    let mut seeds = changed_items(f, &spec.change.files, &spec.change.items);
    // a changed file that holds no item (a manifest, a test's expected output, a fixture) changes its crate: every
    // item of it is a seed, and every test of it runs
    let whole: BTreeSet<String> = spec.change.files.iter().filter(|p| !p.ends_with(".rs")).filter_map(|p| f.package_of_rel(p).map(|c| c.name.clone())).collect();
    seeds.extend(f.k.items.iter().filter(|d| whole.contains(&d.package)).map(|d| d.id.clone()));
    let mut cases = tests_reaching(f, &seeds);
    cases.extend(tests_of_crates(f, &whole));
    cases.sort();
    cases.dedup();
    // the crates whose tests run: the proof's rule (change::proof_packages), the crates the change is in and their
    // direct dependents, and the readers of a public item whose signature it alters; a further dependent's test is
    // named, not run
    let (run_in, why) = affected_crates(f, spec);
    let (cases, further): (Vec<TestCase>, Vec<TestCase>) = cases.into_iter().partition(|c| run_in.contains(&c.package));
    let mut rows: Vec<CodeQueryResult> = spec.change.files.iter().map(|p| named("changed-file", p.clone(), f.package_of_rel(p).map(|c| c.name.clone()), if p.ends_with(".rs") { "in the change" } else { "in the change: not Rust, so every test of its crate" }, vec![])).collect();
    rows.extend(run_in.iter().map(|c| named("affected-crate", c.clone(), Some(c.clone()), why.get(c).map(String::as_str).unwrap_or("affected"), vec![])));
    rows.extend(test_rows(f, &cases, "a test whose closure reaches the change"));
    rows.extend(further.iter().map(|c| item_row(f, "test-further", &c.id, "reaches the change from a further dependent: run only when a public signature it reads changes", vec![("filter", c.filter.clone()), ("test_target", c.target.clone())])));
    let crates: BTreeSet<&str> = cases.iter().map(|c| c.package.as_str()).collect();
    let mut ans = Answer::default();
    ans.count("files changed", spec.change.files.len());
    ans.count("items changed", seeds.len());
    ans.count("tests reaching the change", cases.len());
    ans.count("tests of further dependents, not run", further.len());
    ans.count("crates with such tests", crates.len());
    ans.count("cargo test lines", test_lines(&cases).len());
    ans.count("tests in the scan", f.tests().len());
    ans.rows = rows;
    Ok(ans)
}

/// The crates whose tests a change's test run runs, and why each: the crates its files are in (`changed`), each crate
/// that depends on one of them directly (`direct dependent`), and for each public item whose signature it alters
/// (`spec.change.signatures`) each crate holding an occurrence klepto resolves to it (`reads <item>`); an item no
/// longer in the scan (removed, renamed) has its readers in every crate that depends on its crate, near or far
/// (`depends on <crate>, whose <item> is gone`). The proof's rule, `change::proof_packages`, for a change read from
/// a diff: never the closure of dependents for a body.
pub fn affected_crates(f: &Facts, spec: &ReadingSpec) -> (BTreeSet<String>, BTreeMap<String, String>) {
    let mut why: BTreeMap<String, String> = BTreeMap::new();
    let changed: BTreeSet<String> = spec.change.files.iter().filter_map(|p| f.package_of_rel(p).map(|c| c.name.clone())).chain(spec.change.items.iter().filter_map(|i| item_id(f, i).ok()).filter_map(|id| f.item(&id).map(|d| d.package.clone()))).collect();
    for c in &changed {
        why.insert(c.clone(), "changed".into());
    }
    for (dependent, deps) in &f.k.package_deps {
        if changed.iter().any(|c| deps.contains(c)) {
            why.entry(dependent.clone()).or_insert_with(|| "direct dependent".into());
        }
    }
    for s in &spec.change.signatures {
        let Some((file, path)) = s.split_once('#') else { continue };
        let Some(m) = crate::change::module_of_file(&f.k, &f.root.join(file)) else { continue };
        let item = format!("{}::{path}", m.path);
        let names_it = |r: &Option<klepto::Resolution>| matches!(r, Some(klepto::Resolution::Local(x) | klepto::Resolution::External(x)) if *x == item || x.starts_with(&format!("{item}::")));
        if f.item(&item).is_some() || f.k.functions.iter().any(|x| x.fq_name == item) {
            let roots = f.k.paths.iter().filter(|o| names_it(&o.resolution)).map(|o| o.module_path.first().cloned()).chain(f.k.calls.iter().filter(|o| names_it(&o.resolution)).map(|o| o.module_path.first().cloned()));
            for root in roots.flatten() {
                why.entry(crate::change::package_of(&f.k, &root)).or_insert_with(|| format!("reads {item}"));
            }
        } else if let Some(owner) = f.package_of_rel(file).map(|c| c.name.clone()) {
            let mut reach: BTreeSet<String> = BTreeSet::from([owner.clone()]);
            loop {
                let more: Vec<String> = f.k.package_deps.iter().filter(|(d, deps)| !reach.contains(*d) && deps.iter().any(|x| reach.contains(x))).map(|(d, _)| d.clone()).collect();
                if more.is_empty() {
                    break;
                }
                for d in more {
                    why.entry(d.clone()).or_insert_with(|| format!("depends on {owner}, whose {item} is gone"));
                    reach.insert(d);
                }
            }
        }
    }
    (why.keys().cloned().collect(), why)
}

// ---------------------------------------------------------------------------
// sensitivity

fn sensitivity(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let only: Option<BTreeSet<String>> = (spec.target.trim() != "*").then(|| spec.target.split(',').map(|s| s.trim().replace('-', "_")).collect());
    let mut rows = Vec::new();
    let mut by_level: BTreeMap<String, usize> = BTreeMap::new();
    let marked: Vec<(&String, &super::facts::Marks)> = f.marks.iter().filter(|(id, m)| m.sensitivity.is_some() && only.as_ref().is_none_or(|o| f.item(id).is_some_and(|d| o.contains(&d.package)))).collect();
    for (id, m) in &marked {
        let level = m.sensitivity.clone().unwrap_or_default();
        *by_level.entry(level.clone()).or_default() += 1;
        rows.push(item_row(f, "sensitive-item", id, "marked #[sensitivity]", vec![("level", level.clone()), ("known_level", f.level_rank(&level).is_some().to_string())]));
    }
    // uses of a more sensitive item from a less sensitive one (an unmarked user is public)
    let mut ups = 0;
    for (id, m) in &marked {
        let Some(rank) = m.sensitivity.as_deref().and_then(|l| f.level_rank(l)) else { continue };
        for user in f.referrers_of(id) {
            let user_level = f.marks_of(user).and_then(|m| m.sensitivity.clone());
            let user_rank = user_level.as_deref().map(|l| f.level_rank(l)).unwrap_or(Some(0));
            let Some(user_rank) = user_rank else { continue };
            if user_rank < rank {
                ups += 1;
                rows.push(item_row(
                    f,
                    "sensitivity-up",
                    user,
                    "uses a more sensitive item",
                    vec![("uses", (*id).clone()), ("uses_level", m.sensitivity.clone().unwrap_or_default()), ("own_level", user_level.unwrap_or_else(|| "public (unmarked)".into()))],
                ));
            }
        }
    }
    sort_rows(&mut rows);
    let mut ans = Answer::default();
    ans.count(format!("items marked (marks from the {})", f.marks_from), marked.len());
    for name in &f.levels {
        ans.count(format!("at {name}"), by_level.get(name).copied().unwrap_or(0));
    }
    let unknown: usize = by_level.iter().filter(|(l, _)| f.level_rank(l).is_none()).map(|(_, n)| *n).sum();
    ans.count("at a level the level table does not name", unknown);
    ans.count("uses of a more sensitive item from a less sensitive one", ups);
    ans.count("items with an owner", f.marks.values().filter(|m| m.owner.is_some()).count());
    ans.rows = rows;
    Ok(ans)
}

// ---------------------------------------------------------------------------
// diagnostics

fn diagnostics(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    let (codes, unread) = diag::tables(f);
    // the target: `*`, a crate, a code, or a file of log text (`@path`)
    let text = target.strip_prefix('@').map(|p| std::fs::read_to_string(f.root.join(p)).unwrap_or_default());
    let chosen: Vec<&diag::Declared> = match (&text, target) {
        (Some(t), _) => diag::codes_in(t, &codes),
        (None, "*") => codes.iter().collect(),
        (None, t) => codes.iter().filter(|d| d.code.as_deref() == Some(t) || f.package_of(&d.file) == Some(&t.replace('-', "_"))).collect(),
    };
    let mut rows = Vec::new();
    let mut raised = 0;
    for d in &chosen {
        rows.push(site(
            f,
            "code",
            d.code.clone().unwrap_or_else(|| format!("{}{} (a constant)", d.prefix, d.number)),
            &d.file,
            d.line,
            None,
            Some(&d.variant),
            "declared",
            vec![("message", d.message.clone()), ("variant", d.variant.clone()), ("severity", d.severity.clone().unwrap_or_default()), ("declared_by", d.declared_by.to_string())],
        ));
        for p in &f.k.paths {
            let (Some(id), Some(line)) = (p.resolution.as_ref().and_then(canonical), p.location.line) else { continue };
            if id == d.variant {
                raised += 1;
                rows.push(site(f, "code-raised", d.code.clone().unwrap_or_else(|| d.variant.clone()), &p.location.path, line, p.location.column, p.enclosing_item.as_deref(), "raises it", vec![("variant", d.variant.clone())]));
            }
        }
    }
    let unresolved: Vec<CodeQueryResult> = unread.iter().map(|(file, line, why)| site(f, "code-table-unread", why.clone(), file, *line, None, None, "a table klepto saw but custodian could not read", vec![])).collect();
    let tables: BTreeSet<(&std::path::PathBuf, &str)> = codes.iter().map(|d| (&d.file, d.prefix.as_str())).collect();
    let mut ans = Answer::default();
    ans.count("code tables", tables.len());
    ans.count("codes declared", codes.len());
    ans.count("codes in this reading", chosen.len());
    ans.count("sites raising them", raised);
    ans.count("tables not read", unresolved.len());
    ans.rows = rows;
    ans.unresolved = unresolved;
    Ok(ans)
}

// ---------------------------------------------------------------------------
// bundle

/// A route as one bundle, made by `crate::bundle` (the one redaction, the
/// one marker): a row per file it holds, with the windows held at secret or
/// above, and a row per file the filters left out. The bundle's bytes ride on
/// the reading and are written beside its page.
fn bundle_rows(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    use crate::bundle::FiltersWord;
    let target = spec.target.trim();
    // a bare crate name or file is spelled as its route
    let route = if target.contains(':') {
        target.to_string()
    } else if let Some(p) = f.packages.get(&target.replace('-', "_")) {
        format!("crate:{}", p.name)
    } else {
        format!("path:{target}")
    };
    let mut word = if spec.bundle.filters.is_empty() { FiltersWord::DEFAULT } else { FiltersWord::from_names(spec.bundle.filters.iter().map(String::as_str)).map_err(crate::CustodianError::from)?.with_filters_redact_secret() };
    if std::env::var_os(REVEAL_ENV).is_some() {
        word = word.reveal_explicitly();
    }
    let no_features: Vec<&str> = spec.bundle.no_features.iter().map(String::as_str).collect();
    let (bytes, written) = crate::bundle::bundle_bytes(f, &route, word.check().map_err(crate::CustodianError::from)?, &no_features).map_err(crate::CustodianError::from)?;
    let mut rows = Vec::new();
    for rel in &written.included {
        let held: Vec<String> = written
            .redacted
            .iter()
            .filter(|r| &r.path == rel)
            .map(|r| match r.marker.span {
                bundles::redaction::Span::All => format!("all@{}", r.marker.level),
                bundles::redaction::Span::Lines { start, end } => format!("{start}-{end}@{}", r.marker.level),
            })
            .collect();
        let mut attrs = vec![("filters", word.names().join(", "))];
        if !held.is_empty() {
            attrs.push(("held", held.join(",")));
        }
        let mut r = named("bundled-file", rel.clone(), f.package_of_rel(rel).map(|p| p.name.clone()), "in the bundle", attrs);
        r.path = Some(rel.clone());
        rows.push(r);
    }
    for rel in &written.dropped {
        let mut r = named("dropped-file", rel.clone(), f.package_of_rel(rel).map(|p| p.name.clone()), "left out by the filters", vec![("filters", word.names().join(", "))]);
        r.path = Some(rel.clone());
        rows.push(r);
    }
    let mut ans = Answer::default();
    ans.count("files bundled", written.included.len());
    ans.count("windows held redacted (secret or above)", written.redacted.len());
    ans.count("files left out by the filters", written.dropped.len());
    ans.count("bundle bytes", bytes.len());
    ans.rows = rows;
    ans.artifact = Some(bytes);
    Ok(ans)
}
