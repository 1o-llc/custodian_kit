//! Where custodian keeps what it does in a project: the initialized folder
//! `.custodian/`, its paths named once through pathgen (a dev policy whose
//! base is the project root and whose app is `.custodian`, so every path is a
//! role directory under it), files written whole by write-then-rename, and the
//! journal: one SQLite file holding a receipt per run, the jobs and the test
//! results.

use crate::reading::refusal::ReadingRefusal;
use crate::Result;
use pathgen::{AppPathRole, ArtifactEndpoint, ArtifactOutputDescriptor, PathMutability, PathOwner, PathPolicy, PathSensitivity, PathTemplateVars};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::OnceLock;
use uh_tom_mick::{AdmissionGuard, AtomicAdmission, AtomicCounter};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The initialized folder's name, and pathgen's app name for it.
pub const FOLDER: &str = ".custodian";

const fn file(route_id: &'static str, kind: &'static str, endpoint: ArtifactEndpoint, base_role: AppPathRole, relative_template: &'static str, mutability: PathMutability) -> ArtifactOutputDescriptor {
    ArtifactOutputDescriptor {
        route_id,
        schema: "custodian.layout.v1",
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
    /// The paths table, in 1o (`paths.toml` beside it is read when this is missing).
    pub const PATHS: ArtifactOutputDescriptor = file("custodian.paths", "paths", ArtifactEndpoint::FileTxt, AppPathRole::Config, "v1.paths.1o.paths", PathMutability::Writable);
    /// The 1o schemas, in core-1o's layout, listed in the project's oneo.project.json.
    pub const SCHEMA: ArtifactOutputDescriptor = file("custodian.schema", "schema", ArtifactEndpoint::FileJson, AppPathRole::Config, "schema/{modifier}/v1.schema.json", PathMutability::Writable);
    pub const SOCKET: ArtifactOutputDescriptor = file("custodian.serve", "socket", ArtifactEndpoint::FileTxt, AppPathRole::Runtime, "serve.sock", PathMutability::Ephemeral);
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
}

/// An initialized project: its root and the folder's paths.
#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
    policy: PathPolicy,
}

fn write_err(path: &Path, e: impl std::fmt::Display) -> crate::CustodianError {
    ReadingRefusal::Write.refuse(format!("{}: {e}", path.display()))
}

/// Writes `bytes` to `path` whole: to a sibling temporary file, then a rename,
/// so a reader never sees half a file.
pub fn write_whole(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| write_err(dir, e))?;
    }
    let tmp = path.with_file_name(format!(".{}.{}.tmp", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| write_err(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| write_err(path, e))
}

impl Store {
    fn policy(root: &Path) -> Result<PathPolicy> {
        PathPolicy::dev(FOLDER).map(|p| p.with_dev_base(root)).map_err(|e| write_err(root, e))
    }

    /// The initialized folder at `root` or above, walking up from `start`.
    pub fn find(start: &Path) -> Option<Store> {
        let start = start.canonicalize().ok()?;
        for dir in start.ancestors() {
            let settings = Self::policy(dir).ok()?.artifact_output(&named::SETTINGS, &PathTemplateVars::new()).ok()?.path;
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
        let mut current: Settings = std::fs::read_to_string(&settings).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or(Settings { mode, exclude: Vec::new() });
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
        let mut v = PathTemplateVars::new();
        for (k, val) in vars {
            v = v.with(k, val);
        }
        self.policy.artifact_output(d, &v).map(|r| r.path).map_err(|e| write_err(&self.root, e))
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
        Journal::open(&self.path(&named::JOURNAL, &[])?)
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

/// The journal: receipts, jobs, test results. Its sequence is a checked
/// counter (uh-tom-mick), seeded from the last receipt, advanced by the one
/// writer.
pub struct Journal {
    pub path: PathBuf,
    conn: Connection,
    seq: AtomicCounter,
}

fn jerr(e: impl std::fmt::Display) -> crate::CustodianError {
    ReadingRefusal::Journal.refuse(e)
}

/// The journal's tables, declared once.
pub const JOURNAL_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS runs (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  utc TEXT NOT NULL,
  command TEXT NOT NULL,
  name TEXT NOT NULL,
  spec TEXT,
  spec_hash TEXT,
  code_digest TEXT,
  index_key TEXT,
  plan_id TEXT NOT NULL,
  plan_digest TEXT NOT NULL,
  operation_digest TEXT NOT NULL,
  receipt_digest TEXT NOT NULL,
  line TEXT NOT NULL,
  exit_code INTEGER,
  accepted INTEGER NOT NULL,
  disposition TEXT NOT NULL,
  reason TEXT NOT NULL,
  outputs TEXT NOT NULL,
  job TEXT,
  reused_seq INTEGER,
  millis INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS runs_same_search ON runs (command, spec_hash, code_digest);
CREATE TABLE IF NOT EXISTS jobs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  state TEXT NOT NULL,
  folder TEXT NOT NULL,
  opened_utc TEXT,
  closed_utc TEXT
);
CREATE TABLE IF NOT EXISTS tests (
  seq INTEGER NOT NULL,
  package TEXT NOT NULL,
  target TEXT NOT NULL,
  test TEXT NOT NULL,
  outcome TEXT NOT NULL,
  millis INTEGER
);
";

/// A job as the journal keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub state: String,
    pub folder: String,
    pub opened_utc: Option<String>,
    pub closed_utc: Option<String>,
}

/// One test's outcome in one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRow {
    pub seq: i64,
    pub package: String,
    pub target: String,
    pub test: String,
    pub outcome: String,
    pub millis: Option<i64>,
}

impl Journal {
    pub fn open(path: &Path) -> Result<Journal> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| write_err(dir, e))?;
        }
        let conn = Connection::open(path).map_err(jerr)?;
        conn.busy_timeout(std::time::Duration::from_secs(10)).map_err(jerr)?;
        conn.execute_batch(JOURNAL_SCHEMA).map_err(jerr)?;
        let last: i64 = conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM runs", [], |r| r.get(0)).map_err(jerr)?;
        Ok(Journal { path: path.to_path_buf(), conn, seq: AtomicCounter::new(last.max(0) as u64) })
    }

    /// Records a run; the receipt comes back with its sequence number.
    pub fn record(&self, mut r: Receipt) -> Result<Receipt> {
        let seq = self.seq.increment().map_err(|e| jerr(format!("{e:?}")))? as i64;
        self.conn
            .execute(
                "INSERT INTO runs (seq, utc, command, name, spec, spec_hash, code_digest, index_key, plan_id, plan_digest, operation_digest, receipt_digest, line, exit_code, accepted, disposition, reason, outputs, job, reused_seq, millis)
                 VALUES (?21, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    r.utc,
                    r.command,
                    r.name,
                    r.spec,
                    r.spec_hash,
                    r.code_digest,
                    r.index_key,
                    r.plan_id,
                    r.plan_digest,
                    r.operation_digest,
                    r.receipt_digest,
                    r.line,
                    r.exit_code,
                    r.accepted,
                    r.disposition,
                    r.reason,
                    serde_json::to_string(&r.outputs).unwrap_or_default(),
                    r.job,
                    r.reused_seq,
                    r.millis,
                    seq
                ],
            )
            .map_err(jerr)?;
        r.seq = seq;
        Ok(r)
    }

    fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Receipt> {
        let outputs: String = row.get("outputs")?;
        Ok(Receipt {
            schema: RECEIPT_SCHEMA.to_string(),
            seq: row.get("seq")?,
            utc: row.get("utc")?,
            command: row.get("command")?,
            name: row.get("name")?,
            spec: row.get("spec")?,
            spec_hash: row.get("spec_hash")?,
            code_digest: row.get("code_digest")?,
            index_key: row.get("index_key")?,
            plan_id: row.get("plan_id")?,
            plan_digest: row.get("plan_digest")?,
            operation_digest: row.get("operation_digest")?,
            receipt_digest: row.get("receipt_digest")?,
            line: row.get("line")?,
            exit_code: row.get("exit_code")?,
            accepted: row.get("accepted")?,
            disposition: row.get("disposition")?,
            reason: row.get("reason")?,
            outputs: serde_json::from_str(&outputs).unwrap_or_default(),
            job: row.get("job")?,
            reused_seq: row.get("reused_seq")?,
            millis: row.get("millis")?,
        })
    }

    /// The same search answered before: the newest accepted reading of this
    /// spec hash over this code, not itself a reuse.
    pub fn same_search(&self, spec_hash: &str, code_digest: &str) -> Result<Option<Receipt>> {
        self.conn
            .query_row(
                "SELECT * FROM runs WHERE command = 'reading' AND spec_hash = ?1 AND code_digest = ?2 AND accepted = 1 AND reused_seq IS NULL ORDER BY seq DESC LIMIT 1",
                params![spec_hash, code_digest],
                Self::row,
            )
            .optional()
            .map_err(jerr)
    }

    /// Every run, oldest first.
    pub fn runs(&self) -> Result<Vec<Receipt>> {
        let mut st = self.conn.prepare("SELECT * FROM runs ORDER BY seq").map_err(jerr)?;
        let rows = st.query_map([], Self::row).map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)
    }

    /// The runs of one job, in order.
    pub fn runs_of_job(&self, job: &str) -> Result<Vec<Receipt>> {
        let mut st = self.conn.prepare("SELECT * FROM runs WHERE job = ?1 ORDER BY seq").map_err(jerr)?;
        let rows = st.query_map(params![job], Self::row).map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)
    }

    /// The readings of one spec name, oldest first (reuses left out).
    pub fn readings_of(&self, name: &str) -> Result<Vec<Receipt>> {
        let mut st = self.conn.prepare("SELECT * FROM runs WHERE command = 'reading' AND name = ?1 AND reused_seq IS NULL AND accepted = 1 ORDER BY seq").map_err(jerr)?;
        let rows = st.query_map(params![name], Self::row).map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)
    }

    pub fn put_job(&self, j: &JobRow) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO jobs (id, name, kind, state, folder, opened_utc, closed_utc) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET name = ?2, kind = ?3, state = ?4, folder = ?5, opened_utc = ?6, closed_utc = ?7",
                params![j.id, j.name, j.kind, j.state, j.folder, j.opened_utc, j.closed_utc],
            )
            .map(|_| ())
            .map_err(jerr)
    }

    pub fn job(&self, id_or_name: &str) -> Result<Option<JobRow>> {
        self.conn
            .query_row("SELECT id, name, kind, state, folder, opened_utc, closed_utc FROM jobs WHERE id = ?1 OR name = ?1 ORDER BY opened_utc DESC LIMIT 1", params![id_or_name], |r| {
                Ok(JobRow { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, state: r.get(3)?, folder: r.get(4)?, opened_utc: r.get(5)?, closed_utc: r.get(6)? })
            })
            .optional()
            .map_err(jerr)
    }

    pub fn put_test(&self, t: &TestRow) -> Result<()> {
        self.conn
            .execute("INSERT INTO tests (seq, package, target, test, outcome, millis) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![t.seq, t.package, t.target, t.test, t.outcome, t.millis])
            .map(|_| ())
            .map_err(jerr)
    }

    pub fn tests(&self) -> Result<Vec<TestRow>> {
        let mut st = self.conn.prepare("SELECT seq, package, target, test, outcome, millis FROM tests ORDER BY seq").map_err(jerr)?;
        let rows = st
            .query_map([], |r| Ok(TestRow { seq: r.get(0)?, package: r.get(1)?, target: r.get(2)?, test: r.get(3)?, outcome: r.get(4)?, millis: r.get(5)? }))
            .map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)
    }

    pub fn tests_of_runs(&self, seqs: &[i64]) -> Result<Vec<TestRow>> {
        Ok(self.tests()?.into_iter().filter(|t| seqs.contains(&t.seq)).collect())
    }
}
