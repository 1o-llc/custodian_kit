//! Paths declared, not walked. One table names every place a reading may
//! read: each workspace crate's root and src (from cargo metadata, as klepto
//! reads it, without parsing), the `.custodian/` folder's parts (pathgen's
//! named paths), the project's declaration files by name (`[decl]` in
//! `.custodian/config/paths.toml`), and raw paths a project chose to declare
//! (`[path]`). A target or a declared_in file names a route:
//!
//! - `crate:<name>`: a crate's root; `crate:<name>/src/x.rs`: a file in it
//! - `decl:<name>`: a declared set of TOML files
//! - `custodian:<part>`: specs, readings, receipts, journal, jobs-queued, jobs-running, jobs-done, registry
//! - `path:<raw>`: the escape, free mode only (in a job, only when declared under `[path]`)
//!
//! A [`Route`] is made only by [`PathsTable::resolve`]; a job's reading takes
//! routes, so it cannot read a walked path. Walking an undeclared tree is the
//! sweep's (`sweep::sweep`), which takes a raw path and offers routes to declare.

use crate::reading::refusal::ReadingRefusal;
use crate::store::{named, JobState, Store};
use crate::{klepto_builder, Result, ScanOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The paths table: `.custodian/config/v1.paths.1o.paths` (or the older `paths.toml`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathsFile {
    /// Declaration sets by name: `marketplace = ["marketplace/*.toml"]`. A glob
    /// is one directory and a file pattern in it.
    #[serde(default)]
    pub decl: BTreeMap<String, Vec<String>>,
    /// Raw paths (relative to the root) a job may name as `path:<raw>`.
    #[serde(default)]
    pub path: Vec<String>,
}

/// Who is resolving: a job may only read declared routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolving {
    Free,
    Job,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Crate { name: String, root: PathBuf, file: Option<PathBuf> },
    Decl { name: String, globs: Vec<String> },
    Custodian { part: String, path: PathBuf },
    Path { raw: String },
}

/// A place a reading may read, resolved through the declared table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    name: String,
    place: Place,
}

impl Route {
    /// The route as written (`crate:x`, `decl:y`, `path:z`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The crate a `crate:` route names.
    pub fn crate_name(&self) -> Option<&str> {
        match &self.place {
            Place::Crate { name, file: None, .. } => Some(name),
            _ => None,
        }
    }

    /// The file a `crate:<name>/<file>` route names, relative to the root.
    pub fn crate_file(&self, root: &Path) -> Option<String> {
        match &self.place {
            Place::Crate { file: Some(f), .. } => Some(f.strip_prefix(root).unwrap_or(f).to_string_lossy().replace('\\', "/")),
            _ => None,
        }
    }

    /// The globs a `decl:` or `path:` route names, relative to the root.
    pub fn globs(&self) -> Vec<String> {
        match &self.place {
            Place::Decl { globs, .. } => globs.clone(),
            Place::Path { raw } => vec![raw.clone()],
            Place::Custodian { path, .. } => vec![path.to_string_lossy().into_owned()],
            Place::Crate { root, file, .. } => vec![file.clone().unwrap_or_else(|| root.clone()).to_string_lossy().into_owned()],
        }
    }

    /// The files a declared set holds: each glob's directory listed (one
    /// directory, not walked) and its file pattern matched.
    pub fn files(&self, root: &Path) -> Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for g in self.globs() {
            let rel = g.trim_start_matches("./");
            if rel.contains("**") {
                return Err(ReadingRefusal::DeclaredGlob.refuse(format!("`{g}`: a declared set names one directory and a pattern in it, not `**` (sweep the tree to find the directories)")));
            }
            let (dir, pattern) = match rel.rsplit_once('/') {
                Some((d, p)) => (root.join(d), p.to_string()),
                None => (root.to_path_buf(), rel.to_string()),
            };
            let m = globset::Glob::new(&pattern).map_err(|e| ReadingRefusal::DeclaredGlob.refuse(format!("`{g}`: {e}")))?.compile_matcher();
            let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
                Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file() && p.file_name().is_some_and(|n| m.is_match(n))).collect(),
                Err(_) => Vec::new(),
            };
            files.sort();
            out.extend(files);
        }
        Ok(out)
    }
}

/// The declared table of one project.
#[derive(Debug, Clone)]
pub struct PathsTable {
    pub root: PathBuf,
    /// crate name → its root.
    pub crates: BTreeMap<String, PathBuf>,
    pub file: PathsFile,
    /// `.custodian/` parts, when the project has the folder.
    pub custodian: BTreeMap<String, PathBuf>,
}

/// The table's own file in an initialized project.
/// `config/v1.paths.1o.paths`, or the older `config/paths.toml` when only it is there.
pub fn paths_file(store: &Store) -> Result<PathBuf> {
    let oneo = store.path(&named::PATHS, &[])?;
    let old = store.path(&named::THRESHOLDS, &[("name", "paths")])?;
    Ok(if !oneo.exists() && old.exists() { old } else { oneo })
}

impl PathsTable {
    /// The table for `root`: crates from cargo metadata (klepto's builder, no
    /// parse), the declared sets from the folder's `paths.toml` when there is one.
    pub fn load(root: &Path, store: Option<&Store>) -> Result<PathsTable> {
        let builder = klepto_builder(root, &ScanOptions::graph()).map_err(|e| ReadingRefusal::NoWorkspace.refuse(e))?;
        Self::from_packages(root, builder.packages(), store)
    }

    /// The table from packages klepto already read.
    pub fn from_packages(root: &Path, packages: &BTreeMap<String, klepto::PackageInfo>, store: Option<&Store>) -> Result<PathsTable> {
        let crates = packages.values().map(|p| (p.name.clone(), p.root.clone())).collect();
        let mut file = PathsFile::default();
        let mut custodian = BTreeMap::new();
        if let Some(s) = store {
            let p = paths_file(s)?;
            if let Ok(text) = std::fs::read_to_string(&p) {
                file = match crate::oneo::Doc::of_file(&p) {
                    Some((crate::oneo::Doc::Paths, _)) => crate::oneo::parse(crate::oneo::Doc::Paths, &p.display().to_string(), &text)?,
                    _ => toml::from_str(&text).map_err(|e| ReadingRefusal::SpecInvalid.refuse(format!("{}: {e}", p.display())))?,
                };
            }
            custodian.insert("specs".into(), s.dir_of(&named::QUEUED_SPEC, &[("name", "x")])?.parent().map(Path::to_path_buf).unwrap_or_default());
            custodian.insert("readings".into(), s.reading_dir("x")?.parent().map(Path::to_path_buf).unwrap_or_default());
            custodian.insert("receipts".into(), s.dir_of(&named::RECEIPT, &[("seq", "1")])?);
            custodian.insert("journal".into(), s.path(&named::JOURNAL, &[])?);
            for state in JobState::ALL {
                custodian.insert(format!("jobs-{}", state.as_str()), s.jobs_dir(*state)?);
            }
            custodian.insert("registry".into(), maintenance::registry::path(root));
        }
        Ok(PathsTable { root: root.to_path_buf(), crates, file, custodian })
    }

    /// The route a name names, or the refusal: an undeclared route name, or
    /// a raw path inside a job.
    pub fn resolve(&self, s: &str, how: Resolving) -> Result<Route> {
        let s = s.trim();
        let undeclared = |what: &str| ReadingRefusal::UndeclaredRoute.refuse(format!("`{s}`: {what}"));
        if let Some(rest) = s.strip_prefix("crate:") {
            let (name, file) = match rest.split_once('/') {
                Some((n, f)) => (n, Some(f)),
                None => (rest, None),
            };
            let key = name.replace('-', "_");
            let root = self.crates.get(&key).ok_or_else(|| undeclared("no workspace crate of that name"))?;
            let file = file.map(|f| root.join(f));
            if let Some(f) = &file {
                if !f.is_file() {
                    return Err(undeclared("no such file in the crate"));
                }
            }
            return Ok(Route { name: s.into(), place: Place::Crate { name: key, root: root.clone(), file } });
        }
        if let Some(name) = s.strip_prefix("decl:") {
            let globs = self.file.decl.get(name).ok_or_else(|| undeclared("not among the decl sets of the paths table (.custodian/config/v1.paths.1o.paths)"))?;
            return Ok(Route { name: s.into(), place: Place::Decl { name: name.into(), globs: globs.clone() } });
        }
        if let Some(part) = s.strip_prefix("custodian:") {
            let path = self.custodian.get(part).ok_or_else(|| undeclared("not a part of .custodian/ (or the project has no .custodian/)"))?;
            return Ok(Route { name: s.into(), place: Place::Custodian { part: part.into(), path: path.clone() } });
        }
        let raw = s.strip_prefix("path:").unwrap_or(s);
        match how {
            Resolving::Free => Ok(Route { name: format!("path:{raw}"), place: Place::Path { raw: raw.into() } }),
            Resolving::Job if self.file.path.iter().any(|p| p == raw) => Ok(Route { name: format!("path:{raw}"), place: Place::Path { raw: raw.into() } }),
            Resolving::Job => Err(ReadingRefusal::WalkedPathInJob.refuse(format!("`{s}`: declare it in [path] (or name a crate:/decl: route)"))),
        }
    }

    /// Every route the table declares, by name.
    pub fn routes(&self) -> Vec<(String, String)> {
        let rel = |p: &Path| p.strip_prefix(&self.root).unwrap_or(p).to_string_lossy().replace('\\', "/");
        let mut out: Vec<(String, String)> = self.crates.iter().map(|(n, r)| (format!("crate:{n}"), rel(r))).collect();
        out.extend(self.file.decl.iter().map(|(n, g)| (format!("decl:{n}"), g.join(", "))));
        out.extend(self.custodian.iter().map(|(n, p)| (format!("custodian:{n}"), rel(p))));
        out.extend(self.file.path.iter().map(|p| (format!("path:{p}"), p.clone())));
        out
    }
}

/// Whether a target is a place rather than a name (an item path, an env key,
/// a code): routes, and anything spelled like a file path.
pub fn names_a_place(target: &str) -> bool {
    let t = target.trim();
    ["crate:", "decl:", "path:", "custodian:"].iter().any(|p| t.starts_with(p)) || t.contains('/') || t.ends_with(".rs") || t.ends_with(".toml")
}
