//! The lock face: every package the lock holds in more than one version, read from `cargo tree --duplicates` as cargo
//! prints it (`--prefix depth`: each version a tree of its own, level 1 the packages whose requirement pulls it in).
//! Nothing is recomputed: the versions and who pulls each in are cargo tree's; the requirement each puller wrote is
//! cargo metadata's; the line it is written on is the manifest's. Which single requirement change would leave one
//! version is read off those (one requirement forces the other versions), and is marked inferred: cargo has to
//! resolve again to show it. When cargo tree is refused, Cargo.lock's text says which packages it holds twice, and
//! who forces them is unknown.

use super::manifest::Scan;
use super::{finding, Basis, Check, Duplicate, DuplicateVersion, Finding, Forcing, Place, Ran, ReadFrom, Status};
use crate::exec::{self, Cargo};
use klepto::CargoWorkspace;
use std::collections::BTreeMap;
use std::path::Path;

pub struct LockFace {
    pub duplicates: Vec<Duplicate>,
    pub findings: Vec<Finding>,
}

/// One line of `cargo tree --prefix depth`: its depth, the package, its version, what follows (a path, `(*)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeLine {
    pub depth: usize,
    pub name: String,
    pub version: String,
    pub rest: String,
}

/// `2serde v1.0.200 (*)` → depth 2, `serde`, `1.0.200`, `(*)`.
pub fn parse_line(l: &str) -> Option<TreeLine> {
    let digits: String = l.chars().take_while(char::is_ascii_digit).collect();
    let depth = digits.parse().ok()?;
    let mut words = l[digits.len()..].split_whitespace();
    let name = words.next()?.to_string();
    let version = words.next()?.strip_prefix('v')?.to_string();
    Some(TreeLine { depth, name, version, rest: words.collect::<Vec<_>>().join(" ") })
}

/// cargo tree -d's output as (package, version) trees, in its order.
pub fn parse_tree(out: &str) -> Vec<(TreeLine, Vec<TreeLine>)> {
    let mut groups: Vec<(TreeLine, Vec<TreeLine>)> = Vec::new();
    for l in out.lines().filter_map(parse_line) {
        if l.depth == 0 {
            groups.push((l, Vec::new()));
        } else if let Some(g) = groups.last_mut() {
            g.1.push(l);
        }
    }
    groups
}

/// The requirement a puller wrote for `name` at `version`, from cargo metadata, and where it is written.
fn forcing(scan: &Scan, ws: Option<&CargoWorkspace>, puller: &TreeLine, name: &str, version: &str) -> Forcing {
    let label = format!("{} {}", puller.name, puller.version);
    let path_hint = path_of(&puller.rest);
    let Some(w) = ws else { return Forcing { package: label, requirement: None, local: path_hint.is_some(), at: None } };
    let pkg = w.packages.values().find(|p| p.name == puller.name && p.version == puller.version && path_hint.as_deref().is_none_or(|h| p.manifest_path.parent() == Some(h)));
    match pkg {
        Some(p) => forcing_of(scan, w, p, name, version),
        None => Forcing { package: label, requirement: None, local: path_hint.is_some(), at: None },
    }
}

/// A package's requirement on `name` at `version` (cargo metadata's), and, for a path package, the line it is
/// written on (the root's [workspace.dependencies] for an inherited one).
fn forcing_of(scan: &Scan, w: &CargoWorkspace, p: &klepto::CargoPackage, name: &str, version: &str) -> Forcing {
    let label = format!("{} {}", p.name, p.version);
    let decl = p.dependencies.iter().find(|d| d.dep.package == name && klepto::version_meets(&d.req, version) != Some(false));
    let local = p.source.is_none();
    let at = decl.filter(|_| local).and_then(|d| {
        let m = scan.manifests.get(&p.manifest_path)?;
        let entry = m.deps.iter().find(|x| x.package_name() == name || x.key.replace('-', "_") == d.dep.name)?;
        if entry.workspace {
            let (r, x) = scan.inherited(&p.manifest_path, entry)?;
            return Some(Place { file: scan.rel(&r.path), line: Some(x.line).filter(|l| *l > 0), section: x.section.clone(), field: x.key.clone() });
        }
        Some(Place { file: scan.rel(&m.path), line: Some(entry.line).filter(|l| *l > 0), section: entry.section.clone(), field: entry.key.clone() })
    });
    let _ = w;
    Forcing { package: label, requirement: decl.map(|d| d.req.clone()), local, at }
}

/// The path in what follows a package in cargo tree (`(/abs/dir) (*)`), when it is one.
fn path_of(rest: &str) -> Option<std::path::PathBuf> {
    rest.strip_prefix('(').and_then(|r| r.split(')').next()).filter(|h| Path::new(h).is_absolute()).map(std::path::PathBuf::from)
}

/// Where a requirement is written: its file and line for a path package's own, else the puller (a registry
/// package's requirement is its own, one each).
fn place_key(f: &Forcing) -> String {
    match (&f.at, f.local) {
        (Some(p), true) => format!("{}:{}", p.file, p.line.unwrap_or(0)),
        _ => f.package.clone(),
    }
}

/// Which one requirement change would leave one version: for each version kept (newest first), the places the
/// requirements forcing the others are written; one place in a manifest of this workspace is the answer (an
/// inherited requirement is one line in the root, however many members inherit it).
fn collapse(name: &str, versions: &[DuplicateVersion]) -> String {
    if versions.len() == 1 {
        let v = &versions[0];
        return format!("one version, {} builds: cargo builds {name} {} more than once (another feature set, or the host beside the target; cargo tree -d does not say which): no requirement change collapses it", v.builds, v.version);
    }
    let mut fallback: Option<String> = None;
    for (k, keep) in versions.iter().enumerate().rev() {
        let others: Vec<&Forcing> = versions.iter().enumerate().filter(|(i, _)| *i != k).flat_map(|(_, v)| v.forced_by.iter()).collect();
        let mut places: Vec<String> = others.iter().map(|f| place_key(f)).collect();
        places.sort();
        places.dedup();
        if places.len() != 1 || others.is_empty() {
            continue;
        }
        let one = others[0];
        let by: Vec<&str> = {
            let mut v: Vec<&str> = others.iter().map(|f| f.package.as_str()).collect();
            v.sort();
            v.dedup();
            v
        };
        let label = format!("{} {}{}", name, keep.version, keep.source.as_ref().map(|s| format!(" ({s})")).unwrap_or_default());
        if one.local {
            let at = one.at.as_ref().map(|p| format!(" ({}{})", p.file, p.line.map(|l| format!(":{l}")).unwrap_or_default())).unwrap_or_default();
            let whose = if by.len() == 1 { format!("{}'s requirement", by[0]) } else { format!("the requirement {} inherit", super::shorten(&by.iter().map(|s| s.to_string()).collect::<Vec<_>>(), 6)) };
            return format!(
                "one change: {whose} {}{at} admitting {label} would leave one version (inferred: cargo tree -d after the change is the proof; whether the code builds against {} is the compiler's to say)",
                one.requirement.as_ref().map(|r| format!("`{name} {r}`")).unwrap_or_else(|| format!("on `{name}`")),
                keep.version
            );
        }
        if fallback.is_none() {
            fallback = Some(format!("no change in this workspace: the one requirement forcing the other version(s) is {}'s{} (not a manifest here); keeping {label} needs a release of it that admits it", one.package, one.requirement.as_ref().map(|r| format!(" `{r}`")).unwrap_or_default()));
        }
    }
    fallback.unwrap_or_else(|| {
        let mut places: Vec<String> = versions.iter().flat_map(|v| v.forced_by.iter()).map(place_key).collect();
        places.sort();
        places.dedup();
        format!("no single requirement change: requirements written in {} places force {} versions apart", places.len(), versions.len())
    })
}

/// The face over the workspace `manifest` belongs to (one crate's tree when the target is a crate).
pub fn read(root: &Path, manifest: &Path, scan: &Scan, ws: Option<&CargoWorkspace>, ran: &mut Vec<Ran>) -> LockFace {
    let dir = manifest.parent().unwrap_or(root);
    let mut line = Cargo::tree(manifest.as_os_str()).duplicates().locked().offline().prefix("depth");
    line = match &scan.target {
        super::manifest::Target::Crate(name) => line.package(name.as_str()),
        _ => line.workspace(),
    };
    let walked = super::cargo::run("cargo-tree-duplicates", exec::line("tree", &line), dir, root, ran);
    let mut findings = Vec::new();
    let Some(w) = walked.filter(|w| w.code == Some(0)) else {
        let why = ran.last().map(|r| r.said.clone()).unwrap_or_default();
        let lock = scan.lock_of(manifest);
        let from_lock = lock_text_duplicates(&lock);
        findings.push(finding(Check::TreeRefused, Status::Informational, Basis::Unknown, ReadFrom::Tree, None, None, format!("cargo tree refused: {why}"), "-", format!("cargo's words: {why}"), format!("{} package(s) appear more than once in {} (its text); who forces them is unknown until cargo tree answers", from_lock.len(), scan.rel(&lock)), None));
        return LockFace { duplicates: from_lock, findings };
    };
    // every unit cargo tree printed, by package: its version and source, its pullers (level 1), whether it was a
    // repeat (`(*)`: printed in full elsewhere)
    struct Unit {
        version: String,
        source: Option<String>,
        pullers: Vec<TreeLine>,
        through: usize,
    }
    let mut by: BTreeMap<String, Vec<Unit>> = BTreeMap::new();
    for (top, below) in parse_tree(&w.stdout) {
        let source = path_of(&top.rest).map(|p| scan.rel(&p)).or_else(|| top.rest.strip_prefix('(').and_then(|r| r.split(')').next()).filter(|s| *s != "*" && *s != "proc-macro").map(str::to_string));
        by.entry(top.name.clone()).or_default().push(Unit { version: top.version.clone(), source, pullers: below.iter().filter(|l| l.depth == 1).cloned().collect(), through: below.iter().filter(|l| l.depth > 1).count() });
    }
    let mut duplicates = Vec::new();
    for (name, units) in by {
        let mut versions: Vec<DuplicateVersion> = Vec::new();
        for u in &units {
            let at = match versions.iter().position(|v| v.version == u.version && v.source == u.source) {
                Some(i) => i,
                None => {
                    versions.push(DuplicateVersion { version: u.version.clone(), source: u.source.clone(), builds: 0, pullers_from_metadata: false, forced_by: Vec::new(), through: 0 });
                    versions.len() - 1
                }
            };
            // every root cargo tree -d prints is a node of its own (a version, a feature set, the host beside the
            // target); `(*)` only says its pullers were printed before
            let v = &mut versions[at];
            v.builds += 1;
            v.through += u.through;
            for p in &u.pullers {
                let f = forcing(scan, ws, p, &name, &u.version);
                if !v.forced_by.iter().any(|x| x.package == f.package && x.requirement == f.requirement) {
                    v.forced_by.push(f);
                }
            }
        }
        // a unit cargo tree printed as a repeat (`(*)`, its pullers shown before): its pullers are cargo metadata's
        // resolve's
        for v in versions.iter_mut() {
            if v.forced_by.is_empty() {
                if let Some(w) = ws {
                    for p in w.packages.values().filter(|p| p.name == name && p.version == v.version) {
                        let (direct, _) = w.dependents(&p.id);
                        for d in direct.iter().filter_map(|id| w.packages.get(id)) {
                            let f = forcing_of(scan, w, d, &name, &v.version);
                            if !v.forced_by.iter().any(|x| x.package == f.package) {
                                v.forced_by.push(f);
                            }
                        }
                    }
                    v.pullers_from_metadata = !v.forced_by.is_empty();
                }
            }
        }
        versions.sort_by(|a, b| super::rust::version(&a.version).cmp(&super::rust::version(&b.version)).then(a.version.cmp(&b.version)).then(a.source.cmp(&b.source)));
        let collapse = collapse(&name, &versions);
        let compiles: usize = versions.iter().map(|v| v.builds).sum();
        let d = Duplicate { compiles, basis: Basis::Observed, collapse, name: name.clone(), versions };
        let evidence: Vec<String> = d.versions.iter().map(|v| format!("{}{} ×{} ← {}", v.version, v.source.as_ref().map(|s| format!(" ({s})")).unwrap_or_default(), v.builds, v.forced_by.iter().map(|f| format!("{}{}", f.package, f.requirement.as_ref().map(|r| format!(" ({r})")).unwrap_or_default())).collect::<Vec<_>>().join(", "))).collect();
        let at = d.versions.iter().flat_map(|v| v.forced_by.iter()).find_map(|f| f.at.clone());
        let what = if d.versions.len() == 1 {
            format!("`{name}` {} is built {} times", d.versions[0].version, d.compiles)
        } else {
            format!("`{name}` is in the lock {} times: {}", d.versions.len(), d.versions.iter().map(|v| v.version.as_str()).collect::<Vec<_>>().join(", "))
        };
        findings.push(finding(Check::DuplicateVersions, Status::Informational, Basis::Observed, ReadFrom::Tree, None, at, what, "-", evidence.join("; "), format!("it compiles {} times on this host; {}", d.compiles, d.collapse), None));
        duplicates.push(d);
    }
    LockFace { duplicates, findings }
}

/// Packages Cargo.lock lists under one name more than once, read as TOML (the versions only: who forces them is cargo
/// tree's to say).
pub fn lock_text_duplicates(lock: &Path) -> Vec<Duplicate> {
    let Some(t) = std::fs::read_to_string(lock).ok().and_then(|t| t.parse::<toml::Table>().ok()) else { return Vec::new() };
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in t.get("package").and_then(|p| p.as_array()).into_iter().flatten() {
        if let (Some(n), Some(v)) = (p.get("name").and_then(|x| x.as_str()), p.get("version").and_then(|x| x.as_str())) {
            by.entry(n.to_string()).or_default().push(v.to_string());
        }
    }
    by.into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(name, versions)| Duplicate {
            compiles: versions.len(),
            versions: versions.into_iter().map(|version| DuplicateVersion { version, source: None, builds: 1, pullers_from_metadata: false, forced_by: Vec::new(), through: 0 }).collect(),
            collapse: "unknown: cargo tree did not answer, and who forces each version is its to say".into(),
            basis: Basis::Observed,
            name,
        })
        .collect()
}
