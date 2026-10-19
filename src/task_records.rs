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
//! they are beside them. The records a revision holds are revision_history's
//! ([`TaskRef`], [`solution`], [`verification`], [`close`]); the change
//! records from two counts are change_history's ([`changes`]); this module
//! names the project's history root.

use crate::store::{named, Store};
use crate::Result;
use std::path::{Path, PathBuf};

pub use change_history::changes;
pub use revision_history::{brief_lines, close, close_noted, solution, task_id, verification, TaskRef};

/// `.custodian/data/history/`.
pub fn root(store: &Store) -> Result<PathBuf> {
    Ok(store.path(&named::HISTORY_INDEX, &[])?.parent().map(Path::to_path_buf).unwrap_or_default())
}

/// `job start`: a new revision of the job's task, its task and intent records.
pub fn open(store: &Store, name: &str, kind: &str, job_id: &str, intent: &str) -> Result<TaskRef> {
    Ok(revision_history::open(&root(store)?, name, kind, job_id, intent)?)
}

/// The task a running or done job writes into, from its job file.
pub fn of_job(store: &Store, task_id: Option<&str>, revision: Option<&str>) -> Result<Option<TaskRef>> {
    match (task_id, revision) {
        (Some(t), Some(r)) => Ok(Some(TaskRef { root: root(store)?, task_id: t.to_string(), revision: r.to_string() })),
        _ => Ok(None),
    }
}

/// The task's records, every revision, as `load_task_history` reads them.
pub fn read(store: &Store, task_id: &str) -> Result<Option<task_history::TaskHistory>> {
    Ok(revision_history::read(&root(store)?, task_id)?)
}
