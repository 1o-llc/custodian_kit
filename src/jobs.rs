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
use crate::runs::{keep, now_utc, run_reading_by, Ctx, InJob, Ran, Runner};
use crate::store::{write_whole, JobRow, JobState, Receipt, Store, RECEIPT_SCHEMA};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The job templates custodian ships; `agent init` writes each to
/// `config/jobs/<kind>.toml`, where they are the user's to edit.
#[path = "jobs_runner.rs"]
pub mod runner;

pub const SHIPPED_TEMPLATES: &[(&str, &str)] = &[("triage", TRIAGE), ("features", FEATURES)];

/// Step kinds that change code (a count is taken after each, when the job counts).
/// Readings change nothing, so no count follows them.
pub const CHANGES_CODE: &[&str] = &["apply", "annotate", "transform"];

/// `custodian job start` for a crate's features: the evidence the eligibility reads.
pub const FEATURES: &str = r#"# features: a crate's public API, what depends on it and what a change to it reaches,
# counted at open and close so the closing receipt says how many lines the job touched.
kind = "features"
summary = "the crate's public API, its dependents and its reach: the evidence features judges with"
count = true
count_route = "crate:{target}"

[[step]]
name = "public-api"
kind = "public_api"
target = "crate:{target}"

[[step]]
name = "dependents"
kind = "dependents"
target = "crate:{target}"

[[step]]
name = "reach"
kind = "reach"
target = "crate:{target}"
"#;

/// `custodian triage <CODE | @logfile>`: where a code is raised, what that
/// touches, the tests that prove a fix, and the marks there.
pub const TRIAGE: &str = r#"# triage: a liaise code (or a log's text, as @path) → the declaration and every site raising it,
# what each site reaches, the exact tests that prove a fix, and the marks on the items there.
kind = "triage"
summary = "raised here; touches these; prove a fix with these tests; the rule's owner is this item"
# counted at open and close (the workspace), so the closing receipt says how many lines the job touched
count = true

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

crate::reading::spec::vocabulary! {
    /// How a job may change code.
    JobMode, "job mode" {
        Bounded = "bounded" : "strict: the agent writes no code; it submits change specs, custodian generates and places the code through code_passenger and simple_impl, and every line the job touches must be covered by a transform receipt of the job",
        Unbounded = "unbounded" : "help: readings, plans, impact and cost, the decisions list and reuse by digest are there; the agent writes code itself and nothing is refused; the closing receipt records what ran and the lines touched",
    }
}

/// The ways a job declares what done looks like, each `<form>:<what>`.
pub const VERIFY_FORMS: &[&str] = &["test", "reading", "check"];

/// A verify line checked: `test:<name>`, `reading:<spec>` (a reading that must change), `check:<which>`.
pub fn check_verify(line: &str) -> Result<()> {
    match line.split_once(':') {
        Some((form, what)) if VERIFY_FORMS.contains(&form) && !what.trim().is_empty() => Ok(()),
        _ => Err(ReadingRefusal::SpecInvalid.refuse(format!("`{line}` is not a verification: test:<name> (or test:<module>::* for every test of a module), reading:<spec>, check:<which>"))),
    }
}

/// A name as one folder or file name: lowercase letters, digits and `-`.
pub fn slug_of(s: &str) -> String {
    slug(s)
}

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
    /// The earlier step whose receipt this step reads: the step waits for it,
    /// and each receipt file it left (relative to the root) is this step's
    /// target, or fills `{receipt}` in this step's `target`. A step that left
    /// no receipt gives this one nothing, and this one is skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reads: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub kind: String,
    #[serde(default)]
    pub summary: String,
    /// Take a count of the job's routes at open, after each step that changes code, and at close.
    #[serde(default)]
    pub count: bool,
    /// The route counted (`{target}` filled); the workspace (`*`) when unset.
    #[serde(default)]
    pub count_route: Option<String>,
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
    /// Count the job's routes at open, after each step that changes code, and at close.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<bool>,
    /// The routes counted; the template's `count_route`, else the workspace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<String>,
    /// The task this run writes its history records into (`.custodian/data/history/<task_id>/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// The revision of it this run is (`base`, `fix1`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// `bounded` or `unbounded`; the workspace's `job_mode` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<JobMode>,
    /// What done looks like, declared up front: `test:<name>`, `reading:<spec>`, `check:<which>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verify: Vec<String>,
    /// At open: the job's target resolved to a route or an item through the index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_known: Option<bool>,
    /// At open: the job declared its verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_known: Option<bool>,
    /// The commit the job opened at (`git rev-parse HEAD` at `job start`): the base a failing test is judged
    /// against, and where `test --affected` measures the job's change from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    /// Run the tests the target reaches at open, and keep their outcomes as the job's baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<bool>,
    /// Files the job may touch that no operation writes (a test's expected output, a fixture): globs from the root.
    /// A bounded job's other touched files need a transform.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_file: Vec<String>,
    /// The job's budget, `hours=<n>,battery=<percent>` ([`crate::step_time::Budget`]): a step whose measured time would
    /// take the job past its hours is refused before it starts; under the battery floor no cargo line starts. The
    /// workspace's `budget` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<String>,
}

/// A count a job took, the lines it touched between two counts, the counts
/// file and the job's own data folder: change_history's.
pub use change_history::{Taken, Touched, COUNTS_FILE, JOB_DATA};

/// A job folder's readings: `readings/<name>/<stamp>.{md,json}` in the job's
/// folder, where a reading run in the job is kept.
pub const READINGS_DIR: &str = "readings";

/// The specs of a folder-of-specs job: the files named as specs
/// (`v1.<name>.1o.reading`, `<name>.toml`), never the job file and never
/// the job's own data (which sits under `data/`).
pub fn job_specs(folder: &Path) -> Vec<PathBuf> {
    use crate::reading::spec::SpecForm;
    let mut specs: Vec<PathBuf> = std::fs::read_dir(folder)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file() && matches!(SpecForm::of(p), Some(SpecForm::Oneo | SpecForm::Toml)) && !is_job_file(p)).collect())
        .unwrap_or_default();
    specs.sort();
    specs
}

use change_history::{slug, taken};

/// The routes a job counts, when it counts: the job file's, else the template's, else the workspace.
fn count_routes(jf: &JobFile, t: Option<&Template>) -> Vec<String> {
    // a bounded job counts the whole workspace at open and close: every line it touches must be covered
    if jf.mode == Some(JobMode::Bounded) {
        return vec!["*".to_string()];
    }
    let on = jf.count.unwrap_or_else(|| t.is_some_and(|t| t.count));
    if !on {
        return Vec::new();
    }
    if !jf.routes.is_empty() {
        return jf.routes.clone();
    }
    let target = jf.target.clone().unwrap_or_default();
    vec![t.and_then(|t| t.count_route.clone()).map(|r| r.replace("{target}", &target)).unwrap_or_else(|| "*".to_string())]
}

/// A count of each route, as readings of the job, kept in the folder's counts file.
#[allow(clippy::too_many_arguments)]
fn take_counts(ctx: &Ctx, folder: &Path, in_job: &InJob, label: &str, routes: &[String], said: &mut Vec<String>, runs: &mut Vec<Ran>, by: Runner<'_>) -> Result<()> {
    let mut all = taken(folder);
    for route in routes {
        let name = format!("count-{}-{}", slug(label), if route == "*" { "workspace".to_string() } else { slug(route) });
        let spec = ReadingSpec::from_toml(&format!("name = {}\nquestion = {}\nkind = \"count\"\ntarget = {}\n", toml_str(&name), toml_str(&format!("how much is in {route} at {label}")), toml_str(route)))?;
        let p = folder.join("steps").join(crate::oneo::Doc::Reading.file_name(&name));
        write_whole(&p, spec.to_1o()?.as_bytes())?;
        let ran = run_reading_by(ctx, &p, true, Some(in_job), by)?;
        said.extend(ran.said.iter().cloned());
        let json = ran.receipt.as_ref().and_then(|r| r.outputs.iter().find(|o| o.ends_with(".json")).cloned()).ok_or_else(|| ReadingRefusal::ReadingUnreadable.refuse(format!("{name}: the count wrote no reading")))?;
        // kept relative to the job folder, which moves to done/ at close
        let abs = ctx.root.join(&json);
        let reading = abs.strip_prefix(folder).map(|p| p.to_string_lossy().into_owned()).unwrap_or(json);
        all.push(Taken { label: label.to_string(), route: route.clone(), reading });
        runs.push(ran);
    }
    write_whole(&folder.join(COUNTS_FILE), (serde_json::to_string_pretty(&all)? + "\n").as_bytes())
}

/// Lines touched from a job's counts: per route, each count against the one
/// before it (a step's own delta), and `job` from the first to the last. The
/// count diffs are written beside them under `counts/` (change_history's).
pub use change_history::lines_touched_of;

/// What a set of jobs (ids or names) changed together, from their closing
/// receipts' `job` lines touched.
pub fn lines_within(store: &Store, jobs: &[&str]) -> Result<crate::reading::count::Within> {
    let journal = store.journal()?;
    let mut folders = Vec::new();
    for j in jobs {
        let row = journal.job(j)?.ok_or_else(|| ReadingRefusal::NoJob.refuse(*j))?;
        folders.push(store.root.join(&row.folder));
    }
    Ok(change_history::lines_within(&folders.iter().map(PathBuf::as_path).collect::<Vec<_>>())?)
}

/// The job file's name in 1o: `v1.job.1o.job`.
pub fn job_file_name() -> String {
    crate::oneo::Doc::Job.file_name("job")
}

/// Whether a file in a job folder is its job file (1o, or the older job.toml).
pub fn is_job_file(p: &Path) -> bool {
    p.file_name().is_some_and(|n| n == "job.toml" || n.to_string_lossy() == job_file_name())
}

/// A job folder's job file: `v1.job.1o.job`, else `job.toml`, else empty.
pub fn job_file(dir: &Path) -> JobFile {
    if let Ok(t) = std::fs::read_to_string(dir.join(job_file_name())) {
        return crate::oneo::parse(crate::oneo::Doc::Job, &job_file_name(), &t).unwrap_or_default();
    }
    std::fs::read_to_string(dir.join("job.toml")).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
}

/// Writes a job folder's job file in 1o (the older job.toml beside it is left).
fn write_job_file(dir: &Path, jf: &JobFile) -> Result<()> {
    write_whole(&dir.join(job_file_name()), crate::oneo::render(crate::oneo::Doc::Job, jf)?.as_bytes())
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

/// A job's receipts in order: those carrying its id, and those a line named it by its name (`--job NAME`, as custodian
/// 0.3.11 and before kept it) after it opened, while it is the newest job of that name. A batch's member receipts and
/// its test runs are the job's either way, so its close credits them as it credits a single transform's.
pub fn runs_of(journal: &crate::store::Journal, id: &str, name: Option<&str>) -> Result<Vec<Receipt>> {
    let mut runs = journal.runs_of_job(id)?;
    if let Some(n) = name.filter(|n| *n != id && journal.job(n).ok().flatten().is_some_and(|row| row.id == id)) {
        let opened = runs.first().map_or(i64::MAX, |r| r.seq);
        runs.extend(journal.runs_of_job(n)?.into_iter().filter(|r| r.seq > opened));
        runs.sort_by_key(|r| r.seq);
    }
    Ok(runs)
}

/// A done job whose close was not accepted, moved back to running so `job close` closes it again over the same
/// receipts (the handoff's line for a refused close): `None` when no done job of that name was refused.
pub fn reopen_rejected(store: &Store, which: &str) -> Option<Result<PathBuf>> {
    let done = find(store, JobState::Done, Some(which)).ok()?;
    match crate::decisions::job_running(store) {
        Ok(d) if !d.go => return Some(Err(ReadingRefusal::JobRunning.refuse(d.reason))),
        Err(e) => return Some(Err(e)),
        Ok(_) => {}
    }
    let closing: Closing = serde_json::from_str(&std::fs::read_to_string(done.join("closing.json")).ok()?).ok()?;
    if closing.result == "accepted" {
        return None;
    }
    let folder_name = done.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Some((|| {
        let running = store.job_dir(JobState::Running, &folder_name)?;
        move_job(store, &done, &running)?;
        let rel = running.strip_prefix(&store.root).unwrap_or(&running).to_string_lossy().replace('\\', "/");
        store.journal()?.put_job(&JobRow { id: closing.id.clone(), name: closing.name.clone(), kind: closing.kind.clone(), state: "running".into(), folder: rel, opened_utc: closing.opened_utc.clone(), closed_utc: None })?;
        Ok(running)
    })())
}

/// Moves a job folder, and rewrites every receipt whose outputs sit in it
/// (the journal row and `receipts/<seq>.json`) and the folder's closing
/// receipt to the new place: one rule, so a reading kept in a job resolves
/// wherever the job is.
pub fn move_job(store: &Store, from: &Path, to: &Path) -> Result<Vec<i64>> {
    mv(from, to)?;
    let rel = |p: &Path| p.strip_prefix(&store.root).unwrap_or(p).to_string_lossy().replace('\\', "/");
    let (old, new) = (format!("{}/", rel(from)), format!("{}/", rel(to)));
    let journal = store.journal()?;
    let mut moved = Vec::new();
    for r in journal.rehome(&old, &new)? {
        let file = store.path(&crate::store::named::RECEIPT, &[("seq", &r.seq.to_string())])?;
        write_whole(&file, (serde_json::to_string_pretty(&r)? + "\n").as_bytes())?;
        moved.push(r.seq);
    }
    for name in ["closing.json", "closing.md"] {
        let p = to.join(name);
        if let Ok(text) = std::fs::read_to_string(&p) {
            if text.contains(&old) {
                write_whole(&p, text.replace(&old, &new).as_bytes())?;
            }
        }
    }
    Ok(moved)
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

fn template_in(store: &Store, kind: &str) -> Result<Template> {
    let p = store.path(&crate::store::named::JOB_TEMPLATE, &[("kind", kind)])?;
    let text = std::fs::read_to_string(&p).or_else(|_| SHIPPED_TEMPLATES.iter().find(|(k, _)| *k == kind).map(|(_, t)| t.to_string()).ok_or(())).map_err(|_| ReadingRefusal::NoJob.refuse(format!("no job kind `{kind}` ({})", p.display())))?;
    toml::from_str(&text).map_err(|e| ReadingRefusal::SpecInvalid.refuse(format!("{}: {e}", p.display())))
}

/// The job template of `kind` as the project at or above `root` has it
/// (`config/jobs/<kind>.toml`, where `agent init` writes the shipped ones for
/// the user to edit), else the shipped one; the one loader every job runner
/// and every consumer reads templates through.
pub fn template(root: &Path, kind: &str) -> std::result::Result<Template, crate::api::Refusal> {
    match Store::find(root) {
        Some(store) => Ok(template_in(&store, kind)?),
        None => {
            let text = SHIPPED_TEMPLATES.iter().find(|(k, _)| *k == kind).map(|(_, t)| *t).ok_or_else(|| ReadingRefusal::NoJob.refusal(format!("no job kind `{kind}` (no .custodian at {}; shipped: {})", root.display(), shipped_kinds().join(", "))))?;
            toml::from_str(text).map_err(|e| ReadingRefusal::SpecInvalid.refusal(format!("the shipped {kind} template: {e}")))
        }
    }
}

/// Every job template the project at or above `root` knows, by kind: the
/// shipped ones (as the project has them, edited or not) and any other
/// `config/jobs/<kind>.toml` it holds.
pub fn templates(root: &Path) -> std::result::Result<Vec<Template>, crate::api::Refusal> {
    let mut kinds: BTreeSet<String> = shipped_kinds().into_iter().map(str::to_string).collect();
    if let Some(store) = Store::find(root) {
        let dir = store.path(&crate::store::named::JOB_TEMPLATE, &[("kind", "x")])?.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "toml") {
                    if let Some(k) = p.file_stem().and_then(|k| k.to_str()) {
                        kinds.insert(k.to_string());
                    }
                }
            }
        }
    }
    kinds.iter().map(|k| template(root, k)).collect()
}

fn shipped_kinds() -> Vec<&'static str> {
    SHIPPED_TEMPLATES.iter().map(|(k, _)| *k).collect()
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
    // the step's spec, written in 1o
    let spec = ReadingSpec::from_toml(&t)?;
    let p = folder.join("steps").join(crate::oneo::Doc::Reading.file_name(&name));
    write_whole(&p, spec.to_1o()?.as_bytes())?;
    Ok(p)
}

/// The step executor `job start` uses: the declared reading line, through the
/// executor, with its receipt and its task-history solution record, as before.
struct ExecSteps<'a> {
    ctx: &'a Ctx,
    folder: &'a Path,
    in_job: &'a InJob,
    task: &'a crate::task_records::TaskRef,
    again: bool,
    question: String,
    runs: Vec<Ran>,
    /// The routes the job counts (a count is taken after each step that changes code).
    routes: Vec<String>,
    steps_changed: usize,
}

impl runner::RunStep for ExecSteps<'_> {
    fn run(&mut self, ws: Option<&crate::api::Workspace>, step: &Step, n: usize, target: &str, items: &[String], said: &mut Vec<String>) -> Result<Option<ReadingReport>> {
        let by = match ws {
            Some(ws) => Runner::InProcess(ws),
            None => Runner::Line,
        };
        if step.kind == "preflight" {
            let spec = self.ctx.root.join(target);
            let ran = crate::preflight::run_preflight(self.ctx, &spec, Some(self.in_job), by)?;
            said.extend(ran.said.iter().cloned());
            crate::task_records::solution(self.task, &format!("preflight {} ({}): {}", ran.preflight.spec_name, ran.preflight.operation, if ran.preflight.authorized { "authorized" } else { "not authorized" }))?;
            self.runs.push(Ran { receipt: ran.receipt, receipt_file: ran.receipt_file, summary: None, reused: None, printed: None, said: ran.said });
            return Ok(None);
        }
        if CHANGES_CODE.contains(&step.kind.as_str()) {
            let ran = run_change_step(self.ctx, self.folder, self.in_job, step, target)?;
            said.extend(ran.said.iter().cloned());
            crate::task_records::solution(self.task, &format!("{} {}: {}", step.kind, target, ran.receipt.as_ref().map(|r| format!("run #{} ({})", r.seq, r.reason)).unwrap_or_default()))?;
            self.runs.push(ran);
            self.steps_changed += 1;
            if !self.routes.is_empty() {
                let mut runs = Vec::new();
                take_counts(self.ctx, self.folder, self.in_job, &format!("step {} {}", self.steps_changed, step.name), &self.routes, said, &mut runs, by)?;
            }
            return Ok(None);
        }
        let question = self.question.replace("a step", &format!("step {}", step.name));
        let spec = step_spec(self.folder, n, step, target, items, &question)?;
        let ran = run_reading_by(self.ctx, &spec, self.again, Some(self.in_job), by)?;
        said.extend(ran.said.iter().cloned());
        crate::task_records::solution(self.task, &solution_text(self.ctx, &spec, &ran))?;
        let report = match ran.receipt.as_ref().and_then(|r| r.outputs.iter().find(|o| o.ends_with(".json"))) {
            Some(j) => Some(load(&self.ctx.root.join(j))?),
            None => None,
        };
        self.runs.push(ran);
        Ok(report)
    }

    fn receipt(&self) -> Option<String> {
        self.runs.last().and_then(|r| r.receipt_file.as_ref()).map(|f| self.ctx.rel(f))
    }
}

/// A step's solution record: the spec run, its run number, its reading.
fn solution_text(ctx: &Ctx, spec: &Path, ran: &Ran) -> String {
    let s = ReadingSpec::from_path(spec).ok();
    let what = s.map(|s| format!("{} ({} {})", s.name, s.kind.as_str(), s.target)).unwrap_or_else(|| ctx.rel(spec));
    match &ran.receipt {
        // the reading as the job folder holds it (the folder moves; the record does not)
        Some(r) => format!("{what}: run #{}{}, reading {}", r.seq, r.reused_seq.map(|p| format!(" (reuse of #{p})")).unwrap_or_default(), r.outputs.first().map(|o| o.split_once("/readings/").map(|(_, rest)| format!("readings/{rest}")).unwrap_or_else(|| o.clone())).unwrap_or_default()),
        None => what,
    }
}

/// `job start`: the queued folder moved to running and its readings run.
pub fn start(ctx: &Ctx, which: Option<&str>, again: bool) -> Result<Started> {
    start_with(ctx, which, again, None)
}

/// `job start`, its readings answered in this process over `ws` when it is
/// given (the CLI with no resident), else by the declared lines. The receipts
/// and records are the same either way; each receipt names its runner.
pub fn start_with(ctx: &Ctx, which: Option<&str>, again: bool, ws: Option<&crate::api::Workspace>) -> Result<Started> {
    let by = match ws {
        Some(ws) => Runner::InProcess(ws),
        None => Runner::Line,
    };
    let store = ctx.store()?;
    let running = crate::decisions::job_running(store)?;
    if !running.go {
        return Err(ReadingRefusal::JobRunning.refuse(running.reason));
    }
    let queued = find(store, JobState::Queued, which)?;
    let folder_name = queued.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut jf = job_file(&queued);
    // a verification that can never hold is refused before the job opens
    for v in &jf.verify {
        check_verify(v)?;
    }
    check_tests_match(ctx, &jf.verify, ws)?;
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
    write_job_file(&running, &jf)?;
    let journal = store.journal()?;
    journal.put_job(&JobRow { id: id.clone(), name: name.clone(), kind: kind.clone(), state: "running".into(), folder: ctx.rel(&running), opened_utc: Some(opened.clone()), closed_utc: None })?;
    let mut said = vec![running_decision_said(), format!("job {id} ({name}, {kind}) started: {} → {}", ctx.rel(&queued), ctx.rel(&running))];
    let in_job = InJob { id: id.clone(), readings: running.join(READINGS_DIR) };
    let mut runs = Vec::new();
    // an exercise has no template: its cases are planned from the index (`crate::exercise`)
    let exercise = kind == crate::exercise::KIND;
    let t = if kind == "specs" || exercise { None } else { Some(template_in(store, &kind)?) };
    // the task history: a new revision of the job's task, its task and intent records
    let intent = match &t {
        Some(t) => format!("{} ({} {})", t.summary, t.kind, jf.target.clone().unwrap_or_default()),
        None if exercise => format!("exercise custodian's change path on copies of the project: {}", crate::exercise::options_of(&running)?.line()),
        None => format!("read the job's specs: {name}"),
    };
    let task = crate::task_records::open(store, &name, &kind, &id, &intent)?;
    jf.task_id = Some(task.task_id.clone());
    jf.revision = Some(task.revision.clone());
    // the mode, and the two facts the job starts from: is its target known, is its outcome known
    let mode = jf.mode.unwrap_or_else(|| store.settings().ok().and_then(|s| s.job_mode).unwrap_or(JobMode::Unbounded));
    if mode == JobMode::Bounded && !crate::exec::on_path("code-passenger") {
        return Err(ReadingRefusal::NoWriter.refuse(format!("job {id} is bounded: install code-passenger (cargo install code_passenger)")));
    }
    for v in &jf.verify {
        check_verify(v)?;
    }
    jf.mode = Some(mode);
    let target_known = jf.target.as_deref().is_some_and(|t| target_resolves(ctx, t, ws));
    jf.target_known = Some(target_known);
    jf.outcome_known = Some(!jf.verify.is_empty());
    // the base: the commit the job opens at
    jf.base_commit = crate::baseline::head_commit(&ctx.root);
    write_job_file(&running, &jf)?;
    said.push(format!("task history: {}/{} (task, intent)", task.task_id, task.revision));
    said.push(format!("mode: {} — {}", mode.as_str(), mode.doc()));
    said.push(match (&jf.target, target_known) {
        (Some(t), true) => format!("target known: {t} resolves through the index"),
        (Some(t), false) => format!("target unknown: {t} does not resolve; find it first (custodian plan, or an impact reading)"),
        (None, _) => "target unknown: the job names none; find it first (custodian plan <intent>)".to_string(),
    });
    if jf.verify.is_empty() {
        let q = crate::questions::done_question(&name);
        let asked = crate::questions::ask(store, &task, &q)?;
        said.push(match asked {
            crate::questions::Asked::Answered(d) => format!("outcome known from an earlier answer: {} → {}", d.question, d.answer),
            _ => format!("outcome unknown: no verification declared; decision {} asked: {}", q.id, q.question),
        });
    } else {
        said.push(format!("outcome known: done is {}", jf.verify.join(", ")));
    }
    let routes = count_routes(&jf, t.as_ref());
    take_counts(ctx, &running, &in_job, "open", &routes, &mut said, &mut runs, by)?;
    said.push(match &jf.base_commit {
        Some(c) => format!("base: {c} (a test failing the same way there is not the job's)"),
        None => "base: none (not a git repository): every failing test is the job's".into(),
    });
    if let Some(base) = &jf.base_commit {
        // what already differs from the base at open (a file not committed yet): the job did not touch it
        let open: BTreeMap<String, String> = changed_since(ctx, base).into_iter().map(|p| { let d = digest_now(ctx, &p); (p, d) }).collect();
        write_whole(&running.join(JOB_DATA).join(OPEN_FILES), (serde_json::to_string_pretty(&open)? + "\n").as_bytes())?;
    }
    if jf.baseline == Some(true) {
        let tests = baseline_tests(ctx, jf.target.as_deref().unwrap_or(crate::change::WORKSPACE), ws)?;
        let b = crate::baseline::record_at_open(ctx, &running, &in_job, jf.base_commit.clone(), &tests)?;
        let failed = b.outcomes.iter().filter(|o| o.outcome == "FAILED").count();
        said.push(format!("baseline: {} tests the target reaches run at open, {failed} failing there ({})", b.outcomes.len(), ctx.rel(&running.join(JOB_DATA).join(crate::baseline::BASELINE_FILE))));
    }
    if exercise {
        // every case on a copy in the job's data folder, its findings diagnosis records of the job's task
        let (ran, case_said) = crate::exercise::run_in_job(ctx, &running, &in_job, &task)?;
        said.extend(case_said);
        runs.extend(ran);
    } else if kind == "specs" {
        for s in job_specs(&running) {
            let ran = run_reading_by(ctx, &s, again, Some(&in_job), by)?;
            said.extend(ran.said.iter().cloned());
            crate::task_records::solution(&task, &solution_text(ctx, &s, &ran))?;
            runs.push(ran);
        }
    } else {
        let t = t.expect("a template job has its template");
        let target = jf.target.clone().unwrap_or_default();
        // the steps on scope: independent ones together, dependent ones after what they read; a step that changes
        // code runs code_passenger's line for it (after its preflight gate), and is counted after when the job counts
        let mut exec = ExecSteps { ctx, folder: &running, in_job: &in_job, task: &task, again, question: format!("{} — a step of the {} job for {target}", t.summary, t.kind), runs: Vec::new(), routes: routes.clone(), steps_changed: 0 };
        let (_, events, step_said) = runner::run_steps(&t.step, &target, ws, &mut exec)?;
        said.extend(step_said);
        said.push(format!("steps: {}", events.join(" · ")));
        runs.extend(exec.runs);
    }
    Ok(Started { id, folder: running, runs, said })
}

/// The tests a job's baseline runs at open: those whose calls reach the target (an item; a crate's items for
/// `crate:<name>`; every item for `*`), as (package, target word, test).
fn baseline_tests(ctx: &Ctx, target: &str, ws: Option<&crate::api::Workspace>) -> Result<Vec<(String, String, String)>> {
    // the tests are read: a workspace held open without them is not used
    let owned;
    let f = match ws.filter(|w| w.scope.tests) {
        Some(w) => w.facts(),
        None => {
            owned = crate::reading::facts::Facts::build(&ctx.root, &crate::reading::spec::Scope { tests: true, ..Default::default() }, crate::paths::Resolving::Job)?;
            &owned
        }
    };
    let t = target.trim();
    let seeds: BTreeSet<String> = if t == crate::change::WORKSPACE {
        f.k.items.iter().map(|d| d.id.clone()).collect()
    } else if let Some(c) = t.strip_prefix("crate:").filter(|c| !c.contains('/')) {
        f.k.items.iter().filter(|d| d.package == c.replace('-', "_")).map(|d| d.id.clone()).collect()
    } else {
        crate::change::item_named(&f.k, t).map(|d| d.id.clone()).into_iter().collect()
    };
    // the tests reaching it, and every test of a crate it names
    let crates: BTreeSet<String> = if t == crate::change::WORKSPACE { f.k.packages.keys().cloned().collect() } else { t.strip_prefix("crate:").filter(|c| !c.contains('/')).map(|c| BTreeSet::from([c.replace('-', "_")])).unwrap_or_default() };
    let mut cases = crate::reading::kinds::tests_reaching(f, &seeds);
    cases.extend(crate::reading::kinds::tests_of_crates(f, &crates));
    cases.sort();
    cases.dedup();
    Ok(cases.into_iter().map(|c| (c.package, c.target, c.filter)).collect())
}

/// Whether a job's target resolves: a declared route, or an item of the scan.
fn target_resolves(ctx: &Ctx, target: &str, ws: Option<&crate::api::Workspace>) -> bool {
    // `*` is the workspace: there whenever the scan is
    if target.trim() == crate::change::WORKSPACE {
        return ctx.root.join("Cargo.toml").is_file();
    }
    let table = match ws {
        Some(w) => Some(w.paths().clone()),
        None => crate::paths::PathsTable::load(&ctx.root, ctx.store.as_ref()).ok(),
    };
    if crate::paths::names_a_place(target) {
        return table.is_some_and(|t| t.resolve(target, crate::paths::Resolving::Job).is_ok());
    }
    if table.as_ref().is_some_and(|t| t.crates.contains_key(&target.replace('-', "_"))) {
        return true;
    }
    match ws {
        Some(w) => w.klepto().items.iter().any(|d| crate::path_names_item(&d.id, target)) || w.klepto().graph.resolve_id(target).is_some(),
        None => crate::reading::facts::Facts::build(&ctx.root, &Default::default(), crate::paths::Resolving::Job).is_ok_and(|f| f.k.items.iter().any(|d| crate::path_names_item(&d.id, target))),
    }
}

/// A code-changing step: `transform` (a change spec, after its preflight gate), `apply` (code_passenger's features
/// apply), `annotate` (its annotate): code_passenger's declared line, run through the executor, its receipt kept
/// under the job; a transform's record kept in the job's folder.
fn run_change_step(ctx: &Ctx, folder: &Path, in_job: &InJob, step: &Step, target: &str) -> Result<Ran> {
    let store = ctx.store()?;
    let mut said = Vec::new();
    let (words, name, spec_meta): (Vec<std::ffi::OsString>, String, Option<(String, String)>) = match step.kind.as_str() {
        "transform" => {
            let spec_path = ctx.root.join(target);
            let spec = crate::change::ChangeSpec::from_path(&spec_path)?;
            let code = crate::code_digest(&ctx.root, &crate::reading::spec::Scope { tests: true, ..Default::default() }.scan_options()).map_err(|e| ReadingRefusal::Scan.refuse(e))?;
            let (d, prev) = crate::preflight::gate(store, &spec, &code)?;
            said.push(d.said());
            if !d.go {
                return Err(ReadingRefusal::NoPreflight.refuse(format!("{}: {}", spec.name, d.reason)));
            }
            let out = folder.join(JOB_DATA).join("transforms").join(format!("{}.json", slug(&spec.name)));
            let mut w: Vec<std::ffi::OsString> = vec!["transform".into(), "--spec".into(), spec_path.into_os_string(), "--write".into(), "--out".into(), out.into_os_string()];
            if let Some(p) = prev {
                w.push("--preflight".into());
                w.push(p.seq.to_string().into());
            }
            (w, spec.name.clone(), Some((spec.digest(), code)))
        }
        "apply" => (vec!["features".into(), "apply".into(), "--write".into()], format!("features apply {target}"), None),
        "annotate" => (vec!["annotate".into(), "--write".into()], format!("annotate {target}"), None),
        other => return Err(ReadingRefusal::NoWriterLine.refuse(format!("step {} ({other})", step.name))),
    };
    let line = crate::exec::CodePassenger::at_root(ctx.root.as_os_str()).words(words);
    let mut l = crate::exec::line(format!("code-passenger:{}", step.kind), &line)?;
    l.env.push((CODE_PASSENGER_JOB_ENV.to_string(), in_job.id.clone()));
    let plan = crate::exec::Plan::new(format!("step:{}", step.name)).then(l);
    let walked = plan.walk(crate::exec::Watch::Quiet, |_, _| {})?;
    let w = walked.first().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse(&step.name))?;
    let outputs: Vec<String> = w.stdout.lines().filter_map(|l| l.strip_prefix("record: ")).map(|p| ctx.rel(Path::new(p.trim()))).collect();
    let (spec_hash, code) = spec_meta.map(|(a, b)| (Some(a), Some(b))).unwrap_or((None, None));
    let receipt = crate::receipts::receipt_of(&plan, w, &step.kind, &name, Some(target.to_string()), spec_hash, code, None, outputs, Some(in_job));
    let (r, f) = keep(store, receipt)?;
    said.push(format!("{}: {} → {:?} ({}) #{}", step.kind, r.line, r.exit_code, r.reason, r.seq));
    if !r.accepted {
        let tail: String = w.stderr.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(ReadingRefusal::LineRefused.refuse(format!("{} → {:?} ({}): {tail}", r.line, r.exit_code, r.reason)));
    }
    Ok(Ran { receipt: Some(r), receipt_file: Some(f), summary: None, reused: None, printed: None, said })
}

/// The environment variable code_passenger reads to leave keeping a step's receipt to the job runner.
pub const CODE_PASSENGER_JOB_ENV: &str = "CODE_PASSENGER_JOB";

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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Lines touched, per step that changed code and for the job, from its counts.
    #[serde(default)]
    pub lines_touched: Vec<Touched>,
    /// `bounded` or `unbounded`.
    #[serde(default)]
    pub mode: Option<JobMode>,
    /// When the target became known: `open`, `#<receipt>`, or never (`None`).
    #[serde(default)]
    pub target_known_at: Option<String>,
    /// When the outcome became known (the verification declared): `open`, `#<receipt>`, or never.
    #[serde(default)]
    pub outcome_known_at: Option<String>,
    /// The job's target_known as `job start` set it at open (the job file's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_known: Option<bool>,
    /// The job's outcome_known as `job start` set it at open (the job file's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_known: Option<bool>,
    /// Each declared verification and whether the job's own runs satisfy it: `satisfied`, `failed`, `not run`, or, for a
    /// reading the job made that is the same as the one before it opened, `unchanged`.
    #[serde(default)]
    pub verify: Vec<(String, String)>,
    /// How far past its budget's hours the job closed (`over the budget by 12.0 min (...)`): information, never a
    /// refusal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub over_budget: Option<String>,
    /// For each verification satisfied or failed, the receipt that decided it (`test:t: receipt #4427`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verified_by: Vec<String>,
    /// The transform receipts of the job.
    #[serde(default)]
    pub transforms: Vec<i64>,
    /// Bounded: the files (and lines) the job touched that no transform of it covers.
    #[serde(default)]
    pub uncovered: Vec<crate::transform::Uncovered>,
    /// The task history's result: `accepted`, `pending`, `failed_test`, `rejected`.
    #[serde(default)]
    pub result: String,
    /// Why the result is not `accepted`.
    #[serde(default)]
    pub result_reason: Option<String>,
    /// The decisions the job's task left open.
    #[serde(default)]
    pub decisions_open: Vec<crate::questions::Question>,
    /// Set when the close is refused (a bounded job with uncovered lines): the code and what was refused.
    #[serde(default)]
    pub refusal: Option<crate::api::Refusal>,
    /// The commit the job opened at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    /// The tests that failed in the job and fail the same way at its base, each with its first failing line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pre_existing: Vec<crate::baseline::PreExisting>,
    /// The job's own failing tests, each with why it is the job's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failing: Vec<String>,
    /// Every file the job touched since its base (git's), and what carries it: `transform #<seq>`, `allow_file
    /// <glob>`, `cargo` (the lock file cargo writes from the manifests), or `hand`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touched_files: Vec<(String, String)>,
    /// The time the job's steps took, per step kind, from its receipts ([`crate::step_time::times`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub step_times: Vec<crate::step_time::StepTime>,
    /// The step kinds the job paid one member at a time where a batch form exists ([`crate::step_time::per_member`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_member: Vec<String>,
}

pub const CLOSING_SCHEMA: &str = "custodian.job_closing.v1";

fn previous(root: &Path, all: &[Receipt], folders: &[(String, PathBuf)], name: &str, before: i64, job: &str) -> Option<ReadingReport> {
    let prev = crate::receipts::readings_of(root, all, name, folders).into_iter().rfind(|(r, _)| r.seq < before && r.job.as_deref() != Some(job))?;
    load(&root.join(prev.1.record)).ok()
}

/// `job close`: the closing receipt written into the folder, the folder moved to done.
pub fn close(ctx: &Ctx, which: &str) -> Result<(Closing, PathBuf, Vec<String>)> {
    close_with(ctx, which, None)
}

/// `job close`, its closing count answered in this process over `ws` when given.
pub fn close_with(ctx: &Ctx, which: &str, ws: Option<&crate::api::Workspace>) -> Result<(Closing, PathBuf, Vec<String>)> {
    let by = match ws {
        Some(ws) => Runner::InProcess(ws),
        None => Runner::Line,
    };
    let store = ctx.store()?;
    let running = match find(store, JobState::Running, Some(which)) {
        Ok(r) => r,
        Err(e) => reopen_rejected(store, which).ok_or(e)??,
    };
    let jf = job_file(&running);
    let id = jf.id.clone().ok_or_else(|| ReadingRefusal::JobState.refuse(format!("{} has no id: was it started?", running.display())))?;
    let journal = store.journal()?;
    let row = journal.job(&id)?;
    // the closing count, when the job counts, then the lines touched per step and for the job
    let jkind = jf.kind.clone().unwrap_or_else(|| "specs".into());
    let t = if jkind == "specs" { None } else { template_in(store, &jkind).ok() };
    let routes = count_routes(&jf, t.as_ref());
    let mut count_said = Vec::new();
    if !routes.is_empty() {
        let in_job = InJob { id: id.clone(), readings: running.join(READINGS_DIR) };
        let mut count_runs = Vec::new();
        take_counts(ctx, &running, &in_job, "close", &routes, &mut count_said, &mut count_runs, by)?;
    }
    let mut lines_touched = lines_touched_of(&running, &taken(&running))?;
    if let Some(base) = &jf.base_commit {
        exact_lines(ctx, &running, base, &mut lines_touched);
    }
    let runs = runs_of(&journal, &id, jf.name.as_deref())?;
    // every receipt and every job folder, for the readings of a spec (`receipts::readings_of`)
    let all_runs = journal.runs()?;
    let folders = crate::store::job_folders(store.jobs_dir(JobState::Done)?.parent().unwrap_or(&running));
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
                    "test-line" => findings.push(format!("{PROVE_A_FIX}{}", row.label)),
                    _ => {}
                }
            }
        }
        let changed = report.as_ref().and_then(|rep| previous(&ctx.root, &all_runs, &folders, &r.name, r.seq, &id).map(|prev| diff::compare(&prev, rep)));
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
    // the mode's rule, the verifications, the two facts' times
    let mode = jf.mode.unwrap_or(JobMode::Unbounded);
    let transforms: Vec<(i64, crate::transform::TransformRecord)> = runs
        .iter()
        .filter(|r| r.command == "transform" && r.accepted)
        .filter_map(|r| r.outputs.iter().find(|o| o.ends_with(".json")).and_then(|o| std::fs::read_to_string(ctx.root.join(o)).ok()).and_then(|t| serde_json::from_str(&t).ok()).map(|t| (r.seq, t)))
        .collect();
    let mut uncovered = if mode == JobMode::Bounded { uncovered_lines(ctx, &running, &transforms.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>())? } else { Vec::new() };
    // every file the job touched, code or not: each carried by a transform, allowed, cargo's, or by hand
    let touched_files = match &jf.base_commit {
        Some(base) => files_touched(ctx, &running, base, &jf.allow_file, &transforms)?,
        None => Vec::new(),
    };
    let mut not_allowed = Vec::new();
    if mode == JobMode::Bounded {
        // a file the job allows is carried, whatever its lines; any other file but code is one the job does not
        // allow (CUST0041), never an uncovered line
        uncovered.retain(|u| !touched_files.iter().any(|(p, how)| p == &u.path && how.starts_with("allow_file")));
        for u in uncovered.iter().filter(|u| !is_code_file(&u.path)) {
            not_allowed.push(u.path.clone());
        }
        for (path, how) in touched_files.iter().filter(|(_, how)| how.starts_with("hand")) {
            if !uncovered.iter().any(|u| &u.path == path) {
                let now = std::fs::read(ctx.root.join(path)).map(|b| stable_digest::StableDigest::sha256(&b).as_str().to_string()).unwrap_or_else(|_| "none".into());
                uncovered.push(crate::transform::Uncovered { path: path.clone(), from: how.trim_start_matches("hand from ").to_string(), now, lines: Vec::new() });
                // code by hand is an uncovered line; any other file is one the job does not allow
                if !is_code_file(path) && !not_allowed.contains(path) {
                    not_allowed.push(path.clone());
                }
            }
        }
    }
    // a failing test is the job's unless it fails the same way at the job's base
    let test_in_job = InJob { id: id.clone(), readings: running.join(READINGS_DIR) };
    let judged = crate::baseline::judge(ctx, &running, &test_in_job, jf.base_commit.as_deref(), &tests)?;
    let opened = runs.first().map_or(i64::MAX, |r| r.seq);
    let reading = |spec: &str| reading_verified(&ctx.root, &all_runs, &folders, &id, opened, spec);
    let decided: Vec<(String, &'static str, Option<i64>)> = jf.verify.iter().map(|v| {
        let (state, by) = verified_with(v, &runs, &tests, &reading);
        (v.clone(), state, by)
    }).collect();
    let verify: Vec<(String, String)> = decided.iter().map(|(v, st, _)| (v.clone(), st.to_string())).collect();
    let verified_by: Vec<String> = decided.iter().filter_map(|(v, _, by)| by.map(|n| format!("{v}: receipt #{n}"))).collect();
    let known_at = |at_open: Option<bool>, commands: &[&str]| -> Option<String> {
        if at_open == Some(true) {
            return Some("open".into());
        }
        runs.iter().find(|r| commands.contains(&r.command.as_str())).map(|r| format!("#{}", r.seq))
    };
    let target_known_at = known_at(jf.target_known, &["preflight", "plan"]);
    let outcome_known_at = known_at(jf.outcome_known, &["verify"]);
    let failed_any = !judged.own.is_empty() || runs.iter().any(|r| r.command == "check" && !r.accepted);
    let mut result: (String, Option<String>) = result_of_parts(&uncovered, &verify, failed_any);
    // the task history: changes (from the open and close counts), verifications, the result
    let mut history_said = Vec::new();
    if let Some(task) = crate::task_records::of_job(store, jf.task_id.as_deref(), jf.revision.as_deref())? {
        let counts = taken(&running);
        let mut seen_paths = BTreeSet::new();
        let mut routes: Vec<&String> = counts.iter().map(|c| &c.route).collect();
        routes.sort();
        routes.dedup();
        let mut changes = 0;
        for route in routes {
            let of: Vec<&Taken> = counts.iter().filter(|c| &c.route == route).collect();
            if let (Some(first), Some(last)) = (of.first(), of.last()) {
                if first.reading != last.reading {
                    let (a, b) = (load(&running.join(&first.reading))?, load(&running.join(&last.reading))?);
                    if let (Some(a), Some(b)) = (&a.count, &b.count) {
                        for d in crate::task_records::changes(&task, &ctx.root, a, b)? {
                            if seen_paths.insert(d.one("path").unwrap_or_default().to_string()) {
                                changes += 1;
                            }
                        }
                    }
                }
            }
        }
        for t in &tests {
            // cargo's words (`ok`, `FAILED`, `ignored`), as the journal's tests table keeps them, and the older ones
            let passed = match t.outcome.as_str() {
                "ok" | "passed" | "pass" => Some(true),
                "FAILED" | "failed" | "fail" => Some(false),
                _ => None,
            };
            crate::task_records::verification(&task, &format!("cargo test -p {} {} {}", t.package, t.target, t.test), passed, t.millis.map(|m| format!("{m} ms")).as_deref())?;
        }
        for r in runs.iter().filter(|r| r.command == "check") {
            crate::task_records::verification(&task, &format!("custodian check {}", r.name), Some(r.accepted), Some(&r.reason))?;
        }
        let failed = !judged.own.is_empty() || runs.iter().any(|r| r.command == "check" && !r.accepted);
        let (status, why) = result_of(failed, &uncovered, &verify);
        result = (status.to_string(), why.clone());
        let tree = crate::api::IndexKey::of_root(&ctx.root, &Default::default()).ok().map(|k| k.0);
        crate::task_records::close_noted(&task, status, tree.as_deref(), why.as_deref())?;
        history_said.push(format!("task history: {}/{} ({} changes, {} verifications, result)", task.task_id, task.revision, changes, tests.len() + runs.iter().filter(|r| r.command == "check").count()));
    }
    let kind = jf.kind.clone().unwrap_or_else(|| "specs".into());
    let mut guidance = (kind != "specs").then(|| template_in(store, &kind).ok().map(|t| t.summary)).flatten();
    // an exercise: its headline is the guidance, what to fix (most findings first) its findings
    if kind == crate::exercise::KIND {
        if let Some(s) = crate::exercise::summary_of(&running) {
            guidance = Some(format!("{} (exercise report {id})", s.headline()));
            findings.extend(s.to_fix.iter().cloned());
        }
    }
    // each finding once, in the order first met; the lines that prove a fix only when a test the job ran fails
    let mut seen_findings = BTreeSet::new();
    findings.retain(|f| seen_findings.insert(f.clone()) && (!judged.own.is_empty() || !f.starts_with(PROVE_A_FIX)));
    let decisions_open: Vec<crate::questions::Question> = match (&jf.task_id, crate::questions::open(store)) {
        (Some(t), Ok(qs)) => qs.into_iter().filter(|q| q.task.as_deref().is_some_and(|x| x.starts_with(&format!("{t}/")))).collect(),
        _ => Vec::new(),
    };
    let code_uncovered: Vec<&crate::transform::Uncovered> = uncovered.iter().filter(|u| !not_allowed.contains(&u.path)).collect();
    // one refusal per close: code written by hand (CUST0031) names the files no operation writes too
    let refusal = if !code_uncovered.is_empty() {
        let lines = code_uncovered.iter().map(|u| if u.lines.is_empty() { u.path.clone() } else { format!("{}:{}", u.path, u.lines.iter().map(u32::to_string).collect::<Vec<_>>().join(",")) }).collect::<Vec<_>>().join("; ");
        let files = if not_allowed.is_empty() { String::new() } else { format!("; and files no operation writes, not allowed: {} (allow_file)", not_allowed.join("; ")) };
        Some(ReadingRefusal::BoundedUncovered.refusal(format!("{lines}{files}")))
    } else {
        (!not_allowed.is_empty()).then(|| ReadingRefusal::FileNotAllowed.refusal(format!("{} (an operation writes it, or the job allows it: allow_file)", not_allowed.join("; "))))
    };
    let mut said_judged = judged.said.clone();
    said_judged.extend(judged.pre_existing.iter().map(|p| format!("pre-existing: {} {} {} — {} ({})", p.package, p.target, p.test, p.first_line, p.from)));
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
        lines_touched,
        mode: Some(mode),
        target_known_at,
        outcome_known_at,
        target_known: jf.target_known,
        outcome_known: jf.outcome_known,
        verify,
        verified_by,
        over_budget: crate::step_time::budget_of(store, &jf).ok().flatten().and_then(|b| crate::step_time::over_by(&b, crate::step_time::spent(&runs))),
        transforms: transforms.iter().map(|(s, _)| *s).collect(),
        uncovered,
        result: result.0.clone(),
        result_reason: result.1.clone(),
        decisions_open,
        refusal,
        base_commit: jf.base_commit.clone(),
        pre_existing: judged.pre_existing.clone(),
        failing: judged.own.clone(),
        touched_files,
        step_times: crate::step_time::times(&runs),
        per_member: crate::step_time::per_member(&crate::step_time::times(&runs), &runs),
    };
    let json = serde_json::to_string_pretty(&closing)? + "\n";
    write_whole(&running.join("closing.json"), json.as_bytes())?;
    write_whole(&running.join("closing.md"), render_closing(&closing).as_bytes())?;
    let folder_name = running.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let done = store.job_dir(JobState::Done, &folder_name)?;
    move_job(store, &running, &done)?;
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
        // written by the kit itself: no line ran
        runner: crate::receipts::RUNNER_IN_PROCESS.into(),
    };
    let (r, f) = keep(store, r)?;
    let mut said = count_said;
    said.extend(said_judged);
    said.extend(history_said);
    said.extend(closing.lines_touched.iter().map(|t| format!("lines touched: {} ({})", t.line, t.route)));
    said.extend(vec![
        format!("job {id} closed: {} → {}", ctx.rel(&running), ctx.rel(&done)),
        format!("closing receipt: {}", ctx.rel(&done.join("closing.md"))),
        format!("journal: run #{} job-close, receipt {}", r.seq, ctx.rel(&f)),
    ]);
    Ok((closing, done, said))
}

/// The job's result from its evidence: a bounded job with lines no transform covers is `rejected`; a failed test or
/// check is `failed_test`; no verification declared, or one not satisfied, is `pending` (never `accepted` without
/// evidence); else `accepted`.
fn result_of(failed: bool, uncovered: &[crate::transform::Uncovered], verify: &[(String, String)]) -> (&'static str, Option<String>) {
    let (s, w) = result_of_parts(uncovered, verify, failed);
    (match s.as_str() {
        "rejected" => "rejected",
        "failed_test" => "failed_test",
        "pending" => "pending",
        _ => "accepted",
    }, w)
}

fn result_of_parts(uncovered: &[crate::transform::Uncovered], verify: &[(String, String)], failed: bool) -> (String, Option<String>) {
    if !uncovered.is_empty() {
        return ("rejected".into(), Some(format!("bounded: {} files touched by hand, not by a transform of the job", uncovered.len())));
    }
    if failed || verify.iter().any(|(_, s)| s == "failed") {
        return ("failed_test".into(), Some("a test or check the job ran failed, and not the same way at its base".into()));
    }
    if verify.is_empty() {
        return ("pending".into(), Some("no verification declared: what does done look like? (job start --verify, or answer the decision)".into()));
    }
    let not_run: Vec<&str> = verify.iter().filter(|(_, s)| s != "satisfied").map(|(v, _)| v.as_str()).collect();
    if !not_run.is_empty() {
        return ("pending".into(), Some(format!("not verified yet: {}", not_run.join(", "))));
    }
    ("accepted".into(), None)
}

/// A `reading:<spec>` verification: the job changed what it set out to change. Satisfied when the job read the spec
/// and that reading differs from the latest reading of it before the job opened; a spec never read before the job is
/// satisfied by its first reading. Read in the job and the same as before is `unchanged`; not read in it, `not run`.
/// The readings are the receipts' ([`crate::receipts::readings_of`]); `opened` is the job's first receipt.
fn reading_verified(root: &Path, all: &[Receipt], folders: &[(String, PathBuf)], job: &str, opened: i64, spec: &str) -> &'static str {
    let readings = crate::receipts::readings_of(root, all, spec, folders);
    // the job's last reading of the spec; a reuse names the reading it reused
    let Some(last) = all.iter().rfind(|r| r.job.as_deref() == Some(job) && r.name == spec && (r.command == "reading" || r.command == "run") && r.accepted) else { return "not run" };
    let at = last.reused_seq.unwrap_or(last.seq);
    let Some((_, now)) = readings.iter().find(|(r, _)| r.seq == at) else { return "not run" };
    let Some((_, before)) = readings.iter().rfind(|(r, _)| r.seq < opened) else { return "satisfied" };
    match (load(&root.join(&before.record)), load(&root.join(&now.record))) {
        (Ok(a), Ok(b)) if !diff::compare(&a, &b).is_empty() => "satisfied",
        (Ok(_), Ok(_)) => "unchanged",
        _ => "not run",
    }
}

/// Whether the job's own runs satisfy one verification; a `reading:` line is answered by `reading`.
/// A verification's state and the receipt that decided it: a `test:` line by the newest test receipt that ran a test it
/// names (a failure in any of them fails it), a `check:` by its newest check receipt.
fn verified_with(line: &str, runs: &[Receipt], tests: &[crate::store::TestRow], reading: &dyn Fn(&str) -> &'static str) -> (&'static str, Option<i64>) {
    let Some((form, what)) = line.split_once(':') else { return ("not run", None) };
    let what = what.trim();
    match form {
        "test" => {
            let hits: Vec<&crate::store::TestRow> = tests.iter().filter(|t| test_named(&t.test, what) || t.package == what).collect();
            let failed = |t: &&&crate::store::TestRow| matches!(t.outcome.as_str(), "FAILED" | "failed" | "fail");
            let passed = |t: &&&crate::store::TestRow| matches!(t.outcome.as_str(), "ok" | "passed" | "pass");
            if let Some(t) = hits.iter().filter(failed).max_by_key(|t| t.seq) {
                ("failed", Some(t.seq))
            } else if let Some(t) = hits.iter().filter(passed).max_by_key(|t| t.seq) {
                ("satisfied", Some(t.seq))
            } else {
                ("not run", None)
            }
        }
        "reading" => (reading(what), None),
        "check" => match runs.iter().rev().find(|r| r.command == "check" && r.name == what) {
            Some(r) if r.accepted => ("satisfied", Some(r.seq)),
            Some(r) => ("failed", Some(r.seq)),
            None => ("not run", None),
        },
        _ => ("not run", None),
    }
}

/// One declared verification of a running job as it stands now ([`verifications`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// The line declared (`test:tests::*`, `check:golden`, `reading:<spec>`).
    pub line: String,
    /// `satisfied`, `failed`, `not run`, `unchanged`.
    pub state: String,
    /// The receipt that decided it.
    pub receipt: Option<i64>,
}

/// The running job's declared verifications as its receipts leave them now, as its close would read them: what the
/// close still has to run itself before it decides (a `test:` line `not run`).
pub fn verifications(ctx: &Ctx, which: &str) -> Result<Vec<Verification>> {
    let store = ctx.store()?;
    let running = find(store, JobState::Running, Some(which))?;
    let jf = job_file(&running);
    let id = jf.id.clone().ok_or_else(|| ReadingRefusal::JobState.refuse(format!("{} has no id: was it started?", running.display())))?;
    let journal = store.journal()?;
    let runs = runs_of(&journal, &id, jf.name.as_deref())?;
    let all_runs = journal.runs()?;
    let folders = crate::store::job_folders(store.jobs_dir(JobState::Done)?.parent().unwrap_or(&running));
    let test_seqs: Vec<i64> = runs.iter().filter(|r| r.command == "test").map(|r| r.seq).collect();
    let tests = journal.tests_of_runs(&test_seqs)?;
    let opened = runs.first().map_or(i64::MAX, |r| r.seq);
    let reading = |spec: &str| reading_verified(&ctx.root, &all_runs, &folders, &id, opened, spec);
    Ok(jf.verify.iter().map(|v| {
        let (state, receipt) = verified_with(v, &runs, &tests, &reading);
        Verification { line: v.clone(), state: state.into(), receipt }
    }).collect())
}

/// Whether a test's path (`kitchen::tests::can_cook`) is the one a `test:` verification names: the test by its name
/// or its path's end (`can_cook`, `tests::can_cook`), or every test of a module with `<module>::*` (`tests::*`,
/// `kitchen::tests::*`).
pub fn test_named(test: &str, what: &str) -> bool {
    match what.strip_suffix("::*") {
        Some(module) => test.starts_with(&format!("{module}::")) || test.contains(&format!("::{module}::")),
        None => test == what || test.ends_with(&format!("::{what}")),
    }
}

/// Every `test:` verification names at least one test of the workspace (a test's path, `<module>::*`, or a package),
/// as the close matches them ([`test_named`]); one that names none is refused when it is declared ("no test matches"),
/// never left to be `not run` at the close.
fn check_tests_match(ctx: &Ctx, lines: &[String], ws: Option<&crate::api::Workspace>) -> Result<()> {
    let wanted: Vec<&str> = lines.iter().filter_map(|v| v.strip_prefix("test:")).map(str::trim).collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let owned;
    let f = match ws.filter(|w| w.scope.tests) {
        Some(w) => w.facts(),
        None => {
            owned = crate::reading::facts::Facts::build(&ctx.root, &crate::reading::spec::Scope { tests: true, ..Default::default() }, crate::paths::Resolving::Job)?;
            &owned
        }
    };
    let cases = crate::reading::kinds::tests_of_crates(f, &f.k.packages.keys().cloned().collect());
    for w in wanted {
        if !cases.iter().any(|c| test_named(&c.filter, w) || c.package == w) {
            return Err(ReadingRefusal::SpecInvalid.refuse(format!("test:{w}: no test matches it among the workspace's {} tests (test:<name>, test:<module>::*, test:<package>)", cases.len())));
        }
    }
    Ok(())
}

/// A closing finding naming the cargo test line that proves a fix: listed once, and only when a test fails.
pub const PROVE_A_FIX: &str = "prove a fix with: ";

/// Files cargo writes from the manifests: a job that changes a dependency changes them through cargo, never by hand.
pub const CARGO_WRITTEN: &[&str] = &["Cargo.lock"];

/// Whether a path is a file cargo writes ([`CARGO_WRITTEN`]).
fn cargo_written(path: &str) -> bool {
    CARGO_WRITTEN.iter().any(|c| path == *c || path.ends_with(&format!("/{c}")))
}

/// Files a test line writes: maintenance's timing records of each run (`.maintenance/timings/<run>.jsonl`,
/// [`crate::testrun::write_timings`]). A job's test run writes them, never a hand.
pub const TEST_RUN_WRITTEN: &str = ".maintenance/timings/";

/// Whether a path is a file a test line writes ([`TEST_RUN_WRITTEN`]).
fn test_run_written(path: &str) -> bool {
    path.starts_with(TEST_RUN_WRITTEN) || path.contains(&format!("/{TEST_RUN_WRITTEN}"))
}

/// A file's bytes at `base`, exact: git's text as the line reads it (each line ends in a newline), the last
/// newline taken off when the blob has none (`git cat-file -s`, its size in bytes).
fn base_bytes(root: &Path, base: &str, path: &str) -> Option<Vec<u8>> {
    let spec = format!("{base}:{path}");
    let mut text = git_out(root, &["show", &spec])?.into_bytes();
    let size: usize = git_out(root, &["cat-file", "-s", &spec])?.trim().parse().ok()?;
    while text.len() > size && text.last() == Some(&b'\n') {
        text.pop();
    }
    Some(text)
}

/// A git line's output in the tree at `root` (the kit's declared git line), when it ran and said yes.
fn git_out(root: &Path, words: &[&str]) -> Option<String> {
    let line = crate::exec::Git::in_dir(root.as_os_str()).words(words.iter().map(std::ffi::OsString::from).collect::<Vec<_>>());
    let plan = crate::exec::Plan::new("git").then(crate::exec::line("git", &line).ok()?);
    let w = plan.walk(crate::exec::Watch::Quiet, |_, _| {}).ok()?;
    let w = w.into_iter().next()?;
    (w.code == Some(0)).then_some(w.stdout)
}

/// Whether a path is custodian's or code_passenger's own (never the job's code).
fn own_folder(path: &str) -> bool {
    path.starts_with(&format!("{}/", crate::store::FOLDER)) || path.starts_with(".passenger/")
}

/// The files that differ from `base` now: changed against it, and new files git does not ignore (custodian's and
/// code_passenger's own folders aside).
fn changed_since(ctx: &Ctx, base: &str) -> BTreeSet<String> {
    let mut paths: BTreeSet<String> = BTreeSet::new();
    for out in [git_out(&ctx.root, &["diff", "--name-only", base]), git_out(&ctx.root, &["ls-files", "--others", "--exclude-standard"])].into_iter().flatten() {
        paths.extend(out.lines().map(str::trim).filter(|l| !l.is_empty() && !own_folder(l)).map(str::to_string));
    }
    paths
}

/// A file's digest now (`none` when it is gone): the transform records' form.
fn digest_now(ctx: &Ctx, path: &str) -> String {
    std::fs::read(ctx.root.join(path)).map(|b| stable_digest::StableDigest::sha256(&b).as_str().to_string()).unwrap_or_else(|_| "none".into())
}

/// The files that already differed from the job's base at open, with their digests then (in the job's data).
pub const OPEN_FILES: &str = "open_files.json";

/// Every file the job touched since its base (git: changed against the base commit, and new files git does not
/// ignore; a file that already differed at open counts from its state then), with what carries it: `transform #<seq>` when the job's transforms carry it from its base to now,
/// `allow_file <glob>`, `cargo` for the lock file, else `hand from <digest>` (the digest the hand edit began from).
fn files_touched(ctx: &Ctx, folder: &Path, base: &str, allow: &[String], transforms: &[(i64, crate::transform::TransformRecord)]) -> Result<Vec<(String, String)>> {
    let paths = changed_since(ctx, base);
    let at_open: BTreeMap<String, String> = std::fs::read_to_string(folder.join(JOB_DATA).join(OPEN_FILES)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let globs: Vec<(String, globset::GlobMatcher)> = allow.iter().filter_map(|g| globset::Glob::new(g).ok().map(|m| (g.clone(), m.compile_matcher()))).collect();
    let hex = |b: &[u8]| stable_digest::StableDigest::sha256(b).as_str().to_string();
    let records: Vec<crate::transform::TransformRecord> = transforms.iter().map(|(_, t)| t.clone()).collect();
    let mut out = Vec::new();
    for p in paths {
        let at_base = match at_open.get(&p) {
            Some(d) => d.clone(),
            None => base_bytes(&ctx.root, base, &p).map(|b| hex(&b)).unwrap_or_else(|| "none".into()),
        };
        let now = digest_now(ctx, &p);
        if at_base == now {
            continue;
        }
        let open = BTreeMap::from([(p.clone(), at_base.clone())]);
        let close = BTreeMap::from([(p.clone(), now)]);
        let open = if at_base == "none" { BTreeMap::new() } else { open };
        let close = if close[&p] == "none" { BTreeMap::new() } else { close };
        let how = match crate::transform::coverage(&open, &close, &records).into_iter().next() {
            None => format!("transform #{}", transforms.iter().filter(|(_, t)| t.files.iter().any(|f| f.path == p)).map(|(s, _)| s.to_string()).collect::<Vec<_>>().join(", #")),
            Some(u) => match globs.iter().find(|(_, m)| m.is_match(&p)) {
                Some((g, _)) => format!("allow_file {g}"),
                None if cargo_written(&p) => "cargo".to_string(),
                None if test_run_written(&p) => "test run".to_string(),
                None => format!("hand from {}", u.from),
            },
        };
        out.push((p, how));
    }
    Ok(out)
}

/// The job's lines touched, exact: `git diff --numstat` against the base (a new file's lines all added; the lock file
/// cargo writes aside), in place of the counts' net figure for the job; a step's figure stays the counts' net one,
/// and says so.
fn exact_lines(ctx: &Ctx, folder: &Path, base: &str, touched: &mut [Touched]) {
    let at_open: BTreeMap<String, String> = std::fs::read_to_string(folder.join(JOB_DATA).join(OPEN_FILES)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let Some(numstat) = git_out(&ctx.root, &["diff", "--numstat", base]) else { return };
    let (mut added, mut removed, mut files) = (0usize, 0usize, 0usize);
    for l in numstat.lines() {
        let w: Vec<&str> = l.split('\t').collect();
        if w.len() == 3 && !own_folder(w[2]) && !cargo_written(w[2]) {
            added += w[0].parse::<usize>().unwrap_or(0);
            removed += w[1].parse::<usize>().unwrap_or(0);
            files += 1;
        }
    }
    for p in git_out(&ctx.root, &["ls-files", "--others", "--exclude-standard"]).unwrap_or_default().lines().filter(|p| !p.trim().is_empty() && !own_folder(p) && !cargo_written(p) && at_open.get(*p) != Some(&digest_now(ctx, p))) {
        added += std::fs::read_to_string(ctx.root.join(p)).map(|t| t.lines().count()).unwrap_or(0);
        files += 1;
    }
    for t in touched.iter_mut() {
        if t.label == "job" {
            t.moved.added.lines = added;
            t.moved.removed.lines = removed;
            t.line = format!("job: +{added} \u{2212}{removed} across {files} files in {} crates (git diff --numstat {})", t.crates, &base[..base.len().min(12)]);
        } else if !t.line.contains(": net ") {
            t.line = t.line.replacen(": +", ": net +", 1);
        }
    }
}

/// The languages a bounded job may not write by hand: code (Rust) and its manifests (TOML). Documentation is not
/// code, and is written as the agent writes it.
pub const CODE_LANGUAGES: &[&str] = &["Rust", "TOML"];

/// A bounded job's files that no transform of it covers, from its open and close counts of the workspace, each with
/// the lines changed by hand when code_passenger holds a snapshot of the state the hand edit began from.
/// Whether a file a job touched is code an operation writes: Rust, or a crate's manifest (`add_dependency`,
/// `change_dependency`, `declare_feature`). Its lines by hand are uncovered lines (CUST0031); any other file by hand
/// needs `allow_file` (CUST0041).
pub fn is_code_file(path: &str) -> bool {
    path.ends_with(".rs") || path.rsplit('/').next() == Some("Cargo.toml")
}

fn uncovered_lines(ctx: &Ctx, folder: &Path, transforms: &[crate::transform::TransformRecord]) -> Result<Vec<crate::transform::Uncovered>> {
    let counts = taken(folder);
    let of = |label: &str| counts.iter().rfind(|t| t.route == "*" && t.label == label).cloned();
    let (Some(open), Some(close)) = (of("open"), of("close")) else { return Ok(Vec::new()) };
    let digests = |t: &Taken| -> Result<std::collections::BTreeMap<String, String>> {
        let r = load(&folder.join(&t.reading))?;
        Ok(r.count.map(|c| c.crates.iter().flat_map(|k| k.file_counts.iter()).filter(|f| !f.path.contains('#') && CODE_LANGUAGES.contains(&f.language.as_str())).map(|f| (f.path.clone(), f.digest.clone())).collect()).unwrap_or_default())
    };
    let mut out = crate::transform::coverage(&digests(&open)?, &digests(&close)?, transforms);
    if crate::exec::on_path("code-passenger") {
        for u in out.iter_mut().filter(|u| u.from != "none" && u.now != "none") {
            let line = crate::exec::CodePassenger::at_root(ctx.root.as_os_str()).words(["passenger", "diff", "--from", u.from.as_str(), "--file", u.path.as_str()]);
            let plan = crate::exec::Plan::new("bounded-coverage").then(crate::exec::line("code-passenger:passenger-diff", &line)?);
            if let Some(w) = plan.walk(crate::exec::Watch::Quiet, |_, _| {})?.first() {
                if let Some(l) = w.stdout.lines().find_map(|l| l.strip_prefix("lines: ")) {
                    u.lines = l.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                }
            }
        }
    }
    Ok(out)
}

/// A verification declared on a running job (`custodian job verify LINE`, or the answer to "what does done look
/// like"): added to its job file, with a `verify` receipt, so the closing receipt says when the outcome became known.
pub fn declare_verify(ctx: &Ctx, which: Option<&str>, line: &str) -> Result<(String, Receipt)> {
    check_verify(line)?;
    check_tests_match(ctx, &[line.to_string()], None)?;
    let store = ctx.store()?;
    let running = find(store, JobState::Running, which)?;
    let mut jf = job_file(&running);
    let id = jf.id.clone().ok_or_else(|| ReadingRefusal::JobState.refuse(format!("{} was never started", running.display())))?;
    if !jf.verify.iter().any(|v| v == line) {
        jf.verify.push(line.to_string());
    }
    write_job_file(&running, &jf)?;
    if let Some(t) = crate::task_records::of_job(store, jf.task_id.as_deref(), jf.revision.as_deref())? {
        crate::task_records::verification(&t, &format!("done is: {line}"), None, Some("declared"))?;
    }
    let digest = crate::digest::of_value(&(&id, line));
    let r = Receipt {
        schema: RECEIPT_SCHEMA.into(),
        seq: 0,
        utc: now_utc(),
        command: "verify".into(),
        name: id.clone(),
        spec: None,
        spec_hash: None,
        code_digest: None,
        index_key: None,
        plan_id: format!("job:{id}"),
        plan_digest: digest.clone(),
        operation_digest: digest.clone(),
        receipt_digest: digest,
        line: format!("custodian job verify {line}"),
        exit_code: Some(0),
        accepted: true,
        disposition: "Continue".into(),
        reason: "declared".into(),
        outputs: vec![ctx.rel(&running.join(job_file_name()))],
        job: Some(id.clone()),
        reused_seq: None,
        millis: 0,
        runner: crate::receipts::RUNNER_IN_PROCESS.into(),
    };
    let (r, _) = keep(store, r)?;
    Ok((id, r))
}

/// The two facts a job starts from, as the job file set them at open, and when each became known: `target_known:
/// true at open · outcome_known: false at open, known at #128`.
fn known_line(c: &Closing) -> String {
    let one = |name: &str, at_open: Option<bool>, at: &Option<String>| -> String {
        let open = at_open.map(|b| format!("{b} at open")).unwrap_or_else(|| "not set at open".into());
        match (at_open, at.as_deref()) {
            (Some(true), _) => format!("{name}: {open}"),
            (_, Some(a)) if a != "open" => format!("{name}: {open}, known at {a}"),
            (_, Some(_)) => format!("{name}: {open}"),
            (_, None) => format!("{name}: {open}, never known"),
        }
    };
    format!("{} · {}", one("target_known", c.target_known, &c.target_known_at), one("outcome_known", c.outcome_known, &c.outcome_known_at))
}

/// A file a running job may touch that no operation writes (`custodian job allow GLOB`): the glob added to its
/// job file's `allow_file`, with an `allow` receipt, so the closing receipt says why such a file was touched.
pub fn declare_allow(ctx: &Ctx, which: Option<&str>, glob: &str) -> Result<(String, Receipt)> {
    globset::Glob::new(glob).map_err(|e| ReadingRefusal::SpecInvalid.refuse(format!("`{glob}` is not a glob: {e}")))?;
    let store = ctx.store()?;
    let running = find(store, JobState::Running, which)?;
    let mut jf = job_file(&running);
    let id = jf.id.clone().ok_or_else(|| ReadingRefusal::JobState.refuse(format!("{} was never started", running.display())))?;
    if !jf.allow_file.iter().any(|g| g == glob) {
        jf.allow_file.push(glob.to_string());
    }
    write_job_file(&running, &jf)?;
    let digest = crate::digest::of_value(&(&id, "allow_file", glob));
    let r = Receipt {
        schema: RECEIPT_SCHEMA.into(),
        seq: 0,
        utc: now_utc(),
        command: "allow".into(),
        name: id.clone(),
        spec: None,
        spec_hash: None,
        code_digest: None,
        index_key: None,
        plan_id: format!("job:{id}"),
        plan_digest: digest.clone(),
        operation_digest: digest.clone(),
        receipt_digest: digest,
        line: format!("custodian job allow {glob}"),
        exit_code: Some(0),
        accepted: true,
        disposition: "Continue".into(),
        reason: "allowed".into(),
        outputs: vec![ctx.rel(&running.join(job_file_name()))],
        job: Some(id.clone()),
        reused_seq: None,
        millis: 0,
        runner: crate::receipts::RUNNER_IN_PROCESS.into(),
    };
    let (r, _) = keep(store, r)?;
    Ok((id, r))
}

/// The closing receipt as a page: facts first.
pub fn render_closing(c: &Closing) -> String {
    let mut s = format!("# job {} — {}\n\n", c.id, c.name);
    s.push_str(&format!("- kind: {} · opened {} · closed {}\n- runs: {} (#{}) · readings reused from the cache: {}\n", c.kind, c.opened_utc.clone().unwrap_or_default(), c.closed_utc, c.runs.len(), c.runs.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", #"), c.reused));
    if let Some(g) = &c.guidance {
        s.push_str(&format!("- guidance: {g}\n"));
    }
    if !c.result.is_empty() {
        s.push_str(&format!("- result: **{}**{}\n", c.result, c.result_reason.as_ref().map(|r| format!(" — {r}")).unwrap_or_default()));
    }
    if let Some(m) = c.mode {
        s.push_str(&format!("- mode: {} · {}\n", m.as_str(), known_line(c)));
    }
    if let Some(r) = &c.refusal {
        s.push_str(&format!("- refused: {r}\n"));
    }
    if !c.verify.is_empty() {
        s.push_str("\n## verification\n\n");
        for (v, st) in &c.verify {
            let by = c.verified_by.iter().find_map(|b| b.strip_prefix(&format!("{v}: "))).map(|b| format!(" ({b})")).unwrap_or_default();
            s.push_str(&format!("- {v}: {st}{by}\n"));
        }
    }
    if let Some(o) = &c.over_budget {
        s.push_str(&format!("- time: {o}\n"));
    }
    if let Some(b) = &c.base_commit {
        s.push_str(&format!("- base: {b}\n"));
    }
    if !c.pre_existing.is_empty() {
        s.push_str(&format!("\n## pre-existing ({}): failing the same way at the base, not the job's\n\n", c.pre_existing.len()));
        for p in &c.pre_existing {
            s.push_str(&format!("- {} {} {} — `{}` ({})\n", p.package, p.target, p.test, p.first_line.replace('`', "'"), p.from));
        }
    }
    if !c.failing.is_empty() {
        s.push_str(&format!("\n## failing, the job's ({})\n\n", c.failing.len()));
        for f in &c.failing {
            s.push_str(&format!("- {f}\n"));
        }
    }
    if !c.touched_files.is_empty() {
        s.push_str(&format!("\n## files touched since the base ({})\n\n", c.touched_files.len()));
        for (p, how) in &c.touched_files {
            s.push_str(&format!("- `{p}`: {how}\n"));
        }
    }
    if !c.uncovered.is_empty() {
        s.push_str("\n## touched by hand, not by a transform (bounded)\n\n");
        for u in &c.uncovered {
            s.push_str(&format!("- {}{}\n", u.path, if u.lines.is_empty() { String::new() } else { format!(": lines {}", u.lines.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")) }));
        }
    }
    if !c.transforms.is_empty() {
        s.push_str(&format!("\n- transforms: #{}\n", c.transforms.iter().map(i64::to_string).collect::<Vec<_>>().join(", #")));
    }
    if !c.decisions_open.is_empty() {
        s.push_str("\n## decisions still open\n\n");
        for q in &c.decisions_open {
            s.push_str(&format!("- `{}` {}\n", q.id, q.question));
        }
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
        s.push('\n');
    }
    if !c.lines_touched.is_empty() {
        s.push_str("\n## lines touched\n\n");
        for t in &c.lines_touched {
            s.push_str(&format!("- {} ({}) · count diff `{}`\n", t.line, t.route, t.diff));
        }
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
    if !c.step_times.is_empty() {
        s.push_str("\n## time per step kind\n\n| kind | runs | members | time | per member |\n|---|---|---|---|---|\n");
        for t in &c.step_times {
            s.push_str(&format!("| {} | {} | {} | {} | {} |\n", t.kind, t.runs, t.members, crate::step_time::duration(t.millis), t.per_member().map(crate::step_time::duration).unwrap_or_else(|| "-".into())));
        }
    }
    if !c.per_member.is_empty() {
        s.push_str("\n## paid one member at a time (a batch form exists)\n\n");
        for f in &c.per_member {
            s.push_str(&format!("- {f}\n"));
        }
    }
    s
}

/// `job brief`: the closing receipt of a done job, facts first.
pub fn brief(ctx: &Ctx, which: &str) -> Result<String> {
    let store = ctx.store()?;
    let done = find(store, JobState::Done, Some(which))?;
    let text = std::fs::read_to_string(done.join("closing.json")).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", done.display())))?;
    let c: Closing = serde_json::from_str(&text).map_err(|e| ReadingRefusal::MissingInput.refuse(e))?;
    let mut s = String::new();
    for q in &c.decisions_open {
        s.push_str(&format!("decision open {}: {}{}\n", q.id, q.question, if q.options.is_empty() { String::new() } else { format!(" ({})", q.options.join(" | ")) }));
    }
    s.push_str(&format!("job {} ({}) closed {} — {} runs, {} reused\n", c.id, c.name, c.closed_utc, c.runs.len(), c.reused));
    if !c.result.is_empty() {
        s.push_str(&format!("result: {}{}\n", c.result, c.result_reason.as_ref().map(|r| format!(" ({r})")).unwrap_or_default()));
    }
    if let Some(m) = c.mode {
        s.push_str(&format!("mode: {} · {}\n", m.as_str(), known_line(&c)));
    }
    for u in &c.uncovered {
        s.push_str(&format!("touched by hand (bounded): {}{}\n", u.path, if u.lines.is_empty() { String::new() } else { format!(":{}", u.lines.iter().map(u32::to_string).collect::<Vec<_>>().join(",")) }));
    }
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
    if let Some(task) = job_file(&done).task_id.as_deref().map(|t| crate::task_records::read(store, t)).transpose()?.flatten() {
        for l in crate::task_records::brief_lines(&task) {
            s.push_str(&format!("{l}\n"));
        }
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
    let jf = JobFile { name: Some(name.clone()), kind: Some(kind.to_string()), target: Some(target.to_string()), ..JobFile::default() };
    write_job_file(&dir, &jf)?;
    let started = start(ctx, Some(&name), again)?;
    let (closing, done, said) = close(ctx, &started.id)?;
    Ok((started, closing, done, said))
}
