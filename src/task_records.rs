//! A job is a task history, in James's own format (`task_history`,
//! `nodeplan.1o.diff.v1`). The project's history root is
//! `.custodian/data/history/`: one task directory per job name, one revision
//! per run of it (`base`, then `fix1`, ...).
//!
//! - `job start` writes the task (title, status `candidate`, its kind as the
//!   parent milestone, tags) and the intent.
//! - each step writes a solution (the spec run, its run number and reading),
//!   and, for the files a step changed, change records (path, operation, the
//!   digests before and after: before from the job's open count, after from
//!   the file as it is).
//! - each test or check the job ran writes a verification (check, outcome).
//! - `job close` writes the result (status, and the tree digest: the code's
//!   index key) and refreshes the history's index.
//! - `job brief` reads them back through `load_task_history`.
//!
//! These are the durable task context; the receipts and the journal stay as
//! they are beside them.

use crate::reading::count::CountReport;
use crate::reading::refusal::ReadingRefusal;
use crate::store::{named, Store};
use crate::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use task_history::{DiffDocument, DiffRecordKind};

fn refuse(e: String) -> crate::CustodianError {
    ReadingRefusal::Write.refuse(format!("task history: {e}"))
}

/// `.custodian/data/history/`.
pub fn root(store: &Store) -> Result<PathBuf> {
    Ok(store.path(&named::HISTORY_INDEX, &[])?.parent().map(Path::to_path_buf).unwrap_or_default())
}

/// A job name as a task directory: lowercase letters, digits, `_` and `-`.
pub fn task_id(name: &str) -> String {
    let t: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else if c == '_' { '_' } else { '-' }).collect();
    let t = t.trim_matches('-').to_string();
    if t.is_empty() {
        "job".into()
    } else {
        t
    }
}

/// The task and revision a job writes into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRef {
    pub root: PathBuf,
    pub task_id: String,
    pub revision: String,
}

impl TaskRef {
    pub fn write(&self, doc: DiffDocument) -> Result<DiffDocument> {
        task_history::write_record(&self.root, &self.task_id, &self.revision, doc).map_err(refuse)
    }
}

/// `job start`: a new revision of the job's task, its task and intent records.
pub fn open(store: &Store, name: &str, kind: &str, job_id: &str, intent: &str) -> Result<TaskRef> {
    let root = root(store)?;
    let task_id = task_id(name);
    let revision = task_history::next_revision(&root.join(&task_id));
    let t = TaskRef { root, task_id, revision };
    t.write(DiffDocument::record(DiffRecordKind::Task).with("title", name).with("parent_milestone", kind).with("status", "candidate").with("tag", "custodian").with("tag", kind).with("tag", job_id))?;
    t.write(DiffDocument::record(DiffRecordKind::Intent).with("intent", intent))?;
    Ok(t)
}

/// The task a running or done job writes into, from its job file.
pub fn of_job(store: &Store, task_id: Option<&str>, revision: Option<&str>) -> Result<Option<TaskRef>> {
    match (task_id, revision) {
        (Some(t), Some(r)) => Ok(Some(TaskRef { root: root(store)?, task_id: t.to_string(), revision: r.to_string() })),
        _ => Ok(None),
    }
}

/// A step run: its solution record.
pub fn solution(t: &TaskRef, text: &str) -> Result<DiffDocument> {
    t.write(DiffDocument::record(DiffRecordKind::Solution).with("solution", text))
}

/// The files that differ between two counts of the tree (by their digests),
/// as change records: the digest before from `before`, after from the file as it is.
pub fn changes(t: &TaskRef, tree_root: &Path, before: &CountReport, after: &CountReport) -> Result<Vec<DiffDocument>> {
    let digests = |c: &CountReport| -> BTreeMap<String, String> { c.crates.iter().flat_map(|k| k.file_counts.iter()).filter(|f| !f.path.contains('#')).map(|f| (f.path.clone(), f.digest.clone())).collect() };
    let (b, a) = (digests(before), digests(after));
    let mut paths: Vec<&String> = b.keys().chain(a.keys()).collect();
    paths.sort();
    paths.dedup();
    let mut out = Vec::new();
    for p in paths {
        if b.get(p) == a.get(p) {
            continue;
        }
        let before_digest = b.get(p).cloned().unwrap_or_else(|| "none".to_string());
        out.push(t.write(task_history::change_record(tree_root, p, &before_digest))?);
    }
    Ok(out)
}

/// A test or check the job ran: its verification record.
pub fn verification(t: &TaskRef, check: &str, passed: Option<bool>, note: Option<&str>) -> Result<DiffDocument> {
    let outcome = match passed {
        Some(true) => "pass",
        Some(false) => "fail",
        None => "pending",
    };
    let mut d = DiffDocument::record(DiffRecordKind::Verification).with("check", check).with("outcome", outcome);
    if let Some(n) = note {
        d = d.with("note", n);
    }
    t.write(d)
}

/// `job close`: the result (status and the tree's digest, the hex of the
/// code's index key), and the history's index refreshed.
pub fn close(t: &TaskRef, status: &str, tree_digest: Option<&str>) -> Result<DiffDocument> {
    close_noted(t, status, tree_digest, None)
}

/// [`close`], with why the status is what it is (`pending`: no verification declared; `rejected`: lines no
/// transform covers) as the result record's `note`.
pub fn close_noted(t: &TaskRef, status: &str, tree_digest: Option<&str>, note: Option<&str>) -> Result<DiffDocument> {
    let digest = tree_digest.map(|d| d.trim_start_matches("sha256:").to_string()).filter(|d| d.len() == 64).unwrap_or_else(|| "pending".to_string());
    let mut doc = DiffDocument::record(DiffRecordKind::Result).with("status", status).with("resulting_tree_digest", digest);
    if let Some(n) = note {
        doc = doc.with("note", n);
    }
    let r = t.write(doc)?;
    task_history::write_index(&t.root).map_err(refuse)?;
    Ok(r)
}

/// The task's records, every revision, as `load_task_history` reads them.
pub fn read(store: &Store, task_id: &str) -> Result<Option<task_history::TaskHistory>> {
    let root = root(store)?;
    if !root.exists() {
        return Ok(None);
    }
    let index = task_history::load_task_history(&root).map_err(|e| ReadingRefusal::ReadingUnreadable.refuse(format!("task history: {e}")))?;
    Ok(index.tasks.into_iter().find(|t| t.task_directory == task_id))
}

/// The brief's lines for a task: per revision, its records by kind.
pub fn brief_lines(task: &task_history::TaskHistory) -> Vec<String> {
    let mut out = vec![format!("task history: {} ({} revisions)", task.task_directory, task.revisions.len())];
    for r in &task.revisions {
        let mut line = format!("- {}:", r.name);
        for d in &r.documents {
            let Ok(kind) = d.kind() else { continue };
            let what = match kind {
                DiffRecordKind::Task => format!("task {} ({})", d.one("title").unwrap_or(""), d.one("status").unwrap_or("")),
                DiffRecordKind::Intent => format!("intent: {}", d.one("intent").unwrap_or("")),
                DiffRecordKind::Solution => format!("step: {}", d.one("solution").unwrap_or("")),
                DiffRecordKind::Change => format!("{} {}", d.one("operation").unwrap_or(""), d.one("path").unwrap_or("")),
                DiffRecordKind::Verification => format!("{} {}", d.one("check").unwrap_or(""), d.one("outcome").unwrap_or("")),
                DiffRecordKind::Result => format!("result {} (tree {})", d.one("status").unwrap_or(""), crate::digest::short(d.one("resulting_tree_digest").unwrap_or(""), 12)),
                other => other.as_str().to_string(),
            };
            line.push_str(&format!("\n  - {what}"));
        }
        out.push(line);
    }
    out
}
