//! The code graph (code_passenger.graph.v1), built from klepto: the crates
//! and their manifests, features and dependencies from cargo metadata and
//! klepto's feature model; files, functions, imports and use sites from
//! klepto's parsed and resolved occurrences. Nothing here reads Rust source
//! itself.

#![allow(unused_imports)]
use crate::*;
use klepto::{FeatureEnable, FeatureModel, Klepto, KleptoError};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// What one klepto scan reads: every member's `src/` (and `tests/` when
/// asked), minus `target/`, `.claude/` and the globs given, all relative to
/// the root.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanOptions {
    pub exclude: Vec<String>,
    pub tests: bool,
    pub dependency_macros: bool,
}

impl ScanOptions {
    /// What the graph commands read: `src/` of every member, dependency
    /// macros expanded.
    pub fn graph() -> Self {
        Self { exclude: Vec::new(), tests: false, dependency_macros: true }
    }
}

/// The one klepto scan every command and every reading starts from.
pub fn klepto_scan(root: &Path, opts: &ScanOptions) -> std::result::Result<Klepto, KleptoError> {
    klepto_builder(root, opts)?.parse()
}

/// The code digest: the canonical form of klepto's fingerprint (every file
/// the scan reads, relative to the root, with its content digest), digested.
/// The root is read as the file system resolves it, so every spelling of one directory (`.`, `/var/...` against
/// `/private/var/...`, a symlink) gives the same digest: a preflight and the writer that checks it agree.
pub fn code_digest(root: &Path, opts: &ScanOptions) -> std::result::Result<String, KleptoError> {
    let root = &root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let fp = klepto_builder(root, opts)?.fingerprint()?;
    let manifest: Vec<(String, &String)> = fp.files.iter().map(|(p, h)| (relative_display(root, p), h)).collect();
    Ok(crate::digest::of_value(&manifest))
}

fn finding(severity: &str, code: &str, message: String, path: Option<String>) -> FindingNode {
    FindingNode { severity: severity.to_string(), code: code.to_string(), message, path, line: None, column: None }
}

pub fn build_code_graph(root: impl AsRef<Path>, max_depth: usize) -> Result<CodeGraph> {
    let root = root.as_ref();
    let root_exists = root.exists();
    let mut graph = CodeGraph {
        schema: "code_passenger.graph.v1".to_string(),
        root: display_path(root),
        root_exists,
        max_depth,
        crates: Vec::new(),
        files: Vec::new(),
        features: Vec::new(),
        deps: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        use_sites: Vec::new(),
        findings: Vec::new(),
        summary: CodeGraphSummary::default(),
    };
    if !root_exists {
        graph.findings.push(finding("warn", "CPG001", "root path does not exist; graph is intentionally empty".into(), Some(display_path(root))));
        finish_summary(&mut graph);
        return Ok(graph);
    }
    if !root.join("Cargo.toml").is_file() {
        graph.findings.push(finding("info", "CPG002", "no Cargo.toml manifest at the root".into(), Some(display_path(root))));
        finish_summary(&mut graph);
        return Ok(graph);
    }
    match klepto_scan(root, &ScanOptions::graph()) {
        Ok(k) => fill_graph(&mut graph, &k),
        Err(e) => graph.findings.push(finding("warn", "CPG003", format!("klepto could not scan the workspace: {e}"), Some(display_path(&root.join("Cargo.toml"))))),
    }
    finish_summary(&mut graph);
    Ok(graph)
}

/// The code graph of a scan already made.
pub fn graph_from_klepto(root: &Path, k: &Klepto) -> CodeGraph {
    let mut graph = CodeGraph {
        schema: "code_passenger.graph.v1".to_string(),
        root: display_path(root),
        root_exists: true,
        max_depth: 0,
        crates: Vec::new(),
        files: Vec::new(),
        features: Vec::new(),
        deps: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        use_sites: Vec::new(),
        findings: Vec::new(),
        summary: CodeGraphSummary::default(),
    };
    fill_graph(&mut graph, k);
    finish_summary(&mut graph);
    graph
}

/// The scan's builder: what `klepto_scan` parses and what a fingerprint
/// hashes, one file selection for both.
pub fn klepto_builder(root: &Path, opts: &ScanOptions) -> std::result::Result<klepto::KleptoBuilder, KleptoError> {
    let name = root.file_name().map(|n| n.to_string_lossy().replace('-', "_")).unwrap_or_else(|| "workspace".into());
    let under_root = |g: &str| format!("{}/{}", globset::escape(&root.to_string_lossy()), g.trim_start_matches("./"));
    let mut b = Klepto::new(name).scan_workspace_root(root)?.exclude_generated()?;
    b = b.exclude_glob(&under_root(".claude/**"))?;
    b = b.exclude_glob(&under_root(&format!("{}/**", crate::store::FOLDER)))?;
    for g in &opts.exclude {
        b = b.exclude_glob(&under_root(g))?;
    }
    Ok(b.include_tests(opts.tests).skip_dependency_macros(!opts.dependency_macros).ignore_parse_errors(true))
}

/// The package each scanned file belongs to (klepto's module tree).
pub(crate) fn file_packages(k: &Klepto) -> HashMap<PathBuf, String> {
    k.files
        .iter()
        .filter_map(|f| {
            let m = f.module.as_deref()?;
            Some((f.path.clone(), k.modules.get(m)?.package.clone()))
        })
        .collect()
}

/// `dep:x` / `x` / `x?/f` as Cargo spells a feature's members.
fn enable_text(e: &FeatureEnable) -> String {
    match e {
        FeatureEnable::Feature(f) => f.clone(),
        FeatureEnable::Dep(d) => format!("dep:{d}"),
        FeatureEnable::DepFeature { dep, feature, weak } => format!("{dep}{}/{feature}", if *weak { "?" } else { "" }),
    }
}

/// The optional dependencies a feature names directly (`dep:x`, `x/f`, or an
/// optional dependency's implicit feature `x`).
fn feature_enables_deps(model: &FeatureModel, enables: &[FeatureEnable]) -> Vec<String> {
    let optional: BTreeSet<&str> = model.dependencies.iter().filter(|d| d.optional).map(|d| d.name.as_str()).collect();
    let mut out = BTreeSet::new();
    for e in enables {
        let d = match e {
            FeatureEnable::Dep(d) => d.as_str(),
            FeatureEnable::DepFeature { dep, .. } => dep.as_str(),
            FeatureEnable::Feature(f) => f.as_str(),
        };
        let d = norm_dep(d);
        if optional.contains(d.as_str()) {
            out.insert(d);
        }
    }
    out.into_iter().collect()
}

/// Dependency → the declared features that name it directly (implicit
/// features are not declared; the regex scan never listed them either).
fn features_naming(model: &FeatureModel) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, info) in model.features.iter().filter(|(_, i)| !i.implicit) {
        for d in feature_enables_deps(model, &info.enables) {
            out.entry(d).or_default().push(name.clone());
        }
    }
    out
}

fn source_table(d: &klepto::DepInfo) -> String {
    let table = match d.kind {
        klepto::DepKind::Normal => "dependencies",
        klepto::DepKind::Dev => "dev-dependencies",
        klepto::DepKind::Build => "build-dependencies",
    };
    if d.target.is_some() { format!("target.{table}") } else { table.to_string() }
}

fn origin(o: Option<&klepto::ImportOrigin>) -> ImportOrigin {
    match o {
        Some(klepto::ImportOrigin::Internal) => ImportOrigin::Internal,
        Some(klepto::ImportOrigin::Std) => ImportOrigin::Std,
        Some(klepto::ImportOrigin::Core) => ImportOrigin::Core,
        Some(klepto::ImportOrigin::Alloc) => ImportOrigin::Alloc,
        Some(klepto::ImportOrigin::WorkspaceMember) => ImportOrigin::WorkspaceMember,
        Some(klepto::ImportOrigin::Dependency) => ImportOrigin::Dependency,
        Some(klepto::ImportOrigin::UnknownExternal) | None => ImportOrigin::UnknownExternal,
    }
}

/// `fn <fq_name>` when the location is inside a function, else `file`.
fn scope_of(k: &Klepto, loc: &klepto::FileLocation) -> String {
    k.index.enclosing(loc).map(|f| format!("fn {}", f.fq_name)).unwrap_or_else(|| "file".to_string())
}

fn fill_graph(graph: &mut CodeGraph, k: &Klepto) {
    // a file's cargo package: a bin's crate root belongs to its package
    let pkg_of: HashMap<PathBuf, String> = file_packages(k).into_iter().map(|(f, p)| (f, k.crate_roots.get(&p).cloned().unwrap_or(p))).collect();
    let mut crate_ids: BTreeMap<String, (String, String, PathBuf)> = BTreeMap::new(); // package → (id, cargo name, root)
    for p in k.packages.values() {
        let id = stable_id("crate", &display_path(&p.root));
        let src = p.root.join("src");
        graph.crates.push(CrateNode {
            id: id.clone(),
            name: p.cargo_name.clone(),
            crate_root: display_path(&p.root),
            manifest_path: display_path(&p.manifest_path),
            src_root: src.exists().then(|| display_path(&src)),
        });
        crate_ids.insert(p.name.clone(), (id, p.cargo_name.clone(), p.root.clone()));
    }
    let crate_of = |file: &Path| -> Option<&(String, String, PathBuf)> { pkg_of.get(file).and_then(|p| crate_ids.get(p)) };

    for (pkg, model) in &k.features {
        let Some((crate_id, crate_name, _)) = crate_ids.get(pkg) else { continue };
        let naming = features_naming(model);
        let mut deps: Vec<&klepto::DepInfo> = model.dependencies.iter().collect();
        deps.sort_by(|a, b| (&a.name, source_table(a)).cmp(&(&b.name, source_table(b))));
        deps.dedup_by(|a, b| a.name == b.name && source_table(a) == source_table(b));
        for d in deps {
            graph.deps.push(DependencyNode {
                id: stable_id("dep", &format!("{crate_name}::{}", d.name)),
                crate_id: crate_id.clone(),
                crate_name: crate_name.clone(),
                name: d.name.clone(),
                cargo_key: if norm_dep(&d.package) == d.name { d.package.clone() } else { d.name.clone() },
                optional: d.optional,
                source_table: source_table(d),
                enabled_by_features: naming.get(&d.name).cloned().unwrap_or_default(),
            });
        }
        for (name, info) in model.features.iter().filter(|(_, i)| !i.implicit) {
            graph.features.push(FeatureNode {
                id: stable_id("feat", &format!("{crate_name}::{name}")),
                crate_id: crate_id.clone(),
                crate_name: crate_name.clone(),
                name: name.clone(),
                members: info.enables.iter().map(enable_text).collect(),
                enables_deps: feature_enables_deps(model, &info.enables),
                forwards_dependency_features: info
                    .enables
                    .iter()
                    .filter_map(|e| match e {
                        FeatureEnable::DepFeature { dep, feature, .. } => Some(ForwardedDependencyFeature { dep: norm_dep(dep), feature: feature.clone(), raw: enable_text(e) }),
                        _ => None,
                    })
                    .collect(),
            });
        }
    }

    let mut files: Vec<&klepto::ParsedFile> = k.files.iter().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    for f in files {
        let Some((crate_id, crate_name, croot)) = crate_of(&f.path) else { continue };
        let rel = relative_display(croot, &f.path);
        graph.files.push(FileNode {
            id: stable_id("file", &format!("{crate_name}::{rel}")),
            crate_id: crate_id.clone(),
            crate_name: crate_name.clone(),
            path: display_path(&f.path),
            relative_path: rel,
            line_count: f.source.lines().count(),
            byte_count: f.source.len(),
        });
    }

    for func in &k.functions {
        let Some((crate_id, crate_name, _)) = crate_of(&func.location.path) else { continue };
        graph.functions.push(FunctionNode {
            id: stable_id("fn", &format!("{crate_name}::{}::{}", display_path(&func.location.path), func.fq_name)),
            crate_id: crate_id.clone(),
            crate_name: crate_name.clone(),
            path: display_path(&func.location.path),
            name: func.name.clone(),
            line: func.location.line.unwrap_or(0),
            column: func.location.column.unwrap_or(0),
            visibility: if func.is_public { "pub".to_string() } else { "private".to_string() },
            signature_preview: func.signature.clone(),
        });
    }

    for imp in &k.imports {
        let Some((crate_id, crate_name, _)) = crate_of(&imp.location.path) else { continue };
        let line = imp.location.line.unwrap_or(0);
        graph.imports.push(ImportNode {
            id: stable_id("import", &format!("{crate_name}::{}::{line}::{}", display_path(&imp.location.path), imp.full_path)),
            crate_id: crate_id.clone(),
            crate_name: crate_name.clone(),
            path: display_path(&imp.location.path),
            root: norm_dep(&imp.root),
            full_path: imp.full_path.clone(),
            line,
            column: imp.location.column.unwrap_or(0),
            is_public_use: imp.is_public_use,
            origin: origin(imp.origin.as_ref()),
        });
    }

    let naming: BTreeMap<&String, BTreeMap<String, Vec<String>>> = k.features.iter().map(|(p, m)| (p, features_naming(m))).collect();
    let all_deps: BTreeSet<String> = k.features.values().flat_map(|m| m.dependencies.iter().map(|d| d.name.clone())).collect();
    let sites = k.dep_use_sites(&all_deps).into_iter().map(|s| (s, false)).chain(k.internal_use_sites().into_iter().map(|s| (s, true)));
    for (site, internal) in sites {
        let Some((crate_id, crate_name, _)) = crate_of(&site.location.path) else { continue };
        let model = pkg_of.get(&site.location.path).and_then(|p| k.features.get(p));
        let (Some(line), col) = (site.location.line, site.location.column.unwrap_or(0)) else { continue };
        let kind = match (internal, site.kind) {
            (true, _) => UseSiteKind::InternalPath,
            (_, klepto::UseSiteKind::UseStmt | klepto::UseSiteKind::ExternCrate) => UseSiteKind::UseStmt,
            (_, klepto::UseSiteKind::MacroCall) => UseSiteKind::MacroCall,
            (_, klepto::UseSiteKind::Path | klepto::UseSiteKind::Attribute) => UseSiteKind::Path,
        };
        graph.use_sites.push(UseSiteNode {
            id: stable_id("site", &format!("{crate_name}::{}::{line}::{col}::{}", display_path(&site.location.path), site.path)),
            crate_id: crate_id.clone(),
            crate_name: crate_name.clone(),
            path: display_path(&site.location.path),
            dep: site.dep.clone(),
            full_path: site.path.clone(),
            head: if site.head.is_empty() { "*".to_string() } else { site.head.clone() },
            kind,
            line,
            column: col,
            scope: scope_of(k, &site.location),
            feature_candidates: pkg_of.get(&site.location.path).and_then(|p| naming.get(p)).and_then(|n| n.get(&site.dep)).cloned().unwrap_or_default(),
            optional_dep: model.and_then(|m| m.dep(&site.dep)).is_some_and(|d| d.optional),
        });
    }
    let mut seen = BTreeSet::new();
    graph.use_sites.retain(|s| seen.insert((s.path.clone(), s.line, s.column, s.full_path.clone(), format!("{:?}", s.kind))));
    graph.use_sites.sort_by(|a, b| (&a.path, a.line, a.column, &a.full_path).cmp(&(&b.path, b.line, b.column, &b.full_path)));

    for (p, e) in &k.skipped_files {
        graph.findings.push(finding("warn", "CPG004", format!("klepto could not parse the file: {e}"), Some(display_path(p))));
    }
    if let Some(n) = &k.dep_macro_note {
        graph.findings.push(finding("info", "CPG005", n.clone(), None));
    }
}

pub(crate) fn finish_summary(graph: &mut CodeGraph) {
    graph.summary = CodeGraphSummary {
        crate_count: graph.crates.len(),
        file_count: graph.files.len(),
        feature_count: graph.features.len(),
        dependency_count: graph.deps.len(),
        optional_dependency_count: graph.deps.iter().filter(|d| d.optional).count(),
        function_count: graph.functions.len(),
        import_count: graph.imports.len(),
        use_site_count: graph.use_sites.len(),
        finding_count: graph.findings.len(),
    };
}

pub(crate) fn norm_dep(s: &str) -> String {
    s.trim().replace('-', "_")
}

pub(crate) fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn relative_display(base: &Path, path: &Path) -> String {
    path.strip_prefix(base).map(display_path).unwrap_or_else(|_| display_path(path))
}

/// A stable region id: the prefix and the first 16 hex of the raw text's digest.
pub(crate) fn stable_id(prefix: &str, raw: &str) -> String {
    format!("{prefix}_{}", crate::digest::short(&crate::digest::of(raw), 16))
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out = s.chars().take(max.saturating_sub(3)).collect::<String>();
    out.push_str("...");
    out
}
