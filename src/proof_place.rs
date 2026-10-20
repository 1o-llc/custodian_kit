// Copyright © 2026 1o. All rights reserved.

//! The proof's one place per workspace, and the one build kept there: a copy of the workspace's source at a path
//! that does not change, and one build folder beside it, kept between runs. Every cargo line that proves or tests a
//! workspace on this machine runs there: code_passenger's proof (its `ProofDir` holds this place) and custodian's
//! `test run`. A run syncs the copy (only the files whose bytes differ are written, so cargo's fingerprints of
//! everything else hold) and runs cargo with `CARGO_TARGET_DIR` on the kept build and [`PROOF_ENV`]: a crate whose
//! source did not move is not compiled again. One run at a time holds the place (a lock file, let go when the run
//! ends, by the OS if it dies).
//!
//! The place is `$CODE_PASSENGER_PROOF_DIR` when set, else `<temp>/code_passenger-proof/<name>-<digest of the
//! workspace's path>`: `src/` the copy, `target/` the build, `workspace` the path it copies. Never inside the
//! workspace's `target/`: klepto reads no source under a `target` folder, so a scan of the copy would find nothing
//! there. A place whose workspace is gone (a test's fixture, a removed worktree) is taken away when another is taken.

use crate::reading::refusal::ReadingRefusal;
use crate::Result;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// What every cargo line on the kept build runs with besides its build folder: no debug info (a proof runs nothing
/// under a debugger, and debug info is most of a test build's bytes). The same for every line, so one build serves
/// them all.
pub const PROOF_ENV: &[(&str, &str)] = &[("CARGO_PROFILE_DEV_DEBUG", "0"), ("CARGO_PROFILE_TEST_DEBUG", "0")];

/// The variable naming the place.
pub const PROOF_DIR_ENV: &str = "CODE_PASSENGER_PROOF_DIR";

/// The place for one workspace, held.
pub struct ProofPlace {
    workspace: PathBuf,
    src: PathBuf,
    target: PathBuf,
    /// Held while the run lasts.
    _lock: std::fs::File,
}

fn io(e: impl std::fmt::Display) -> crate::CustodianError {
    ReadingRefusal::Write.refuse(e)
}

/// The places under `all` whose workspace no longer exists, taken away (each held first, so one in use stays).
fn prune(all: &Path) {
    let Ok(entries) = std::fs::read_dir(all) else { return };
    for e in entries.flatten() {
        let dir = e.path();
        let Ok(ws) = std::fs::read_to_string(dir.join("workspace")) else { continue };
        if Path::new(ws.trim()).exists() {
            continue;
        }
        let Ok(lock) = std::fs::OpenOptions::new().write(true).open(dir.join("lock")) else { continue };
        if lock.try_lock().is_ok() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// What the copy leaves out: the builds, git, the passenger store.
fn skip_dir(name: &std::ffi::OsStr) -> bool {
    name == "target" || name == ".git" || name == ".passenger"
}

/// Where the place of `workspace` is (its root, canonical), held or not.
pub fn place_of(workspace: &Path) -> PathBuf {
    let workspace = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    match std::env::var_os(PROOF_DIR_ENV) {
        Some(d) => PathBuf::from(d),
        None => {
            let key = stable_digest::StableDigest::sha256(workspace.to_string_lossy().as_bytes()).as_str().chars().take(16).collect::<String>();
            let name = workspace.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            std::env::temp_dir().join("code_passenger-proof").join(format!("{name}-{key}"))
        }
    }
}

impl ProofPlace {
    /// The place for `workspace` (its root), held until dropped, the copy synced with the workspace as it is; refused
    /// on a dying battery ([`crate::power`]), as the cargo lines it is taken for would be.
    pub fn take(workspace: &Path) -> Result<Self> {
        // the place is taken to run cargo: James's battery rule first
        crate::power::cargo_may_start()?;
        let workspace = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
        if std::env::var_os(PROOF_DIR_ENV).is_none() {
            prune(&std::env::temp_dir().join("code_passenger-proof"));
        }
        let base = place_of(&workspace);
        std::fs::create_dir_all(&base).map_err(io)?;
        std::fs::write(base.join("workspace"), workspace.to_string_lossy().as_bytes()).map_err(io)?;
        let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(base.join("lock")).map_err(io)?;
        lock.lock().map_err(|e| io(format!("{}: {e}", base.join("lock").display())))?;
        let me = ProofPlace { workspace, src: base.join("src"), target: base.join("target"), _lock: lock };
        me.sync()?;
        Ok(me)
    }

    /// The workspace the place copies.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// The copy of the workspace's root.
    pub fn root(&self) -> &Path {
        &self.src
    }

    /// The kept build folder: every cargo line here runs with it as `CARGO_TARGET_DIR`.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// The environment every cargo line here runs with: the kept build, and [`PROOF_ENV`].
    pub fn env(&self) -> Vec<(String, String)> {
        let mut v = vec![("CARGO_TARGET_DIR".to_string(), self.target.display().to_string())];
        v.extend(PROOF_ENV.iter().map(|(k, x)| (k.to_string(), x.to_string())));
        v
    }

    /// The copy made the same as the workspace: a file whose bytes differ written, a file the workspace no longer
    /// has removed, nothing else touched.
    pub fn sync(&self) -> Result<()> {
        let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
        let walk = WalkDir::new(&self.workspace).into_iter().filter_entry(|e| !(e.depth() > 0 && e.file_type().is_dir() && skip_dir(e.file_name())));
        for entry in walk.filter_map(|e| e.ok()) {
            if entry.depth() == 0 || entry.file_type().is_symlink() {
                continue;
            }
            let rel = entry.path().strip_prefix(&self.workspace).map_err(io)?.to_path_buf();
            let to = self.src.join(&rel);
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&to).map_err(io)?;
            } else if entry.file_type().is_file() {
                let bytes = std::fs::read(entry.path()).map_err(io)?;
                if std::fs::read(&to).ok().as_deref() != Some(&bytes[..]) {
                    if let Some(d) = to.parent() {
                        std::fs::create_dir_all(d).map_err(io)?;
                    }
                    std::fs::write(&to, &bytes).map_err(io)?;
                }
            }
            seen.insert(rel);
        }
        // what the workspace no longer has goes from the copy too (a file a proof made, a file deleted since)
        let stale: Vec<PathBuf> = WalkDir::new(&self.src)
            .contents_first(true)
            .into_iter()
            .filter_entry(|e| !(e.depth() > 0 && e.file_type().is_dir() && skip_dir(e.file_name())))
            .filter_map(|e| e.ok())
            .filter(|e| e.depth() > 0)
            .filter_map(|e| e.path().strip_prefix(&self.src).ok().map(Path::to_path_buf))
            .filter(|rel| !seen.contains(rel))
            .collect();
        for rel in stale {
            let p = self.src.join(&rel);
            if p.is_dir() {
                let _ = std::fs::remove_dir(&p);
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
        Ok(())
    }
}
