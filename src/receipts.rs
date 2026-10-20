//! Receipts: one run as the journal keeps it and as `receipts/<seq>.json`
//! holds it (`custodian.receipt.v1`, receipt_history's [`Receipt`]), how a
//! walked line becomes one, and how one is kept. The journal's tables, jobs
//! and test rows are receipt_history's too, opened through `store`.

use crate::exec::{Plan, Walked};
use crate::runs::{inventory, now_utc, InJob};
use crate::store::{named, write_whole, Store};
use crate::Result;
use std::path::{Path, PathBuf};

/// The receipt, its schema and its runners: receipt_history's.
pub use receipt_history::{line_runner, Receipt, RECEIPT_SCHEMA, RUNNER_IN_PROCESS, RUNNER_LINE};

/// The receipt of a walked line.
#[allow(clippy::too_many_arguments)]
pub fn receipt_of(plan: &Plan, w: &Walked, command: &str, name: &str, spec: Option<String>, spec_hash: Option<String>, code_digest: Option<String>, index_key: Option<String>, outputs: Vec<String>, job: Option<&InJob>) -> Receipt {
    Receipt {
        schema: RECEIPT_SCHEMA.to_string(),
        seq: 0,
        utc: now_utc(),
        command: command.to_string(),
        name: name.to_string(),
        spec,
        spec_hash,
        code_digest,
        index_key,
        plan_id: plan.plan.plan_id.clone(),
        // xccute's canonical forms of the plan, the line and the decision, digested one way
        plan_digest: crate::digest::tagged(plan.plan.stable_material()),
        operation_digest: crate::digest::tagged(w.operation.stable_material()),
        receipt_digest: crate::digest::tagged(w.receipt.stable_material()),
        line: w.operation.display.clone(),
        exit_code: w.code,
        accepted: w.decision.status.accepted,
        disposition: format!("{:?}", w.decision.disposition),
        reason: w.decision.reason.clone(),
        outputs,
        job: job.map(|j| j.id.clone()),
        reused_seq: None,
        millis: w.millis,
        runner: RUNNER_LINE.to_string(),
    }
}

/// Journals a receipt and writes `receipts/<seq>.json`; the inventory follows.
pub fn keep(store: &Store, r: Receipt) -> Result<(Receipt, PathBuf)> {
    let journal = store.journal()?;
    let r = journal.record(r)?;
    let file = store.path(&named::RECEIPT, &[("seq", &r.seq.to_string())])?;
    write_whole(&file, (serde_json::to_string_pretty(&r)? + "\n").as_bytes())?;
    inventory(store)?;
    Ok((r, file))
}


/// The words a reading run's receipt was written with besides the kit's `reading`: before the command line took the
/// kit's word, its own subcommand (`run`; and `count`, `diagnostics`, `bundle`, `sweep` when they answered a spec).
pub const OLDER_READING_WORDS: &[&str] = &["run", "count", "diagnostics", "bundle", "sweep"];

/// Whether a receipt is a reading run, made, reused or refused: the kit's word `reading`; `run`, the command line's
/// reading line before it; or another older word when the run answered a spec (it carries the spec's hash).
pub fn is_reading_run(r: &Receipt) -> bool {
    r.command == "reading" || r.command == "run" || (OLDER_READING_WORDS.contains(&r.command.as_str()) && r.spec_hash.is_some())
}

/// Where a reading's files are, relative to the project root: its record (the facts) and its page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReadingFiles {
    pub record: String,
    pub page: String,
}

/// The reading a receipt made, the one rule for it: a run that read a spec (`reading`, or `run` before the kit's
/// word), accepted, not a reuse, with the spec's hash, whose record (its `.json` output) is where the receipt says,
/// else in its job's folder wherever that folder is now (`readings/<name>/<file>`, the receipt's own job first).
/// `folders` is [`crate::store::job_folders`], read once by a caller that asks for many. The readings index, the
/// readings `differences readings` compares and the reading a job's `reading:` verification reads all come from here.
pub fn reading_of(root: &Path, r: &Receipt, folders: &[(String, PathBuf)]) -> Option<ReadingFiles> {
    if !(r.command == "reading" || r.command == "run") || r.reused_seq.is_some() || !r.accepted || r.spec_hash.is_none() {
        return None;
    }
    let json = r.outputs.iter().find(|o| o.ends_with(".json"))?;
    let page = r.outputs.iter().find(|o| o.ends_with(".md")).cloned().unwrap_or_else(|| json.replace(".json", ".md"));
    if root.join(json).is_file() {
        return Some(ReadingFiles { record: json.clone(), page });
    }
    let rel = |q: &Path| q.strip_prefix(root).unwrap_or(q).to_string_lossy().into_owned();
    let name = Path::new(json).file_name()?.to_string_lossy().into_owned();
    let own = folders.iter().filter(|(_, d)| r.job.as_deref().is_some_and(|j| crate::jobs::job_file(d).id.as_deref() == Some(j)));
    own.chain(folders.iter()).map(|(_, d)| d.join(crate::jobs::READINGS_DIR).join(&r.name).join(&name)).find(|q| q.is_file()).map(|q| ReadingFiles { record: rel(&q), page: rel(&q.with_extension("md")) })
}

/// A spec's readings as the receipts hold them, oldest first: each receipt of that name that made a reading
/// ([`reading_of`]), with its files.
pub fn readings_of(root: &Path, runs: &[Receipt], spec: &str, folders: &[(String, PathBuf)]) -> Vec<(Receipt, ReadingFiles)> {
    runs.iter().filter(|r| r.name == spec).filter_map(|r| reading_of(root, r, folders).map(|f| (r.clone(), f))).collect()
}
