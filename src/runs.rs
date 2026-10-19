//! Runs: a reading started by the executor (a `custodian exec-reading` line),
//! the same-search check before it, the receipt after it, the queue of specs,
//! and the inventory rendered from the receipts. Two modes: with an
//! initialized folder every run is journaled and its files kept; without one
//! (free mode) the reading is printed and nothing is stored.

use crate::exec::{self, at, Custodian, Plan, Watch};
use crate::reading::refusal::ReadingRefusal;
use crate::reading::spec::{ReadingKind, ReadingSpec};
use crate::reading::{ReadingSummary, CHANGE_FILES_ENV};
use crate::store::{named, write_whole, Receipt, Store};
use crate::{code_digest, Result};
use std::path::{Path, PathBuf};

/// Where a run happens.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub root: PathBuf,
    /// The initialized folder; `None` is free mode.
    pub store: Option<Store>,
    /// The custodian binary the reading lines run.
    pub exe: PathBuf,
}

impl Ctx {
    /// From a directory: the initialized folder at it or above, else free mode
    /// rooted at `start`.
    pub fn find(start: &Path, exe: PathBuf) -> Ctx {
        match Store::find(start) {
            Some(s) => Ctx { root: s.root.clone(), store: Some(s), exe },
            None => Ctx { root: start.canonicalize().unwrap_or_else(|_| start.to_path_buf()), store: None, exe },
        }
    }

    pub fn store(&self) -> Result<&Store> {
        self.store.as_ref().ok_or_else(|| ReadingRefusal::NotInitialized.refuse(self.root.display()))
    }

    pub fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).unwrap_or(p).to_string_lossy().replace('\\', "/")
    }
}

/// A job a run belongs to: its id, and the folder its readings go to.
#[derive(Debug, Clone)]
pub struct InJob {
    pub id: String,
    pub readings: PathBuf,
}

/// What one run did, and where.
#[derive(Debug, Clone)]
pub struct Ran {
    pub receipt: Option<Receipt>,
    pub receipt_file: Option<PathBuf>,
    pub summary: Option<ReadingSummary>,
    /// The earlier run whose reading answered, for a reuse.
    pub reused: Option<Receipt>,
    /// The page as printed, in free mode.
    pub printed: Option<String>,
    /// What the run says it did, one line each.
    pub said: Vec<String>,
}

pub fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

/// The spec's digest: its canonical form (the parsed spec, keys sorted), so
/// comments and spacing do not make it another search.
pub fn spec_hash(spec: &ReadingSpec) -> String {
    crate::digest::of_value(spec)
}

pub use crate::receipts::{keep, receipt_of};

/// The summary line a reading line printed last.
fn summary_of(stdout: &str) -> Option<ReadingSummary> {
    stdout.lines().rev().find(|l| !l.trim().is_empty()).and_then(|l| serde_json::from_str(l).ok())
}

/// The files a git ref's diff names, by the declared `git diff --name-only` line.
fn changed_files(ctx: &Ctx, git_ref: &str, job: Option<&InJob>) -> Result<(Vec<String>, Vec<String>)> {
    let plan = Plan::new(format!("git-diff:{git_ref}")).then(exec::line("git-diff", &exec::git_diff_names(&ctx.root, git_ref))?);
    let walked = plan.walk(Watch::Quiet, |_, _| {})?;
    let w = walked.first().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse("git"))?;
    let mut said = Vec::new();
    if let Some(store) = &ctx.store {
        let (r, f) = keep(store, receipt_of(&plan, w, "git", git_ref, None, None, None, None, vec![], job))?;
        said.push(format!("git: {} (run #{}, receipt {})", r.line, r.seq, ctx.rel(&f)));
    }
    if !w.decision.status.accepted {
        return Err(ReadingRefusal::LineRefused.refuse(format!("{}: {}", w.operation.display, w.stderr.trim())));
    }
    Ok((w.stdout.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect(), said))
}

/// One reading: the same-search check, the line run through the executor,
/// the receipt.
pub fn run_reading(ctx: &Ctx, spec_path: &Path, again: bool, job: Option<&InJob>) -> Result<Ran> {
    run_reading_by(ctx, spec_path, again, job, Runner::Line)
}

/// Who answers a reading: the declared line, run through the executor, or
/// this process over a workspace it holds open. Either way the same line is
/// built, the same decision taken on its status, the same receipt kept; the
/// receipt says which (`runner`).
#[derive(Clone, Copy)]
pub enum Runner<'w> {
    Line,
    InProcess(&'w crate::api::Workspace),
}

/// [`run_reading`] with the runner named.
pub fn run_reading_by(ctx: &Ctx, spec_path: &Path, again: bool, job: Option<&InJob>, runner: Runner<'_>) -> Result<Ran> {
    let spec = ReadingSpec::from_path(spec_path)?;
    let mut said = Vec::new();
    let mut env = Vec::new();
    if spec.kind == ReadingKind::AffectedTests {
        if let Some(r) = &spec.change.git_ref {
            let (files, s) = changed_files(ctx, r, job)?;
            said.extend(s);
            env.push((CHANGE_FILES_ENV.to_string(), files.join("\n")));
        }
    }
    let hash = spec_hash(&spec);
    let digest = code_digest(&ctx.root, &spec.scope.scan_options()).map_err(|e| ReadingRefusal::Scan.refuse(e))?;
    let spec_rel = ctx.rel(&spec_path.canonicalize().unwrap_or_else(|_| spec_path.to_path_buf()));

    if let (Some(store), false, true) = (&ctx.store, again, env.is_empty()) {
        let (decided, prev) = crate::decisions::same_search(&store.journal()?, &hash, &digest)?;
        said.push(decided.said());
        if let (false, Some(prev)) = (decided.go, prev) {
            let mut outputs = prev.outputs.clone();
            if let Some(j) = job {
                outputs = copy_into(&prev.outputs, &j.readings.join(&spec.name), &ctx.root)?;
            }
            let reuse = Receipt { seq: 0, utc: now_utc(), reused_seq: Some(prev.seq), job: job.map(|j| j.id.clone()), outputs, millis: 0, ..prev.clone() };
            let (r, f) = keep(store, reuse)?;
            said.push(format!(
                "this is search #{} from {}: same spec ({}), same code — reused, not read again (--again reads again)",
                prev.seq,
                prev.utc,
                crate::digest::short(&hash, 12)
            ));
            said.push(format!("reading: {}", r.outputs.first().map(String::as_str).unwrap_or("-")));
            said.push(format!("journal: run #{} (reuse of #{}), receipt {}", r.seq, prev.seq, ctx.rel(&f)));
            return Ok(Ran { receipt: Some(r), receipt_file: Some(f), summary: None, reused: Some(prev), printed: None, said });
        }
    }

    let out = match (job, &ctx.store) {
        (Some(j), _) => j.readings.display().to_string(),
        (None, Some(store)) => store.reading_dir("x")?.parent().map(|p| p.display().to_string()).unwrap_or_default(),
        (None, None) => "-".to_string(),
    };
    let stamp = stamp();
    let mut command = Custodian::exec_reading(spec_path.as_os_str(), ctx.root.as_os_str(), out.as_str(), stamp.as_str());
    if let Some(j) = job {
        command = command.job(j.id.as_str());
    }
    let mut l = exec::line(format!("read:{}", spec.name), &at(&ctx.exe, command))?;
    let change_files = env.iter().find(|(k, _)| k == CHANGE_FILES_ENV).map(|(_, v)| v.clone());
    l.env = env;
    let plan = Plan::new(format!("reading:{}", spec.name)).then(l);
    let walked = match runner {
        Runner::Line => plan.walk(Watch::Quiet, |_, _| {})?,
        Runner::InProcess(ws) => {
            // the line's own body, here: its summary as its last line, or its refusal as a failed status
            let started = std::time::Instant::now();
            let how = if job.is_some() { crate::paths::Resolving::Job } else { crate::paths::Resolving::Free };
            let w = match crate::reading::answer(spec_path, &ctx.root, &out, &stamp, how, Some(ws), change_files.as_deref()) {
                Ok(summary) => plan.answered(0, serde_json::to_string(&summary)? + "\n", String::new(), started.elapsed().as_millis() as i64)?,
                Err(e) => plan.answered(1, String::new(), e.to_string(), started.elapsed().as_millis() as i64)?,
            };
            vec![w]
        }
    };
    let runner_word = match runner {
        Runner::Line => crate::receipts::RUNNER_LINE,
        Runner::InProcess(_) => crate::receipts::RUNNER_IN_PROCESS,
    };
    let w = walked.first().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse(&spec.name))?;
    let summary = summary_of(&w.stdout);
    let printed = (ctx.store.is_none() && job.is_none()).then(|| {
        let mut lines: Vec<&str> = w.stdout.lines().collect();
        if summary.is_some() {
            lines.pop();
        }
        lines.join("\n")
    });
    let outputs: Vec<String> = summary.iter().flat_map(|s| [s.page.as_ref(), s.json.as_ref(), s.bundle.as_ref()]).flatten().map(|p| ctx.rel(p)).collect();
    let mut receipt = receipt_of(&plan, w, "reading", &spec.name, Some(spec_rel), Some(hash), Some(digest), summary.as_ref().and_then(|s| s.index_key.clone()), outputs, job);
    receipt.runner = runner_word.to_string();
    if !w.decision.status.accepted {
        let why = w.stderr.trim().to_string();
        if let Some(store) = &ctx.store {
            let (r, f) = keep(store, receipt)?;
            said.push(format!("journal: run #{} refused ({}), receipt {}", r.seq, r.reason, ctx.rel(&f)));
        }
        return Err(ReadingRefusal::LineRefused.refuse(format!("{} — {why} {}", w.operation.display, said.join("; "))));
    }
    match &ctx.store {
        Some(store) => {
            let (r, f) = keep(store, receipt)?;
            if let Some(s) = &summary {
                said.push(format!("reading: {}", s.page.as_ref().map(|p| ctx.rel(p)).unwrap_or_default()));
            }
            said.push(format!("receipt: {}", ctx.rel(&f)));
            said.push(format!("journal: run #{} {} {} ({} ms, {})", r.seq, r.command, r.name, r.millis, r.reason));
            Ok(Ran { receipt: Some(r), receipt_file: Some(f), summary, reused: None, printed, said })
        }
        None => {
            said.push(format!("free mode: nothing stored (line {}, receipt digest {})", receipt.line, crate::digest::short(&receipt.receipt_digest, 16)));
            Ok(Ran { receipt: None, receipt_file: None, summary, reused: None, printed, said })
        }
    }
}

/// Copies an earlier reading's files into a job's folder, whole.
fn copy_into(outputs: &[String], dir: &Path, root: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for o in outputs {
        let from = root.join(o);
        let Some(name) = from.file_name() else { continue };
        let to = dir.join(name);
        let bytes = std::fs::read(&from).map_err(|e| ReadingRefusal::ReadingUnreadable.refuse(format!("{}: {e}", from.display())))?;
        write_whole(&to, &bytes)?;
        out.push(to.strip_prefix(root).unwrap_or(&to).to_string_lossy().replace('\\', "/"));
    }
    Ok(out)
}

/// The specs waiting in `specs/queued`, in name order.
pub fn queued_specs(store: &Store) -> Result<Vec<PathBuf>> {
    let dir = store.dir_of(&named::QUEUED_SPEC, &[("name", "x")])?;
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| crate::reading::spec::SpecForm::of(p).is_some()).collect()).unwrap_or_default();
    v.sort();
    Ok(v)
}

/// Runs every queued spec in order, moving each to `specs/done` once read.
pub fn run_queue(ctx: &Ctx, again: bool) -> Result<Vec<Ran>> {
    let store = ctx.store()?;
    let mut out = Vec::new();
    for spec in queued_specs(store)? {
        let mut ran = run_reading(ctx, &spec, again, None)?;
        // the done spec keeps its form and its name; a second one of the name gets the stamp
        let form = crate::reading::spec::SpecForm::of(&spec).unwrap_or(crate::reading::spec::SpecForm::Toml);
        let stem = crate::reading::spec::SpecForm::stem(&spec).unwrap_or_default();
        let done_dir = store.dir_of(&named::DONE_SPEC, &[("name", "x")])?;
        let mut done = done_dir.join(form.file_name(&stem));
        if done.exists() {
            done = done_dir.join(form.file_name(&format!("{stem}-{}", stamp())));
        }
        if let Some(d) = done.parent() {
            std::fs::create_dir_all(d).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", d.display())))?;
        }
        std::fs::rename(&spec, &done).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", done.display())))?;
        ran.said.push(format!("spec moved: {} → {}", ctx.rel(&spec), ctx.rel(&done)));
        out.push(ran);
    }
    Ok(out)
}

/// `data/inventory.md`, rendered from the journal's receipts.
pub fn inventory(store: &Store) -> Result<PathBuf> {
    let runs = store.journal()?.runs()?;
    let mut s = String::new();
    s.push_str("# inventory\n\nEvery run custodian made here, from its receipts (the journal), oldest first.\n\n");
    s.push_str("| # | utc | command | name | exit | reason | reused | job | outputs |\n|---|---|---|---|---|---|---|---|---|\n");
    for r in &runs {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.seq,
            r.utc,
            r.command,
            r.name,
            r.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
            r.reason,
            r.reused_seq.map(|n| format!("#{n}")).unwrap_or_default(),
            r.job.clone().unwrap_or_default(),
            r.outputs.join(" ")
        ));
    }
    let p = store.path(&named::INVENTORY, &[])?;
    write_whole(&p, s.as_bytes())?;
    Ok(p)
}
