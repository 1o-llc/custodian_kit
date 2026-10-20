// Copyright © 2026 1o. All rights reserved.

//! What custodian's steps cost in time, read from its own receipts: every receipt holds the time its line took
//! (`millis`), so the records say what a preflight, a transform, a batch's compile, a test line and a reading take in
//! this workspace. From them:
//!
//! - [`times`]: the time per step kind, with how many members it covered (a batch preflight's receipt covers its
//!   members; a batch transform's compile covers the batch);
//! - [`estimate`]: a job's planned cost before it runs, each figure `measured` (from this workspace's receipts) or
//!   `unknown` (no receipt of that kind yet), never a guess;
//! - [`Budget`] and [`over_budget`]: a job's `hours` and `battery` words; a step whose measured cost would take the
//!   job past its hours is refused before it starts ([`ReadingRefusal::OverBudget`]), and the battery is the floor
//!   [`crate::power`] keeps;
//! - [`per_member`]: the step kinds a job paid one member at a time when a batch form exists (the shape of each of
//!   the costs the fleet_ops swaps paid: the index parsed per reading, a proof per function, a preflight per function,
//!   a test line per function), named so the closing receipt says them.

use crate::reading::refusal::ReadingRefusal;
use crate::store::Receipt;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The step kinds, as the receipts' `command` names them.
pub const PREFLIGHT: &str = "preflight";
pub const TRANSFORM: &str = "transform";
pub const COMPILE: &str = "compile";
pub const TEST: &str = "test line";
pub const READING: &str = "reading";
pub const PARSE: &str = "index parse";

/// The time one step kind took, over the receipts read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepTime {
    /// `preflight`, `transform`, `compile`, `test line`, `reading`, `index parse`.
    pub kind: String,
    /// Receipts of the kind.
    pub runs: usize,
    /// The members they covered (a batch's receipt covers each of its members).
    pub members: usize,
    pub millis: i64,
    /// Receipts that covered one member where the kind has a batch form.
    #[serde(default)]
    pub one_at_a_time: usize,
}

impl StepTime {
    /// The time per member, when any ran.
    pub fn per_member(&self) -> Option<i64> {
        (self.members > 0).then(|| self.millis / self.members as i64)
    }

    /// The time per run, when any ran.
    pub fn per_run(&self) -> Option<i64> {
        (self.runs > 0).then(|| self.millis / self.runs as i64)
    }
}

/// The number a batch's receipt names: `3 of 295 authorized` → 295, `batch of 61 after proof #742` → 61.
fn batch_size(text: &str) -> Option<usize> {
    if let Some((_, rest)) = text.split_once(" of ") {
        if let Some(n) = rest.split(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse().ok()) {
            return Some(n);
        }
    }
    None
}

/// A receipt's step kind and the members it covered; `None`: no step of these kinds, a command line's own receipt
/// around the step (`custodian preflight X`, `custodian test run`: the step's own receipt holds its time), or a
/// member's copy of its batch's receipt (the batch's own receipt holds the time).
pub fn kind_of(r: &Receipt) -> Option<(&'static str, usize)> {
    let command_line = r.line.starts_with("custodian ") && r.command != "preflight";
    match r.command.as_str() {
        "preflight" if r.reason == crate::preflight::AUTHORIZED || r.reason.starts_with(crate::preflight::NOT_AUTHORIZED) => Some((PREFLIGHT, 1)),
        "preflight" => None,
        crate::preflight::BATCH_COMMAND => Some((PREFLIGHT, batch_size(&r.reason).unwrap_or(1).max(1))),
        "transform" if r.name.contains("(batch of ") || r.line.contains("--batch") || command_line => None,
        "transform" => Some((TRANSFORM, 1)),
        "code-passenger.transform-batch" => Some((COMPILE, r.name.strip_prefix("batch of ").and_then(|n| n.split(' ').next()).and_then(|n| n.parse().ok()).unwrap_or(1).max(1))),
        "test" if command_line => None,
        "test" => Some((TEST, 1)),
        "reading" if r.reused_seq.is_none() => Some((READING, 1)),
        _ => None,
    }
}

/// The time per step kind over `receipts`; the index parses are the readings whose index was parsed for them.
pub fn times(receipts: &[Receipt]) -> Vec<StepTime> {
    let mut by: BTreeMap<&str, StepTime> = BTreeMap::new();
    for r in receipts {
        let Some((kind, members)) = kind_of(r) else { continue };
        let t = by.entry(kind).or_insert_with(|| StepTime { kind: kind.into(), ..Default::default() });
        t.runs += 1;
        t.members += members;
        t.millis += r.millis;
        if members == 1 && matches!(kind, PREFLIGHT | TRANSFORM) {
            t.one_at_a_time += 1;
        }
        if kind == READING && r.reason.contains("index parsed") {
            let p = by.entry(PARSE).or_insert_with(|| StepTime { kind: PARSE.into(), ..Default::default() });
            p.runs += 1;
            p.members += 1;
            p.one_at_a_time += 1;
            p.millis += r.reason.split("index parsed (").nth(1).and_then(|x| x.split(' ').next()).and_then(|n| n.parse::<i64>().ok()).unwrap_or(0);
        }
    }
    [PREFLIGHT, TRANSFORM, COMPILE, TEST, READING, PARSE].iter().filter_map(|k| by.remove(k)).collect()
}

/// How often a kind paid one member at a time before it is named: two could be a proof and its first member.
pub const PER_MEMBER_AT: usize = 3;

/// The step kinds `times` paid one member at a time where a batch form exists, each with its number and the form
/// that pays it once: preflights one by one (`preflight --batch`), a proof per function (`transform --batch`), the
/// index parsed per reading (`custodian serve`, or one run that holds the workspace open), the same test line run
/// again and again (one `test run` after the batch).
pub fn per_member(times: &[StepTime], receipts: &[Receipt]) -> Vec<String> {
    let mut out = Vec::new();
    let secs = |ms: i64| format!("{:.1} s", ms as f64 / 1000.0);
    for t in times {
        let line = match t.kind.as_str() {
            PREFLIGHT if t.one_at_a_time >= PER_MEMBER_AT => Some(format!("{} preflights paid one by one ({} each): `custodian preflight --batch LIST` reads the code once for all of them", t.one_at_a_time, secs(t.per_run().unwrap_or(0)))),
            TRANSFORM if t.one_at_a_time >= PER_MEMBER_AT => Some(format!("{} transforms proved one by one ({} each, a compile each): `custodian transform --batch LIST` proves one alone and the rest under one compile", t.one_at_a_time, secs(t.per_run().unwrap_or(0)))),
            PARSE if t.one_at_a_time >= PER_MEMBER_AT => Some(format!("the index parsed {} times ({} each): one run that holds the workspace open (a batch, or `custodian serve`) parses it once", t.one_at_a_time, secs(t.per_run().unwrap_or(0)))),
            _ => None,
        };
        out.extend(line);
    }
    let mut lines: BTreeMap<&str, usize> = BTreeMap::new();
    for r in receipts.iter().filter(|r| r.command == "test") {
        *lines.entry(r.name.as_str()).or_default() += 1;
    }
    let repeated: Vec<(&&str, &usize)> = lines.iter().filter(|(_, n)| **n >= PER_MEMBER_AT).collect();
    if !repeated.is_empty() {
        out.push(format!("test lines run again and again: {} — one `custodian test run` after the batch runs each once", repeated.iter().map(|(l, n)| format!("{l} ×{n}")).collect::<Vec<_>>().join(", ")));
    }
    out
}

/// What a job plans to run: its members (change specs waiting), the crates they land in (a compile each), the test
/// lines after them (one per crate), and the kept build's size now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Planned {
    pub members: usize,
    pub crates: usize,
    pub test_lines: usize,
    /// The kept build folder's bytes ([`crate::proof_place`]), when there is one.
    pub build_bytes: Option<u64>,
}

/// One figure of an estimate: `295 preflights × 0.2 s`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Figure {
    pub what: String,
    pub n: usize,
    /// Each one's time, from this workspace's receipts; `None`: unknown (none of the kind ran here).
    pub each_millis: Option<i64>,
}

impl Figure {
    pub fn millis(&self) -> Option<i64> {
        self.each_millis.map(|e| e * self.n as i64)
    }

    pub fn said(&self) -> String {
        match self.each_millis {
            Some(e) => format!("{} {} × {} (measured)", self.n, self.what, duration(e)),
            None => format!("{} {} × unknown (none ran here yet)", self.n, self.what),
        }
    }
}

/// A job's planned cost.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estimate {
    pub figures: Vec<Figure>,
    pub build_bytes: Option<u64>,
    /// The known figures summed.
    pub millis: i64,
    /// Figures with no measured time: the total leaves them out.
    pub unknown: usize,
}

/// A time in words: `0.2 s`, `4.1 min`, `1.3 h`.
pub fn duration(ms: i64) -> String {
    let s = ms as f64 / 1000.0;
    if s < 90.0 {
        format!("{s:.1} s")
    } else if s < 5400.0 {
        format!("{:.1} min", s / 60.0)
    } else {
        format!("{:.1} h", s / 3600.0)
    }
}

impl Estimate {
    /// The line the brief, the job's start and the handoff print.
    pub fn line(&self) -> String {
        if self.figures.iter().all(|f| f.n == 0) {
            return "nothing planned: no change spec waits".into();
        }
        let mut parts: Vec<String> = self.figures.iter().filter(|f| f.n > 0).map(Figure::said).collect();
        parts.push(match self.build_bytes {
            Some(b) => format!("build {:.1} GB (measured: the kept build now)", b as f64 / 1e9),
            None => "build unknown (no kept build yet)".into(),
        });
        format!("{}, ≈ {}{}", parts.join(", "), duration(self.millis), if self.unknown > 0 { format!(" (plus {} figure(s) unknown)", self.unknown) } else { String::new() })
    }
}

/// The planned cost of `planned` from the measured `times`: the members' preflights at a preflight's per-member time
/// (a batch's, when one ran here, else one by one), a compile per crate (a batch's compile, else a transform's), a test
/// line per crate.
pub fn estimate(times: &[StepTime], planned: &Planned) -> Estimate {
    let of = |k: &str| times.iter().find(|t| t.kind == k);
    let preflight = of(PREFLIGHT).and_then(StepTime::per_member);
    let compile = of(COMPILE).and_then(StepTime::per_run).or_else(|| of(TRANSFORM).and_then(StepTime::per_run));
    let test = of(TEST).and_then(StepTime::per_run);
    let figures = vec![Figure { what: "preflights".into(), n: planned.members, each_millis: preflight }, Figure { what: "compiles".into(), n: planned.crates, each_millis: compile }, Figure { what: "test lines".into(), n: planned.test_lines, each_millis: test }];
    let millis = figures.iter().filter_map(Figure::millis).sum();
    let unknown = figures.iter().filter(|f| f.n > 0 && f.each_millis.is_none()).count();
    Estimate { figures, build_bytes: planned.build_bytes, millis, unknown }
}

/// A job's budget: `hours=4,battery=20`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    pub hours: Option<f64>,
    /// The battery floor: under it, and not charging, no cargo line starts ([`crate::power::MIN_PERCENT`] by default).
    pub battery: Option<u8>,
}

impl Budget {
    /// `hours=4,battery=20` (either word alone); a word that is neither is refused.
    pub fn parse(text: &str) -> Result<Budget> {
        let mut b = Budget::default();
        for w in text.split(',').map(str::trim).filter(|w| !w.is_empty()) {
            let bad = || ReadingRefusal::OverBudget.refuse(format!("`{w}`: a budget is `hours=<n>,battery=<percent>`"));
            match w.split_once('=') {
                Some(("hours", n)) => b.hours = Some(n.trim().parse::<f64>().ok().filter(|h| *h > 0.0).ok_or_else(bad)?),
                Some(("battery", n)) => b.battery = Some(n.trim().trim_end_matches('%').parse::<u8>().ok().filter(|p| *p <= 100).ok_or_else(bad)?),
                _ => return Err(bad()),
            }
        }
        Ok(b)
    }

    /// The words it is read from.
    pub fn word(&self) -> String {
        let mut v = Vec::new();
        if let Some(h) = self.hours {
            v.push(format!("hours={h}"));
        }
        if let Some(p) = self.battery {
            v.push(format!("battery={p}"));
        }
        v.join(",")
    }
}

/// The workspace's budget file, beside its settings (`.custodian/config/budget.toml`, `budget = "hours=4,battery=20"`):
/// a job's budget when its job file names none.
pub const BUDGET_FILE: &str = "budget.toml";

/// The workspace's default budget words, when its budget file names one.
pub fn workspace_budget(store: &crate::store::Store) -> Option<String> {
    let settings = store.path(&crate::store::named::SETTINGS, &[]).ok()?;
    let text = std::fs::read_to_string(settings.parent()?.join(BUDGET_FILE)).ok()?;
    text.parse::<toml::Table>().ok()?.get("budget")?.as_str().map(str::to_string)
}

/// The budget a job runs under: its job file's, else the workspace's; none when neither names one.
pub fn budget_of(store: &crate::store::Store, job: &crate::jobs::JobFile) -> Result<Option<Budget>> {
    job.budget.clone().or_else(|| workspace_budget(store)).map(|w| Budget::parse(&w)).transpose()
}

/// The bytes of the files under `dir` (the kept build), when it exists.
pub fn bytes_under(dir: &std::path::Path) -> Option<u64> {
    dir.is_dir().then(|| walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
}

/// What waits to run in the workspace at `root`: the change specs in `changes` (its change spec folder) no
/// accepted transform receipt has made yet, the crates they land in (the first segment of each target, a compile and
/// a test line each), and the kept build's size ([`crate::proof_place::place_of`]).
pub fn planned(root: &std::path::Path, changes: &std::path::Path, receipts: &[Receipt]) -> Planned {
    let made: std::collections::BTreeSet<&str> = receipts.iter().filter(|r| r.command == "transform" && r.accepted).map(|r| r.name.split(" (batch of ").next().unwrap_or(&r.name)).collect();
    let mut members = 0;
    let mut crates = std::collections::BTreeSet::new();
    for e in std::fs::read_dir(changes).into_iter().flatten().flatten() {
        let Ok(spec) = crate::change::ChangeSpec::from_path(&e.path()) else { continue };
        if made.contains(spec.name.as_str()) {
            continue;
        }
        members += 1;
        let t = spec.target.trim();
        crates.insert(t.strip_prefix("crate:").map(|c| c.split('/').next().unwrap_or(c)).unwrap_or_else(|| t.split("::").next().unwrap_or(t)).replace('-', "_"));
    }
    Planned { members, crates: crates.len(), test_lines: crates.len(), build_bytes: bytes_under(&crate::proof_place::place_of(root).join("target")) }
}

/// [`planned`] for a store: its change spec folder (`data/specs/changes/`).
pub fn planned_in(store: &crate::store::Store, receipts: &[Receipt]) -> Planned {
    let changes = store.dir_of(&crate::store::named::CHANGE_SPEC, &[("name", "x")]).unwrap_or_else(|_| store.root.join(".custodian/data/specs/changes"));
    planned(&store.root, &changes, receipts)
}

/// The time a job's receipts spent (a member's copy of its batch's receipt counted once, in the batch's).
pub fn spent(receipts: &[Receipt]) -> i64 {
    receipts.iter().filter(|r| !(r.command == "transform" && r.name.contains("(batch of "))).map(|r| r.millis).sum()
}

/// The refusal for a step measured at `step_millis` (`None`: not measured, nothing to refuse on) when the job has
/// spent `spent_millis` of a budget of `budget.hours`.
pub fn over_budget(budget: &Budget, spent_millis: i64, step: &str, step_millis: Option<i64>) -> Result<()> {
    let (Some(h), Some(s)) = (budget.hours, step_millis) else { return Ok(()) };
    let limit = (h * 3_600_000.0) as i64;
    if spent_millis + s > limit {
        return Err(ReadingRefusal::OverBudget.refuse(format!("the job has spent {} of its {}; {step} is measured at {} here, which would take it to {}", duration(spent_millis), duration(limit), duration(s), duration(spent_millis + s))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(seq: i64, command: &str, name: &str, reason: &str, millis: i64) -> Receipt {
        let mut x: Receipt = serde_json::from_value(serde_json::json!({"schema": "custodian.receipt.v1", "seq": seq, "utc": "2026-10-20T00:00:00Z", "command": command, "name": name, "spec": null, "spec_hash": null, "code_digest": null, "index_key": null, "plan_id": "p", "plan_digest": "d", "operation_digest": "d", "receipt_digest": "d", "line": "", "exit_code": 0, "accepted": true, "disposition": "Continue", "reason": reason, "outputs": [], "millis": millis})).unwrap();
        x.job = Some("job-1".into());
        x
    }

    #[test]
    fn the_receipts_predict_a_batch_and_name_the_steps_paid_one_member_at_a_time() {
        // known durations: a batch preflight of 100 members in 20 s (0.2 s each), two batch compiles of 4 min,
        // three test lines of 30 s, five single preflights of 6 s
        let mut rs = vec![r(1, "preflight-batch", "a.list", "100 of 100 authorized", 20_000)];
        rs.push(r(2, "code-passenger.transform-batch", "batch of 99 after proof #1", "done", 240_000));
        rs.push(r(3, "code-passenger.transform-batch", "batch of 10 after proof #1", "done", 240_000));
        rs.push(r(4, "transform", "x (batch of 99 after proof #1 #2)", "done", 240_000));
        for (i, n) in [(5, "core"), (6, "core"), (7, "core")] {
            rs.push(r(i, "test", n, "passed", 30_000));
        }
        for i in 8..13 {
            rs.push(r(i, "preflight", &format!("p{i}"), "authorized", 6_000));
        }
        let t = times(&rs);
        let pre = t.iter().find(|x| x.kind == PREFLIGHT).unwrap();
        assert_eq!((pre.runs, pre.members, pre.millis, pre.one_at_a_time), (6, 105, 50_000, 5));
        assert_eq!(t.iter().find(|x| x.kind == COMPILE).unwrap().per_run(), Some(240_000), "a member's copy of the batch's receipt is not a second compile");
        let e = estimate(&t, &Planned { members: 295, crates: 16, test_lines: 16, build_bytes: Some(3_100_000_000) });
        // 295 × (50 s / 105) + 16 × 240 s + 16 × 30 s
        assert_eq!(e.millis, 295 * (50_000 / 105) + 16 * 240_000 + 16 * 30_000);
        let line = e.line();
        assert!(line.contains("295 preflights × 0.5 s (measured)") && line.contains("16 compiles × 4.0 min (measured)") && line.contains("build 3.1 GB") && line.contains("≈ 74.3 min"), "{line}");
        // nothing measured: unknown, never a guess
        let none = estimate(&[], &Planned { members: 3, crates: 1, test_lines: 1, build_bytes: None });
        assert!(none.line().contains("3 preflights × unknown") && none.line().contains("plus 3 figure(s) unknown"), "{}", none.line());
        // the steps paid one at a time where a batch form exists
        let flags = per_member(&t, &rs);
        assert!(flags.iter().any(|f| f.starts_with("5 preflights paid one by one") && f.contains("preflight --batch")), "{flags:?}");
        assert!(flags.iter().any(|f| f.starts_with("test lines run again and again: core ×3")), "{flags:?}");
        assert!(!flags.iter().any(|f| f.contains("transforms proved one by one")), "{flags:?}");
    }

    #[test]
    fn a_step_past_the_hours_budget_is_refused_before_it_starts() {
        let b = Budget::parse("hours=4,battery=20").unwrap();
        assert_eq!((b.hours, b.battery), (Some(4.0), Some(20)));
        assert!(Budget::parse("days=2").is_err());
        assert!(over_budget(&b, 3_600_000, "transform --batch", Some(3_600_000)).is_ok());
        let e = over_budget(&b, 3 * 3_600_000, "transform --batch", Some(2 * 3_600_000)).unwrap_err().to_string();
        assert!(e.starts_with("[CUST0043]") && e.contains("spent 3.0 h of its 4.0 h") && e.contains("measured at 2.0 h"), "{e}");
        assert!(over_budget(&b, 10 * 3_600_000, "a step", None).is_ok(), "not measured: nothing to refuse on");
    }
}
