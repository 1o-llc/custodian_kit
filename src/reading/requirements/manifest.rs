//! The manifest scanner: every Cargo.toml the reading needs, read as TOML with toml_edit (the spans give each key its
//! line), no cargo. It exists for the state cargo cannot speak in yet (a broken manifest), and every finding it makes
//! is the scanner's, marked so; once `cargo metadata` reads the workspace, cargo's word stands over it.
//!
//! Cargo's own rules, as the scanner follows them:
//!
//! - A crate's **governing root** is the crate itself when its manifest has `[workspace]`; else the root
//!   `package.workspace` names; else the nearest ancestor directory whose Cargo.toml has `[workspace]` and does not
//!   exclude the crate (an `exclude` entry that is a prefix of its path, unless a `members` entry is too). Never the
//!   top folder by default: a nested independent workspace is its own root.
//! - A root's **members** are its `members` entries (globs expanded over the directories that hold a Cargo.toml) and,
//!   implicitly, every path dependency of a member that sits in the root's directory and is not excluded.
//! - A crate whose governing root does not hold it is refused ("believes it's in a workspace when it's not").
//! - `workspace = true` reads the governing root's `[workspace.dependencies]` (a path there is relative to the root),
//!   `field.workspace = true` its `[workspace.package]`, `[lints] workspace = true` its `[workspace.lints]`.
//! - A path dependency's directory must hold a package of the dependency's name (exact, `-` and `_` apart), at a
//!   version its requirement admits.

use super::{finding, rel, Basis, Check, Finding, Place, ReadFrom, RequirementsOptions, Root, State, Status};
use crate::reading::spec::Scope;
use crate::reading::refusal::ReadingRefusal;
use crate::Result;
use klepto::{CfgPredicate, DepInfo, DepKind, FeatureEnable, FeatureModel};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Component, Path, PathBuf};
use toml_edit::{ImDocument, Item, TableLike, Value};

/// A `[package]` field: absent, written, or inherited from the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Absent,
    Written(String, u32),
    Inherited(u32),
}

impl Field {
    pub fn line(&self) -> Option<u32> {
        match self {
            Field::Absent => None,
            Field::Written(_, l) | Field::Inherited(l) => Some(*l),
        }
    }
}

/// A manifest's `[package]`.
#[derive(Debug, Clone)]
pub struct Package {
    pub name: Option<String>,
    pub line: u32,
    pub version: Field,
    pub edition: Field,
    pub rust_version: Field,
    /// `package.workspace = "<dir>"`.
    pub workspace: Option<(String, u32)>,
    /// Every field written `field.workspace = true` (or `field = { workspace = true }`), with its line.
    pub inherited: Vec<(String, u32)>,
}

/// A manifest's `[workspace]`.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    pub line: u32,
    pub members: Vec<(String, u32)>,
    pub default_members: Vec<(String, u32)>,
    pub exclude: Vec<(String, u32)>,
    pub dependencies: Vec<Dep>,
    /// `[workspace.package]`: field → its value when a string, and its line.
    pub package: BTreeMap<String, (Option<String>, u32)>,
    pub lints: bool,
}

/// One dependency entry as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    /// The key: the name source uses.
    pub key: String,
    /// `package = "..."`.
    pub package: Option<String>,
    /// `dependencies`, `dev-dependencies`, `target.'cfg(unix)'.dependencies`, `workspace.dependencies`,
    /// `patch.crates-io`, `replace`.
    pub section: String,
    pub kind: DepKind,
    pub target: Option<String>,
    pub line: u32,
    pub version: Option<String>,
    pub path: Option<String>,
    pub git: Option<String>,
    pub registry: Option<String>,
    pub workspace: bool,
    pub optional: bool,
    pub default_features: Option<bool>,
    pub features: Vec<String>,
    /// Every key the entry writes.
    pub keys: Vec<String>,
}

impl Dep {
    /// The package it names: `package`, else the key.
    pub fn package_name(&self) -> &str {
        self.package.as_deref().unwrap_or(&self.key)
    }
}

/// One Cargo.toml, read.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub path: PathBuf,
    pub dir: PathBuf,
    pub text: String,
    /// Not TOML: toml_edit's words.
    pub error: Option<String>,
    pub package: Option<Package>,
    pub workspace: Option<Workspace>,
    pub deps: Vec<Dep>,
    /// `[patch.<source>]` and `[replace]` entries.
    pub patches: Vec<Dep>,
    /// `[features]`: name, entries, line.
    pub features: Vec<(String, Vec<String>, u32)>,
    pub lints_workspace: Option<u32>,
    pub has_build_rs: bool,
    pub links: Option<String>,
}

impl Manifest {
    pub fn name(&self) -> Option<String> {
        self.package.as_ref().and_then(|p| p.name.clone())
    }

    pub fn is_root(&self) -> bool {
        self.workspace.is_some()
    }
}

fn line_of(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

fn str_of(i: &Item) -> Option<String> {
    i.as_str().map(str::to_string)
}

fn strings_of(i: &Item) -> Vec<String> {
    i.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn value_strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// The line a key of a table is written on: its key's span, else its value's.
fn key_line(text: &str, t: &dyn TableLike, k: &str) -> u32 {
    t.get_key_value(k).and_then(|(key, item)| key.span().or_else(|| item.span()).or_else(|| item.as_table().and_then(|t| t.span()))).map(|s| line_of(text, s.start)).unwrap_or(0)
}

/// A dependency entry: `"1.0"`, `{ version = .., ... }` or a `[dependencies.x]` table.
fn dep_of(text: &str, section: &str, kind: DepKind, target: Option<&str>, t: &dyn TableLike, key: &str, item: &Item) -> Dep {
    let mut d = Dep {
        key: key.to_string(),
        package: None,
        section: section.to_string(),
        kind,
        target: target.map(str::to_string),
        line: key_line(text, t, key),
        version: None,
        path: None,
        git: None,
        registry: None,
        workspace: false,
        optional: false,
        default_features: None,
        features: Vec::new(),
        keys: Vec::new(),
    };
    if let Some(v) = item.as_str() {
        d.version = Some(v.to_string());
        d.keys.push("version".into());
        return d;
    }
    let Some(e) = item.as_table_like() else { return d };
    for (k, v) in e.iter() {
        d.keys.push(k.to_string());
        match k {
            "version" => d.version = str_of(v),
            "package" => d.package = str_of(v),
            "path" => d.path = str_of(v),
            "git" => d.git = str_of(v),
            "registry" => d.registry = str_of(v),
            "workspace" => d.workspace = v.as_bool().unwrap_or(false),
            "optional" => d.optional = v.as_bool().unwrap_or(false),
            "default-features" | "default_features" => d.default_features = v.as_bool(),
            "features" => d.features = strings_of(v),
            _ => {}
        }
    }
    d
}

const DEP_TABLES: &[(&str, DepKind)] = &[
    ("dependencies", DepKind::Normal),
    ("dev-dependencies", DepKind::Dev),
    ("dev_dependencies", DepKind::Dev),
    ("build-dependencies", DepKind::Build),
    ("build_dependencies", DepKind::Build),
];

fn deps_in(text: &str, t: &dyn TableLike, prefix: &str, target: Option<&str>, out: &mut Vec<Dep>) {
    for (name, kind) in DEP_TABLES {
        if let Some(tab) = t.get(name).and_then(Item::as_table_like) {
            let section = format!("{prefix}{name}");
            for (k, v) in tab.iter() {
                out.push(dep_of(text, &section, *kind, target, tab, k, v));
            }
        }
    }
}

/// Reads one manifest; a file that is not TOML is kept with its error.
pub fn read_manifest(path: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(path).ok()?;
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut m = Manifest {
        path: path.to_path_buf(),
        has_build_rs: dir.join("build.rs").is_file(),
        dir,
        text: text.clone(),
        error: None,
        package: None,
        workspace: None,
        deps: Vec::new(),
        patches: Vec::new(),
        features: Vec::new(),
        lints_workspace: None,
        links: None,
    };
    let doc = match ImDocument::parse(text.clone()) {
        Ok(d) => d,
        Err(e) => {
            m.error = Some(e.to_string().trim().to_string());
            return Some(m);
        }
    };
    let top = doc.as_table();
    if let Some(p) = top.get("package").and_then(Item::as_table_like) {
        let line = top.get("package").and_then(|i| i.as_table().and_then(|t| t.span()).or_else(|| i.span())).map(|s| line_of(&text, s.start)).unwrap_or(1);
        let mut pkg = Package { name: p.get("name").and_then(str_of), line, version: Field::Absent, edition: Field::Absent, rust_version: Field::Absent, workspace: None, inherited: Vec::new() };
        for (k, v) in p.iter() {
            let l = key_line(&text, p, k);
            let inherits = v.as_table_like().and_then(|t| t.get("workspace")).and_then(Item::as_bool) == Some(true);
            if inherits {
                pkg.inherited.push((k.to_string(), l));
            }
            let field = if inherits { Field::Inherited(l) } else { v.as_str().map(|s| Field::Written(s.to_string(), l)).unwrap_or(Field::Absent) };
            match k {
                "version" => pkg.version = field,
                "edition" => pkg.edition = field,
                "rust-version" | "rust_version" => pkg.rust_version = field,
                "workspace" => pkg.workspace = v.as_str().map(|s| (s.to_string(), l)),
                "links" => m.links = v.as_str().map(str::to_string),
                "build" if v.as_str().is_some() || v.as_bool() == Some(true) => m.has_build_rs = true,
                _ => {}
            }
        }
        m.package = Some(pkg);
    }
    deps_in(&text, top, "", None, &mut m.deps);
    if let Some(targets) = top.get("target").and_then(Item::as_table_like) {
        for (spec, t) in targets.iter() {
            if let Some(t) = t.as_table_like() {
                deps_in(&text, t, &format!("target.'{spec}'."), Some(spec), &mut m.deps);
            }
        }
    }
    if let Some(f) = top.get("features").and_then(Item::as_table_like) {
        for (k, v) in f.iter() {
            m.features.push((k.to_string(), strings_of(v), key_line(&text, f, k)));
        }
    }
    if let Some(l) = top.get("lints").and_then(Item::as_table_like) {
        if l.get("workspace").and_then(Item::as_bool) == Some(true) {
            m.lints_workspace = Some(key_line(&text, l, "workspace"));
        }
    }
    if let Some(p) = top.get("patch").and_then(Item::as_table_like) {
        for (src, t) in p.iter() {
            if let Some(t) = t.as_table_like() {
                let section = format!("patch.{src}");
                for (k, v) in t.iter() {
                    m.patches.push(dep_of(&text, &section, DepKind::Normal, None, t, k, v));
                }
            }
        }
    }
    if let Some(t) = top.get("replace").and_then(Item::as_table_like) {
        for (k, v) in t.iter() {
            m.patches.push(dep_of(&text, "replace", DepKind::Normal, None, t, k, v));
        }
    }
    if let Some(w) = top.get("workspace").and_then(Item::as_table_like) {
        let line = top.get("workspace").and_then(|i| i.as_table().and_then(|t| t.span()).or_else(|| i.span())).map(|s| line_of(&text, s.start)).unwrap_or(1);
        let entries = |k: &str| -> Vec<(String, u32)> {
            match w.get(k).and_then(|i| i.as_array()) {
                Some(a) => a.iter().filter_map(|v| v.as_str().map(|s| (s.to_string(), v.span().map(|sp| line_of(&text, sp.start)).unwrap_or(line)))).collect(),
                None => Vec::new(),
            }
        };
        let mut ws = Workspace { line, members: entries("members"), default_members: entries("default-members"), exclude: entries("exclude"), ..Default::default() };
        if let Some(d) = w.get("dependencies").and_then(Item::as_table_like) {
            for (k, v) in d.iter() {
                ws.dependencies.push(dep_of(&text, "workspace.dependencies", DepKind::Normal, None, d, k, v));
            }
        }
        if let Some(p) = w.get("package").and_then(Item::as_table_like) {
            for (k, v) in p.iter() {
                ws.package.insert(k.to_string(), (v.as_str().map(str::to_string), key_line(&text, p, k)));
            }
        }
        ws.lints = w.get("lints").is_some();
        m.workspace = Some(ws);
    }
    let _ = value_strings;
    Some(m)
}

/// `a/./b/../c` → `a/c`, without touching the disk.
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn has_glob(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// What the reading is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `.` or `*`: every workspace under the root.
    All,
    /// `crate:<name>`.
    Crate(String),
    /// A directory holding a Cargo.toml.
    Dir(PathBuf),
}

impl Target {
    pub fn of(root: &Path, target: &str) -> Target {
        let t = target.trim();
        if t == "." || t == "*" || t.is_empty() {
            return Target::All;
        }
        if let Some(name) = t.strip_prefix("crate:") {
            return Target::Crate(name.split('/').next().unwrap_or(name).to_string());
        }
        let raw = t.strip_prefix("path:").unwrap_or(t).trim_end_matches("Cargo.toml").trim_end_matches('/');
        let p = Path::new(raw);
        Target::Dir(normalize(&if p.is_absolute() { p.to_path_buf() } else { root.join(p) }))
    }
}

/// A crate the reading is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateRef {
    pub name: String,
    pub manifest: PathBuf,
}

/// The manifests a reading reads, and what the scanner found in them.
#[derive(Debug, Clone)]
pub struct Scan {
    pub root: PathBuf,
    pub target: Target,
    /// Every manifest read, by its path.
    pub manifests: BTreeMap<PathBuf, Manifest>,
    /// The Cargo.tomls the walk found under the root.
    pub found: Vec<PathBuf>,
    /// Each workspace root's members (manifest paths), explicit and implicit.
    pub members: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    /// Each package manifest's governing root (`None`: no workspace above it).
    pub governing: BTreeMap<PathBuf, Option<PathBuf>>,
    /// The manifests in the reading's scope.
    pub analyzed: BTreeSet<PathBuf>,
    /// Lockfiles, toolchain files and cargo configs the answer rests on.
    pub inputs: Vec<PathBuf>,
    findings: Vec<Finding>,
    satisfied: BTreeMap<String, usize>,
}

/// Directories never walked for manifests besides the sweep's own (`.claude` holds worktrees: copies of the tree).
const SKIP: &[&str] = &[".claude"];

impl Scan {
    /// The manifests the target needs, read and checked.
    pub fn read(root: &Path, scope: &Scope, target: &str) -> Result<Scan> {
        let mut s = Self::discover(root, scope, target)?;
        s.check();
        Ok(s)
    }

    /// The manifests the target needs, read; nothing checked (what the reuse key digests).
    pub fn discover(root: &Path, scope: &Scope, target: &str) -> Result<Scan> {
        let excluded = globset_of(&scope.exclude);
        let found = crate::sweep::manifests(root, SKIP, |rel| excluded.as_ref().is_some_and(|g| g.is_match(rel)));
        let mut s = Scan {
            root: root.to_path_buf(),
            target: Target::of(root, target),
            manifests: BTreeMap::new(),
            found: found.clone(),
            members: BTreeMap::new(),
            governing: BTreeMap::new(),
            analyzed: BTreeSet::new(),
            inputs: Vec::new(),
            findings: Vec::new(),
            satisfied: BTreeMap::new(),
        };
        for f in &found {
            if let Some(m) = read_manifest(f) {
                s.manifests.insert(f.clone(), m);
            }
        }
        // every workspace root found, its members
        let roots: Vec<PathBuf> = s.manifests.values().filter(|m| m.is_root()).map(|m| m.path.clone()).collect();
        for r in &roots {
            let members = s.members_of(r);
            s.members.insert(r.clone(), members);
        }
        // the scope
        let starts: Vec<PathBuf> = match &s.target {
            Target::All => {
                let mut v: Vec<PathBuf> = roots.clone();
                v.extend(s.members.values().flatten().cloned());
                let top = root.join("Cargo.toml");
                if s.manifests.contains_key(&top) {
                    v.push(top);
                }
                v
            }
            Target::Crate(name) => {
                let hit: Vec<PathBuf> = s.manifests.values().filter(|m| m.name().is_some_and(|n| n == *name || n.replace('-', "_") == name.replace('-', "_"))).map(|m| m.path.clone()).collect();
                let in_ws: Vec<PathBuf> = hit.iter().filter(|p| s.members.values().any(|ms| ms.contains(*p))).cloned().collect();
                let pick = if in_ws.is_empty() { hit } else { in_ws };
                if pick.is_empty() {
                    return Err(ReadingRefusal::TargetUnknown.refuse(format!("crate:{name}: no Cargo.toml under the root names a package `{name}`")));
                }
                pick
            }
            Target::Dir(d) => {
                let m = d.join("Cargo.toml");
                if !s.manifests.contains_key(&m) {
                    match read_manifest(&m) {
                        Some(x) => {
                            s.manifests.insert(m.clone(), x);
                        }
                        None => return Err(ReadingRefusal::TargetUnknown.refuse(format!("{}: no Cargo.toml there", d.display()))),
                    }
                }
                if s.manifests[&m].is_root() && !s.members.contains_key(&m) {
                    let members = s.members_of(&m);
                    s.members.insert(m.clone(), members);
                }
                let mut v = vec![m.clone()];
                v.extend(s.members.get(&m).into_iter().flatten().cloned());
                v
            }
        };
        let mut work = starts;
        while let Some(m) = work.pop() {
            if !s.analyzed.insert(m.clone()) {
                continue;
            }
            if !s.manifests.contains_key(&m) {
                if let Some(x) = read_manifest(&m) {
                    s.manifests.insert(m.clone(), x);
                } else {
                    continue;
                }
            }
            // its governing root, and that root's members when the target is a crate in it
            let gov = s.governing_root(&m);
            if let Some(g) = &gov {
                if !s.members.contains_key(g) {
                    let members = s.members_of(g);
                    s.members.insert(g.clone(), members);
                }
                work.push(g.clone());
            }
            s.governing.insert(m.clone(), gov.clone());
            // its path dependencies
            for dir in s.path_dirs(&m) {
                work.push(dir.join("Cargo.toml"));
            }
        }
        // the inputs besides the manifests: each analyzed root's lock, the toolchain files and cargo configs above
        let mut inputs: BTreeSet<PathBuf> = BTreeSet::new();
        for r in s.analyzed.iter().filter(|p| s.manifests.get(*p).is_some_and(Manifest::is_root)) {
            inputs.insert(r.with_file_name("Cargo.lock"));
        }
        if let Some(c) = s.cargo_root() {
            inputs.insert(s.lock_of(&c));
        }
        for dir in root.ancestors() {
            for f in ["rust-toolchain.toml", "rust-toolchain", ".cargo/config.toml", ".cargo/config"] {
                inputs.insert(dir.join(f));
            }
        }
        s.inputs = inputs.into_iter().filter(|p| p.is_file()).collect();
        Ok(s)
    }

    pub fn rel(&self, p: &Path) -> String {
        rel(&self.root, p)
    }

    /// The lockfile cargo uses for the workspace `manifest` belongs to.
    pub fn lock_of(&self, manifest: &Path) -> PathBuf {
        let root = self.governing.get(manifest).cloned().flatten().unwrap_or_else(|| manifest.to_path_buf());
        root.with_file_name("Cargo.lock")
    }

    /// The manifest cargo is run on: the root's own, or the target's.
    pub fn cargo_root(&self) -> Option<PathBuf> {
        match &self.target {
            Target::All => Some(self.root.join("Cargo.toml")).filter(|p| self.manifests.contains_key(p)),
            Target::Crate(_) | Target::Dir(_) => self.target_manifests().into_iter().next(),
        }
    }

    /// The target's own manifests.
    pub fn target_manifests(&self) -> Vec<PathBuf> {
        match &self.target {
            Target::All => Vec::new(),
            Target::Crate(name) => {
                let mut v: Vec<PathBuf> = self.analyzed.iter().filter(|p| self.manifests.get(*p).and_then(Manifest::name).is_some_and(|n| n.replace('-', "_") == name.replace('-', "_"))).cloned().collect();
                v.sort_by_key(|p| !self.members.values().any(|ms| ms.contains(p)));
                v
            }
            Target::Dir(d) => vec![d.join("Cargo.toml")],
        }
    }

    /// A root's members: its entries (globs expanded over the directories the walk found), then every path
    /// dependency of a member in its directory that it does not exclude.
    fn members_of(&mut self, root: &Path) -> BTreeSet<PathBuf> {
        let Some(m) = self.manifests.get(root) else { return BTreeSet::new() };
        let Some(ws) = m.workspace.clone() else { return BTreeSet::new() };
        let dir = m.dir.clone();
        let mut out: BTreeSet<PathBuf> = BTreeSet::new();
        // the root itself, when it is a package too
        if m.package.is_some() {
            out.insert(root.to_path_buf());
        }
        for (e, _) in ws.members.iter().chain(&ws.default_members) {
            if has_glob(e) {
                let Some(g) = globset_of(&[e.trim_end_matches('/').to_string()]) else { continue };
                for f in &self.found {
                    let d = f.parent().unwrap_or(f);
                    if let Ok(r) = d.strip_prefix(&dir) {
                        if g.is_match(r.to_string_lossy().replace('\\', "/")) && !self.excluded(root, &ws, f) {
                            out.insert(f.clone());
                        }
                    }
                }
            } else {
                let f = normalize(&dir.join(e)).join("Cargo.toml");
                if f.is_file() {
                    out.insert(f);
                }
            }
        }
        // implicit members: path dependencies under the root's directory, not excluded
        let mut work: Vec<PathBuf> = out.iter().cloned().collect();
        while let Some(p) = work.pop() {
            if !self.manifests.contains_key(&p) {
                match read_manifest(&p) {
                    Some(x) => {
                        self.manifests.insert(p.clone(), x);
                    }
                    None => continue,
                }
            }
            for d in self.path_dirs_with(&p, Some((root, &ws))) {
                let f = d.join("Cargo.toml");
                if d.starts_with(&dir) && f.is_file() && !self.excluded(root, &ws, &f) && out.insert(f.clone()) {
                    work.push(f);
                }
            }
        }
        out
    }

    /// Cargo's `is_excluded`: an `exclude` entry is a prefix of the path, and no `members` entry is.
    fn excluded(&self, root: &Path, ws: &Workspace, manifest: &Path) -> bool {
        let dir = root.parent().unwrap_or(root);
        let excluded = ws.exclude.iter().any(|(e, _)| manifest.starts_with(normalize(&dir.join(e))));
        let explicit = ws.members.iter().any(|(e, _)| !has_glob(e) && manifest.starts_with(normalize(&dir.join(e))));
        excluded && !explicit
    }

    /// The governing root of a package manifest, by cargo's rule (see the module's doc).
    pub fn governing_root(&mut self, manifest: &Path) -> Option<PathBuf> {
        let m = self.manifests.get(manifest)?;
        if m.is_root() {
            return Some(manifest.to_path_buf());
        }
        if let Some((w, _)) = m.package.as_ref().and_then(|p| p.workspace.clone()) {
            let r = normalize(&m.dir.join(&w)).join("Cargo.toml");
            if !self.manifests.contains_key(&r) {
                if let Some(x) = read_manifest(&r) {
                    self.manifests.insert(r.clone(), x);
                }
            }
            return Some(r);
        }
        let dir = m.dir.clone();
        for a in dir.ancestors().skip(1) {
            let c = a.join("Cargo.toml");
            if !self.manifests.contains_key(&c) {
                match read_manifest(&c) {
                    Some(x) => {
                        self.manifests.insert(c.clone(), x);
                    }
                    None => continue,
                }
            }
            let Some(ws) = self.manifests.get(&c).and_then(|x| x.workspace.clone()) else { continue };
            if !self.excluded(&c, &ws, manifest) {
                return Some(c);
            }
        }
        None
    }

    /// The root entry a `workspace = true` dependency inherits.
    pub fn inherited<'a>(&'a self, manifest: &Path, d: &Dep) -> Option<(&'a Manifest, &'a Dep)> {
        let root = self.governing.get(manifest).cloned().flatten()?;
        let r = self.manifests.get(&root)?;
        r.workspace.as_ref()?.dependencies.iter().find(|x| x.key == d.key).map(|x| (r, x))
    }

    /// A dependency as it is in effect: an inherited one takes the root's package, path (from the root's
    /// directory), version, git and registry.
    pub fn effective(&self, manifest: &Path, d: &Dep) -> Dep {
        if !d.workspace {
            return d.clone();
        }
        match self.inherited(manifest, d) {
            Some((r, x)) => {
                let mut e = d.clone();
                e.package = x.package.clone();
                e.version = x.version.clone();
                e.git = x.git.clone();
                e.registry = x.registry.clone();
                e.path = x.path.as_ref().map(|p| r.dir.join(p).to_string_lossy().into_owned());
                e.features = x.features.iter().chain(&d.features).cloned().collect();
                e.default_features = x.default_features;
                e
            }
            None => d.clone(),
        }
    }

    /// The directories of a manifest's path dependencies (inherited ones included) and its patches'.
    fn path_dirs(&self, manifest: &Path) -> Vec<PathBuf> {
        self.path_dirs_with(manifest, None)
    }

    fn path_dirs_with(&self, manifest: &Path, root: Option<(&Path, &Workspace)>) -> Vec<PathBuf> {
        let Some(m) = self.manifests.get(manifest) else { return Vec::new() };
        let mut out = Vec::new();
        for d in &m.deps {
            if d.workspace {
                let entry = match root {
                    Some((r, ws)) => ws.dependencies.iter().find(|x| x.key == d.key).map(|x| (r.parent().unwrap_or(r).to_path_buf(), x.clone())),
                    None => self.inherited(manifest, d).map(|(r, x)| (r.dir.clone(), x.clone())),
                };
                if let Some((dir, x)) = entry {
                    if let Some(p) = &x.path {
                        out.push(normalize(&dir.join(p)));
                    }
                }
            } else if let Some(p) = &d.path {
                out.push(normalize(&m.dir.join(p)));
            }
        }
        if let Some(ws) = &m.workspace {
            for d in ws.dependencies.iter().chain(&m.patches) {
                if let Some(p) = &d.path {
                    out.push(normalize(&m.dir.join(p)));
                }
            }
        } else {
            for d in &m.patches {
                if let Some(p) = &d.path {
                    out.push(normalize(&m.dir.join(p)));
                }
            }
        }
        out
    }

    /// Every package manifest in scope.
    pub fn packages(&self) -> impl Iterator<Item = &Manifest> {
        self.analyzed.iter().filter_map(|p| self.manifests.get(p)).filter(|m| m.package.is_some())
    }

    /// Whether a manifest is a member of some root.
    pub fn is_member(&self, manifest: &Path) -> bool {
        self.members.values().any(|ms| ms.contains(manifest))
    }

    /// The crates the reading is about: the target's (a root's members with it), or every member in scope. With
    /// cargo's workspace read, its members are the truth.
    pub fn crates_in_scope(&self, ws: Option<&klepto::CargoWorkspace>) -> Vec<CrateRef> {
        let mut paths: Vec<PathBuf> = match &self.target {
            Target::All => self.packages().filter(|m| self.is_member(&m.path) || m.path == self.root.join("Cargo.toml")).map(|m| m.path.clone()).collect(),
            _ => {
                let mut v = Vec::new();
                for t in self.target_manifests() {
                    if self.manifests.get(&t).is_some_and(|m| m.package.is_some()) {
                        v.push(t.clone());
                    }
                    if let Some(ms) = self.members.get(&t) {
                        v.extend(ms.iter().cloned());
                    }
                }
                v
            }
        };
        if let (Some(w), Target::All) = (ws, &self.target) {
            paths = w.member_packages().iter().map(|p| p.manifest_path.clone()).collect();
        }
        paths.sort();
        paths.dedup();
        paths.into_iter().filter_map(|p| self.manifests.get(&p).and_then(Manifest::name).map(|name| CrateRef { name, manifest: p })).collect()
    }

    /// An effective `[package]` field: written, or the governing root's `[workspace.package]` value.
    pub fn package_field(&self, m: &Manifest, field: &str) -> Option<String> {
        let p = m.package.as_ref()?;
        let f = match field {
            "edition" => &p.edition,
            "rust-version" => &p.rust_version,
            "version" => &p.version,
            _ => return None,
        };
        match f {
            Field::Written(v, _) => Some(v.clone()),
            Field::Inherited(_) => {
                let root = self.governing.get(&m.path).cloned().flatten()?;
                self.manifests.get(&root)?.workspace.as_ref()?.package.get(field).and_then(|(v, _)| v.clone())
            }
            Field::Absent => None,
        }
    }

    /// Each manifest read and the digest of its bytes.
    pub fn digests(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for p in self.found.iter().chain(&self.analyzed) {
            if let Some(m) = self.manifests.get(p) {
                out.insert(self.rel(p), crate::digest::of(m.text.as_bytes()));
            }
        }
        out
    }

    /// The lock of the workspace cargo is run on, and its digest.
    pub fn lock_digest(&self) -> Option<(String, String)> {
        let lock = self.lock_of(&self.cargo_root()?);
        std::fs::read(&lock).ok().map(|b| (self.rel(&lock), crate::digest::of(&b)))
    }

    /// The digest of every input: manifests, locks, toolchain files, cargo configs, and the spec's requirements.
    pub fn digest(&self, options: &RequirementsOptions) -> String {
        let files: Vec<(String, String)> = self.inputs.iter().filter_map(|p| std::fs::read(p).ok().map(|b| (self.rel(p), crate::digest::of(&b)))).collect();
        crate::digest::of_value(&(self.digests(), files, options, format!("{:?}", self.target)))
    }

    /// The scanner's findings.
    pub fn findings(&self) -> Vec<Finding> {
        self.findings.clone()
    }

    /// The workspace as the scanner found it.
    pub fn state(&self) -> State {
        let mut roots = Vec::new();
        for r in self.analyzed.iter().filter(|p| self.manifests.get(*p).is_some_and(Manifest::is_root)) {
            let m = &self.manifests[r];
            let ws = m.workspace.as_ref().expect("a root");
            let nested = r.parent().and_then(Path::parent).is_some_and(|up| up.ancestors().any(|a| a.join("Cargo.toml") != *r && self.manifests.get(&a.join("Cargo.toml")).is_some_and(Manifest::is_root)));
            roots.push(Root {
                manifest: self.rel(r),
                members: self.members.get(r).into_iter().flatten().map(|p| self.manifests.get(p).and_then(Manifest::name).unwrap_or_else(|| self.rel(p))).collect(),
                nested,
                dependencies: ws.dependencies.len(),
                package_fields: ws.package.keys().cloned().collect(),
                lints: ws.lints,
            });
        }
        State {
            roots,
            manifests: self.analyzed.len(),
            outside: self.outside().iter().map(|p| self.rel(p)).collect(),
            satisfied: self.satisfied.clone(),
            lock: self.cargo_root().map(|c| self.lock_of(&c)).filter(|l| l.is_file()).map(|l| self.rel(&l)),
            metadata: "not run".into(),
            ..Default::default()
        }
    }

    /// Manifests under the root that no workspace holds and nothing in scope depends on.
    pub fn outside(&self) -> Vec<PathBuf> {
        if self.target != Target::All {
            return Vec::new();
        }
        self.found.iter().filter(|p| !self.analyzed.contains(*p)).cloned().collect()
    }

    fn place(&self, m: &Manifest, line: u32, section: &str, field: &str) -> Place {
        Place { file: self.rel(&m.path), line: (line > 0).then_some(line), section: section.to_string(), field: field.to_string() }
    }

    fn ok(&mut self, what: &str) {
        *self.satisfied.entry(what.to_string()).or_default() += 1;
    }

    /// Every check over the manifests in scope.
    fn check(&mut self) {
        let mut out: Vec<Finding> = Vec::new();
        let analyzed: Vec<PathBuf> = self.analyzed.iter().cloned().collect();
        for p in &analyzed {
            let Some(m) = self.manifests.get(p).cloned() else { continue };
            if let Some(e) = &m.error {
                out.push(finding(Check::ManifestUnreadable, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, None, Some(self.place(&m, 0, "", "")), format!("{} is not TOML", self.rel(p)), "a manifest cargo can parse", e.lines().take(3).collect::<Vec<_>>().join(" "), "cargo refuses every command that reads this workspace", Some(format!("fix the TOML of {}", self.rel(p)))));
                continue;
            }
            if m.is_root() {
                self.check_root(&m, &mut out);
            }
            if m.package.is_some() {
                self.check_package(&m, &mut out);
            }
        }
        self.check_spread(&mut out);
        let outside = self.outside();
        if !outside.is_empty() {
            let names: Vec<String> = outside.iter().map(|p| self.rel(p)).collect();
            out.push(finding(Check::OutsideAnyWorkspace, Status::Informational, Basis::Observed, ReadFrom::Manifest, None, None, format!("{} manifest(s) under the root that no workspace holds and nothing depends on", outside.len()), "-", super::shorten(&names, 20), "not read further; a crate copied in to be integrated is one of them until a root lists it (cargo refuses to build such a crate inside a workspace's directory)", None));
        }
        self.findings = out;
    }

    fn check_root(&mut self, m: &Manifest, out: &mut Vec<Finding>) {
        let ws = m.workspace.clone().expect("a root");
        let members = self.members.get(&m.path).cloned().unwrap_or_default();
        for (e, line) in ws.members.iter().chain(&ws.default_members) {
            if has_glob(e) {
                continue;
            }
            let f = normalize(&m.dir.join(e)).join("Cargo.toml");
            if f.is_file() {
                self.ok("members entries found");
            } else {
                out.push(finding(Check::MemberMissing, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, None, Some(self.place(m, *line, "workspace", "members")), format!("members entry `{e}` names {} with no Cargo.toml", self.rel(&normalize(&m.dir.join(e)))), "every members entry names a directory holding a Cargo.toml", format!("no file at {}", self.rel(&f)), "cargo refuses the workspace (\"failed to load manifest for workspace member\")", Some(format!("put the crate at {} or drop the entry `{e}` from {}", self.rel(&normalize(&m.dir.join(e))), self.rel(&m.path)))));
            }
        }
        // two members with one name
        let mut by_name: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for p in &members {
            if let Some(n) = self.manifests.get(p).and_then(Manifest::name) {
                by_name.entry(n).or_default().push(p.clone());
            }
        }
        for (n, ps) in by_name.iter().filter(|(_, ps)| ps.len() > 1) {
            let files: Vec<String> = ps.iter().map(|p| self.rel(p)).collect();
            out.push(finding(Check::DuplicatePackageName, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, Some(n), Some(self.place(m, ws.line, "workspace", "members")), format!("{} members are named `{n}`", ps.len()), "one package name per workspace", files.join(", "), "cargo refuses the workspace (\"two packages named\")", Some(format!("rename one of them, or exclude one from {}", self.rel(&m.path)))));
        }
        // the root's patches (cargo loads every one); its [workspace.dependencies] paths are checked where a member
        // inherits them, since cargo loads an entry only then
        for d in &m.patches {
            if let Some(p) = &d.path {
                self.check_path(m, d, &normalize(&m.dir.join(p)), out);
            }
        }
        // entries nobody inherits
        for d in &ws.dependencies {
            let used = members.iter().filter_map(|p| self.manifests.get(p)).any(|x| x.deps.iter().any(|y| y.workspace && y.key == d.key));
            if !used {
                let nowhere = d.path.as_ref().map(|p| normalize(&m.dir.join(p))).filter(|dir| !dir.join("Cargo.toml").is_file()).map(|dir| format!("; its path {} holds no Cargo.toml", self.rel(&dir))).unwrap_or_default();
                out.push(finding(Check::WorkspaceDependencyUnused, Status::Informational, Basis::Observed, ReadFrom::Manifest, None, Some(self.place(m, d.line, "workspace.dependencies", &d.key)), format!("[workspace.dependencies].{} is inherited by no member{nowhere}", d.key), "-", format!("no member of {} writes `{} = {{ workspace = true }}`", self.rel(&m.path), d.key), "nothing while no member inherits it: cargo reads an entry only when one does (then a missing path is an error)", None));
            }
        }
    }

    fn check_package(&mut self, m: &Manifest, out: &mut Vec<Finding>) {
        let pkg = m.package.clone().expect("a package");
        let name = pkg.name.clone().unwrap_or_else(|| self.rel(&m.dir));
        let krate = Some(name.as_str());
        let gov = self.governing.get(&m.path).cloned().flatten();
        let root = gov.as_ref().and_then(|g| self.manifests.get(g)).cloned();
        // membership
        if let (Some(g), Some(r)) = (&gov, &root) {
            if *g != m.path {
                match &r.workspace {
                    None => {
                        let (w, l) = pkg.workspace.clone().unwrap_or_default();
                        out.push(finding(Check::WorkspaceNotARoot, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, krate, Some(self.place(m, l, "package", "workspace")), format!("package.workspace = \"{w}\" names {} which has no [workspace]", self.rel(g)), "package.workspace names a workspace root", format!("{} has no [workspace] table", self.rel(g)), "cargo refuses the crate", None));
                    }
                    Some(_) if !self.members.get(g).is_some_and(|ms| ms.contains(&m.path)) => {
                        out.push(finding(
                            Check::NotAMember,
                            Status::Unsatisfied,
                            Basis::Inferred,
                            ReadFrom::Manifest,
                            krate,
                            Some(self.place(m, pkg.line, "package", "name")),
                            format!("its governing root {} does not list it among its members", self.rel(g)),
                            "the nearest workspace root above a crate (that does not exclude it) lists it as a member",
                            format!("{} is the nearest ancestor with [workspace]; its members and their path dependencies do not include {}", self.rel(g), self.rel(&m.dir)),
                            "cargo refuses the crate: \"current package believes it's in a workspace when it's not\"",
                            Some(format!("add `{}` to [workspace].members of {}, or to its `exclude` (then the crate is its own), or give the crate an empty [workspace] of its own", rel(&r.dir, &m.dir), self.rel(g))),
                        ));
                    }
                    Some(_) => self.ok("crates their root lists"),
                }
            }
        }
        // inherited [package] fields
        for (field, line) in &pkg.inherited {
            let set = root.as_ref().and_then(|r| r.workspace.as_ref()).is_some_and(|w| w.package.contains_key(field));
            if set {
                self.ok("inherited [package] fields found in the root");
            } else {
                let where_ = gov.as_ref().map(|g| self.rel(g)).unwrap_or_else(|| "no workspace root above it".into());
                out.push(finding(Check::WorkspaceFieldMissing, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, krate, Some(self.place(m, *line, "package", field)), format!("`{field}.workspace = true` and the root's [workspace.package] has no `{field}`"), format!("[workspace.package].{field} in the governing root"), format!("governing root: {where_}"), "cargo refuses the crate (\"error inheriting `{field}` from workspace root manifest\")".replace("{field}", field), Some(format!("set [workspace.package].{field} in {where_}, or write `{field}` in {}", self.rel(&m.path)))));
            }
        }
        if let Some(line) = m.lints_workspace {
            if root.as_ref().and_then(|r| r.workspace.as_ref()).is_some_and(|w| w.lints) {
                self.ok("[lints] workspace = true found in the root");
            } else {
                out.push(finding(Check::WorkspaceLintsMissing, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, krate, Some(self.place(m, line, "lints", "workspace")), "[lints] workspace = true and the root has no [workspace.lints]", "[workspace.lints] in the governing root", gov.as_ref().map(|g| self.rel(g)).unwrap_or_else(|| "no workspace root above it".into()), "cargo refuses the crate", None));
            }
        }
        // dependencies
        for d in &m.deps {
            let place = self.place(m, d.line, &d.section, &d.key);
            if d.workspace {
                match self.inherited(&m.path, d) {
                    Some(_) => self.ok("`workspace = true` dependencies found in their root"),
                    None => {
                        let (root_said, action) = match &gov {
                            Some(g) => (format!("governing root {} has no [workspace.dependencies].{}", self.rel(g), d.key), format!("add `{}` to [workspace.dependencies] in {} (its version: the one the lock already holds, if it holds one; this reading does not choose one)", d.key, self.rel(g))),
                            None => ("no workspace root above the crate".to_string(), format!("give {} a version or path for `{}` itself, or put it in a workspace whose root declares it", self.rel(&m.path), d.key)),
                        };
                        out.push(finding(Check::WorkspaceDependencyMissing, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, krate, Some(place.clone()), format!("`{}` inherits from the workspace, and {root_said}", d.key), format!("[workspace.dependencies].{} in the crate's governing root", d.key), format!("{}: `{} = {{ workspace = true }}`; {root_said}", self.rel(&m.path), d.key), "cargo refuses the workspace before it resolves anything (\"error inheriting dependency from workspace root manifest\")", Some(action)));
                    }
                }
                let ignored: Vec<&String> = d.keys.iter().filter(|k| !matches!(k.as_str(), "workspace" | "features" | "optional" | "default-features" | "default_features" | "public")).collect();
                if !ignored.is_empty() {
                    out.push(finding(Check::InheritedKeyIgnored, Status::Unsatisfied, Basis::Inferred, ReadFrom::Manifest, krate, Some(place.clone()), format!("`{}` inherits from the workspace and also writes {}", d.key, ignored.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")), "only features, optional and default-features beside workspace = true", "the keys as written", "cargo ignores them (\"unused manifest key\"): the root's entry decides the source and version", Some(format!("drop {} from the entry, or change the root's entry", ignored.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")))));
                }
            }
            let e = self.effective(&m.path, d);
            if let Some(p) = &e.path {
                let dir = if d.workspace { normalize(Path::new(p)) } else { normalize(&m.dir.join(p)) };
                self.check_path(m, &e, &dir, out);
            }
            if d.kind == DepKind::Dev && d.optional {
                out.push(finding(Check::OptionalDevDependency, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, krate, Some(place.clone()), format!("dev-dependency `{}` is optional", d.key), "dev-dependencies are never optional", "optional = true in a dev-dependencies table", "cargo refuses the crate (\"dev-dependencies are not allowed to be optional\")", Some(format!("drop `optional` from `{}`", d.key))));
            }
            if let Some(reg) = &e.registry {
                if !registry_configured(&self.root, reg) {
                    out.push(finding(Check::RegistryUnconfigured, Status::Unsatisfied, Basis::Inferred, ReadFrom::Manifest, krate, Some(place.clone()), format!("`{}` comes from registry `{reg}`, which no cargo config declares", d.key), format!("[registries.{reg}] in a .cargo/config.toml at or above the root, or in CARGO_HOME"), "no [registries] entry of that name in the configs read", "cargo refuses the dependency (\"registry index was not found in any configuration\")", None));
                }
            }
            if d.package.is_some() {
                out.push(finding(Check::RenamedDependency, Status::Informational, Basis::Observed, ReadFrom::Manifest, krate, Some(place.clone()), format!("`{}` is package `{}`", d.key, d.package_name()), "-", format!("package = \"{}\"", d.package_name()), format!("source names it `{}`; cargo's resolve and every lock name it `{}`", d.key.replace('-', "_"), d.package_name()), None));
            }
        }
        self.check_features(m, &name, out);
        if self.is_member(&m.path) || self.target_manifests().contains(&m.path) {
            self.check_unused(m, &name, out);
        }
    }

    /// A path dependency's directory holds a package of its name, at a version its requirement admits.
    fn check_path(&mut self, m: &Manifest, d: &Dep, dir: &Path, out: &mut Vec<Finding>) {
        let krate = m.name();
        let place = self.place(m, d.line, &d.section, &d.key);
        let target = dir.join("Cargo.toml");
        let there = if self.manifests.contains_key(&target) { self.manifests.get(&target).cloned() } else { read_manifest(&target) };
        let Some(t) = there else {
            out.push(finding(Check::PathMissing, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, krate.as_deref(), Some(place), format!("`{}` points at {}, which holds no Cargo.toml", d.key, self.rel(dir)), "a path dependency names a directory holding the package", format!("path = \"{}\"; {} does not exist", d.path.clone().unwrap_or_default(), self.rel(&target)), "cargo refuses the workspace (\"failed to read\" / \"failed to load source for dependency\")", Some(format!("put `{}` at {}, or point the path at where it is", d.package_name(), self.rel(dir)))));
            return;
        };
        let name = t.name();
        if name.as_deref() != Some(d.package_name()) {
            let holds = name.clone().map(|n| format!("package `{n}`")).unwrap_or_else(|| "no [package]".into());
            out.push(finding(Check::PathNamesOtherPackage, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, krate.as_deref(), Some(place), format!("`{}` asks for package `{}` at {}, which holds {holds}", d.key, d.package_name(), self.rel(dir)), "the package at the path has the dependency's package name (exactly: `-` and `_` differ)", format!("{}: name = {:?}", self.rel(&target), name), "cargo refuses it (\"no matching package named\")", Some(format!("name the dependency `{}` (or `package = \"{}\"`), or point the path at `{}`", name.clone().unwrap_or_default(), name.unwrap_or_default(), d.package_name()))));
            return;
        }
        if let (Some(req), Some(v)) = (&d.version, self.package_field(&t, "version")) {
            if klepto::version_meets(req, &v) == Some(false) {
                out.push(finding(Check::PathVersionMismatch, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, krate.as_deref(), Some(place), format!("`{}` requires {req}, and the package at {} is {v}", d.key, self.rel(dir)), format!("the package at the path meets the requirement `{req}`"), format!("{}: version = \"{v}\"; semver: `{req}` does not admit {v}", self.rel(&target)), "cargo refuses it (\"failed to select a version for the requirement\"): a real conflict, and which side moves is a choice this reading does not make", Some("make the requirement and the package at the path agree: change one of them (which one is a decision)".into())));
                return;
            }
        }
        self.ok("path dependencies found with their package");
    }

    /// Every `[features]` entry, in its four forms, against klepto's model of the manifest as written.
    fn check_features(&mut self, m: &Manifest, name: &str, out: &mut Vec<Finding>) {
        let model = self.feature_model(m);
        for (f, entries, line) in &m.features {
            for e in entries {
                let place = self.place(m, *line, "features", f);
                let bad = match FeatureEnable::parse(e) {
                    FeatureEnable::Feature(g) => (!model.features.contains_key(&g)).then(|| format!("`{e}` is neither a feature of `{name}` nor an optional dependency of it")),
                    FeatureEnable::Dep(d) => match model.dep(&d) {
                        None => Some(format!("`{e}`: `{name}` has no dependency `{d}`")),
                        Some(x) if !x.optional => Some(format!("`{e}`: `{d}` is not optional")),
                        Some(_) => None,
                    },
                    FeatureEnable::DepFeature { dep, feature, .. } => {
                        let decls: Vec<&DepInfo> = model.dependencies.iter().filter(|x| x.name == dep).collect();
                        if decls.is_empty() {
                            Some(format!("`{e}`: `{name}` has no dependency `{dep}`"))
                        } else if decls.iter().all(|x| x.kind == DepKind::Dev) {
                            Some(format!("`{e}`: `{dep}` is only a dev-dependency, which a feature cannot turn on"))
                        } else {
                            self.path_feature_missing(m, &dep, &feature).map(|at| format!("`{e}`: the package at {at} has no feature `{feature}`"))
                        }
                    }
                };
                match bad {
                    Some(what) => out.push(finding(Check::FeatureEntryUnknown, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, Some(name), Some(place), what, "each entry names a feature, `dep:` an optional dependency, `x/f` a dependency (and, for a path dependency, a feature it has)", format!("{f} = [{}]", entries.iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(", ")), "cargo refuses the crate", Some(format!("fix the entry `{e}` of feature `{f}`")))),
                    None => self.ok("feature entries that name what they turn on"),
                }
            }
        }
    }

    /// klepto's feature model of a manifest as written (inherited dependencies through their root entry).
    pub fn feature_model(&self, m: &Manifest) -> FeatureModel {
        let deps: Vec<DepInfo> = m
            .deps
            .iter()
            .map(|d| {
                let e = self.effective(&m.path, d);
                DepInfo {
                    name: d.key.replace('-', "_"),
                    package: e.package_name().to_string(),
                    optional: d.optional,
                    kind: d.kind,
                    target: d.target.as_deref().map(CfgPredicate::parse_str),
                    default_features: e.default_features.unwrap_or(true),
                    features: e.features.clone(),
                    implicit_feature: false,
                }
            })
            .collect();
        FeatureModel::from_declared(&m.name().unwrap_or_default().replace('-', "_"), m.features.iter().map(|(n, l, _)| (n.clone(), l.clone())), deps)
    }

    /// For `dep/feature` on a path dependency: the package's directory when it has no such feature (`None` when it
    /// has it, or when the dependency is not a path one: that is cargo's to say).
    fn path_feature_missing(&self, m: &Manifest, dep: &str, feature: &str) -> Option<String> {
        let d = m.deps.iter().find(|d| d.key.replace('-', "_") == dep)?;
        let e = self.effective(&m.path, d);
        let p = e.path.as_ref()?;
        let dir = if d.workspace { normalize(Path::new(p)) } else { normalize(&m.dir.join(p)) };
        let t = self.manifests.get(&dir.join("Cargo.toml")).cloned().or_else(|| read_manifest(&dir.join("Cargo.toml")))?;
        let model = self.feature_model(&t);
        (!model.features.contains_key(feature)).then(|| self.rel(&dir))
    }

    /// A dependency no `.rs` file of the crate names as a word (a text search; marked inferred).
    fn check_unused(&mut self, m: &Manifest, name: &str, out: &mut Vec<Finding>) {
        let mut words: HashSet<String> = HashSet::new();
        for f in crate::sweep::source_files(&m.dir, "rs") {
            if let Ok(t) = std::fs::read_to_string(&f) {
                words.extend(t.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).map(str::to_string));
            }
        }
        let forwarded: BTreeSet<String> = m.features.iter().flat_map(|(_, es, _)| es.iter()).filter_map(|e| match FeatureEnable::parse(e) {
            FeatureEnable::Dep(d) | FeatureEnable::DepFeature { dep: d, .. } => Some(d),
            FeatureEnable::Feature(_) => None,
        }).collect();
        let mut seen = BTreeSet::new();
        for d in &m.deps {
            let word = d.key.replace('-', "_");
            if !seen.insert(word.clone()) || words.contains(&word) || forwarded.contains(&word) || d.package_name().ends_with("-sys") {
                continue;
            }
            out.push(finding(Check::PossiblyUnused, Status::Informational, Basis::Inferred, ReadFrom::Sources, Some(name), Some(self.place(m, d.line, &d.section, &d.key)), format!("no .rs file of `{name}` names `{word}`"), "-", format!("a word search of {}'s .rs files (nested crates left out) found no `{word}`", self.rel(&m.dir)), "possibly unused; a macro, a build script, a lib name that differs from the key or a linked-only crate hides a use from a text search", None));
        }
    }

    /// One package required with different requirements across the crates in scope.
    fn check_spread(&mut self, out: &mut Vec<Finding>) {
        let mut reqs: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
        for m in self.packages() {
            if !self.is_member(&m.path) {
                continue;
            }
            for d in &m.deps {
                let e = self.effective(&m.path, d);
                if let (Some(v), None, None) = (&e.version, &e.path, &e.git) {
                    reqs.entry(e.package_name().to_string()).or_default().entry(v.clone()).or_default().insert(m.name().unwrap_or_default());
                }
            }
        }
        for (pkg, by) in reqs.into_iter().filter(|(_, by)| by.len() > 1) {
            let ev: Vec<String> = by.iter().map(|(r, cs)| format!("`{r}`: {}", super::shorten(&cs.iter().cloned().collect::<Vec<_>>(), 6))).collect();
            out.push(finding(Check::RequirementSpread, Status::Informational, Basis::Observed, ReadFrom::Manifest, None, None, format!("`{pkg}` is required {} different ways", by.len()), "-", ev.join("; "), "cargo picks one version per semver-compatible range; requirements in different ranges keep more than one version in the lock (the lock face says which)", None));
        }
    }
}

/// Globs relative to the root (`**/fixtures/**`), as one set; `None` when there are none.
fn globset_of(globs: &[String]) -> Option<globset::GlobSet> {
    if globs.is_empty() {
        return None;
    }
    let mut b = globset::GlobSetBuilder::new();
    for g in globs {
        if let Ok(x) = globset::GlobBuilder::new(g).literal_separator(true).build() {
            b.add(x);
        }
    }
    b.build().ok()
}

/// Whether a `.cargo/config.toml` at or above `root`, or CARGO_HOME's, declares `[registries.<name>]`.
fn registry_configured(root: &Path, name: &str) -> bool {
    let mut files: Vec<PathBuf> = root.ancestors().flat_map(|a| [a.join(".cargo/config.toml"), a.join(".cargo/config")]).collect();
    let home = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")));
    if let Some(h) = home {
        files.push(h.join("config.toml"));
        files.push(h.join("config"));
    }
    files.iter().filter_map(|f| std::fs::read_to_string(f).ok()).filter_map(|t| t.parse::<toml::Table>().ok()).any(|t| t.get("registries").and_then(|r| r.get(name)).is_some())
}
