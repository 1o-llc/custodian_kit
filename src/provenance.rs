// Copyright © 2026 1o. All rights reserved.

//! An experiment's provenance: a job opened over a specimen, everything that identifies what it started from, what
//! changed it and on whose authorization, what was measured, and what was decided, kept in the job's folder so a
//! rejected experiment is kept exactly as an accepted one is.
//!
//! A job is an experiment when `job start` names its specimen (the immutable baseline it was copied from: a folder or
//! an archive). At open the record holds the specimen's identity (a folder's tree digest and code digest, an
//! archive's digest), and the job's tree as it opens: its tree digest, its code digest (the one every preflight
//! binds), its lock, and every file that differs from the specimen (a lock put on the copy shows here, by name and
//! digest). Each measurement (`job measure`) is copied into the folder with its digest. At close the record adds the
//! tree as it closes, each change the job wrote (the spec's digest, the preflight receipt and its decision, the code
//! digest before and after, the writer), every file the job changed, and the disposition: the evidence's result, or a
//! rejection the experimenter gave with its reason. Nothing here decides `accepted`: only the job's evidence does.
//!
//! The tree digest covers every file of a tree but `target`, `.git`, `.passenger`, custodian's own folder and the
//! one file `init` writes at the root (`oneo.project.json`): the specimen and a governed copy of it read the same
//! until something changes the copy.

use crate::preflight::LockState;
use crate::reading::refusal::ReadingRefusal;
use crate::store::{write_whole, Receipt};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const PROVENANCE_SCHEMA: &str = "custodian.provenance.v1";
/// The record, in the job folder's data.
pub const PROVENANCE_FILE: &str = "provenance.json";
/// The trees' manifests (`<rel path>\t<sha256>` a line), beside the record.
pub const TREE_FILES: [&str; 3] = ["tree-specimen.tsv", "tree-open.tsv", "tree-close.tsv"];
/// Measurements, copied into the job folder's data.
pub const MEASUREMENTS_DIR: &str = "measurements";

/// What a tree digest leaves out: build output, version control, the passenger's store, custodian's own folder and
/// the schema pointer `init` writes at the root.
const LEFT_OUT_DIRS: &[&str] = &["target", ".git", ".passenger", crate::store::FOLDER];
const LEFT_OUT_ROOT_FILES: &[&str] = &["oneo.project.json"];

/// A tree as it was read: every file's digest, digested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeState {
    pub utc: String,
    /// `sha256:<hex>` over every `(path, sha256)` in path order.
    pub tree_digest: String,
    pub files: usize,
    /// The code digest (the one a preflight binds); `None` when the tree is no workspace klepto reads.
    pub code_digest: Option<String>,
    pub lock: Option<LockState>,
}

/// The specimen: what the experiment's tree was copied from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Specimen {
    /// As named on the line.
    pub path: String,
    /// `folder` or `archive`.
    pub form: String,
    /// A folder's tree state; an archive's digest is `tree_digest` with `files` 1.
    pub state: TreeState,
}

/// One change the job wrote, from its transform record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub spec_name: String,
    pub spec_digest: String,
    pub operation: String,
    pub target: String,
    /// The preflight receipt it was written on, and that receipt's decision.
    pub preflight_seq: Option<i64>,
    pub preflight_decision: Option<String>,
    pub code_digest: Option<String>,
    pub code_digest_after: Option<String>,
    pub lock: Option<LockState>,
    pub writer: Option<String>,
    pub written: bool,
    /// Each file: `(path, before, after)` digests.
    pub files: Vec<(String, String, String)>,
    /// The transform's receipt and its record, relative to the root.
    pub receipt: i64,
    pub record: Option<String>,
}

/// A measurement kept with the experiment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measurement {
    pub label: String,
    /// The file as given.
    pub source: String,
    /// Its copy, relative to the job's folder (`data/measurements/<label>/<name>`): the folder moves from running to
    /// done at close, the path in it does not.
    pub file: String,
    pub digest: String,
    pub bytes: u64,
    pub utc: String,
}

/// What was decided, and on what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disposition {
    /// The job's result: `accepted`, `rejected`, `failed_test`, `pending`.
    pub result: String,
    pub reason: Option<String>,
    /// `evidence` (the job's verifications decided it) or `experimenter` (a rejection given at close).
    pub by: String,
    /// The closing receipt.
    pub receipt: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub schema: String,
    pub job: String,
    pub specimen: Option<Specimen>,
    pub open: TreeState,
    /// The files that differ between the specimen and the tree at open: `+path` added, `-path` gone, `~path` changed.
    pub open_vs_specimen: Vec<String>,
    #[serde(default)]
    pub measurements: Vec<Measurement>,
    #[serde(default)]
    pub close: Option<TreeState>,
    /// The files the job changed between its open and its close, written the same way.
    #[serde(default)]
    pub close_vs_open: Vec<String>,
    #[serde(default)]
    pub changes: Vec<Change>,
    #[serde(default)]
    pub disposition: Option<Disposition>,
}

/// Every file of the tree at `root` but what a tree digest leaves out, each with its sha256, in path order. A symlink
/// is a file whose content is the path it points at; it is never followed.
pub fn tree_files(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    let walk = walkdir::WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| e.depth() == 0 || !(e.file_type().is_dir() && e.file_name().to_str().is_some_and(|n| LEFT_OUT_DIRS.contains(&n))));
    for e in walk {
        let e = e.map_err(|e| ReadingRefusal::MissingInput.refuse(e))?;
        if e.file_type().is_dir() {
            continue;
        }
        let rel = e.path().strip_prefix(root).unwrap_or(e.path()).to_string_lossy().replace('\\', "/");
        if e.depth() == 1 && LEFT_OUT_ROOT_FILES.contains(&rel.as_str()) {
            continue;
        }
        let bytes = if e.file_type().is_symlink() {
            std::fs::read_link(e.path()).map(|t| t.to_string_lossy().into_owned().into_bytes())
        } else {
            std::fs::read(e.path())
        }
        .map_err(|x| ReadingRefusal::MissingInput.refuse(format!("{}: {x}", e.path().display())))?;
        files.insert(rel, crate::digest::tagged(&bytes));
    }
    Ok(files)
}

/// The state of the tree at `root`, with its file list.
pub fn tree_state(root: &Path) -> Result<(TreeState, BTreeMap<String, String>)> {
    let files = tree_files(root)?;
    let code = crate::code_digest(root, &crate::reading::spec::Scope { tests: true, ..Default::default() }.scan_options()).ok();
    let state = TreeState { utc: crate::runs::now_utc(), tree_digest: crate::digest::of_value(&files), files: files.len(), code_digest: code, lock: crate::preflight::lock_of(root) };
    Ok((state, files))
}

/// How two file lists differ: `+path` in `b` only, `-path` in `a` only, `~path` in both with other contents.
pub fn differences(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (p, d) in b {
        match a.get(p) {
            None => out.push(format!("+{p}")),
            Some(x) if x != d => out.push(format!("~{p}")),
            _ => {}
        }
    }
    out.extend(a.keys().filter(|p| !b.contains_key(*p)).map(|p| format!("-{p}")));
    out.sort_by(|x, y| x[1..].cmp(&y[1..]));
    out
}

fn data(folder: &Path) -> PathBuf {
    folder.join(crate::jobs::JOB_DATA)
}

fn write_tsv(path: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    let text: String = files.iter().map(|(p, d)| format!("{p}\t{d}\n")).collect();
    write_whole(path, text.as_bytes())
}

fn read_tsv(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path).map(|t| t.lines().filter_map(|l| l.split_once('\t')).map(|(p, d)| (p.to_string(), d.to_string())).collect()).unwrap_or_default()
}

/// The record of the job whose folder is `folder`, when it is an experiment.
pub fn read(folder: &Path) -> Option<Provenance> {
    serde_json::from_str(&std::fs::read_to_string(data(folder).join(PROVENANCE_FILE)).ok()?).ok()
}

fn keep(folder: &Path, p: &Provenance) -> Result<PathBuf> {
    let file = data(folder).join(PROVENANCE_FILE);
    write_whole(&file, (serde_json::to_string_pretty(p)? + "\n").as_bytes())?;
    Ok(file)
}

/// Opens the record of job `job` (folder `folder`) over the tree at `root`, its specimen at `specimen`: the
/// specimen's identity, the tree as it opens, the files that differ.
pub fn open(folder: &Path, root: &Path, job: &str, specimen: &Path) -> Result<Provenance> {
    let (open, files) = tree_state(root)?;
    let d = data(folder);
    write_tsv(&d.join(TREE_FILES[1]), &files)?;
    let (spec, vs) = if specimen.is_dir() {
        let (s, sf) = tree_state(specimen)?;
        write_tsv(&d.join(TREE_FILES[0]), &sf)?;
        (Specimen { path: specimen.display().to_string(), form: "folder".into(), state: s }, differences(&sf, &files))
    } else if specimen.is_file() {
        let bytes = std::fs::read(specimen).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", specimen.display())))?;
        let s = TreeState { utc: crate::runs::now_utc(), tree_digest: crate::digest::tagged(&bytes), files: 1, code_digest: None, lock: None };
        (Specimen { path: specimen.display().to_string(), form: "archive".into(), state: s }, Vec::new())
    } else {
        return Err(ReadingRefusal::MissingInput.refuse(format!("the specimen {} is neither a folder nor a file", specimen.display())));
    };
    let p = Provenance { schema: PROVENANCE_SCHEMA.into(), job: job.into(), specimen: Some(spec), open, open_vs_specimen: vs, measurements: Vec::new(), close: None, close_vs_open: Vec::new(), changes: Vec::new(), disposition: None };
    keep(folder, &p)?;
    Ok(p)
}

/// Keeps `file` with the experiment of the job at `folder`: copied into its data under `label`, its digest recorded.
pub fn measure(folder: &Path, _root: &Path, file: &Path, label: &str) -> Result<Measurement> {
    let mut p = read(folder).ok_or_else(|| ReadingRefusal::JobState.refuse(format!("{}: the job is no experiment (job start --specimen names one)", folder.display())))?;
    let label = crate::jobs::slug_of(label);
    let bytes = std::fs::read(file).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", file.display())))?;
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "measurement".into());
    let to = data(folder).join(MEASUREMENTS_DIR).join(&label).join(&name);
    if to.exists() && std::fs::read(&to).ok().as_deref() != Some(bytes.as_slice()) {
        return Err(ReadingRefusal::Write.refuse(format!("{}: a measurement of that label and name is kept already with other contents; give it another label", to.display())));
    }
    write_whole(&to, &bytes)?;
    let m = Measurement { label, source: file.display().to_string(), file: to.strip_prefix(folder).unwrap_or(&to).to_string_lossy().replace('\\', "/"), digest: crate::digest::tagged(&bytes), bytes: bytes.len() as u64, utc: crate::runs::now_utc() };
    p.measurements.retain(|x| !(x.label == m.label && x.file == m.file));
    p.measurements.push(m.clone());
    keep(folder, &p)?;
    Ok(m)
}

/// Each change the job wrote, from its transform receipts' records, with the decision of the preflight each went on.
pub fn changes(root: &Path, transforms: &[i64], runs: &[Receipt]) -> Vec<Change> {
    let mut out = Vec::new();
    for seq in transforms {
        let Some(r) = runs.iter().find(|r| r.seq == *seq) else { continue };
        let record = r.outputs.iter().find(|o| o.ends_with(".json")).cloned();
        let Some(t) = record.as_ref().and_then(|o| std::fs::read_to_string(root.join(o)).ok()).and_then(|t| serde_json::from_str::<crate::transform::TransformRecord>(&t).ok()) else { continue };
        let decision = t.preflight_seq.and_then(|n| runs.iter().find(|x| x.seq == n)).map(|x| x.reason.clone());
        out.push(Change {
            spec_name: t.spec_name,
            spec_digest: t.spec_digest,
            operation: t.operation,
            target: t.target,
            preflight_seq: t.preflight_seq,
            preflight_decision: decision,
            code_digest: t.code_digest,
            code_digest_after: t.code_digest_after,
            lock: t.lock,
            writer: t.writer,
            written: t.written,
            files: t.files.iter().map(|f| (f.path.clone(), f.before.clone(), f.after.clone())).collect(),
            receipt: *seq,
            record,
        });
    }
    out
}

/// Closes the record of the job now at `folder` (its done folder): the tree as it closes, the files the job changed,
/// its changes, and the disposition. `None` when the job is no experiment.
pub fn close(folder: &Path, root: &Path, closing: &crate::jobs::Closing, runs: &[Receipt], rejected_by_experimenter: bool, closing_seq: Option<i64>) -> Result<Option<Provenance>> {
    let Some(mut p) = read(folder) else { return Ok(None) };
    let (state, files) = tree_state(root)?;
    let d = data(folder);
    write_tsv(&d.join(TREE_FILES[2]), &files)?;
    p.close_vs_open = differences(&read_tsv(&d.join(TREE_FILES[1])), &files);
    p.close = Some(state);
    p.changes = changes(root, &closing.transforms, runs);
    p.disposition = Some(Disposition { result: closing.result.clone(), reason: closing.result_reason.clone(), by: if rejected_by_experimenter { "experimenter".into() } else { "evidence".into() }, receipt: closing_seq });
    keep(folder, &p)?;
    Ok(Some(p))
}

/// The running job of the project at `root` that is an experiment (its folder holds a provenance record), when one
/// is open: while it is, its tree is written only by a transform on an authorized preflight.
pub fn running_experiment(root: &Path) -> Option<(String, PathBuf)> {
    let store = crate::store::Store::find(root)?;
    let dir = store.jobs_dir(crate::store::JobState::Running).ok()?;
    let mut folders: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| data(p).join(PROVENANCE_FILE).is_file()).collect();
    folders.sort();
    let folder = folders.into_iter().next()?;
    let job = read(&folder).map(|p| p.job).unwrap_or_default();
    Some((job, folder))
}

/// The record in lines a person reads.
pub fn render(p: &Provenance) -> String {
    let mut s = String::from("## provenance\n\n");
    if let Some(sp) = &p.specimen {
        s.push_str(&format!("- specimen ({}): `{}` {} ({} files{})\n", sp.form, sp.path, sp.state.tree_digest, sp.state.files, sp.state.code_digest.as_ref().map(|c| format!(", code {c}")).unwrap_or_default()));
    }
    let lock = |l: &Option<LockState>| l.as_ref().map(|l| format!("{} {}", l.path, l.digest)).unwrap_or_else(|| "no Cargo.lock".into());
    s.push_str(&format!("- open: tree {} ({} files), code {}, {}\n", p.open.tree_digest, p.open.files, p.open.code_digest.clone().unwrap_or_else(|| "-".into()), lock(&p.open.lock)));
    if !p.open_vs_specimen.is_empty() {
        s.push_str(&format!("- differs from the specimen at open: {}\n", p.open_vs_specimen.join(", ")));
    }
    for c in &p.changes {
        s.push_str(&format!(
            "- change {} ({} {}): spec {}, preflight #{} {}, code {} → {}, writer {}, {}\n",
            c.spec_name,
            c.operation,
            c.target,
            c.spec_digest,
            c.preflight_seq.map(|n| n.to_string()).unwrap_or_else(|| "-".into()),
            c.preflight_decision.clone().unwrap_or_else(|| "-".into()),
            c.code_digest.clone().unwrap_or_else(|| "-".into()),
            c.code_digest_after.clone().unwrap_or_else(|| "-".into()),
            c.writer.clone().unwrap_or_else(|| "-".into()),
            if c.written { "written" } else { "not written" }
        ));
    }
    for m in &p.measurements {
        s.push_str(&format!("- measurement {}: `{}` {} ({} bytes)\n", m.label, m.file, m.digest, m.bytes));
    }
    if let Some(c) = &p.close {
        s.push_str(&format!("- close: tree {} ({} files), code {}, {}\n", c.tree_digest, c.files, c.code_digest.clone().unwrap_or_else(|| "-".into()), lock(&c.lock)));
        s.push_str(&format!("- changed by the job: {}\n", if p.close_vs_open.is_empty() { "nothing".to_string() } else { p.close_vs_open.join(", ") }));
    }
    if let Some(d) = &p.disposition {
        s.push_str(&format!("- disposition: {} by {}{}\n", d.result, d.by, d.reason.as_ref().map(|r| format!(" ({r})")).unwrap_or_default()));
    }
    s
}
