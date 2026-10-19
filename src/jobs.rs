//! Jobs as folders (agent mode). A job is a folder dropped into
//! `jobs/queued/`: spec files read in name order, or a `job.toml` naming a
//! job kind whose template lists the steps (each a spec the template writes,
//! its target taken from the job or from an earlier step's reading). `job
//! start` moves the folder to `jobs/running/`, runs its readings in order
//! under the job's id, each through the executor with its receipt, and keeps
//! the readings in the folder. `job close` writes the closing receipt
//! (`closing.md`, `closing.json`: the specs read, the readings reused, what
//! changed since each spec was last read, the tests run, the sensitive items
//! touched) and moves the folder to `jobs/done/`. `job brief` prints the
//! closing receipt's facts, the part an agent brief carries.

use crate::diff::{self, ReadingDiff};
use crate::reading::refusal::ReadingRefusal;
use crate::reading::spec::ReadingSpec;
use crate::reading::{load, ReadingReport};
use crate::runs::{keep, now_utc, run_reading, Ctx, InJob, Ran};
use crate::store::{write_whole, JobRow, JobState, Receipt, Store, RECEIPT_SCHEMA};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The job templates custodian ships; `agent init` writes each to
/// `config/jobs/<kind>.toml`, where they are the user's to edit.
pub const SHIPPED_TEMPLATES: &[(&str, &str)] = &[("triage", TRIAGE)];

/// `custodian triage <CODE | @logfile>`: where a code is raised, what that
/// touches, the tests that prove a fix, and the marks there.
pub const TRIAGE: &str = r#"# triage: a liaise code (or a log's text, as @path) → the declaration and every site raising it,
# what each site reaches, the exact tests that prove a fix, and the marks on the items there.
kind = "triage"
summary = "raised here; touches these; prove a fix with these tests; the rule's owner is this item"

[[step]]
name = "codes"
kind = "diagnostics"
target = "{target}"

[[step]]
name = "reach"
kind = "reach"
# one reading per item that raises a code the first step found
for_each = { step = "codes", row = "code-raised", field = "item" }

[[step]]
name = "tests"
kind = "affected_tests"
target = "*"
# the change is the items that raise the code
change_items = { step = "codes", row = "code-raised", field = "item" }

[[step]]
name = "marks"
kind = "sensitivity"
target = "*"
"#;

/// Values taken from an earlier step's reading: the rows of a kind, one
/// attribute (`label`, `path` and `crate` name the row's own fields).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct From {
    pub step: String,
    pub row: String,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub for_each: Option<From>,
    #[serde(default)]
    pub change_items: Option<From>,
    /// Extra spec lines (TOML) appended to each step's spec: scope, readers, declared_in.
    #[serde(default)]
    pub with: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub kind: String,
    #[serde(default)]
    pub summary: String,
    pub step: Vec<Step>,
}

/// `job.toml` in a job's folder: all optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobFile {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// `specs` (the folder's spec files, the default) or a template's kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// A template's `{target}`.
    #[serde(default)]
    pub target: Option<String>,
    /// For an imported job: the digest of the bundle it came from.
    #[serde(default)]
    pub origin: Option<String>,
}

fn job_file(dir: &Path) -> JobFile {
    std::fs::read_to_string(dir.join("job.toml")).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
}

fn folders(store: &Store, state: JobState) -> Result<Vec<PathBuf>> {
    let dir = store.jobs_dir(state)?;
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
    v.sort();
    Ok(v)
}

/// The folder in `state` a name or id names (the folder's name, or its
/// job.toml's `id` or `name`); the first one when none is named.
fn find(store: &Store, state: JobState, which: Option<&str>) -> Result<PathBuf> {
    let all = folders(store, state)?;
    let found = match which {
        None => all.into_iter().next(),
        Some(w) => all.into_iter().find(|p| {
            let jf = job_file(p);
            p.file_name().is_some_and(|n| n == w) || jf.id.as_deref() == Some(w) || jf.name.as_deref() == Some(w)
        }),
    };
    found.ok_or_else(|| ReadingRefusal::NoJob.refuse(format!("{} in jobs/{}", which.unwrap_or("(the first)"), state.as_str())))
}

fn mv(from: &Path, to: &Path) -> Result<()> {
    if let Some(p) = to.parent() {
        std::fs::create_dir_all(p).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", p.display())))?;
    }
    if to.exists() {
        return Err(ReadingRefusal::JobState.refuse(format!("{} is already there", to.display())));
    }
    std::fs::rename(from, to).map_err(|e| ReadingRefusal::Write.refuse(format!("{} → {}: {e}", from.display(), to.display())))
}

/// What a job's start did.
#[derive(Debug)]
pub struct Started {
    pub id: String,
    pub folder: PathBuf,
    pub runs: Vec<Ran>,
    pub said: Vec<String>,
}

fn template(store: &Store, kind: &str) -> Result<Template> {
    let p = store.path(&crate::store::named::JOB_TEMPLATE, &[("kind", kind)])?;
    let text = std::fs::read_to_string(&p).or_else(|_| SHIPPED_TEMPLATES.iter().find(|(k, _)| *k == kind).map(|(_, t)| t.to_string()).ok_or(())).map_err(|_| ReadingRefusal::NoJob.refuse(format!("no job kind `{kind}` ({})", p.display())))?;
    toml::from_str(&text).map_err(|e| ReadingRefusal::SpecInvalid.refuse(format!("{}: {e}", p.display())))
}

/// The values an earlier step's readings hold.
fn values(done: &[(String, Vec<ReadingReport>)], from: &From) -> Vec<String> {
    let mut out = BTreeSet::new();
    for (name, reports) in done {
        if *name != from.step {
            continue;
        }
        for r in reports {
            for row in r.query.results.iter().filter(|x| x.kind == from.row) {
                let v = match from.field.as_str() {
                    "label" => Some(row.label.clone()),
                    "path" => row.path.clone(),
                    "crate" => row.crate_name.clone(),
                    f => row.attributes.get(f).cloned(),
                };
                if let Some(v) = v.filter(|v| !v.is_empty()) {
                    out.insert(v);
                }
            }
        }
    }
    out.into_iter().collect()
}

fn toml_str(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// Writes a step's spec into the job folder, for one target.
fn step_spec(folder: &Path, n: usize, step: &Step, target: &str, items: &[String], question: &str) -> Result<PathBuf> {
    let name = if n == 0 { step.name.clone() } else { format!("{}-{n}", step.name) };
    let mut t = format!("name = {}\nquestion = {}\nkind = {}\ntarget = {}\n", toml_str(&name), toml_str(question), toml_str(&step.kind), toml_str(target));
    if let Some(w) = &step.with {
        t.push_str(w);
        t.push('\n');
    }
    if !items.is_empty() {
        t.push_str(&format!("\n[change]\nitems = [{}]\n", items.iter().map(|i| toml_str(i)).collect::<Vec<_>>().join(", ")));
    }
    let p = folder.join("steps").join(format!("{name}.toml"));
    ReadingSpec::from_toml(&t)?;
    write_whole(&p, t.as_bytes())?;
    Ok(p)
}

/// `job start`: the queued folder moved to running and its readings run.
pub fn start(ctx: &Ctx, which: Option<&str>, again: bool) -> Result<Started> {
    let store = ctx.store()?;
    let running = crate::decisions::job_running(store)?;
    if !running.go {
        return Err(ReadingRefusal::JobRunning.refuse(running.reason));
    }
    let queued = find(store, JobState::Queued, which)?;
    let folder_name = queued.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut jf = job_file(&queued);
    let opened = now_utc();
    let name = jf.name.clone().unwrap_or_else(|| folder_name.clone());
    let id = jf.id.clone().unwrap_or_else(|| format!("job-{}", crate::digest::short(&crate::digest::of(format!("{name}\n{opened}")), 12)));
    let kind = jf.kind.clone().unwrap_or_else(|| "specs".to_string());
    let running = store.job_dir(JobState::Running, &folder_name)?;
    let running_decision_said = || "decided: is a job already running? — none running: start".to_string();
    mv(&queued, &running)?;
    jf.id = Some(id.clone());
    jf.name = Some(name.clone());
    jf.kind = Some(kind.clone());
    write_whole(&running.join("job.toml"), toml::to_string(&jf).map_err(|e| ReadingRefusal::Write.refuse(e))?.as_bytes())?;
    let journal = store.journal()?;
    journal.put_job(&JobRow { id: id.clone(), name: name.clone(), kind: kind.clone(), state: "running".into(), folder: ctx.rel(&running), opened_utc: Some(opened.clone()), closed_utc: None })?;
    let mut said = vec![running_decision_said(), format!("job {id} ({name}, {kind}) started: {} → {}", ctx.rel(&queued), ctx.rel(&running))];
    let in_job = InJob { id: id.clone(), readings: running.join("readings") };
    let mut runs = Vec::new();
    if kind == "specs" {
        let mut specs: Vec<PathBuf> = std::fs::read_dir(&running).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "toml") && p.file_name().is_some_and(|n| n != "job.toml")).collect()).unwrap_or_default();
        specs.sort();
        for s in specs {
            let ran = run_reading(ctx, &s, again, Some(&in_job))?;
            said.extend(ran.said.iter().cloned());
            runs.push(ran);
        }
    } else {
        let t = template(store, &kind)?;
        let target = jf.target.clone().unwrap_or_default();
        let mut done: Vec<(String, Vec<ReadingReport>)> = Vec::new();
        for step in &t.step {
            let question = format!("{} — step {} of the {} job for {target}", t.summary, step.name, t.kind);
            let items = step.change_items.as_ref().map(|f| values(&done, f)).unwrap_or_default();
            let targets: Vec<String> = match (&step.for_each, &step.target) {
                (Some(f), _) => values(&done, f),
                (None, Some(tt)) => vec![tt.replace("{target}", &target)],
                (None, None) => vec![target.clone()],
            };
            let mut reports = Vec::new();
            let gate = match (&step.change_items, &step.for_each) {
                (Some(f), _) => Some(crate::decisions::step_input(&step.name, &f.step, &items)),
                (None, Some(f)) => Some(crate::decisions::step_input(&step.name, &f.step, &targets)),
                _ => None,
            };
            if let Some(g) = &gate {
                said.push(g.said());
            }
            if gate.as_ref().is_some_and(|g| !g.go) {
                // the decision said there is nothing to read: the step is not run
            } else {
                for (n, tgt) in targets.iter().enumerate() {
                    let spec = step_spec(&running, n, step, tgt, &items, &question)?;
                    let ran = run_reading(ctx, &spec, again, Some(&in_job))?;
                    said.extend(ran.said.iter().cloned());
                    if let Some(j) = ran.receipt.as_ref().and_then(|r| r.outputs.iter().find(|o| o.ends_with(".json"))) {
                        reports.push(load(&ctx.root.join(j))?);
                    }
                    runs.push(ran);
                }
            }
            done.push((step.name.clone(), reports));
        }
    }
    Ok(Started { id, folder: running, runs, said })
}

/// One spec's part of the closing receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecClosed {
    pub name: String,
    pub seq: i64,
    pub reused_of: Option<i64>,
    pub reading: Option<String>,
    pub counts: Vec<crate::reading::Count>,
    /// Against the last reading of the same spec before this job.
    pub changed: Option<ReadingDiff>,
}

/// The closing receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Closing {
    pub schema: String,
    pub id: String,
    pub name: String,
    pub kind: String,
    pub opened_utc: Option<String>,
    pub closed_utc: String,
    pub specs: Vec<SpecClosed>,
    pub runs: Vec<i64>,
    pub reused: usize,
    pub tests: Vec<crate::store::TestRow>,
    /// `(item, level)` for every item at confidential or above the readings name.
    pub sensitive: Vec<(String, String)>,
    /// Every item the readings name.
    pub touched: Vec<String>,
    /// The template's own guidance line, for a template job.
    pub guidance: Option<String>,
    /// For triage: where each code is raised, the tests, the marks (mechanism / boundary / supersedes / owner).
    pub findings: Vec<String>,
}

pub const CLOSING_SCHEMA: &str = "custodian.job_closing.v1";

fn previous(store: &Store, name: &str, before: i64, job: &str, root: &Path) -> Option<ReadingReport> {
    let prev = store.journal().ok()?.readings_of(name).ok()?.into_iter().filter(|r| r.seq < before && r.job.as_deref() != Some(job)).last()?;
    prev.outputs.iter().find(|o| o.ends_with(".json")).and_then(|j| load(&root.join(j)).ok())
}

/// `job close`: the closing receipt written into the folder, the folder moved to done.
pub fn close(ctx: &Ctx, which: &str) -> Result<(Closing, PathBuf, Vec<String>)> {
    let store = ctx.store()?;
    let running = find(store, JobState::Running, Some(which))?;
    let jf = job_file(&running);
    let id = jf.id.clone().ok_or_else(|| ReadingRefusal::JobState.refuse(format!("{} has no id: was it started?", running.display())))?;
    let journal = store.journal()?;
    let row = journal.job(&id)?;
    let runs = journal.runs_of_job(&id)?;
    let level_floor = 2usize; // confidential in the shipped table; a registry's own table is read below
    let mut specs = Vec::new();
    let mut sensitive = BTreeSet::new();
    let mut touched = BTreeSet::new();
    let mut findings = Vec::new();
    for r in runs.iter().filter(|r| r.command == "reading") {
        let json = r.outputs.iter().find(|o| o.ends_with(".json"));
        let report = json.and_then(|j| load(&ctx.root.join(j)).ok());
        if let Some(rep) = &report {
            touched.extend(rep.items.iter().cloned());
            for row in rep.query.results.iter() {
                if let (Some(item), Some(level)) = (row.attributes.get("item"), row.attributes.get("sensitivity")) {
                    let rank = ["public", "internal", "confidential", "secret", "key_material"].iter().position(|l| l == level);
                    if rank.is_none_or(|r| r >= level_floor) {
                        sensitive.insert((item.clone(), level.clone()));
                    }
                }
                match row.kind.as_str() {
                    "code" => findings.push(format!("{} declared at {}:{} — {}", row.label, row.path.clone().unwrap_or_default(), row.line.unwrap_or(0), row.attributes.get("message").cloned().unwrap_or_default())),
                    "code-raised" => {
                        let mut marks = Vec::new();
                        for k in ["mechanism", "boundary", "supersedes", "owner", "sensitivity"] {
                            if let Some(v) = row.attributes.get(k) {
                                marks.push(format!("{k} {v}"));
                            }
                        }
                        findings.push(format!(
                            "raised here: {}:{} in {}{}",
                            row.path.clone().unwrap_or_default(),
                            row.line.unwrap_or(0),
                            row.attributes.get("item").cloned().unwrap_or_default(),
                            if marks.is_empty() { String::new() } else { format!(" ({})", marks.join(", ")) }
                        ))
                    }
                    "test-line" => findings.push(format!("prove a fix with: {}", row.label)),
                    _ => {}
                }
            }
        }
        let changed = report.as_ref().and_then(|rep| previous(store, &r.name, r.seq, &id, &ctx.root).map(|prev| diff::compare(&prev, rep)));
        specs.push(SpecClosed {
            name: r.name.clone(),
            seq: r.seq,
            reused_of: r.reused_seq,
            reading: r.outputs.first().cloned(),
            counts: report.map(|rep| rep.counts).unwrap_or_default(),
            changed,
        });
    }
    let test_seqs: Vec<i64> = runs.iter().filter(|r| r.command == "test").map(|r| r.seq).collect();
    let tests = journal.tests_of_runs(&test_seqs)?;
    let kind = jf.kind.clone().unwrap_or_else(|| "specs".into());
    let guidance = (kind != "specs").then(|| template(store, &kind).ok().map(|t| t.summary)).flatten();
    let closing = Closing {
        schema: CLOSING_SCHEMA.into(),
        id: id.clone(),
        name: jf.name.clone().unwrap_or_default(),
        kind,
        opened_utc: row.and_then(|r| r.opened_utc),
        closed_utc: now_utc(),
        reused: specs.iter().filter(|s| s.reused_of.is_some()).count(),
        specs,
        runs: runs.iter().map(|r| r.seq).collect(),
        tests,
        sensitive: sensitive.into_iter().collect(),
        touched: touched.into_iter().collect(),
        guidance,
        findings,
    };
    let json = serde_json::to_string_pretty(&closing)? + "\n";
    write_whole(&running.join("closing.json"), json.as_bytes())?;
    write_whole(&running.join("closing.md"), render_closing(&closing).as_bytes())?;
    let folder_name = running.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let done = store.job_dir(JobState::Done, &folder_name)?;
    mv(&running, &done)?;
    journal.put_job(&JobRow { id: id.clone(), name: closing.name.clone(), kind: closing.kind.clone(), state: "done".into(), folder: ctx.rel(&done), opened_utc: closing.opened_utc.clone(), closed_utc: Some(closing.closed_utc.clone()) })?;
    let digest = crate::digest::tagged(&json);
    let r = Receipt {
        schema: RECEIPT_SCHEMA.into(),
        seq: 0,
        utc: closing.closed_utc.clone(),
        command: "job-close".into(),
        name: id.clone(),
        spec: None,
        spec_hash: None,
        code_digest: None,
        index_key: None,
        plan_id: format!("job:{id}"),
        plan_digest: digest.clone(),
        operation_digest: digest.clone(),
        receipt_digest: digest,
        line: format!("custodian job close {id}"),
        exit_code: Some(0),
        accepted: true,
        disposition: "Stop".into(),
        reason: "closed".into(),
        outputs: vec![ctx.rel(&done.join("closing.md")), ctx.rel(&done.join("closing.json"))],
        job: Some(id.clone()),
        reused_seq: None,
        millis: 0,
    };
    let (r, f) = keep(store, r)?;
    let said = vec![
        format!("job {id} closed: {} → {}", ctx.rel(&running), ctx.rel(&done)),
        format!("closing receipt: {}", ctx.rel(&done.join("closing.md"))),
        format!("journal: run #{} job-close, receipt {}", r.seq, ctx.rel(&f)),
    ];
    Ok((closing, done, said))
}

/// The closing receipt as a page: facts first.
pub fn render_closing(c: &Closing) -> String {
    let mut s = format!("# job {} — {}\n\n", c.id, c.name);
    s.push_str(&format!("- kind: {} · opened {} · closed {}\n- runs: {} (#{}) · readings reused from the cache: {}\n", c.kind, c.opened_utc.clone().unwrap_or_default(), c.closed_utc, c.runs.len(), c.runs.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", #"), c.reused));
    if let Some(g) = &c.guidance {
        s.push_str(&format!("- guidance: {g}\n"));
    }
    if !c.findings.is_empty() {
        s.push_str("\n## findings\n\n");
        for f in &c.findings {
            s.push_str(&format!("- {f}\n"));
        }
    }
    s.push_str("\n## readings\n\n");
    for sp in &c.specs {
        let counts: Vec<String> = sp.counts.iter().map(|x| format!("{} {}", x.what, x.n)).collect();
        let reused = sp.reused_of.map(|n| format!(" (reused #{n})")).unwrap_or_default();
        s.push_str(&format!("- {} #{}{reused}: {} — {}\n", sp.name, sp.seq, sp.reading.clone().unwrap_or_default(), counts.join("; ")));
    }
    s.push_str("\n## what changed since each spec was last read\n\n");
    let mut any = false;
    for sp in &c.specs {
        match &sp.changed {
            Some(d) if !d.is_empty() => {
                any = true;
                let moved: Vec<String> = d.counts.iter().filter(|x| x.before != x.after).map(|x| format!("{} {}→{}", x.what, x.before.map(|n| n.to_string()).unwrap_or("-".into()), x.after.map(|n| n.to_string()).unwrap_or("-".into()))).collect();
                s.push_str(&format!("- {}: {} added, {} removed; {}\n", sp.name, d.added.len(), d.removed.len(), moved.join("; ")));
            }
            Some(_) => s.push_str(&format!("- {}: nothing changed\n", sp.name)),
            None => s.push_str(&format!("- {}: first reading\n", sp.name)),
        }
    }
    if !any {
        s.push_str("\n");
    }
    s.push_str(&format!("\n## tests run ({})\n\n", c.tests.len()));
    for t in &c.tests {
        s.push_str(&format!("- {} {} {}: {}\n", t.package, t.target, t.test, t.outcome));
    }
    s.push_str(&format!("\n## sensitive items touched ({})\n\n", c.sensitive.len()));
    for (item, level) in &c.sensitive {
        s.push_str(&format!("- {item}: {level}\n"));
    }
    s.push_str(&format!("\n## items touched: {}\n", c.touched.len()));
    s
}

/// `job brief`: the closing receipt of a done job, facts first.
pub fn brief(ctx: &Ctx, which: &str) -> Result<String> {
    let store = ctx.store()?;
    let done = find(store, JobState::Done, Some(which))?;
    let text = std::fs::read_to_string(done.join("closing.json")).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", done.display())))?;
    let c: Closing = serde_json::from_str(&text).map_err(|e| ReadingRefusal::MissingInput.refuse(e))?;
    let mut s = format!("job {} ({}) closed {} — {} runs, {} reused\n", c.id, c.name, c.closed_utc, c.runs.len(), c.reused);
    if let Some(g) = &c.guidance {
        s.push_str(&format!("guidance: {g}\n"));
    }
    for f in &c.findings {
        s.push_str(&format!("- {f}\n"));
    }
    for sp in &c.specs {
        let counts: Vec<String> = sp.counts.iter().take(4).map(|x| format!("{} {}", x.what, x.n)).collect();
        let change = match &sp.changed {
            Some(d) if !d.is_empty() => format!(" — changed: +{} −{}", d.added.len(), d.removed.len()),
            Some(_) => " — unchanged".into(),
            None => " — first reading".into(),
        };
        s.push_str(&format!("- {}: {}{change} ({})\n", sp.name, counts.join("; "), sp.reading.clone().unwrap_or_default()));
    }
    if !c.tests.is_empty() {
        let failed = c.tests.iter().filter(|t| t.outcome != "ok" && t.outcome != "ignored").count();
        s.push_str(&format!("tests: {} run, {} failed\n", c.tests.len(), failed));
    }
    if !c.sensitive.is_empty() {
        s.push_str(&format!("sensitive items touched: {}\n", c.sensitive.iter().map(|(i, l)| format!("{i} ({l})")).collect::<Vec<_>>().join(", ")));
    }
    s.push_str(&format!("closing receipt: {}\n", ctx.rel(&done.join("closing.md"))));
    Ok(s)
}

/// A template job started from the command line (`custodian triage CODE`):
/// its folder written to queued, started, closed.
pub fn run_template(ctx: &Ctx, kind: &str, target: &str, again: bool) -> Result<(Started, Closing, PathBuf, Vec<String>)> {
    let store = ctx.store()?;
    let name = format!("{kind}-{}", crate::runs::stamp());
    let dir = store.job_dir(JobState::Queued, &name)?;
    let jf = JobFile { id: None, name: Some(name.clone()), kind: Some(kind.to_string()), target: Some(target.to_string()), origin: None };
    write_whole(&dir.join("job.toml"), toml::to_string(&jf).map_err(|e| ReadingRefusal::Write.refuse(e))?.as_bytes())?;
    let started = start(ctx, Some(&name), again)?;
    let (closing, done, said) = close(ctx, &started.id)?;
    Ok((started, closing, done, said))
}
