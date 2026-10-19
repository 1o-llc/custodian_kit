//! Receipts: one run as the journal keeps it and as `receipts/<seq>.json`
//! holds it (`custodian.receipt.v1`), how a walked line becomes one, and how
//! one is kept. Nothing here is not a receipt: the journal's tables, jobs and
//! test rows live in `store`.

use crate::exec::{Plan, Walked};
use crate::runs::{inventory, now_utc, InJob};
use crate::store::{named, write_whole, Store};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One run, as the journal keeps it and as `receipts/<seq>.json` holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub schema: String,
    pub seq: i64,
    pub utc: String,
    /// What ran: `reading`, `test`, `check`, `git`, `gate`, `job-close`.
    pub command: String,
    /// The spec, the crate, the check.
    pub name: String,
    pub spec: Option<String>,
    pub spec_hash: Option<String>,
    /// klepto's fingerprint of the files the scan read.
    pub code_digest: Option<String>,
    /// The registry's polymorphic index key of the reading (`kidx_...`).
    pub index_key: Option<String>,
    pub plan_id: String,
    pub plan_digest: String,
    pub operation_digest: String,
    /// xccute's receipt digest over plan, operation and decision.
    pub receipt_digest: String,
    /// The line as it ran.
    pub line: String,
    pub exit_code: Option<i32>,
    pub accepted: bool,
    pub disposition: String,
    pub reason: String,
    pub outputs: Vec<String>,
    pub job: Option<String>,
    /// For a reuse: the run whose reading answered.
    pub reused_seq: Option<i64>,
    pub millis: i64,
}

pub const RECEIPT_SCHEMA: &str = "custodian.receipt.v1";

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

