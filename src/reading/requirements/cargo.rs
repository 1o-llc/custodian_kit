//! What cargo and the toolchain say, each a declared line ([`crate::exec`]) run in the workspace's directory:
//! `cargo metadata --frozen` (the resolve, no lock written, no network), `--no-deps` when that is refused (the members
//! as cargo reads them), `rustc -vV` and `rustup show active-toolchain`. The JSON is klepto's to read
//! ([`klepto::CargoWorkspace`]); nothing here recomputes what cargo answers.

use super::manifest::Scan;
use super::{finding, Basis, Check, Face, Finding, Ran, ReadFrom, Severity, Status, Toolchain};
use crate::exec::{self, Cargo, Plan, Rustc, Rustup, Watch};
use klepto::CargoWorkspace;
use std::path::{Path, PathBuf};

/// The first lines of what a line said on stderr: cargo's reason.
fn said(stderr: &str, n: usize) -> String {
    stderr.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("Blocking") && !l.starts_with("Updating")).take(n).collect::<Vec<_>>().join(" / ")
}

/// One declared line run in `dir`: its output, or `None` when it could not start (the refusal kept in `ran`).
pub fn run(id: &str, line: crate::Result<exec::Line>, dir: &Path, root: &Path, ran: &mut Vec<Ran>) -> Option<exec::Walked> {
    // cargo's words name files by their absolute path: written from the root, as the rest of the report is
    let here = format!("{}/", root.display());
    let said = |s: &str, n: usize| said(s, n).replace(&here, "");
    let shown = |l: &exec::Line| l.preview.display().to_string();
    let l = match line {
        Ok(l) => l.in_dir(dir),
        Err(e) => {
            ran.push(Ran { line: id.to_string(), dir: super::rel(root, dir), code: None, millis: 0, said: e.to_string() });
            return None;
        }
    };
    let display = shown(&l);
    match Plan::new(id).then(l).walk(Watch::Quiet, |_, _| {}) {
        Ok(mut w) if !w.is_empty() => {
            let w = w.remove(0);
            ran.push(Ran { line: display, dir: super::rel(root, dir), code: w.code, millis: w.millis, said: if w.code == Some(0) { String::new() } else { said(&w.stderr, 6) } });
            Some(w)
        }
        Ok(_) => None,
        Err(e) => {
            ran.push(Ran { line: display, dir: super::rel(root, dir), code: None, millis: 0, said: e.to_string() });
            None
        }
    }
}

/// What `cargo metadata` answered.
#[derive(Debug, Clone)]
pub struct Meta {
    pub manifest: PathBuf,
    /// `--frozen`: the resolve.
    pub resolved: Option<CargoWorkspace>,
    /// `--no-deps`, read when `--frozen` was refused.
    pub members_only: Option<CargoWorkspace>,
    /// What cargo said refusing `--frozen` (or why it did not run).
    pub refused: Option<String>,
    pub no_deps_refused: Option<String>,
}

impl Meta {
    /// The workspace as cargo read it: resolved, else members only.
    pub fn workspace(&self) -> Option<&CargoWorkspace> {
        self.resolved.as_ref().or(self.members_only.as_ref())
    }

    pub fn line(&self) -> String {
        match (&self.resolved, &self.members_only) {
            (Some(w), _) => format!("read (--frozen): {} packages, {} members", w.packages.len(), w.members.len()),
            (None, Some(w)) => format!("--frozen refused ({}); --no-deps read {} members", self.refused.clone().unwrap_or_default(), w.members.len()),
            (None, None) => format!("refused: {}", self.refused.clone().unwrap_or_default()),
        }
    }

    /// cargo's word as findings.
    pub fn findings(&self, scan: &Scan) -> Vec<Finding> {
        let at = Some(super::Place { file: scan.rel(&self.manifest), line: None, section: String::new(), field: String::new() });
        match (&self.resolved, &self.refused) {
            (Some(w), _) => vec![finding(Check::MetadataRead, Status::Satisfied, Basis::Observed, ReadFrom::Metadata, None, at, format!("cargo metadata --frozen read {} packages, {} of them members", w.packages.len(), w.members.len()), "cargo reads the workspace and resolves it from the lock as it is", format!("exit 0 at {}", scan.rel(&self.manifest)), "the manifests parse, inheritance resolves, the lock matches them: cargo's resolve is the truth for the rest of this reading", None)],
            (None, Some(r)) => {
                let lock = r.contains("lock file") || r.contains("--locked") || r.contains("--frozen");
                let offline = r.contains("offline") || r.contains("network");
                let (requirement, consequence, action) = if lock {
                    ("the lock matches the manifests", "cargo would write the lock to build; --frozen forbids it here, so the resolve was not read", "update the lock (`cargo update -p <name>`, or a build that writes it): it writes Cargo.lock, which this reading never does")
                } else if offline {
                    ("every package of the lock is in the local registry cache", "cargo needs the network to read it; this reading stays offline", "fetch it (`cargo fetch`), then read again")
                } else {
                    ("cargo reads the manifests", "nothing cargo resolves can be read until it is fixed: the scanner's findings stand in for it (their steps are the fix)", "")
                };
                let action = (!action.is_empty()).then(|| action.to_string());
                vec![finding(Check::MetadataRefused, Status::Unsatisfied, Basis::Observed, ReadFrom::Metadata, None, at, format!("cargo metadata --frozen refused: {r}"), requirement, format!("cargo's words: {r}"), consequence, action)]
            }
            (None, None) => Vec::new(),
        }
    }
}

/// `cargo metadata` of the workspace `manifest` belongs to: `--frozen`, then `--no-deps` if that is refused.
pub fn metadata(root: &Path, manifest: &Path, ran: &mut Vec<Ran>) -> Meta {
    let dir = manifest.parent().unwrap_or(root);
    let mut m = Meta { manifest: manifest.to_path_buf(), resolved: None, members_only: None, refused: None, no_deps_refused: None };
    let frozen = Cargo::metadata(manifest.as_os_str()).format_version("1").frozen();
    match run("cargo-metadata", exec::line("metadata", &frozen), dir, root, ran) {
        Some(w) if w.code == Some(0) => match CargoWorkspace::from_json(&w.stdout) {
            Ok(ws) => m.resolved = Some(ws),
            Err(e) => m.refused = Some(format!("its JSON did not read: {e}")),
        },
        Some(_) | None => m.refused = ran.last().map(|r| r.said.clone()),
    }
    if m.resolved.is_none() {
        let nodeps = Cargo::metadata(manifest.as_os_str()).format_version("1").no_deps();
        match run("cargo-metadata-no-deps", exec::line("metadata", &nodeps), dir, root, ran) {
            Some(w) if w.code == Some(0) => m.members_only = CargoWorkspace::from_json(&w.stdout).ok(),
            Some(_) => m.no_deps_refused = ran.last().map(|r| r.said.clone()),
            None => {}
        }
    }
    m
}

/// The scanner's errors against cargo's word, as one line; where they part, a finding says so (cargo's word stands).
pub fn agreement(scan: &Scan, meta: &Meta, findings: &mut Vec<Finding>) -> String {
    // the workspace cargo read is the one its manifest belongs to; an error in another workspace under the root is
    // not one cargo was asked about
    let read_root = scan.governing.get(&meta.manifest).cloned().flatten().unwrap_or_else(|| meta.manifest.clone());
    let in_read = |f: &Finding| {
        f.at.as_ref().map(|p| scan.root.join(&p.file)).is_none_or(|m| m == meta.manifest || scan.governing.get(&m).cloned().flatten().unwrap_or_else(|| m.clone()) == read_root)
    };
    let all: Vec<&Finding> = findings.iter().filter(|f| f.from == ReadFrom::Manifest && f.status == Status::Unsatisfied && f.severity == Severity::Error && f.face == Face::Prerequisites).collect();
    let elsewhere = all.iter().filter(|f| !in_read(f)).count();
    let others = if elsewhere == 0 { String::new() } else { format!("; {elsewhere} scanner error(s) are in other workspaces under the root, which cargo was not run on (read one: custodian requirements <its directory>)") };
    let errors: Vec<&Finding> = all.into_iter().filter(|f| in_read(f)).collect();
    let checks: Vec<String> = {
        let mut v: Vec<String> = errors.iter().map(|f| f.check.as_str().to_string()).collect();
        v.sort();
        v.dedup();
        v
    };
    let at = Some(super::Place { file: scan.rel(&meta.manifest), line: None, section: String::new(), field: String::new() });
    match (&meta.resolved, &meta.refused, errors.is_empty()) {
        (Some(_), _, true) => format!("agree: the scanner found no error in the workspace cargo read, and cargo read it{others}"),
        (Some(_), _, false) => {
            let n = errors.len();
            findings.push(finding(Check::ScannerAndCargoDisagree, Status::Unsatisfied, Basis::Observed, ReadFrom::Metadata, None, at, format!("the scanner holds {n} error(s) ({}) and cargo read the workspace", checks.join(", ")), "cargo's word stands", "cargo metadata --frozen exited 0", "cargo accepted what the scanner calls an error: the scanner's reading is wrong there, or cargo only warns (or the crate is outside the workspace cargo read)", None));
            format!("part: cargo read the workspace; the scanner holds {n} error(s) in it: {}{others}", checks.join(", "))
        }
        (None, Some(r), true) => {
            findings.push(finding(Check::ScannerAndCargoDisagree, Status::Unsatisfied, Basis::Observed, ReadFrom::Metadata, None, at, "cargo refused, and the scanner found no error", "cargo's word stands", format!("cargo: {r}"), "what cargo refused is something the scanner does not read; cargo's words above name it", None));
            format!("part: cargo refused and the scanner found nothing it reads{others}")
        }
        (None, Some(r), false) => {
            // what cargo's words name that the scanner's errors name too: a key (`hex`), else a manifest
            let named: Vec<String> = errors
                .iter()
                .filter_map(|f| f.at.as_ref())
                .filter_map(|p| {
                    if !p.field.is_empty() && r.contains(&format!("`{}`", p.field)) {
                        Some(format!("`{}`", p.field))
                    } else {
                        r.contains(&p.file).then(|| p.file.clone())
                    }
                })
                .collect();
            if named.is_empty() {
                format!("both refuse: the scanner on {}; cargo's first words name something else ({r}){others}", checks.join(", "))
            } else {
                format!("agree: cargo refused naming {}, as the scanner did{others}", named[0])
            }
        }
        (None, None, _) => "cargo was not read".into(),
    }
}

/// The toolchain files at or above `dir` (`rust-toolchain.toml`, `rust-toolchain`), the first one found, as written.
fn toolchain_file(dir: &Path) -> Option<String> {
    dir.ancestors().flat_map(|a| [a.join("rust-toolchain.toml"), a.join("rust-toolchain")]).find(|f| f.is_file()).and_then(|f| std::fs::read_to_string(&f).ok().map(|t| format!("{}: {}", f.display(), t.split_whitespace().collect::<Vec<_>>().join(" "))))
}

/// The toolchain files only (cargo not run).
pub fn toolchain_files(root: &Path) -> Toolchain {
    Toolchain { file: toolchain_file(root), ..Default::default() }
}

/// `rustc -vV` and `rustup show active-toolchain`, run in `dir` (the toolchain a directory gets is rustup's choice
/// by it), and the toolchain file above it.
pub fn toolchain(root: &Path, manifest_or_dir: &Path, ran: &mut Vec<Ran>) -> Toolchain {
    let dir = if manifest_or_dir.is_file() { manifest_or_dir.parent().unwrap_or(root) } else { manifest_or_dir };
    let mut t = Toolchain { file: toolchain_file(dir), ..Default::default() };
    if let Some(w) = run("rustc-version", exec::line("rustc", &Rustc::version_verbose()), dir, root, ran).filter(|w| w.code == Some(0)) {
        for l in w.stdout.lines() {
            if let Some(v) = l.strip_prefix("release: ") {
                t.release = Some(v.trim().to_string());
            }
            if let Some(v) = l.strip_prefix("host: ") {
                t.host = Some(v.trim().to_string());
            }
        }
    }
    if let Some(w) = run("rustup-active", exec::line("rustup", &Rustup::show().words(["active-toolchain"])), dir, root, ran).filter(|w| w.code == Some(0)) {
        t.active = w.stdout.lines().next().map(|l| l.trim().to_string());
    }
    t
}
