//! Where custodian keeps what it does in a project: the initialized folder
//! `.custodian/`, its paths named once through pathgen (a dev policy whose
//! base is the project root and whose app is `.custodian`, so every path is a
//! role directory under it), files written whole by write-then-rename, and the
//! journal: one SQLite file holding a receipt per run, the jobs and the test
//! results (receipt_history's journal, opened at the path named here).

use crate::reading::refusal::ReadingRefusal;
use crate::Result;
use pathgen::{AppPathRole, ArtifactEndpoint, ArtifactOutputDescriptor, PathMutability, PathOwner, PathPolicy, PathSensitivity, PathTemplateVars};
use rusqlite::{params, Connection};
use std::sync::OnceLock;
use uh_tom_mick::{AdmissionGuard, AtomicAdmission};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The initialized folder's name, and pathgen's app name for it.
pub const FOLDER: &str = ".custodian";

/// How long a write to the journal waits for another writer: the one wait the journal has (receipt_history's).
pub use receipt_history::JOURNAL_BUSY;

const fn file(route_id: &'static str, kind: &'static str, endpoint: ArtifactEndpoint, base_role: AppPathRole, relative_template: &'static str, mutability: PathMutability) -> ArtifactOutputDescriptor {
    part("custodian.layout.v1", route_id, kind, endpoint, base_role, relative_template, mutability)
}

/// A part whose record has a schema of its own (a reading's record, a check's JSON, a log).
const fn part(schema: &'static str, route_id: &'static str, kind: &'static str, endpoint: ArtifactEndpoint, base_role: AppPathRole, relative_template: &'static str, mutability: PathMutability) -> ArtifactOutputDescriptor {
    ArtifactOutputDescriptor {
        route_id,
        schema,
        kind,
        version: 1,
        endpoint,
        base_role,
        relative_template,
        sensitivity: PathSensitivity::Internal,
        mutability,
        owner: PathOwner::CurrentUser,
    }
}

/// Every file custodian keeps, named once.
pub mod named {
    use super::*;
    pub const SETTINGS: ArtifactOutputDescriptor = file("custodian.settings", "settings", ArtifactEndpoint::FileToml, AppPathRole::Config, "custodian.toml", PathMutability::Writable);
    pub const JOB_TEMPLATE: ArtifactOutputDescriptor = file("custodian.job_template", "job_template", ArtifactEndpoint::FileToml, AppPathRole::Config, "jobs/{kind}.toml", PathMutability::Writable);
    pub const THRESHOLDS: ArtifactOutputDescriptor = file("custodian.thresholds", "thresholds", ArtifactEndpoint::FileToml, AppPathRole::Config, "{name}.toml", PathMutability::Writable);
    pub const QUEUED_SPEC: ArtifactOutputDescriptor = file("custodian.spec.queued", "spec", ArtifactEndpoint::FileToml, AppPathRole::Data, "specs/queued/{name}.toml", PathMutability::Writable);
    pub const DONE_SPEC: ArtifactOutputDescriptor = file("custodian.spec.done", "spec", ArtifactEndpoint::FileToml, AppPathRole::Data, "specs/done/{name}.toml", PathMutability::Writable);
    pub const READING_PAGE: ArtifactOutputDescriptor = file("custodian.reading.page", "reading", ArtifactEndpoint::FileMarkdown, AppPathRole::Data, "readings/{name}/{stamp}.md", PathMutability::AppendOnly);
    pub const RECEIPT: ArtifactOutputDescriptor = file("custodian.receipt", "receipt", ArtifactEndpoint::FileJson, AppPathRole::Data, "receipts/{seq}.json", PathMutability::AppendOnly);
    pub const INVENTORY: ArtifactOutputDescriptor = file("custodian.inventory", "inventory", ArtifactEndpoint::FileMarkdown, AppPathRole::Data, "inventory.md", PathMutability::Writable);
    pub const PLAN: ArtifactOutputDescriptor = file("custodian.plan", "plan", ArtifactEndpoint::FileToml, AppPathRole::Data, "plans/{name}.toml", PathMutability::Writable);
    pub const JOURNAL: ArtifactOutputDescriptor = file("custodian.journal", "journal", ArtifactEndpoint::FileTxt, AppPathRole::State, "journal.sqlite", PathMutability::Writable);
    pub const JOB_FILE: ArtifactOutputDescriptor = file("custodian.job", "job", ArtifactEndpoint::FileToml, AppPathRole::State, "jobs/{state}/{job}/v1.job.1o.job", PathMutability::Writable);
    pub const WRITER: ArtifactOutputDescriptor = file("custodian.writer", "writer", ArtifactEndpoint::FileTxt, AppPathRole::State, "writer.sqlite", PathMutability::Writable);
    /// The task-history root's index (`task_history`'s format); the root holds a task per job name.
    pub const HISTORY_INDEX: ArtifactOutputDescriptor = file("custodian.history", "history", ArtifactEndpoint::FileTxt, AppPathRole::Data, "history/v1.history_index.1o.diff", PathMutability::Writable);
    /// The paths table, in 1o (`paths.toml` beside it is read when this is missing).
    pub const PATHS: ArtifactOutputDescriptor = file("custodian.paths", "paths", ArtifactEndpoint::FileTxt, AppPathRole::Config, "v1.paths.1o.paths", PathMutability::Writable);
    /// The 1o schemas, in core-1o's layout, listed in the project's oneo.project.json.
    pub const SCHEMA: ArtifactOutputDescriptor = file("custodian.schema", "schema", ArtifactEndpoint::FileJson, AppPathRole::Config, "schema/{modifier}/v1.schema.json", PathMutability::Writable);
    pub const SOCKET: ArtifactOutputDescriptor = file("custodian.serve", "socket", ArtifactEndpoint::FileTxt, AppPathRole::Runtime, "serve.sock", PathMutability::Ephemeral);
    /// A change spec (`crate::change`), in 1o; `<name>.toml` beside it is its twin.
    pub const CHANGE_SPEC: ArtifactOutputDescriptor = file("custodian.change", "change", ArtifactEndpoint::FileTxt, AppPathRole::Data, "specs/changes/v1.{name}.1o.change", PathMutability::Writable);
    /// A preflight's record: what a change spec entails over the code it was read against, and the decision.
    pub const PREFLIGHT: ArtifactOutputDescriptor = file("custodian.preflight", "preflight", ArtifactEndpoint::FileJson, AppPathRole::Data, "preflight/{name}/{stamp}.json", PathMutability::AppendOnly);
    /// A transform's record (code_passenger's, `crate::transform::TransformRecord`): what it wrote, file by file.
    pub const TRANSFORM: ArtifactOutputDescriptor = file("custodian.transform", "transform", ArtifactEndpoint::FileJson, AppPathRole::Data, "transforms/{name}/{stamp}.json", PathMutability::AppendOnly);
    /// A linkage record (`crate::linkage`), in 1o; `<name>.toml` beside it is its twin.
    pub const LINKAGE: ArtifactOutputDescriptor = file("custodian.linkage", "linkage", ArtifactEndpoint::FileTxt, AppPathRole::Data, "linkage/v1.{name}.1o.linkage", PathMutability::Writable);
    /// An intent's plan (`crate::plan`): the investigation, the reuse, the cost, the decisions, the steps.
    pub const INTENT_PLAN: ArtifactOutputDescriptor = file("custodian.intent_plan", "plan", ArtifactEndpoint::FileJson, AppPathRole::Data, "plans/intent/{name}.json", PathMutability::Writable);
    /// A reading's record, beside its page: the facts the page was made from (`reading::load` reads it back).
    pub const READING_RECORD: ArtifactOutputDescriptor = part("custodian.reading.v1", "custodian.reading.record", "reading", ArtifactEndpoint::FileJson, AppPathRole::Data, "readings/{name}/{stamp}.json", PathMutability::AppendOnly);
    /// What a maintenance check line printed, as JSON.
    pub const CHECK_RUN: ArtifactOutputDescriptor = part("maintenance.check_run.v1", "custodian.check", "check", ArtifactEndpoint::FileJson, AppPathRole::Data, "checks/{stamp}.json", PathMutability::AppendOnly);
    /// A log kept for a later reading (custodian's `diagnostics from -` reads it from stdin).
    pub const LOG: ArtifactOutputDescriptor = part("custodian.log.v1", "custodian.log", "log", ArtifactEndpoint::FileTxt, AppPathRole::Data, "logs/{name}.txt", PathMutability::AppendOnly);
    /// A tree snapshot (`tree_history::tree`), one per point.
    pub const TREE: ArtifactOutputDescriptor = file("custodian.tree", "tree", ArtifactEndpoint::FileJson, AppPathRole::Data, "trees/{point}.json", PathMutability::AppendOnly);
    /// The parsed index, held once per code digest (klepto's model in its serde form, as zstd-compressed CBOR): the
    /// next scan of the same code reads it instead of parsing.
    pub const INDEX_MODEL: ArtifactOutputDescriptor = file("custodian.index", "index", ArtifactEndpoint::FileCbor, AppPathRole::State, "index/{digest}/model.cbor.zst", PathMutability::Writable);
    /// A reading's or a reuse's cost record (`crate::cost`), under its receipt's number.
    pub const COST: ArtifactOutputDescriptor = part("custodian.cost.v1", "custodian.cost", "cost", ArtifactEndpoint::FileJson, AppPathRole::Data, "costs/{seq}.json", PathMutability::AppendOnly);
    /// An archive (`crate::archive`): the crates a line named, as one ZIP, dated, every earlier one kept.
    pub const ARCHIVE: ArtifactOutputDescriptor = part("custodian.archive.v1", "custodian.archive", "archive", ArtifactEndpoint::FileTxt, AppPathRole::Data, "archives/{name}/{stamp}.zip", PathMutability::AppendOnly);

    /// Every part of the folder, each once: what `custodian list` prints and what the golden test pins.
    pub const ALL: &[ArtifactOutputDescriptor] = &[
        SETTINGS, PATHS, SCHEMA, THRESHOLDS, JOB_TEMPLATE, QUEUED_SPEC, DONE_SPEC, READING_PAGE, RECEIPT, INVENTORY, PLAN, HISTORY_INDEX, JOURNAL, JOB_FILE, SOCKET,
        READING_RECORD, CHECK_RUN, LOG, WRITER, CHANGE_SPEC, PREFLIGHT, TRANSFORM, LINKAGE, INTENT_PLAN, TREE, INDEX_MODEL, COST, ARCHIVE,
    ];

    /// The folder's policy for the project at `root`: pathgen's dev policy whose base is the root and whose app
    /// is [`FOLDER`], so a part is `<root>/.custodian/<role>/<template>`.
    pub fn policy(root: &Path) -> std::result::Result<PathPolicy, pathgen::PathgenError> {
        Ok(PathPolicy::dev(FOLDER)?.with_dev_base(root))
    }

    /// A part's path under `policy`, its template filled from `vars`; a value that would leave the folder is refused.
    pub fn resolve(policy: &PathPolicy, d: &ArtifactOutputDescriptor, vars: &[(&str, &str)]) -> std::result::Result<PathBuf, pathgen::PathgenError> {
        let mut v = PathTemplateVars::new();
        for (k, val) in vars {
            v = v.with(k, val);
        }
        policy.artifact_output(d, &v).map(|r| r.path)
    }
}

/// This process's one writer: uh-tom-mick's admission word, one active at a time.
fn admission() -> &'static AtomicAdmission {
    static A: OnceLock<AtomicAdmission> = OnceLock::new();
    A.get_or_init(|| AtomicAdmission::new(1).expect("an admission of one"))
}

/// The writer this run holds: admitted in this process (uh-tom-mick) and
/// across processes (an exclusive SQLite transaction on `state/writer.sqlite`,
/// released when the run ends, a crash included).
pub struct Writer {
    _guard: AdmissionGuard<'static>,
    _lock: Connection,
}

/// Plain: specs, readings, a journal. Agent: the same, and jobs as folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Plain,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub mode: Mode,
    /// Globs (relative to the root) no reading reads, on top of a spec's own.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// A job's mode when its job file names none: `bounded` or `unbounded` (unbounded when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_mode: Option<crate::jobs::JobMode>,
}

/// An initialized project: its root and the folder's paths.
#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
    policy: PathPolicy,
}

fn jerr(e: impl std::fmt::Display) -> crate::CustodianError {
    ReadingRefusal::Journal.refuse(e)
}

fn write_err(path: &Path, e: impl std::fmt::Display) -> crate::CustodianError {
    ReadingRefusal::Write.refuse(format!("{}: {e}", path.display()))
}

/// Writes `bytes` to `path` whole: to a sibling temporary file, then a rename,
/// so a reader never sees half a file (receipt_history's, refused as the kit's `Write`).
pub fn write_whole(path: &Path, bytes: &[u8]) -> Result<()> {
    Ok(receipt_history::write_whole(path, bytes)?)
}

impl Store {
    fn policy(root: &Path) -> Result<PathPolicy> {
        named::policy(root).map_err(|e| write_err(root, e))
    }

    /// The initialized folder at `root` or above, walking up from `start`.
    pub fn find(start: &Path) -> Option<Store> {
        let start = start.canonicalize().ok()?;
        for dir in start.ancestors() {
            let settings = named::resolve(&Self::policy(dir).ok()?, &named::SETTINGS, &[]).ok()?;
            if settings.is_file() {
                return Some(Store { root: dir.to_path_buf(), policy: Self::policy(dir).ok()? });
            }
        }
        None
    }

    /// Makes the folder at `root` (plain or agent layout); an existing one is
    /// kept and its mode raised to agent when asked.
    pub fn init(root: &Path, mode: Mode) -> Result<(Store, Vec<PathBuf>)> {
        let root = root.canonicalize().map_err(|e| write_err(root, e))?;
        let store = Store { policy: Self::policy(&root)?, root };
        let mut made = Vec::new();
        let settings = store.path(&named::SETTINGS, &[])?;
        let mut current: Settings = std::fs::read_to_string(&settings).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or(Settings { mode, exclude: Vec::new(), job_mode: None });
        if mode == Mode::Agent {
            current.mode = Mode::Agent;
        }
        write_whole(&settings, toml::to_string(&current).map_err(|e| write_err(&settings, e))?.as_bytes())?;
        made.push(settings);
        let mut dirs = vec![store.dir_of(&named::QUEUED_SPEC, &[("name", "x")])?, store.dir_of(&named::DONE_SPEC, &[("name", "x")])?, store.dir_of(&named::RECEIPT, &[("seq", "1")])?];
        if current.mode == Mode::Agent {
            for state in JobState::ALL {
                dirs.push(store.jobs_dir(*state)?);
            }
            for (kind, text) in crate::jobs::SHIPPED_TEMPLATES {
                let p = store.path(&named::JOB_TEMPLATE, &[("kind", kind)])?;
                if !p.exists() {
                    write_whole(&p, text.as_bytes())?;
                    made.push(p);
                }
            }
        }
        let thresholds = store.path(&named::THRESHOLDS, &[("name", "features")])?;
        if !thresholds.exists() {
            write_whole(&thresholds, crate::features::SHIPPED_THRESHOLDS.as_bytes())?;
            made.push(thresholds);
        }
        for d in dirs {
            std::fs::create_dir_all(&d).map_err(|e| write_err(&d, e))?;
            made.push(d);
        }
        // the 1o schemas where the language server finds them, and an empty paths table in 1o
        let schema_dir = store.dir_of(&named::SCHEMA, &[("modifier", "x")])?.parent().map(Path::to_path_buf).unwrap_or_default();
        made.extend(crate::oneo::write_schemas(&schema_dir)?);
        // the task-history records' `diff` modifier, declared once in task_history
        let diff = schema_dir.join(task_history::ONE_O_DIFF_MODIFIER).join("v1.schema.json");
        write_whole(&diff, task_history::DIFF_SCHEMA_JSON.as_bytes())?;
        made.push(diff);
        let rel = schema_dir.strip_prefix(&store.root).unwrap_or(&schema_dir).to_string_lossy().replace('\\', "/");
        made.push(crate::oneo::declare_schema_dir(&store.root, &rel)?);
        let paths = store.path(&named::PATHS, &[])?;
        let old_paths = store.path(&named::THRESHOLDS, &[("name", "paths")])?;
        if !paths.exists() && !old_paths.exists() {
            write_whole(&paths, crate::oneo::render(crate::oneo::Doc::Paths, &crate::paths::PathsFile::default())?.as_bytes())?;
            made.push(paths);
        }
        let journal = store.journal()?;
        made.push(journal.path.clone());
        Ok((store, made))
    }

    pub fn settings(&self) -> Result<Settings> {
        let p = self.path(&named::SETTINGS, &[])?;
        let text = std::fs::read_to_string(&p).map_err(|e| ReadingRefusal::NotInitialized.refuse(format!("{}: {e}", p.display())))?;
        toml::from_str(&text).map_err(|e| ReadingRefusal::NotInitialized.refuse(format!("{}: {e}", p.display())))
    }

    /// A named file, its template filled.
    pub fn path(&self, d: &ArtifactOutputDescriptor, vars: &[(&str, &str)]) -> Result<PathBuf> {
        named::resolve(&self.policy, d, vars).map_err(|e| write_err(&self.root, e))
    }

    /// The directory a named file sits in.
    pub fn dir_of(&self, d: &ArtifactOutputDescriptor, vars: &[(&str, &str)]) -> Result<PathBuf> {
        Ok(self.path(d, vars)?.parent().map(Path::to_path_buf).unwrap_or_default())
    }

    /// `readings/<name>/` for a spec.
    pub fn reading_dir(&self, name: &str) -> Result<PathBuf> {
        self.dir_of(&named::READING_PAGE, &[("name", name), ("stamp", "x")])
    }

    pub fn jobs_dir(&self, state: JobState) -> Result<PathBuf> {
        Ok(self.dir_of(&named::JOB_FILE, &[("state", state.as_str()), ("job", "x")])?.parent().map(Path::to_path_buf).unwrap_or_default())
    }

    pub fn job_dir(&self, state: JobState, job: &str) -> Result<PathBuf> {
        self.dir_of(&named::JOB_FILE, &[("state", state.as_str()), ("job", job)])
    }

    pub fn journal(&self) -> Result<Journal> {
        Ok(Journal::open(&self.path(&named::JOURNAL, &[])?)?)
    }

    /// The project's writer, or the refusal: another run (in this process or
    /// another) holds it.
    pub fn admit(&self) -> Result<Writer> {
        let guard = admission().try_enter().map_err(|e| ReadingRefusal::WriterHeld.refuse(format!("in this process: {e:?}")))?;
        let p = self.path(&named::WRITER, &[])?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| write_err(d, e))?;
        }
        let lock = Connection::open(&p).map_err(jerr)?;
        lock.busy_timeout(std::time::Duration::from_millis(0)).map_err(jerr)?;
        lock.execute_batch("CREATE TABLE IF NOT EXISTS writer (pid INTEGER); PRAGMA locking_mode = EXCLUSIVE;").map_err(jerr)?;
        lock.execute_batch("BEGIN EXCLUSIVE;").map_err(|e| ReadingRefusal::WriterHeld.refuse(format!("{}: {e}", p.display())))?;
        lock.execute("INSERT INTO writer (pid) VALUES (?1)", params![std::process::id()]).map_err(jerr)?;
        Ok(Writer { _guard: guard, _lock: lock })
    }
}

/// A job folder's state: where its folder sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Done,
}

impl JobState {
    pub const ALL: &'static [JobState] = &[JobState::Queued, JobState::Running, JobState::Done];

    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Done => "done",
        }
    }
}

/// Every job folder under `jobs_root` (`state/jobs/`), with its state, in
/// [`JobState::ALL`]'s order, each state's folders in name order.
pub fn job_folders(jobs_root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for state in JobState::ALL {
        let mut v: Vec<PathBuf> = std::fs::read_dir(jobs_root.join(state.as_str())).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
        v.sort();
        out.extend(v.into_iter().map(|p| (state.as_str().to_string(), p)));
    }
    out
}

pub use crate::receipts::{Receipt, RECEIPT_SCHEMA};

/// The journal: receipts, jobs, test results, in one SQLite file; its tables,
/// its sequence (uh-tom-mick's checked counter) and its rows are
/// receipt_history's.
pub use receipt_history::{JobRow, Journal, TestRow, JOURNAL_SCHEMA};
