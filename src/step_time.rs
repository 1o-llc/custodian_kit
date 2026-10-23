// Copyright © 2026 1o. All rights reserved.

//! What custodian's steps cost in time, read from its own receipts: every receipt holds the time its line took
//! (`millis`), so the records say what a preflight, a transform, a batch's compile, a test line and a reading take in
//! this workspace. From them:
//!
//! - [`times`]: the time per step kind, with how many members it covered (a batch preflight's receipt covers its
//!   members; a batch transform's compile covers the batch);
//! - [`estimate`]: a job's planned cost before it runs, each figure `measured` (from this workspace's receipts) or
//!   `unknown` (no receipt of that kind yet), never a guess;
//! - [`Budget`] and [`over_by`]: a job's `hours` and `battery` words; the hours are information (the brief and the
//!   closing receipt say how far past them a job is), never a refusal; the battery is the floor [`crate::power`]
//!   keeps, the one rule that stops a step;
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
/// A patterns reading (`patterns-<target>`): the one reading a toolkit job pays before its specs exist.
pub const PATTERNS: &str = "patterns reading";
/// A `custodian test run` line: the tests' build and every test line it runs, as one wait.
pub const TEST_RUN: &str = "test run";
/// A `custodian transform --batch` line: the batch's preflight, its compile and its expansions, as one wait.
pub const BATCH: &str = "batch transform";
/// A `custodian job start` or `job close` line: its checkpoint and its count reading.
pub const JOB: &str = "job open or close";

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
    /// Of them, the receipts of a batch form (`preflight --batch`): its runs, members and time, so a batch's rate per
    /// member is read apart from one member's.
    #[serde(default)]
    pub batch_runs: usize,
    #[serde(default)]
    pub batch_members: usize,
    #[serde(default)]
    pub batch_millis: i64,
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

    /// The time per member of the batch form, when a batch ran.
    pub fn per_batch_member(&self) -> Option<i64> {
        (self.batch_members > 0).then(|| self.batch_millis / self.batch_members as i64)
    }

    /// The time of one run that was not a batch, when one ran.
    pub fn per_single_run(&self) -> Option<i64> {
        let runs = self.runs - self.batch_runs;
        (runs > 0).then(|| (self.millis - self.batch_millis) / runs as i64)
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
        "transform" if command_line && r.line.contains("--batch") && !r.line.contains("--revert") => Some((BATCH, 1)),
        "transform" if r.name.contains("(batch of ") || r.line.contains("--batch") || command_line => None,
        "transform" => Some((TRANSFORM, 1)),
        "code-passenger.transform-batch" => Some((COMPILE, r.name.strip_prefix("batch of ").and_then(|n| n.split(' ').next()).and_then(|n| n.parse().ok()).unwrap_or(1).max(1))),
        "test" if r.line.starts_with("custodian test run") => Some((TEST_RUN, 1)),
        "test" if command_line => None,
        "test" => Some((TEST, 1)),
        "reading" if r.reused_seq.is_none() && r.name.starts_with("patterns-") => Some((PATTERNS, 1)),
        "reading" if r.reused_seq.is_none() => Some((READING, 1)),
        "job" if r.line.starts_with("custodian job start") || r.line.starts_with("custodian job close") => Some((JOB, 1)),
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
        if r.command == crate::preflight::BATCH_COMMAND {
            t.batch_runs += 1;
            t.batch_members += members;
            t.batch_millis += r.millis;
        }
        if matches!(kind, READING | PATTERNS) && r.reason.contains("index parsed") {
            let p = by.entry(PARSE).or_insert_with(|| StepTime { kind: PARSE.into(), ..Default::default() });
            p.runs += 1;
            p.members += 1;
            p.one_at_a_time += 1;
            p.millis += r.reason.split("index parsed (").nth(1).and_then(|x| x.split(' ').next()).and_then(|n| n.parse::<i64>().ok()).unwrap_or(0);
        }
    }
    [PREFLIGHT, TRANSFORM, COMPILE, BATCH, TEST, TEST_RUN, READING, PATTERNS, PARSE, JOB].iter().filter_map(|k| by.remove(k)).collect()
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

/// What a job plans to run: its members (change specs waiting), the toolkit shapes they fall in (a proof and a batch
/// each), the dependency changes among them, the crates they land in (a test run each), and the kept build's size now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Planned {
    /// The specs waiting that are not dependency changes.
    pub members: usize,
    pub crates: usize,
    pub test_lines: usize,
    /// The kept build folder's bytes ([`crate::proof_place`]), when there is one.
    pub build_bytes: Option<u64>,
    /// The shapes the members fall in ([`shape_of`]): a proof and its batch each.
    #[serde(default)]
    pub shapes: usize,
    /// Of the shapes, those with more members than their proof (a batch compile each).
    #[serde(default)]
    pub batches: usize,
    /// The dependency changes waiting (`change_dependency`): a preflight and a compile each, before the swaps.
    #[serde(default)]
    pub dependencies: usize,
    /// What the job has spent so far ([`spent`]), when the plan is a running job's: the line then says the whole job.
    #[serde(default)]
    pub spent_millis: Option<i64>,
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
    /// What the running job has spent so far ([`Planned::spent_millis`]).
    #[serde(default)]
    pub spent_millis: Option<i64>,
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
            return "unknown until the reading: no change spec waits yet (the patterns reading, or the specs written, make the plan)".into();
        }
        let mut parts: Vec<String> = self.figures.iter().filter(|f| f.n > 0).map(Figure::said).collect();
        parts.push(match self.build_bytes {
            Some(b) => format!("build {:.1} GB (measured: the kept build now)", b as f64 / 1e9),
            None => "build unknown (no kept build yet)".into(),
        });
        let job = self.spent_millis.map(|s| format!("; the job ≈ {} with the {} it has spent", duration(s + self.millis), duration(s))).unwrap_or_default();
        format!("{}, ≈ {}{}{job}", parts.join(", "), duration(self.millis), if self.unknown > 0 { format!(" (plus {} figure(s) unknown)", self.unknown) } else { String::new() })
    }
}

/// The planned cost of `planned` from the measured `times`, each figure the step as this workspace's receipts measured
/// it: per dependency change a preflight, an index parse and a compile; per shape an index parse, the proof's
/// preflight and its compile; the members past the proofs at the batch preflight's rate per member (one member's when
/// no batch ran here); per shape with a batch the batch's compile (the batch line's time less its preflight); a test
/// run per crate; and, in a running job, its close.
pub fn estimate(times: &[StepTime], planned: &Planned) -> Estimate {
    let of = |k: &str| times.iter().find(|t| t.kind == k);
    let pre = of(PREFLIGHT);
    let single_preflight = pre.and_then(StepTime::per_single_run).or_else(|| pre.and_then(StepTime::per_member));
    let batch_preflight = pre.and_then(StepTime::per_batch_member).or(single_preflight);
    let parse = of(PARSE).and_then(StepTime::per_run);
    let proof = of(TRANSFORM).and_then(StepTime::per_run).or_else(|| of(COMPILE).and_then(StepTime::per_run));
    // the batch line holds its preflight: its compile is the line less the preflight's time per batch
    let batch_preflight_run = pre.filter(|p| p.batch_runs > 0).map(|p| p.batch_millis / p.batch_runs as i64).unwrap_or(0);
    let batch = of(BATCH).and_then(StepTime::per_run).map(|b| (b - batch_preflight_run).max(0)).or_else(|| of(COMPILE).and_then(StepTime::per_run));
    let test = of(TEST_RUN).and_then(StepTime::per_run).or_else(|| of(TEST).and_then(StepTime::per_run));
    let close = of(JOB).and_then(StepTime::per_run);
    let dependency = match (single_preflight, parse, proof) {
        (Some(a), Some(b), Some(c)) => Some(a + b + c),
        _ => None,
    };
    let past_proofs = planned.members.saturating_sub(planned.shapes);
    let figures = vec![
        Figure { what: "dependency changes (preflight, index parse, compile)".into(), n: planned.dependencies, each_millis: dependency },
        Figure { what: "index parses (one per shape)".into(), n: planned.shapes, each_millis: parse },
        Figure { what: "proof preflights".into(), n: planned.shapes, each_millis: single_preflight },
        Figure { what: "proof compiles".into(), n: planned.shapes, each_millis: proof },
        Figure { what: "batch preflights (per function)".into(), n: past_proofs, each_millis: batch_preflight },
        Figure { what: "batch compiles".into(), n: planned.batches, each_millis: batch },
        Figure { what: "test runs".into(), n: planned.test_lines, each_millis: test },
        Figure { what: "job close".into(), n: usize::from(planned.spent_millis.is_some() && planned.members + planned.dependencies > 0), each_millis: close },
    ];
    let millis = figures.iter().filter_map(Figure::millis).sum();
    let unknown = figures.iter().filter(|f| f.n > 0 && f.each_millis.is_none()).count();
    Estimate { figures, build_bytes: planned.build_bytes, millis, unknown, spent_millis: planned.spent_millis }
}

/// The step kinds that change code: the ones a budget refuses past its hours. The job's own verification (its tests,
/// its close) is never refused: a job past its budget still verifies and closes.
pub fn is_change_step(kind: &str) -> bool {
    matches!(kind, PREFLIGHT | TRANSFORM | COMPILE | BATCH)
}

/// A spec's shape: the toolkit generator that writes the whole function, else the toolkit pieces its body calls (the
/// `let`, `value` and `raw` steps aside) sorted and joined with `+`; a spec with no toolkit piece is its operation.
/// The specs of one shape prove once and batch together.
pub fn shape_of(spec: &crate::change::ChangeSpec) -> String {
    if let Some(f) = &spec.function {
        if let Some(g) = &f.generator {
            return g.piece.clone();
        }
        let pieces: std::collections::BTreeSet<&str> = f.body.iter().map(|p| p.piece.as_str()).filter(|p| !matches!(*p, "let" | "value" | "raw")).collect();
        if !pieces.is_empty() {
            return pieces.into_iter().collect::<Vec<_>>().join("+");
        }
    }
    spec.operation.map(|o| o.as_str().to_string()).unwrap_or_else(|| "change".into())
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
    planned_from(root, &[changes.to_path_buf()], receipts, None)
}

/// [`planned`] over every folder in `folders` that holds change specs (the change spec folder, a patterns reading's
/// `<stamp>.specs` beside it), counting only the spec files written at or after `since` (a running job's opening:
/// a spec an earlier job left and never made is not this job's plan). A spec counted once by its name; the specs
/// grouped by [`shape_of`]; a `change_dependency` spec a dependency change, not a member.
pub fn planned_from(root: &std::path::Path, folders: &[std::path::PathBuf], receipts: &[Receipt], since: Option<std::time::SystemTime>) -> Planned {
    let made: std::collections::BTreeSet<&str> = receipts.iter().filter(|r| r.command == "transform" && r.accepted).map(|r| r.name.split(" (batch of ").next().unwrap_or(&r.name)).collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut p = Planned::default();
    let mut crates = std::collections::BTreeSet::new();
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();
    for folder in folders {
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(folder).into_iter().flatten().flatten().map(|e| e.path()).collect();
        files.sort();
        for path in files {
            if let Some(since) = since {
                if std::fs::metadata(&path).and_then(|m| m.modified()).is_ok_and(|m| m < since) {
                    continue;
                }
            }
            let Ok(spec) = crate::change::ChangeSpec::from_path(&path) else { continue };
            if made.contains(spec.name.as_str()) || !seen.insert(spec.name.clone()) {
                continue;
            }
            let t = spec.target.trim();
            crates.insert(t.strip_prefix("crate:").map(|c| c.split('/').next().unwrap_or(c)).unwrap_or_else(|| t.split("::").next().unwrap_or(t)).replace('-', "_"));
            if spec.operation == Some(crate::change::Operation::ChangeDependency) {
                p.dependencies += 1;
            } else {
                p.members += 1;
                *shapes.entry(shape_of(&spec)).or_default() += 1;
            }
        }
    }
    p.shapes = shapes.len();
    p.batches = shapes.values().filter(|n| **n > 1).count();
    p.crates = crates.len();
    p.test_lines = crates.len();
    p.build_bytes = bytes_under(&crate::proof_place::place_of(root).join("target"));
    p
}

/// [`planned`] for a store: its change spec folder (`data/specs/changes/`).
pub fn planned_in(store: &crate::store::Store, receipts: &[Receipt]) -> Planned {
    planned_from(&store.root, &spec_folders(store), receipts, None)
}

/// The folders a store's change specs wait in: its change spec folder and each patterns reading's spec folder
/// (`data/readings/patterns-<target>/<stamp>.specs/`).
pub fn spec_folders(store: &crate::store::Store) -> Vec<std::path::PathBuf> {
    let changes = store.dir_of(&crate::store::named::CHANGE_SPEC, &[("name", "x")]).unwrap_or_else(|_| store.root.join(".custodian/data/specs/changes"));
    let readings = store.dir_of(&crate::store::named::READING_PAGE, &[("name", "x"), ("stamp", "x")]).ok().and_then(|d| d.parent().map(std::path::Path::to_path_buf)).unwrap_or_else(|| store.root.join(".custodian/data/readings"));
    let mut out = vec![changes];
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(&readings)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("patterns-"))
        .flat_map(|e| std::fs::read_dir(e.path()).into_iter().flatten().flatten().map(|x| x.path()))
        .filter(|p| p.is_dir() && p.extension().is_some_and(|x| x == "specs"))
        .collect();
    found.sort();
    out.extend(found);
    out
}

/// What waits in a running job, opened at `opened_utc` (RFC 3339): [`planned_from`] over [`spec_folders`], the specs
/// written since it opened, with what its receipts (`own`) have spent ([`spent`]).
pub fn planned_in_job(store: &crate::store::Store, receipts: &[Receipt], own: &[Receipt], opened_utc: &str) -> Planned {
    let since = chrono::DateTime::parse_from_rfc3339(opened_utc).ok().map(|t| std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(t.timestamp().max(0) as u64));
    let mut p = planned_from(&store.root, &spec_folders(store), receipts, since);
    p.spent_millis = Some(spent(own));
    p
}

/// The time a job's receipts spent, each unit of work once: every receipt covers the span from its start (its
/// time less its `millis`) to its time, and the spent time is the length of their union, so a command line's receipt
/// and the code-passenger receipts inside it, and a member's copy of its batch's receipt, are counted once. A receipt
/// whose time does not read counts its `millis` alone.
pub fn spent(receipts: &[Receipt]) -> i64 {
    let mut spans: Vec<(i64, i64)> = Vec::new();
    let mut loose = 0;
    for r in receipts.iter().filter(|r| r.millis > 0) {
        match chrono::DateTime::parse_from_rfc3339(&r.utc) {
            // the time is to the second: its span ends at the end of that second
            Ok(t) => {
                let end = t.timestamp_millis() + 999;
                spans.push((end - r.millis, end));
            }
            Err(_) => loose += r.millis,
        }
    }
    spans.sort();
    let mut total = 0;
    let mut cur: Option<(i64, i64)> = None;
    for (a, b) in spans {
        match cur {
            Some((ca, cb)) if a <= cb => cur = Some((ca, cb.max(b))),
            Some((ca, cb)) => {
                total += cb - ca;
                cur = Some((a, b));
            }
            None => cur = Some((a, b)),
        }
    }
    total + cur.map(|(a, b)| b - a).unwrap_or(0) + loose
}

/// How far a job that has spent `spent_millis` is past its budget's hours, when it is: information for the brief and
/// the closing receipt (`over the budget by 12.0 min`). Nothing is refused by hours; the battery rule
/// ([`crate::power`]) is the only one that stops a step.
pub fn over_by(budget: &Budget, spent_millis: i64) -> Option<String> {
    let limit = (budget.hours? * 3_600_000.0) as i64;
    (spent_millis > limit).then(|| format!("over the budget by {} ({} spent of {})", duration(spent_millis - limit), duration(spent_millis), duration(limit)))
}

/// The refusal for a step measured at `step_millis` (`None`: not measured, nothing to refuse on) when the job has
/// spent `spent_millis` of a budget of `budget.hours`. Kept for its callers; custodian refuses nothing by hours
/// (James, 2026-10-23): it says [`over_by`] instead.
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
        // 295 members in 16 shapes, 16 of them batched: two compiles a shape (its proof, its batch), never one a crate
        let e = estimate(&t, &Planned { members: 295, crates: 1, test_lines: 1, shapes: 16, batches: 16, build_bytes: Some(3_100_000_000), ..Default::default() });
        // the proof's preflight is one member's (6 s), the batch's members at the batch's rate (0.2 s)
        assert_eq!(pre.per_single_run(), Some(6_000));
        assert_eq!(pre.per_batch_member(), Some(200));
        // 16 × 6 s + 16 × 240 s (proof) + 279 × 0.2 s + 16 × 240 s (batch) + 1 × 30 s; no parse measured
        assert_eq!(e.millis, 16 * 6_000 + 16 * 240_000 + 279 * 200 + 16 * 240_000 + 30_000);
        let line = e.line();
        assert!(line.contains("16 proof compiles × 4.0 min (measured)") && line.contains("16 batch compiles × 4.0 min (measured)") && line.contains("279 batch preflights (per function) × 0.2 s") && line.contains("build 3.1 GB") && line.contains("16 index parses (one per shape) × unknown"), "{line}");
        // nothing measured: unknown, never a guess
        let none = estimate(&[], &Planned { members: 3, crates: 1, test_lines: 1, shapes: 1, batches: 1, build_bytes: None, ..Default::default() });
        assert!(none.line().contains("1 proof preflights × unknown") && none.line().contains("figure(s) unknown"), "{}", none.line());
        // before the reading nothing waits: the line says so, never a figure from an old spec
        assert!(estimate(&t, &Planned::default()).line().starts_with("unknown until the reading"), "{}", estimate(&t, &Planned::default()).line());
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

    fn at(seq: i64, utc: &str, command: &str, name: &str, line: &str, reason: &str, millis: i64) -> Receipt {
        let mut x = r(seq, command, name, reason, millis);
        x.utc = utc.into();
        x.line = line.into();
        x
    }

    #[test]
    fn the_budget_counts_a_command_line_and_the_receipts_inside_it_once() {
        // xccute_observations' first batch (receipts #4260–#4340): the outer `custodian transform --batch` line of
        // 280.3 s holds the batch preflight (51.4 s), code-passenger's compile (184.6 s) and a member's copy of it each
        let mut rs = vec![at(1, "2026-10-23T09:30:05Z", "preflight-batch", "list", "custodian preflight --batch list", "80 of 80 authorized", 51_391)];
        rs.push(at(2, "2026-10-23T09:33:11Z", "code-passenger.transform-batch", "batch of 80 after proof #4259", "code-passenger --root w transform", "done", 184_570));
        for i in 3..83 {
            rs.push(at(i, "2026-10-23T09:33:11Z", "transform", &format!("m{i} (batch of 80 after proof #4259)"), "code-passenger --root w transform", "done", 184_570));
        }
        rs.push(at(83, "2026-10-23T09:33:11Z", "transform", "list", "custodian transform --write --batch list", "done", 280_288));
        // and a test line run on its own after it
        rs.push(at(84, "2026-10-23T09:41:46Z", "test", "t", "sh -c cargo test", "passed", 2_233));
        let old_sum: i64 = rs.iter().filter(|r| !(r.command == "transform" && r.name.contains("(batch of "))).map(|r| r.millis).sum();
        assert_eq!(old_sum, 51_391 + 184_570 + 280_288 + 2_233, "the old count: each outer line and what ran inside it");
        assert_eq!(spent(&rs), 280_288 + 2_233, "each unit of work once");
        // a receipt whose time does not read counts its own time
        let mut odd = at(85, "never", "test", "t", "", "passed", 1_000);
        odd.utc = "not a time".into();
        assert_eq!(spent(&[odd]), 1_000);
    }

    #[test]
    fn the_hours_are_information_never_a_refusal() {
        let b = Budget::parse("hours=0.5").unwrap();
        assert_eq!(over_by(&b, 20 * 60_000), None);
        assert_eq!(over_by(&b, 42 * 60_000).as_deref(), Some("over the budget by 12.0 min (42.0 min spent of 30.0 min)"));
        assert_eq!(over_by(&Budget::default(), 10 * 3_600_000), None, "no hours: nothing to say");
    }

    #[test]
    fn the_budget_never_refuses_the_jobs_own_verification() {
        for k in [PREFLIGHT, TRANSFORM, COMPILE, BATCH] {
            assert!(is_change_step(k), "{k}");
        }
        for k in [TEST, TEST_RUN, READING, PATTERNS, PARSE, JOB] {
            assert!(!is_change_step(k), "{k}: verification and closing are never refused");
        }
    }

    #[test]
    fn the_plan_counts_the_patterns_readings_specs_and_never_an_old_unmade_one() {
        let root = std::env::temp_dir().join(format!("custodian-kit-planned-{}", std::process::id()));
        let changes = root.join("changes");
        let reading = root.join("readings/patterns-crate_core/20261023T091730Z.specs");
        std::fs::create_dir_all(&changes).unwrap();
        std::fs::create_dir_all(&reading).unwrap();
        // an old fleet_ops spec an earlier job left and never made
        std::fs::write(changes.join("fleet-ops-dep.toml"), "name = \"fleet-ops-dep\"\nobjective = \"o\"\noperation = \"change_dependency\"\ntarget = \"crate:fleet_ops\"\n[dependency]\nname = \"simple_impl\"\nversion = \"0.1.12\"\n").unwrap();
        let since = std::time::SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(1_100));
        std::fs::write(changes.join("core-dep.toml"), "name = \"core-dep\"\nobjective = \"o\"\noperation = \"change_dependency\"\ntarget = \"crate:core\"\n[dependency]\nname = \"simple_impl\"\nversion = \"0.1.12\"\n").unwrap();
        let swap = |name: &str, pieces: &[&str]| {
            let fields = |p: &str| match p {
                "ensure" => "condition = \"x\"\nerror = \"e\"\n",
                "raw" => "code = \"x();\"\n",
                _ => "condition = \"x\"\nreturn_value = \"y\"\n",
            };
            let body: String = pieces.iter().map(|p| format!("[[function.body]]\npiece = \"{p}\"\n{}", fields(p))).collect();
            format!("name = \"{name}\"\nobjective = \"o\"\noperation = \"replace_body\"\ntarget = \"core::m::{name}\"\nsame_expansion = true\n[function]\nname = \"{name}\"\n{body}")
        };
        for (i, pieces) in [vec!["return_if"], vec!["return_if"], vec!["return_if"], vec!["ensure", "return_if"], vec!["raw"]].into_iter().enumerate() {
            std::fs::write(reading.join(format!("f{i}.toml")), swap(&format!("f{i}"), &pieces)).unwrap();
        }
        let folders = vec![changes.clone(), reading.clone()];
        let p = planned_from(&root, &folders, &[], Some(since));
        assert_eq!((p.members, p.shapes, p.batches, p.dependencies, p.crates), (5, 3, 1, 1, 1), "{p:?}");
        // with no job (no since), the old spec waits too: a second crate
        let all = planned_from(&root, &folders, &[], None);
        assert_eq!((all.dependencies, all.crates), (2, 2));
        // a member a transform made is not planned again
        let made = r(9, "transform", "f4 (batch of 2 after proof #1)", "done", 1);
        assert_eq!(planned_from(&root, &folders, &[made], Some(since)).members, 4);
    }
}
