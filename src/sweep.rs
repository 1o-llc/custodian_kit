//! The sweep: the one place custodian walks a tree. A read-only pass over a
//! raw path nothing declares, listing what is there (files by extension, the
//! crates whose manifests it finds, the directories holding TOML declaration
//! files) and offering the routes a project could add to its paths table.
//! It executes nothing and changes nothing; a job cannot run it, since a job
//! reads declared routes only.

use crate::paths::PathsTable;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Swept {
    pub root: String,
    pub files: usize,
    pub by_extension: BTreeMap<String, usize>,
    /// Directories (relative) holding a Cargo.toml.
    pub crates: Vec<String>,
    /// Directory → its TOML files.
    pub toml_dirs: BTreeMap<String, usize>,
    /// Routes not in the paths table, as lines to add under `[decl]`.
    pub offered: Vec<String>,
}

fn skipped(name: &str) -> bool {
    matches!(name, "target" | ".git" | "node_modules" | crate::store::FOLDER)
}

/// Walks `dir` (relative to `root` or absolute), never following links.
pub fn sweep(root: &Path, dir: &Path, table: Option<&PathsTable>) -> Result<Swept> {
    let base = if dir.is_absolute() { dir.to_path_buf() } else { root.join(dir) };
    let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/");
    let mut s = Swept { root: rel(&base), ..Default::default() };
    for e in WalkDir::new(&base).follow_links(false).into_iter().filter_entry(|e| !e.file_name().to_str().is_some_and(skipped)).filter_map(|e| e.ok()) {
        if !e.file_type().is_file() {
            continue;
        }
        s.files += 1;
        let p = e.path();
        let ext = p.extension().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| "(none)".into());
        *s.by_extension.entry(ext.clone()).or_default() += 1;
        if p.file_name().is_some_and(|n| n == "Cargo.toml") {
            s.crates.push(rel(p.parent().unwrap_or(root)));
        } else if ext == "toml" {
            *s.toml_dirs.entry(rel(p.parent().unwrap_or(root))).or_default() += 1;
        }
    }
    s.crates.sort();
    let declared: Vec<String> = table.map(|t| t.file.decl.values().flatten().cloned().collect()).unwrap_or_default();
    for (d, n) in &s.toml_dirs {
        let glob = format!("{d}/*.toml");
        if !declared.contains(&glob) {
            let name = d.rsplit('/').next().unwrap_or(d).replace(['.', ' '], "_");
            s.offered.push(format!("{name} = [\"{glob}\"]  # {n} files"));
        }
    }
    Ok(s)
}

pub fn page(s: &Swept) -> String {
    let mut out = format!("# sweep — {}\n\n- files: {}\n- crates (a Cargo.toml): {}\n- directories with TOML files: {}\n\n## by extension\n\n", s.root, s.files, s.crates.len(), s.toml_dirs.len());
    for (e, n) in &s.by_extension {
        out.push_str(&format!("- {e}: {n}\n"));
    }
    out.push_str("\n## crates\n\n");
    for c in &s.crates {
        out.push_str(&format!("- {c}\n"));
    }
    out.push_str("\n## routes to declare (add under [decl] in .custodian/config/paths.toml)\n\n```toml\n[decl]\n");
    for o in &s.offered {
        out.push_str(&format!("{o}\n"));
    }
    out.push_str("```\n");
    out
}
