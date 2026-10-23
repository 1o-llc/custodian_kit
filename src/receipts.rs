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
    let mut r = r;
    // the number is above every row and every receipt file in the folder: a file another checkout's receipts
    // brought in is never written over
    let (_, file) = journal.keep(&mut r, |n| store.path(&named::RECEIPT, &[("seq", &n.to_string())]).map_err(|e| receipt_history::Refusal::Write.refuse(e)))?;
    inventory(store)?;
    Ok((r, file))
}

/// A receipt as one identity across checkouts: what it says, its number aside (the same run kept in two checkouts'
/// history before they parted reads the same; two runs given one number do not).
pub fn identity(r: &Receipt) -> String {
    let mut v = serde_json::to_value(r).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.remove("seq");
    }
    crate::digest::of_value(&v)
}

/// What [`merge`] did: the receipts of the other folder already here, the ones kept here under new numbers (their
/// number there, their number here), and the file that maps them.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Merged {
    /// The other folder, as given.
    pub other: String,
    /// The other folder's identity: the digest of its receipts' identities in number order.
    pub other_digest: String,
    pub same: usize,
    pub kept: Vec<(i64, i64)>,
    /// Numbers both folders give to different receipts (each renumbered here).
    pub collided: Vec<i64>,
    pub map: Option<String>,
}

/// The receipt files of `dir` (`<seq>.json`), read.
pub fn receipts_in(dir: &Path) -> Vec<Receipt> {
    receipt_history::journal::receipt_files(dir).into_iter().filter_map(|(_, p)| std::fs::read_to_string(p).ok()).filter_map(|t| serde_json::from_str(&t).ok()).collect()
}

/// Two checkouts' receipts made one history by union: a receipt's identity is (the folder it came from, its number),
/// so every receipt of `other` (a `.custodian` folder or its `data/receipts`) that this store does not hold (by
/// [`identity`]) is kept here under the next free number, in its own order, and the map from its number there to
/// its number here is written beside the receipts (`merged-<digest>.json`). Nothing here is renumbered or removed;
/// merging the same folder twice keeps nothing the second time.
pub fn merge(store: &Store, other: &Path) -> Result<Merged> {
    let dir = [other.join("data/receipts"), other.join(".custodian/data/receipts"), other.to_path_buf()].into_iter().find(|d| !receipt_history::journal::receipt_files(d).is_empty()).ok_or_else(|| crate::reading::refusal::ReadingRefusal::MissingInput.refuse(format!("{}: no receipt files (`<seq>.json`) in it, its data/receipts or its .custodian/data/receipts", other.display())))?;
    let mut theirs = receipts_in(&dir);
    theirs.sort_by_key(|r| r.seq);
    let journal = store.journal()?;
    let ours = journal.runs()?;
    let here: std::collections::BTreeMap<String, i64> = ours.iter().map(|r| (identity(r), r.seq)).collect();
    let by_seq: std::collections::BTreeSet<i64> = ours.iter().map(|r| r.seq).collect();
    let other_digest = crate::digest::of_value(&theirs.iter().map(identity).collect::<Vec<_>>());
    let mut m = Merged { other: other.display().to_string(), other_digest: other_digest.clone(), ..Default::default() };
    for t in theirs {
        if here.contains_key(&identity(&t)) {
            m.same += 1;
            continue;
        }
        if by_seq.contains(&t.seq) {
            m.collided.push(t.seq);
        }
        let was = t.seq;
        let (r, _) = keep(store, t)?;
        m.kept.push((was, r.seq));
    }
    if !m.kept.is_empty() {
        let file = dir_of_receipts(store)?.join(format!("merged-{}.json", crate::digest::short(&other_digest, 12)));
        write_whole(&file, (serde_json::to_string_pretty(&m)? + "\n").as_bytes())?;
        m.map = Some(file.display().to_string());
    }
    Ok(m)
}

/// The store's receipt folder.
fn dir_of_receipts(store: &Store) -> Result<PathBuf> {
    Ok(store.path(&named::RECEIPT, &[("seq", "1")])?.parent().map(Path::to_path_buf).unwrap_or_default())
}

/// The numbers a receipt folder `other` gives to receipts this store numbers otherwise: each a collision a plain
/// copy (or a git merge) of the two folders would make; [`merge`] is the fix.
pub fn collisions(store: &Store, other: &Path) -> Result<Vec<i64>> {
    let ours: std::collections::BTreeMap<i64, String> = store.journal()?.runs()?.iter().map(|r| (r.seq, identity(r))).collect();
    Ok(receipts_in(other).iter().filter(|t| ours.get(&t.seq).is_some_and(|id| id != &identity(t))).map(|t| t.seq).collect())
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
