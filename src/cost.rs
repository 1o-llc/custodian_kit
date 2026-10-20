// Copyright © 2026 1o. All rights reserved.

//! What a reading costs, and what a reuse saves, in the tokens an agent would
//! have spent reading the same files by hand.
//!
//! The rule, one place: a reading's cost is the files its answer names, each
//! once: a row's file, else (a row that names a crate and no file) that
//! crate's `Cargo.toml`. Their bytes are read from disk when the reading is
//! made, and the tokens are an estimate, `bytes / 4` ([`BYTES_PER_TOKEN`]):
//! no tokenizer is run, so the number says how much there was to read, not
//! what a model would bill. A reading whose rows name no file (a count by
//! language, an empty answer) costs nothing here.
//!
//! The record is kept beside the receipt, under the receipt's number
//! (`data/costs/<seq>.json`, [`crate::store::named::COST`]): a reading's
//! record carries its `cost` and the files it summed; a reuse's record
//! carries the same number as `saved` (the reading it reused did the
//! reading once; the reuse did not read again). The receipt itself is
//! receipt_history's and is left as it is.
//!
//! [`sum`] adds them up: read against reused, tokens spent against saved,
//! per reading kind and per job; [`brief_line`] is the line `custodian agent
//! brief` prints.

use crate::reading::ReadingReport;
use crate::store::{named, write_whole, Store};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const COST_SCHEMA: &str = "custodian.cost.v1";

/// The estimate's divisor: about four bytes of source to a token.
pub const BYTES_PER_TOKEN: u64 = 4;

/// How the estimate is made, said wherever a number is shown.
pub const ESTIMATE: &str = "estimate: bytes/4 of the files each reading touched";

/// What `bytes` comes to in tokens, by the estimate.
pub fn tokens_of(bytes: u64) -> u64 {
    bytes.div_ceil(BYTES_PER_TOKEN)
}

/// An amount read: how many files, their bytes, the tokens they come to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub files: usize,
    pub bytes: u64,
    pub tokens: u64,
}

impl Tokens {
    fn add(&mut self, o: &Tokens) {
        self.files += o.files;
        self.bytes += o.bytes;
        self.tokens += o.tokens;
    }
}

/// One file a reading's answer names, and its size when the reading was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCost {
    /// Relative to the root.
    pub path: String,
    pub bytes: u64,
}

/// The cost record of one receipt: a reading's `cost`, or a reuse's `saved`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostRecord {
    pub schema: String,
    /// The receipt this is of.
    pub seq: i64,
    /// The receipt's time, UTC.
    pub utc: String,
    /// The spec's name.
    pub name: String,
    /// The reading kind (`impact`, `reach`, ...).
    pub kind: String,
    pub job: Option<String>,
    /// A reading: what reading its files by hand would have cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Tokens>,
    /// A reuse: the reused reading's cost, not spent again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<Tokens>,
    /// A reuse: the reading it reused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reused_seq: Option<i64>,
    /// A reading: the files summed, in path order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileCost>,
    /// How the number is made ([`ESTIMATE`]).
    pub estimate: String,
}

/// The files a reading's answer names, each once, in path order: a row's file, else a crate row's `Cargo.toml`
/// (its `dir` attribute). The one rule.
pub fn files_of(report: &ReadingReport) -> Vec<String> {
    let mut out = BTreeSet::new();
    for r in report.query.results.iter().chain(&report.unresolved) {
        match (&r.path, r.attributes.get("dir")) {
            (Some(p), _) if !p.is_empty() => {
                out.insert(p.trim_start_matches("./").to_string());
            }
            (_, Some(d)) if r.crate_name.is_some() && !d.is_empty() => {
                out.insert(format!("{}/Cargo.toml", d.trim_end_matches('/')));
            }
            _ => {}
        }
    }
    out.into_iter().collect()
}

/// A reading's cost: the bytes of [`files_of`] under `root` as they are now (a file gone is left out), and their
/// tokens by the estimate.
pub fn of_report(root: &Path, report: &ReadingReport) -> (Tokens, Vec<FileCost>) {
    let files: Vec<FileCost> = files_of(report).into_iter().filter_map(|p| std::fs::metadata(root.join(&p)).ok().filter(|m| m.is_file()).map(|m| FileCost { path: p, bytes: m.len() })).collect();
    let bytes = files.iter().map(|f| f.bytes).sum();
    (Tokens { files: files.len(), bytes, tokens: tokens_of(bytes) }, files)
}

/// The record of a reading made under receipt `seq`.
pub fn read_record(root: &Path, seq: i64, utc: &str, job: Option<&str>, report: &ReadingReport) -> CostRecord {
    let (cost, files) = of_report(root, report);
    CostRecord {
        schema: COST_SCHEMA.into(),
        seq,
        utc: utc.into(),
        name: report.name.clone(),
        kind: report.kind.as_str().into(),
        job: job.map(str::to_string),
        cost: Some(cost),
        saved: None,
        reused_seq: None,
        files,
        estimate: ESTIMATE.into(),
    }
}

/// The record of a reuse (receipt `seq`) of `reading`'s record: its cost, saved.
pub fn reuse_record(seq: i64, utc: &str, job: Option<&str>, reading: &CostRecord) -> CostRecord {
    CostRecord {
        schema: COST_SCHEMA.into(),
        seq,
        utc: utc.into(),
        name: reading.name.clone(),
        kind: reading.kind.clone(),
        job: job.map(str::to_string),
        cost: None,
        saved: reading.cost,
        reused_seq: Some(reading.seq),
        files: Vec::new(),
        estimate: ESTIMATE.into(),
    }
}

/// `data/costs/` of a project.
pub fn dir(store: &Store) -> Result<PathBuf> {
    store.dir_of(&named::COST, &[("seq", "1")])
}

/// Keeps a record as `data/costs/<seq>.json`, whole.
pub fn keep(store: &Store, rec: &CostRecord) -> Result<PathBuf> {
    keep_in(&dir(store)?, rec)
}

/// Keeps a record in `dir` (`data/costs/`), whole: the path the layout names for its number.
pub fn keep_in(dir: &Path, rec: &CostRecord) -> Result<PathBuf> {
    let file = dir.join(format!("{}.json", rec.seq));
    write_whole(&file, (serde_json::to_string_pretty(rec)? + "\n").as_bytes())?;
    Ok(file)
}

/// The cost of the reading receipt `r` made: its kept record, else (a reading made before costs were kept) its
/// record measured now from the reading's own record. A reuse only reuses a reading of the same code (the same
/// search key), so its files are the bytes they were.
pub fn reading_cost(root: &Path, store: &Store, r: &crate::store::Receipt) -> Option<CostRecord> {
    let dir = dir(store).ok()?;
    if let Some(c) = load(&dir, r.seq).filter(|c| c.cost.is_some()) {
        return Some(c);
    }
    let folders = crate::store::job_folders(store.jobs_dir(crate::store::JobState::Done).ok()?.parent()?);
    let files = crate::receipts::reading_of(root, r, &folders)?;
    let report = crate::reading::load(&root.join(files.record)).ok()?;
    Some(read_record(root, r.seq, &r.utc, r.job.as_deref(), &report))
}

/// One line saying a record: `cost: 3 files, 1,243 bytes, ~311 tokens (estimate: ...)`, or what a reuse saved.
pub fn said(rec: &CostRecord) -> String {
    match (&rec.cost, &rec.saved) {
        (Some(c), _) => format!("cost: {} files, {} bytes, ~{} tokens ({ESTIMATE}) — .custodian/data/costs/{}.json", grouped(c.files as u64), grouped(c.bytes), grouped(c.tokens), rec.seq),
        (None, Some(s)) => format!("saved by reuse: ~{} tokens, the cost of reading #{} ({ESTIMATE})", grouped(s.tokens), rec.reused_seq.unwrap_or_default()),
        _ => "cost: not measured".into(),
    }
}

/// The record kept under receipt `seq`, if any.
pub fn load(dir: &Path, seq: i64) -> Option<CostRecord> {
    std::fs::read_to_string(dir.join(format!("{seq}.json"))).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// Every record in `dir`, by receipt number.
pub fn load_all(dir: &Path) -> Vec<CostRecord> {
    let mut out: Vec<CostRecord> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).filter_map(|p| std::fs::read_to_string(p).ok()).filter_map(|t| serde_json::from_str::<CostRecord>(&t).ok()).collect())
        .unwrap_or_default();
    out.sort_by_key(|r| r.seq);
    out
}

/// One line of the sums: how many readings and reuses, what was read and what was saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sum {
    pub readings: usize,
    pub reuses: usize,
    pub read: Tokens,
    pub saved: Tokens,
}

impl Sum {
    fn take(&mut self, r: &CostRecord) {
        if let Some(c) = &r.cost {
            self.readings += 1;
            self.read.add(c);
        }
        if let Some(s) = &r.saved {
            self.reuses += 1;
            self.saved.add(s);
        }
    }
}

/// The sums over a set of records: all of them, per reading kind, per job (`-` for runs outside a job).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sums {
    pub all: Sum,
    pub by_kind: BTreeMap<String, Sum>,
    pub by_job: BTreeMap<String, Sum>,
    /// Reading and reuse receipts in the same range with no cost record (made before costs were kept).
    pub unmeasured: Vec<i64>,
}

/// The sums of `records`, of one job when named, after receipt `since` when named. `receipts` are the reading
/// receipts of the same range (`command` `reading`): those with no record are listed as unmeasured.
pub fn sum(records: &[CostRecord], receipts: &[crate::store::Receipt], job: Option<&str>, since: Option<i64>) -> Sums {
    let keep = |seq: i64, j: Option<&str>| since.is_none_or(|s| seq > s) && job.is_none_or(|want| j == Some(want));
    let mut out = Sums::default();
    let have: BTreeSet<i64> = records.iter().map(|r| r.seq).collect();
    for r in records.iter().filter(|r| keep(r.seq, r.job.as_deref())) {
        out.all.take(r);
        out.by_kind.entry(r.kind.clone()).or_default().take(r);
        out.by_job.entry(r.job.clone().unwrap_or_else(|| "-".into())).or_default().take(r);
    }
    out.unmeasured = receipts.iter().filter(|r| crate::receipts::is_reading_run(r) && r.accepted && keep(r.seq, r.job.as_deref()) && !have.contains(&r.seq)).map(|r| r.seq).collect();
    out
}

/// A number with thousands separated: `1,234,567`.
pub fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The brief's line: `tokens: ~N read, ~M saved by reuse (estimate: bytes/4 of the files each reading touched)`.
pub fn brief_line(s: &Sums) -> String {
    format!("tokens: ~{} read, ~{} saved by reuse ({ESTIMATE})", grouped(s.all.read.tokens), grouped(s.all.saved.tokens))
}

/// `custodian cost`'s page: the totals, then per kind and per job.
pub fn render(s: &Sums, job: Option<&str>, since: Option<i64>) -> String {
    let unmeasured_runs = |v: &[i64]| v.iter().map(|n| format!("#{n}")).collect::<Vec<_>>();
    let mut out = String::from("# cost\n\n");
    let scope = match (job, since) {
        (Some(j), Some(n)) => format!("job {j}, after receipt #{n}"),
        (Some(j), None) => format!("job {j}"),
        (None, Some(n)) => format!("every run after receipt #{n}"),
        (None, None) => "every run".into(),
    };
    out.push_str(&format!("- over: {scope}\n- {}\n", brief_line(s)));
    out.push_str(&format!(
        "- read: {} readings, {} files, {} bytes, ~{} tokens\n- reused: {} reuses, ~{} tokens not read again\n",
        s.all.readings,
        grouped(s.all.read.files as u64),
        grouped(s.all.read.bytes),
        grouped(s.all.read.tokens),
        s.all.reuses,
        grouped(s.all.saved.tokens)
    ));
    if !s.unmeasured.is_empty() {
        let runs = unmeasured_runs(&s.unmeasured);
        let shown = if runs.len() > 8 { format!("{} … and {} more", runs[..8].join(", "), runs.len() - 8) } else { runs.join(", ") };
        out.push_str(&format!("- not measured: {} readings and reuses made before costs were kept ({shown})\n", s.unmeasured.len()));
    }
    out.push_str(&format!("- how: a reading's cost is the files its answer names (a row's file, else a crate row's Cargo.toml), their bytes when it was made, ~tokens = bytes/{BYTES_PER_TOKEN}; a reuse saves the cost of the reading it reused\n"));
    let table = |title: &str, rows: &BTreeMap<String, Sum>| -> String {
        let mut t = format!("\n## {title}\n\n| {} | readings | ~tokens read | reuses | ~tokens saved |\n|---|---|---|---|---|\n", if title.ends_with("kind") { "kind" } else { "job" });
        for (k, v) in rows {
            t.push_str(&format!("| {k} | {} | {} | {} | {} |\n", v.readings, grouped(v.read.tokens), v.reuses, grouped(v.saved.tokens)));
        }
        if rows.is_empty() {
            t.push_str("| none | 0 | 0 | 0 | 0 |\n");
        }
        t
    };
    out.push_str(&table("per reading kind", &s.by_kind));
    out.push_str(&table("per job", &s.by_job));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CodeQueryResult;

    fn row(path: Option<&str>, krate: Option<&str>, dir: Option<&str>) -> CodeQueryResult {
        CodeQueryResult {
            id: "r".into(),
            kind: "k".into(),
            label: "l".into(),
            crate_name: krate.map(str::to_string),
            path: path.map(str::to_string),
            line: None,
            column: None,
            score_reason: vec![],
            attributes: dir.map(|d| [("dir".to_string(), d.to_string())].into_iter().collect()).unwrap_or_default(),
        }
    }

    fn report(rows: Vec<CodeQueryResult>) -> ReadingReport {
        let query = serde_json::to_value(crate::parse_code_query_spec(&[])).unwrap();
        let summary = serde_json::to_value(crate::CodeGraphSummary::default()).unwrap();
        let mut r: ReadingReport = serde_json::from_value(serde_json::json!({
            "schema": "custodian.reading.v1", "name": "impact-x", "question": "q", "kind": "impact", "target": "x", "spec": "s", "stamp": "20261020T010000Z",
            "root": "/", "counts": [], "query": {"schema": "code_passenger.query.v1", "graph_schema": "klepto.receipt.v1", "root": "/", "root_exists": true, "max_depth": 0,
            "query": query, "result_count": 0, "truncated": false, "results": [], "summary": summary, "findings": []}, "unresolved": [], "binaries": {}, "not_read": []
        }))
        .unwrap();
        r.query.results = rows;
        r
    }

    /// A reading's cost is the sum of its files, each counted once; a crate row with no file counts its manifest; a
    /// row naming nothing counts nothing.
    #[test]
    fn a_readings_cost_is_the_sum_of_its_files() {
        let root = std::env::temp_dir().join(format!("cost-files-{}", std::process::id()));
        std::fs::create_dir_all(root.join("a/src")).unwrap();
        std::fs::write(root.join("a/src/lib.rs"), vec![b'x'; 1000]).unwrap();
        std::fs::write(root.join("a/src/two.rs"), vec![b'y'; 203]).unwrap();
        std::fs::write(root.join("a/Cargo.toml"), vec![b'z'; 40]).unwrap();
        let r = report(vec![row(Some("a/src/lib.rs"), Some("a"), None), row(Some("a/src/lib.rs"), Some("a"), None), row(Some("a/src/two.rs"), None, None), row(None, Some("a"), Some("a")), row(None, None, None)]);
        assert_eq!(files_of(&r), ["a/Cargo.toml", "a/src/lib.rs", "a/src/two.rs"]);
        let (t, files) = of_report(&root, &r);
        assert_eq!(t, Tokens { files: 3, bytes: 1243, tokens: 311 }, "1000 + 203 + 40 bytes, /4 rounded up");
        assert_eq!(files.iter().map(|f| f.bytes).sum::<u64>(), t.bytes);
        let rec = read_record(&root, 7, "2026-10-20T01:00:00Z", Some("j-1"), &r);
        assert_eq!((rec.cost, rec.saved, rec.kind.as_str()), (Some(t), None, "impact"));
    }

    /// A reuse saves exactly what the reading it reused cost, and the sums keep reading and reuse apart, per kind
    /// and per job; a reading receipt with no record is counted as not measured.
    #[test]
    fn a_reuse_saves_the_readings_cost_and_the_brief_says_both() {
        let reading = CostRecord { schema: COST_SCHEMA.into(), seq: 3, name: "impact-x".into(), kind: "impact".into(), cost: Some(Tokens { files: 2, bytes: 4000, tokens: 1000 }), estimate: ESTIMATE.into(), ..Default::default() };
        let reuse = reuse_record(9, "t", Some("j-2"), &reading);
        assert_eq!((reuse.saved, reuse.cost, reuse.reused_seq), (reading.cost, None, Some(3)));
        let receipt = |seq: i64| crate::store::Receipt {
            schema: crate::store::RECEIPT_SCHEMA.into(),
            seq,
            utc: "t".into(),
            command: "reading".into(),
            name: "n".into(),
            spec: None,
            spec_hash: Some("s".into()),
            code_digest: None,
            index_key: None,
            plan_id: "p".into(),
            plan_digest: "p".into(),
            operation_digest: "o".into(),
            receipt_digest: "r".into(),
            line: "l".into(),
            exit_code: Some(0),
            accepted: true,
            disposition: "Continue".into(),
            reason: "read".into(),
            outputs: vec![],
            job: None,
            reused_seq: None,
            millis: 0,
            runner: crate::receipts::RUNNER_LINE.into(),
        };
        let s = sum(&[reading.clone(), reuse.clone()], &[receipt(1), receipt(3), receipt(9)], None, None);
        assert_eq!((s.all.readings, s.all.reuses, s.all.read.tokens, s.all.saved.tokens), (1, 1, 1000, 1000));
        assert_eq!(s.by_kind["impact"].saved.tokens, 1000);
        assert_eq!((s.by_job["-"].read.tokens, s.by_job["j-2"].saved.tokens), (1000, 1000));
        assert_eq!(s.unmeasured, [1]);
        assert_eq!(brief_line(&s), "tokens: ~1,000 read, ~1,000 saved by reuse (estimate: bytes/4 of the files each reading touched)");
        let only = sum(&[reading, reuse], &[], Some("j-2"), None);
        assert_eq!((only.all.read.tokens, only.all.saved.tokens), (0, 1000));
        assert_eq!(grouped(1_234_567), "1,234,567");
    }
}
