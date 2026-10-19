//! Receipts: one run as the journal keeps it and as `receipts/<seq>.json`
//! holds it (`custodian.receipt.v1`, receipt_history's [`Receipt`]), how a
//! walked line becomes one, and how one is kept. The journal's tables, jobs
//! and test rows are receipt_history's too, opened through `store`.

use crate::exec::{Plan, Walked};
use crate::runs::{inventory, now_utc, InJob};
use crate::store::{named, write_whole, Store};
use crate::Result;
use std::path::PathBuf;

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

