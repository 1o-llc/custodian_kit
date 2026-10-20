// Copyright © 2026 1o. All rights reserved.

//! `handoff [JOB]`: the message a person reads after a job, made from the
//! records only: the job file, the closing receipt, the receipts the job
//! ran, its task history (intent, change, verification, problem, solution
//! and result records), the transform records and the count diff. Nothing
//! in it is written by hand; where the records are silent it says so.
//!
//! Its order is fixed: what is waiting on a person first (open decisions,
//! change specs a preflight refused, verification not satisfied, a refused
//! close), then what the job was for, what changed, what was decided and
//! by whom, what was verified, and what runs next. Each sentence says one
//! fact, in the names the records use. Every list stops at [`SHOWN`] with
//! "… and N more", and the whole message at [`MAX_LINES`]: `job brief` has
//! every fact.
//!
//! [`Reader::Agent`] adds the exact lines to run and the routes the job
//! worked on; [`Reader::James`] says the same next steps in sentences.

use crate::jobs::{job_file, Closing, JobFile};
use crate::questions::{Decision, Question};
use crate::reading::refusal::ReadingRefusal;
use crate::store::{job_folders, Receipt, Store, TestRow};
use crate::transform::TransformRecord;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use task_history::{DiffDocument, DiffRecordKind};

/// How many entries a list shows before "… and N more".
pub const SHOWN: usize = 5;

/// How many next steps a handoff names before "… and N more": every one is something to do, so more than a list of facts.
pub const NEXT_SHOWN: usize = 8;

/// The most lines a handoff has; past it, one line says how many more there were.
pub const MAX_LINES: usize = 60;

/// Who reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reader {
    /// A person: plain sentences, the next steps as sentences.
    James,
    /// An agent: the same, with the exact lines to run and the routes.
    Agent,
}

impl Reader {
    pub const WORDS: &'static [&'static str] = &["james", "agent"];

    pub fn of_word(w: &str) -> Option<Reader> {
        match w {
            "james" => Some(Reader::James),
            "agent" => Some(Reader::Agent),
            _ => None,
        }
    }
}

/// A transform the job wrote, with its receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Written {
    pub seq: i64,
    pub utc: String,
    pub spec: String,
    pub operation: String,
    pub target: String,
    pub files: usize,
}

/// A change spec a preflight in the job did not authorize, and no later preflight of it did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused {
    pub seq: i64,
    pub utc: String,
    pub spec: String,
    pub reason: String,
    /// The spec's file, as the refused preflight's receipt names it.
    #[serde(default)]
    pub file: Option<String>,
}

/// A decision of the job's task, answered, with the receipt that answered it when one names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decided {
    pub decision: Decision,
    pub seq: Option<i64>,
    pub utc: Option<String>,
}

/// One test line the job ran: its package and target, its receipt, how many of its tests failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestLine {
    pub package: String,
    pub target: String,
    pub seq: i64,
    pub utc: String,
    pub passed: usize,
    #[serde(default)]
    pub ignored: usize,
    pub failed: Vec<String>,
}

/// A test line that stopped before any test reported an outcome (a build that failed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stopped {
    pub seq: i64,
    pub utc: String,
    /// The line's name in its receipt: `nodeplanctl lib`.
    pub name: String,
    pub code: Option<i32>,
}

/// Lines a crate moved in the job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrateLines {
    pub name: String,
    pub added: usize,
    pub removed: usize,
    pub files: usize,
}

/// Everything a handoff says, read from the records.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handoff {
    pub id: String,
    pub name: String,
    /// `queued`, `running`, `done`.
    pub state: String,
    pub folder: String,
    pub mode: Option<String>,
    pub opened_utc: Option<String>,
    pub closed_utc: Option<String>,
    /// `accepted`, `pending`, `failed_test`, `rejected`; empty while open.
    pub result: String,
    pub result_reason: Option<String>,
    /// `<task>/<revision>`.
    pub task: Option<String>,
    pub intent: Option<String>,
    pub target: Option<String>,
    /// Facts from receipts after this number only.
    pub since: Option<i64>,
    // waiting on a person
    pub decisions_open: Vec<Question>,
    pub refused: Vec<Refused>,
    /// Each declared verification and its state at close (`satisfied`, `failed`, `not run`, `unchanged`; `open`
    /// while the job runs).
    pub verify: Vec<(String, String)>,
    pub close_refusal: Option<String>,
    pub uncovered: Vec<String>,
    // what changed
    pub lines: Option<String>,
    /// How many counts the job took (`data/counts.json`): with fewer than two, no lines were counted.
    pub counts_taken: usize,
    pub added: usize,
    pub removed: usize,
    pub files_changed: usize,
    pub crates: Vec<CrateLines>,
    pub written: Vec<Written>,
    // what was decided
    pub decided: Vec<Decided>,
    // what was verified
    pub tests: Vec<TestLine>,
    /// Failed tests that also failed in a run before the job's first transform.
    pub pre_existing: Vec<String>,
    /// The job's first transform receipt: the change a failure is measured against.
    pub first_change: Option<i64>,
    pub readings_made: Vec<String>,
    pub readings_reused: Vec<String>,
    /// Reading runs refused (their receipts).
    pub readings_refused: Vec<i64>,
    /// Test lines that stopped before any outcome.
    pub stopped: Vec<Stopped>,
    /// The targets of the job's readings, and the job's own target: the routes it worked on.
    pub routes: Vec<String>,
    pub runs: usize,
}

/// `2026-10-19T21:19:31Z` → `2026-10-19 21:19Z`.
pub fn when(utc: &str) -> String {
    match (utc.get(..10), utc.get(11..16)) {
        (Some(d), Some(t)) => format!("{d} {t}Z"),
        _ => utc.to_string(),
    }
}

/// The job folder a name or id names (its newest run, by the closing receipt's time), in done then running then
/// queued; with none named, the job closed last.
pub fn job_folder(store: &Store, which: Option<&str>) -> Result<PathBuf> {
    let root = store.jobs_dir(crate::store::JobState::Done)?.parent().map(Path::to_path_buf).unwrap_or_default();
    let closed_at = |p: &Path| closing_of(p).map(|c| c.closed_utc).unwrap_or_default();
    let mut found: Vec<(String, String, PathBuf)> = job_folders(&root)
        .into_iter()
        .filter(|(_, p)| match which {
            None => p.join("closing.json").is_file(),
            Some(w) => {
                let jf = job_file(p);
                p.file_name().is_some_and(|n| n == w) || jf.id.as_deref() == Some(w) || jf.name.as_deref() == Some(w)
            }
        })
        .map(|(state, p)| (if state == "done" { "1" } else { "2" }.to_string(), closed_at(&p), p))
        .collect();
    // done before running before queued; among done, the newest close last
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.pop().map(|(_, _, p)| p).ok_or_else(|| ReadingRefusal::NoJob.refuse(which.map(|w| format!("{w} in state/jobs")).unwrap_or_else(|| "no job has closed".into())))
}

/// The jobs that ran a receipt after `since`, in the order of their first such receipt.
pub fn jobs_since(store: &Store, receipts: &[Receipt], since: i64) -> Result<Vec<PathBuf>> {
    let mut ids: Vec<String> = Vec::new();
    for r in receipts.iter().filter(|r| r.seq > since) {
        if let Some(j) = &r.job {
            if !ids.contains(j) {
                ids.push(j.clone());
            }
        }
    }
    ids.iter().map(|id| job_folder(store, Some(id))).collect()
}

fn closing_of(folder: &Path) -> Option<Closing> {
    std::fs::read_to_string(folder.join("closing.json")).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// Every closed job's test outcomes (their closing receipts), for what failed before a change.
fn all_closed_tests(store: &Store) -> Vec<TestRow> {
    let root = store.jobs_dir(crate::store::JobState::Done).ok().and_then(|d| d.parent().map(Path::to_path_buf)).unwrap_or_default();
    job_folders(&root).into_iter().filter_map(|(_, p)| closing_of(&p)).flat_map(|c| c.tests).collect()
}

fn failed(t: &TestRow) -> bool {
    !matches!(t.outcome.as_str(), "ok" | "ignored")
}

/// The records of one revision of a task.
fn revision_records(store: &Store, task: &str, revision: &str) -> Result<Vec<DiffDocument>> {
    let Some(t) = crate::task_records::read(store, task)? else { return Ok(Vec::new()) };
    Ok(t.revisions.into_iter().find(|r| r.name == revision).map(|r| r.documents).unwrap_or_default())
}

/// What the records say about the job in `folder`. `receipts` are the journal's (every run), `since` limits the
/// facts that have a receipt to those after it.
pub fn of_job(root: &Path, store: &Store, folder: &Path, receipts: &[Receipt], since: Option<i64>) -> Result<Handoff> {
    let jf: JobFile = job_file(folder);
    let closing = closing_of(folder);
    let id = jf.id.clone().or_else(|| closing.as_ref().map(|c| c.id.clone())).unwrap_or_else(|| folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let state = folder.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let after = |seq: i64| since.is_none_or(|s| seq > s);
    let runs: Vec<&Receipt> = receipts.iter().filter(|r| r.job.as_deref() == Some(id.as_str())).collect();
    let mut h = Handoff {
        id: id.clone(),
        name: jf.name.clone().or_else(|| closing.as_ref().map(|c| c.name.clone())).unwrap_or_default(),
        state,
        folder: folder.strip_prefix(&store.root).or_else(|_| folder.strip_prefix(root)).unwrap_or(folder).to_string_lossy().into_owned(),
        mode: jf.mode.map(|m| m.as_str().to_string()).or_else(|| closing.as_ref().and_then(|c| c.mode).map(|m| m.as_str().to_string())),
        opened_utc: closing.as_ref().and_then(|c| c.opened_utc.clone()).or_else(|| runs.first().map(|r| r.utc.clone())),
        closed_utc: closing.as_ref().map(|c| c.closed_utc.clone()),
        target: jf.target.clone(),
        since,
        runs: match (&closing, since) {
            (Some(c), None) if !c.runs.is_empty() => c.runs.len(),
            _ => runs.iter().filter(|r| after(r.seq)).count(),
        },
        ..Handoff::default()
    };
    // the task history: intent, result, decisions
    let records = match (&jf.task_id, &jf.revision) {
        (Some(t), Some(r)) => {
            h.task = Some(format!("{t}/{r}"));
            revision_records(store, t, r)?
        }
        _ => Vec::new(),
    };
    let last = |kind: DiffRecordKind, key: &str| records.iter().filter(|d| d.kind().ok() == Some(kind)).filter_map(|d| d.one(key).ok()).next_back().map(str::to_string);
    h.intent = last(DiffRecordKind::Intent, "intent");
    h.result = closing.as_ref().map(|c| c.result.clone()).filter(|r| !r.is_empty()).or_else(|| last(DiffRecordKind::Result, "status")).unwrap_or_default();
    h.result_reason = closing.as_ref().and_then(|c| c.result_reason.clone()).or_else(|| last(DiffRecordKind::Result, "note"));
    // decisions: open ones asked by the task, answered ones of its problem records or its own solutions
    let decided = crate::questions::decided(store)?;
    let asked: BTreeSet<String> = records.iter().filter(|d| matches!(d.kind().ok(), Some(DiffRecordKind::Problem | DiffRecordKind::Solution))).filter_map(|d| d.one("decision_id").ok().map(str::to_string)).collect();
    let open = crate::questions::open(store)?;
    h.decisions_open = match &h.task {
        Some(t) => open.into_iter().filter(|q| q.task.as_deref() == Some(t.as_str()) || asked.contains(&q.id)).collect(),
        None => closing.as_ref().map(|c| c.decisions_open.clone()).unwrap_or_default(),
    };
    for qid in &asked {
        if let Some(d) = decided.get(qid) {
            let r = receipts.iter().find(|r| r.command == "decide" && r.line.contains(qid.as_str()));
            if r.is_none_or(|r| after(r.seq)) {
                h.decided.push(Decided { decision: d.clone(), seq: r.map(|r| r.seq), utc: r.map(|r| r.utc.clone()) });
            }
        }
    }
    // the verifications, the refused close
    h.verify = match &closing {
        Some(c) if !c.verify.is_empty() => c.verify.clone(),
        _ => jf.verify.iter().map(|v| (v.clone(), if closing.is_some() { "not judged".into() } else { "open".into() })).collect(),
    };
    if let Some(c) = &closing {
        h.close_refusal = c.refusal.as_ref().map(|r| r.to_string());
        h.uncovered = c.uncovered.iter().map(|u| u.path.clone()).collect();
    }
    // the change specs: written by a transform, or refused by a preflight and never authorized after
    let mut seen = BTreeSet::new();
    for r in runs.iter().filter(|r| r.command == "transform" && r.accepted && after(r.seq)) {
        let Some(out) = r.outputs.iter().find(|o| o.ends_with(".json")) else { continue };
        let Some(t) = std::fs::read_to_string(root.join(out)).ok().and_then(|t| serde_json::from_str::<TransformRecord>(&t).ok()) else { continue };
        if t.written && seen.insert(out.clone()) {
            h.written.push(Written { seq: r.seq, utc: r.utc.clone(), spec: t.spec_name.clone(), operation: t.operation.clone(), target: t.target.clone(), files: t.files.len() });
        }
    }
    h.first_change = runs.iter().filter(|r| r.command == "transform" && r.accepted).map(|r| r.seq).min();
    let spec_of = |r: &Receipt| r.name.trim_end_matches(".toml").rsplit('/').next().unwrap_or(&r.name).trim_start_matches("v1.").trim_end_matches(".1o.change").to_string();
    let mut preflights: BTreeMap<String, Vec<&Receipt>> = BTreeMap::new();
    for r in runs.iter().filter(|r| r.command == "preflight") {
        preflights.entry(spec_of(r)).or_default().push(r);
    }
    for (spec, rs) in preflights {
        let authorized = rs.iter().any(|r| r.reason.contains("authorized") && !r.reason.contains("not authorized"));
        let written = h.written.iter().any(|w| w.spec == spec);
        if let Some(r) = rs.iter().find(|r| r.reason.contains("not authorized") || r.reason.contains("refused")) {
            if !authorized && !written && after(r.seq) {
                let file = rs.iter().find_map(|x| x.spec.clone());
                h.refused.push(Refused { seq: r.seq, utc: r.utc.clone(), spec, reason: r.reason.clone(), file });
            }
        }
    }
    // lines touched: the job's count diff, per crate
    h.counts_taken = change_history::taken(folder).len();
    if let Some(t) = closing.as_ref().and_then(|c| c.lines_touched.iter().find(|t| t.label == "job")) {
        h.lines = Some(t.line.clone());
        h.added = t.moved.added.lines;
        h.removed = t.moved.removed.lines;
        h.files_changed = t.files;
        if let Some(d) = std::fs::read_to_string(folder.join(&t.diff)).ok().and_then(|x| serde_json::from_str::<crate::reading::count::CountDiff>(&x).ok()) {
            h.crates = d
                .crates
                .iter()
                .map(|c| CrateLines { name: c.name.clone(), added: c.moved.added.lines, removed: c.moved.removed.lines, files: c.files_added.len() + c.files_removed.len() + c.files_changed.len() })
                .collect();
            h.crates.sort_by(|a, b| (b.added + b.removed).cmp(&(a.added + a.removed)).then(b.files.cmp(&a.files)).then(a.name.cmp(&b.name)));
        }
    }
    // tests: the closing receipt's outcomes, by line; what failed before the first change
    let tests: Vec<TestRow> = closing.as_ref().map(|c| c.tests.clone()).unwrap_or_default().into_iter().filter(|t| after(t.seq)).collect();
    let mut by_line: BTreeMap<(String, String, i64), TestLine> = BTreeMap::new();
    for t in &tests {
        let e = by_line.entry((t.package.clone(), t.target.clone(), t.seq)).or_insert_with(|| TestLine {
            package: t.package.clone(),
            target: t.target.clone(),
            seq: t.seq,
            utc: receipts.iter().find(|r| r.seq == t.seq).map(|r| r.utc.clone()).unwrap_or_default(),
            passed: 0,
            ignored: 0,
            failed: Vec::new(),
        });
        if failed(t) {
            e.failed.push(t.test.clone());
        } else if t.outcome == "ok" {
            e.passed += 1;
        } else {
            e.ignored += 1;
        }
    }
    h.tests = by_line.into_values().collect();
    h.tests.sort_by_key(|t| t.seq);
    // a cargo line (the line runner's) that was not accepted and left no outcome: it stopped before its tests ran
    let reported: BTreeSet<i64> = tests.iter().map(|t| t.seq).collect();
    h.stopped = runs
        .iter()
        .filter(|r| r.command == "test" && r.runner == crate::receipts::RUNNER_LINE && !r.accepted && !reported.contains(&r.seq) && after(r.seq))
        .map(|r| Stopped { seq: r.seq, utc: r.utc.clone(), name: r.name.clone(), code: r.exit_code })
        .collect();
    if let Some(first) = h.first_change {
        let before: BTreeSet<(String, String, String)> = all_closed_tests(store).into_iter().filter(|t| t.seq < first && failed(t)).map(|t| (t.package, t.target, t.test)).collect();
        h.pre_existing = tests.iter().filter(|t| failed(t) && before.contains(&(t.package.clone(), t.target.clone(), t.test.clone()))).map(|t| format!("{} {} {}", t.package, t.target, t.test)).collect();
    }
    // readings made and reused, and the routes they read
    let folders = job_folders(&store.jobs_dir(crate::store::JobState::Done)?.parent().map(Path::to_path_buf).unwrap_or_default());
    let mut routes: BTreeSet<String> = jf.target.iter().cloned().collect();
    let reading_run = |r: &Receipt| crate::receipts::is_reading_run(r);
    h.readings_refused = runs.iter().filter(|r| reading_run(r) && !r.accepted && after(r.seq)).map(|r| r.seq).collect();
    for r in runs.iter().filter(|r| reading_run(r) && r.accepted && after(r.seq)) {
        if r.reused_seq.is_some() {
            h.readings_reused.push(r.name.clone());
        } else {
            h.readings_made.push(r.name.clone());
        }
        if let Some(f) = crate::receipts::reading_of(root, r, &folders).or_else(|| r.reused_seq.and_then(|n| receipts.iter().find(|x| x.seq == n)).and_then(|x| crate::receipts::reading_of(root, x, &folders))) {
            if let Ok(rep) = crate::reading::load(&root.join(&f.record)) {
                if rep.target != "*" && !rep.target.is_empty() {
                    routes.insert(rep.target);
                }
            }
        }
    }
    h.routes = routes.into_iter().collect();
    Ok(h)
}

/// Up to [`SHOWN`] of a list, then "… and N more".
fn listed(v: &[String], what: &str) -> Vec<String> {
    listed_up_to(v, what, SHOWN)
}

fn listed_up_to(v: &[String], what: &str, n: usize) -> Vec<String> {
    let mut out: Vec<String> = v.iter().take(n).map(|x| format!("- {x}")).collect();
    if v.len() > n {
        out.push(format!("- … and {} more {what}", v.len() - n));
    }
    out
}

/// A verification's state, as a sentence ends it.
fn standing(state: &str) -> String {
    match state {
        "satisfied" => "is satisfied".into(),
        "not run" => "has not run".into(),
        "failed" => "failed".into(),
        "unchanged" => "is unchanged: the reading it names did not change".into(),
        "open" => "is open: the job has not closed".into(),
        "not judged" => "was not judged at close".into(),
        other => format!("is {other}"),
    }
}

/// Receipt numbers as runs: `#46–#51, #60`.
fn seqs(v: &[i64]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j + 1 < v.len() && v[j + 1] == v[j] + 1 {
            j += 1;
        }
        out.push(if j > i { format!("#{}–#{}", v[i], v[j]) } else { format!("#{}", v[i]) });
        i = j + 1;
    }
    out.join(", ")
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The cargo line that runs exactly a test line's failed tests.
pub fn failed_line(t: &TestLine) -> String {
    let target = match t.target.as_str() {
        "lib" => "--lib".to_string(),
        "lib+bins" | "all" => "--lib --bins".to_string(),
        x => match x.strip_prefix("test:") {
            Some(name) => format!("--test {name}"),
            None => match x.strip_prefix("bin:") {
                Some(name) => format!("--bin {name}"),
                None => format!("--{x}"),
            },
        },
    };
    format!("cargo test -p {} {target} -- {} --exact", t.package, t.failed.join(" "))
}

/// The handoff as text, in its fixed order.
pub fn render(h: &Handoff, reader: Reader) -> String {
    let mut out: Vec<String> = vec![format!("# handoff: {} ({})", h.name, h.id), String::new()];
    // the head: when, how, and how it ended
    let span = match (&h.opened_utc, &h.closed_utc) {
        (Some(o), Some(c)) if o.get(..10) == c.get(..10) => format!("from {} to {}", when(o), c.get(11..16).map(|t| format!("{t}Z")).unwrap_or_default()),
        (Some(o), Some(c)) => format!("from {} to {}", when(o), when(c)),
        (Some(o), None) => format!("since {}", when(o)),
        _ => "with no receipt".into(),
    };
    let mode = h.mode.as_ref().map(|m| format!(" {m}")).unwrap_or_default();
    let ended = match (h.state.as_str(), h.result.as_str()) {
        ("done", "") => "It closed with no result recorded.".to_string(),
        ("done", r) => format!("It closed {r}{}.", h.result_reason.as_ref().map(|x| format!(": {x}")).unwrap_or_default()),
        (s, _) => format!("It is {s}, not closed."),
    };
    out.push(format!("The job {} ran{mode} {span}, {}. {ended}", h.name, plural(h.runs, "run", "runs")));
    if let Some(s) = h.since {
        out.push(format!("Only what happened after receipt #{s} is told."));
    }
    // 1. waiting on a person
    let mut waiting: Vec<String> = Vec::new();
    for q in &h.decisions_open {
        waiting.push(format!("Decision {} is open: {}{}", q.id, q.question, if q.options.is_empty() { String::new() } else { format!(" ({})", q.options.join(" | ")) }));
    }
    for r in &h.refused {
        waiting.push(format!("The change spec {} was refused by its preflight at {} (receipt #{}: {}) and never ran.", r.spec, when(&r.utc), r.seq, r.reason));
    }
    for (v, s) in h.verify.iter().filter(|(_, s)| s != "satisfied") {
        waiting.push(format!("The verification {v} {}.", standing(s)));
    }
    if let Some(r) = &h.close_refusal {
        waiting.push(format!("The close was refused: {r}"));
    }
    if !h.uncovered.is_empty() {
        waiting.push(format!("{} touched by hand in a bounded job: {}.", plural(h.uncovered.len(), "file was", "files were"), h.uncovered.join(", ")));
    }
    out.push(String::new());
    out.push("## Waiting on a person".into());
    if waiting.is_empty() {
        out.push("Nothing is waiting on a person.".into());
    } else {
        out.extend(listed(&waiting, "items"));
    }
    // 2. what it was for
    out.push(String::new());
    out.push("## What it was for".into());
    out.push(match &h.intent {
        Some(i) => format!("{i}{}", if i.ends_with('.') { "" } else { "." }),
        None => "The records name no intent.".into(),
    });
    if let Some(t) = &h.target {
        out.push(format!("Its target was {t}."));
    }
    // 3. what changed
    out.push(String::new());
    out.push("## What changed".into());
    match &h.lines {
        Some(_) => {
            out.push(format!("{} added and {} removed, across {} in {}.", plural(h.added, "line", "lines"), h.removed, plural(h.files_changed, "file", "files"), plural(h.crates.len(), "crate", "crates")));
            let crates: Vec<String> = h.crates.iter().map(|c| format!("{}: +{} −{} in {}", c.name, c.added, c.removed, plural(c.files, "file", "files"))).collect();
            out.extend(listed(&crates, "crates"));
        }
        None if h.counts_taken >= 2 => out.push("No lines changed: the counts at open and close are the same.".into()),
        None => out.push("Lines were not counted: the job took no count at open and close.".into()),
    }
    if h.written.is_empty() {
        out.push("No change spec was written through a transform.".into());
    } else {
        let ops: BTreeSet<&str> = h.written.iter().map(|w| w.operation.as_str()).collect();
        out.push(format!("{} written through transforms, by {}: {}.", plural(h.written.len(), "change spec was", "change specs were"), plural(ops.len(), "operation", "operations"), ops.into_iter().collect::<Vec<_>>().join(", ")));
        let w: Vec<String> = h.written.iter().map(|w| format!("{} ({} on {}, {}) at {}, receipt #{}", w.spec, w.operation, w.target, plural(w.files, "file", "files"), when(&w.utc), w.seq)).collect();
        out.extend(listed(&w, "change specs"));
    }
    // 4. what was decided
    out.push(String::new());
    out.push("## What was decided".into());
    if h.decided.is_empty() {
        out.push("No decision of this job's task was answered.".into());
    } else {
        let d: Vec<String> = h
            .decided
            .iter()
            .map(|d| {
                format!(
                    "{}: {} → {}. Why: {}. By: {}.{}",
                    d.decision.id,
                    d.decision.question,
                    d.decision.answer,
                    d.decision.why.clone().unwrap_or_else(|| "no reason recorded".into()),
                    d.decision.by.clone().unwrap_or_else(|| "the record names no one".into()),
                    match (&d.utc, d.seq) {
                        (Some(u), Some(n)) => format!(" At {}, receipt #{n}.", when(u)),
                        _ => String::new(),
                    }
                )
            })
            .collect();
        out.extend(listed(&d, "decisions"));
    }
    // 5. what was verified
    out.push(String::new());
    out.push("## What was verified".into());
    for (v, _) in h.verify.iter().filter(|(_, s)| s == "satisfied") {
        out.push(format!("The verification {v} is satisfied."));
    }
    if h.verify.is_empty() {
        out.push("The job declared no verification.".into());
    }
    if h.tests.is_empty() && h.stopped.is_empty() {
        out.push("No test line ran in the job.".into());
    } else {
        let passed: usize = h.tests.iter().map(|t| t.passed).sum();
        let ignored: usize = h.tests.iter().map(|t| t.ignored).sum();
        let failed: usize = h.tests.iter().map(|t| t.failed.len()).sum();
        let times: Vec<&str> = h.tests.iter().map(|t| t.utc.as_str()).chain(h.stopped.iter().map(|t| t.utc.as_str())).filter(|u| !u.is_empty()).collect();
        let (first, last) = (times.iter().min().copied().unwrap_or(""), times.iter().max().copied().unwrap_or(""));
        out.push(format!(
            "{} ran in {} between {} and {}: {passed} passed, {failed} failed{}.",
            plural(passed + failed + ignored, "test", "tests"),
            plural(h.tests.len() + h.stopped.len(), "test line", "test lines"),
            when(first),
            last.get(11..16).map(|t| format!("{t}Z")).unwrap_or_default(),
            if ignored > 0 { format!(", {ignored} ignored") } else { String::new() }
        ));
        let mut bad: Vec<String> = h.tests.iter().filter(|t| !t.failed.is_empty()).map(|t| format!("{} {}: {} failed (receipt #{})", t.package, t.target, t.failed.len(), t.seq)).collect();
        bad.extend(h.stopped.iter().map(|t| format!("{}: stopped with exit {} before any test reported (receipt #{})", t.name, t.code.map(|c| c.to_string()).unwrap_or_else(|| "none".into()), t.seq)));
        out.extend(listed(&bad, "test lines with failures"));
        if failed > 0 {
            match h.first_change {
                Some(first) if h.pre_existing.len() == failed => out.push(format!("All {failed} failed before the job's first change too (a run before receipt #{first}): they are pre-existing.")),
                Some(first) if !h.pre_existing.is_empty() => out.push(format!("{} of the {failed} failed before the job's first change too (a run before receipt #{first}); the rest are not known to be pre-existing.", h.pre_existing.len())),
                Some(first) => out.push(format!("None of the {failed} is known to be pre-existing: no run of them comes before the job's first change (receipt #{first}).")),
                None => out.push(format!("The job wrote no transform, so the {failed} failures are not measured against a change of its own.")),
            }
        }
    }
    out.push(format!("{} made and {} reused.", plural(h.readings_made.len(), "reading was", "readings were"), h.readings_reused.len()));
    if !h.readings_refused.is_empty() {
        out.push(format!("{} refused ({}).", plural(h.readings_refused.len(), "reading run was", "reading runs were"), seqs(&h.readings_refused)));
    }
    // 6. what runs next
    out.push(String::new());
    out.push("## What runs next".into());
    let failed_lines: Vec<&TestLine> = h.tests.iter().filter(|t| !t.failed.is_empty()).collect();
    let mut next: Vec<String> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for q in &h.decisions_open {
        next.push(format!("Decide {}: {}", q.id, q.question));
        lines.push(format!("custodian decide {} <answer> --why \"<the reason>\"", q.id));
    }
    for r in &h.refused {
        next.push(format!("Fix the change spec {} until its preflight authorizes it, or drop it.", r.spec));
        lines.push(format!("custodian preflight {}", r.file.clone().unwrap_or_else(|| format!(".custodian/data/specs/changes/{}.toml", r.spec))));
    }
    let reopen = h.state == "done" && (h.result != "accepted" || !h.decisions_open.is_empty());
    if reopen && (!failed_lines.is_empty() || !h.stopped.is_empty() || h.verify.iter().any(|(_, s)| s != "satisfied")) {
        next.push(format!("Open the next revision of {} and finish it there.", h.name));
        lines.push(format!("custodian job start {}", h.name));
    }
    let unsatisfied: Vec<&(String, String)> = h.verify.iter().filter(|(_, s)| s != "satisfied").collect();
    for (v, _) in &unsatisfied {
        match v.split_once(':') {
            Some(("reading", spec)) => {
                next.push(format!("Read {spec} in the job: done is that reading changing from the one before it."));
                lines.push(format!("custodian run {spec}"));
            }
            Some(("test", t)) => {
                next.push(format!("Run the test {t} until it passes."));
                lines.push("custodian test run --affected".into());
            }
            Some(("check", c)) => {
                next.push(format!("Run the check {c} until it is clean."));
                lines.push(format!("custodian check {c}"));
            }
            _ => {}
        }
    }
    if !failed_lines.is_empty() {
        let failed: usize = failed_lines.iter().map(|t| t.failed.len()).sum();
        next.push(format!("Make the {failed} failing tests pass, or show from a run before the change that they failed already."));
        lines.extend(failed_lines.iter().map(|t| failed_line(t)));
    }
    if !h.stopped.is_empty() {
        next.push(format!("Build {} again and read why {} stopped.", plural(h.stopped.len(), "test line", "test lines"), if h.stopped.len() == 1 { "it" } else { "they" }));
        lines.extend(h.stopped.iter().map(|t| format!("custodian test run --affected   # {} stopped at receipt #{}", t.name, t.seq)));
    }
    if reopen && !lines.iter().any(|l| l.starts_with("custodian job close")) {
        lines.push(format!("custodian job close {}", h.name));
    }
    if h.state != "done" {
        next.push("Close the job when its verification holds.".to_string());
        lines.push(format!("custodian job close {}", h.id));
    }
    if next.is_empty() {
        out.push("Nothing: the job is complete.".into());
    } else {
        out.extend(listed_up_to(&next, "steps", NEXT_SHOWN));
    }
    if reader == Reader::Agent {
        if !lines.is_empty() {
            out.push(String::new());
            out.push("The lines, in order:".into());
            // a test line names every failed test: it is shown whole, never cut
            out.extend(lines.iter().take(SHOWN * 2).map(|l| format!("    {l}")));
            if lines.len() > SHOWN * 2 {
                out.push(format!("    … and {} more lines", lines.len() - SHOWN * 2));
            }
        }
        out.push(String::new());
        out.push("The routes and targets it read:".into());
        if h.routes.is_empty() {
            out.push("- none recorded".into());
        } else {
            out.extend(listed(&h.routes, "routes"));
        }
        out.push(format!("Its folder: {}{}", h.folder, h.task.as_ref().map(|t| format!(" · its task history: {t}")).unwrap_or_default()));
    }
    if out.len() > MAX_LINES {
        let more = out.len() - MAX_LINES + 1;
        out.truncate(MAX_LINES - 1);
        out.push(format!("… and {more} more lines: custodian job brief {} has every fact.", h.id));
    }
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_test_lines_cargo_line_names_only_its_failures() {
        let t = TestLine { package: "fleet_ops".into(), target: "lib".into(), seq: 1, utc: String::new(), passed: 9, ignored: 0, failed: vec!["a::b".into(), "c".into()] };
        assert_eq!(failed_line(&t), "cargo test -p fleet_ops --lib -- a::b c --exact");
        let t = TestLine { target: "test:stop_rules".into(), ..t };
        assert_eq!(failed_line(&t), "cargo test -p fleet_ops --test stop_rules -- a::b c --exact");
        assert_eq!(when("2026-10-19T21:19:31Z"), "2026-10-19 21:19Z");
        assert_eq!(seqs(&[46, 47, 48, 51, 60, 61]), "#46–#48, #51, #60–#61");
    }

    #[test]
    fn a_long_handoff_stops_at_its_cap_and_says_how_much_more() {
        let h = Handoff {
            id: "job-1".into(),
            name: "n".into(),
            state: "running".into(),
            decisions_open: (0..40).map(|i| Question { id: format!("q-{i}"), question: format!("which {i}?"), options: vec![], why: None, source: "s".into(), task: None }).collect(),
            ..Handoff::default()
        };
        let text = render(&h, Reader::James);
        assert!(text.contains("- … and 35 more items"), "{text}");
        assert!(text.lines().count() <= MAX_LINES, "{text}");
    }
}
