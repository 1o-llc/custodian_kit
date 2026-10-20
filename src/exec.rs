//! The execution facade: every run custodian makes is a declared line run
//! through xccute, so a receipt exists for each. The programs are declared once
//! here, as fleet_commands declares the kit's (a `CompositeShell` root, a
//! `SimpleSubCommand` leaf per way it is called, its exit statuses named):
//! custodian itself (`exec-reading`, the reading as a child process), cargo
//! (`test`, `check`), git (`diff --name-only`), code-passenger (`gate`,
//! `transform`), the cheap model's command line (`claude -p`, the exercise's
//! intents) and maintenance (its own declared lines, from its runner). A plan of lines is walked from
//! the first by each line's exit-status policy, and every line walked leaves
//! xccute's `RuntimeOperationReceipt` (plan digest, operation digest,
//! decision) with what it printed and how long it took.

use crate::reading::refusal::ReadingRefusal;
use crate::Result;
use simple_impl_derive::{CompositeShell, SimpleSubCommand};
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Stdio;
use std::time::Instant;
use xccute_contract::{CommandPreview, CommandValidationResult, CompositeShellCommand, ValidatedCommand};
use xccute_runtime::{ExitDisposition, ExitStatusDecision, ExitStatusPolicy, RuntimeExitStatus, RuntimeOperation, RuntimeOperationPlan, RuntimeOperationReceipt};

/// custodian itself.
#[derive(CompositeShell, Debug, Clone, Copy, Default)]
#[shell(program = "custodian")]
#[composite(command = "exec-reading", method = "exec_reading", ty = ExecReading, init = "spec,root,out,stamp")]
pub struct Custodian;

/// `custodian exec-reading --spec SPEC --root ROOT --out DIR|- --stamp STAMP`: one
/// reading, written under `DIR` (or printed, `-`), its summary the last line.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "exec-reading")]
#[exit_status(code = 0, word = "read")]
#[exit_status(code = 1, word = "refused")]
pub struct ExecReading {
    #[builder(init_required, into)]
    #[arg(kv = "--spec")]
    pub spec: OsString,
    #[builder(init_required, into)]
    #[arg(kv = "--root")]
    pub root: OsString,
    #[builder(init_required, into)]
    #[arg(kv = "--out")]
    pub out: OsString,
    #[builder(init_required, into)]
    #[arg(kv = "--stamp")]
    pub stamp: OsString,
    /// The job the reading is part of: declared routes only.
    #[builder(into)]
    #[arg(kv = "--job")]
    pub job: Option<OsString>,
}

/// cargo.
#[derive(CompositeShell, Debug, Clone, Copy, Default)]
#[shell(program = "cargo")]
#[composite(command = "test", method = "test", ty = CargoTest, init = "manifest_path,package")]
#[composite(command = "check", method = "check", ty = CargoCheck, init = "manifest_path")]
#[composite(command = "expand", method = "expand", ty = CargoExpand, init = "manifest_path,package")]
#[composite(command = "generate-lockfile", method = "generate_lockfile", ty = CargoGenerateLockfile, init = "manifest_path")]
pub struct Cargo;

/// `cargo generate-lockfile --manifest-path M --offline`: a crate's lockfile from the registry already on disk.
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "generate-lockfile")]
#[exit_status(code = 0, word = "locked")]
pub struct CargoGenerateLockfile {
    #[builder(init_required, into)]
    #[arg(kv = "--manifest-path")]
    pub manifest_path: OsString,
    #[arg(flag = "--offline")]
    pub offline: bool,
}

/// A crate's lockfile written from the registry on disk, as a declared line.
pub fn lock_crate(dir: &Path) -> Result<()> {
    let l = Cargo::generate_lockfile(dir.join("Cargo.toml").as_os_str()).offline();
    let walked = Plan::new("cargo-generate-lockfile").then(line("lock", &l)?).walk(Watch::Quiet, |_, _| {})?;
    match walked.first() {
        Some(w) if w.code == Some(0) => Ok(()),
        Some(w) => Err(ReadingRefusal::LineRefused.refuse(format!("cargo generate-lockfile: exit {:?}: {}", w.code, w.stderr.trim()))),
        None => Err(ReadingRefusal::LineNotStarted.refuse("cargo generate-lockfile")),
    }
}

/// `cargo expand --manifest-path M -p PACKAGE --lib --ugly [ITEM]`: a crate's code after every macro is expanded,
/// as rustc expands it (cargo-expand's subcommand).
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "expand")]
#[exit_status(code = 0, word = "expanded")]
#[exit_status(code = 101, word = "did not build")]
pub struct CargoExpand {
    #[builder(init_required, into)]
    #[arg(kv = "--manifest-path")]
    pub manifest_path: OsString,
    #[builder(init_required, into)]
    #[arg(kv = "-p")]
    pub package: OsString,
    #[arg(flag = "--lib")]
    pub lib: bool,
    #[arg(flag = "--tests")]
    pub tests: bool,
    #[arg(flag = "--ugly")]
    pub ugly: bool,
    /// One item's path inside the crate (`module::Type`), or the whole crate.
    #[arg(positional)]
    pub item: Vec<OsString>,
}

/// `cargo expand` of a crate's library (the whole of it, or one item), run as a declared line: what it printed.
pub fn expand_crate(dir: &Path, package: &str) -> Result<String> {
    expand_item(dir, package, None)
}

/// `cargo expand` of one item of a crate's library (`None`: the whole library), run as a declared line.
pub fn expand_item(dir: &Path, package: &str, item: Option<&str>) -> Result<String> {
    let mut l = Cargo::expand(dir.join("Cargo.toml").as_os_str(), package).lib().ugly();
    if let Some(i) = item {
        l = l.item([i]);
    }
    let walked = Plan::new("cargo-expand").then(line("expand", &l)?).walk(Watch::Quiet, |_, _| {})?;
    let w = walked.into_iter().next().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse("cargo expand"))?;
    if w.code != Some(0) {
        return Err(ReadingRefusal::LineRefused.refuse(format!("cargo expand -p {package}: exit {:?}: {}", w.code, w.stderr.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n"))));
    }
    Ok(w.stdout)
}

/// `cargo test --manifest-path M -p PACKAGE [--lib --bins | --test T | --bin B] -- FILTER... --exact`.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "test")]
#[exit_status(code = 0, word = "passed")]
#[exit_status(code = 101, word = "tests failed")]
#[validate(with = "cargo_test_valid")]
pub struct CargoTest {
    #[builder(init_required, into)]
    #[arg(kv = "--manifest-path")]
    pub manifest_path: OsString,
    #[builder(init_required, into)]
    #[arg(kv = "-p")]
    pub package: OsString,
    #[arg(flag = "--lib")]
    pub lib: bool,
    #[arg(flag = "--bins")]
    pub bins: bool,
    #[builder(into)]
    #[arg(kv = "--test")]
    pub test: Option<OsString>,
    /// One binary's unit tests.
    #[builder(into)]
    #[arg(kv = "--bin")]
    pub bin: Option<OsString>,
    /// `--`, the filters, then `--exact`.
    #[arg(positional)]
    pub rest: Vec<OsString>,
}

fn cargo_test_valid(line: &CargoTest) -> CommandValidationResult {
    if !line.rest.is_empty() && line.rest.first().map(OsString::as_os_str) != Some(OsStr::new("--")) {
        return Err(xccute_contract::CommandValidationError::structural("the filters follow `--`").with_field("rest"));
    }
    Ok(())
}

/// `cargo check --manifest-path M [--workspace] [--all-targets] [--offline] [--message-format F]`: whether the code
/// compiles, the first error in what it printed (the exercise's compile step).
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "check")]
#[exit_status(code = 0, word = "compiles")]
#[exit_status(code = 101, word = "does not compile")]
pub struct CargoCheck {
    #[builder(init_required, into)]
    #[arg(kv = "--manifest-path")]
    pub manifest_path: OsString,
    #[arg(flag = "--workspace")]
    pub workspace: bool,
    #[arg(flag = "--all-targets")]
    pub all_targets: bool,
    #[arg(flag = "--offline")]
    pub offline: bool,
    #[builder(into)]
    #[arg(kv = "--message-format")]
    pub message_format: Option<OsString>,
}

/// `cargo check` over every crate and target of the workspace at `root`, offline, one short line per message.
pub fn cargo_check(root: &Path) -> CargoCheckRooted<Cargo> {
    Cargo::check(root.join("Cargo.toml").as_os_str()).workspace().all_targets().offline().message_format("short")
}

/// `cargo test` for one package and target, exactly these tests (all of them
/// when `filters` is empty).
pub fn cargo_test(root: &Path, package: &str, test_target: Option<&str>, filters: &[String]) -> CargoTestRooted<Cargo> {
    let mut line = Cargo::test(root.join("Cargo.toml").as_os_str(), package);
    line = match test_target {
        Some(t) => line.test(t),
        None => line.lib().bins(),
    };
    if !filters.is_empty() {
        let mut rest: Vec<OsString> = vec!["--".into()];
        rest.extend(filters.iter().map(OsString::from));
        rest.push("--exact".into());
        line = line.rest(rest);
    }
    line
}

/// git.
#[derive(CompositeShell, Debug, Clone, Copy, Default)]
#[shell(program = "git")]
#[composite(command = "", method = "in_dir", ty = GitIn, init = "dir")]
pub struct Git;

/// `git -C DIR WORD...`.
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(no_segment)]
#[exit_status(code = 0, word = "done")]
pub struct GitIn {
    #[builder(init_required, into)]
    #[arg(kv = "-C")]
    pub dir: OsString,
    #[arg(positional)]
    pub words: Vec<OsString>,
}

/// `git -C ROOT diff --name-only REF`: the files a ref's diff touches.
pub fn git_diff_names(root: &Path, git_ref: &str) -> GitInRooted<Git> {
    Git::in_dir(root.as_os_str()).words(["diff", "--name-only", git_ref])
}

/// `git -C ROOT rev-parse HEAD`: the commit the tree is at.
pub fn git_head(root: &Path) -> GitInRooted<Git> {
    Git::in_dir(root.as_os_str()).words(["rev-parse", "HEAD"])
}

/// code_passenger's program.
#[derive(CompositeShell, Debug, Clone, Copy, Default)]
#[shell(program = "code-passenger")]
#[composite(command = "", method = "at_root", ty = PassengerAt, init = "root")]
pub struct CodePassenger;

/// `code-passenger --root ROOT WORD...` (its `gate --spec S [--write]`).
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(no_segment)]
#[exit_status(code = 0, word = "nothing to do or converged")]
#[exit_status(code = 1, word = "work found or the oracle gave up")]
#[exit_status(code = 2, word = "a person has to decide")]
pub struct PassengerAt {
    #[builder(init_required, into)]
    #[arg(kv = "--root")]
    pub root: OsString,
    #[arg(positional)]
    pub words: Vec<OsString>,
}

/// `code-passenger --root ROOT gate --spec SPEC [--write]`.
pub fn passenger_gate(root: &Path, spec: &Path, write: bool) -> PassengerAtRooted<CodePassenger> {
    let mut words: Vec<OsString> = vec!["gate".into(), "--spec".into(), spec.as_os_str().to_os_string()];
    if write {
        words.push("--write".into());
    }
    CodePassenger::at_root(root.as_os_str()).words(words)
}

/// The cheap model's command line, `claude` (the exercise's intents become change specs through it).
#[derive(CompositeShell, Debug, Clone, Copy, Default)]
#[shell(program = "claude")]
#[composite(command = "-p", method = "print", ty = ModelPrint, init = "model,prompt")]
pub struct ModelCli;

/// `claude -p --model MODEL PROMPT`: one answer to one prompt, printed, nothing else done.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "-p")]
#[exit_status(code = 0, word = "answered")]
#[exit_status(code = 1, word = "not answered")]
pub struct ModelPrint {
    #[builder(init_required, into)]
    #[arg(kv = "--model")]
    pub model: OsString,
    #[builder(init_required, into)]
    #[arg(positional)]
    pub prompt: OsString,
}

/// Whether a program is on PATH (a bounded job needs code-passenger, its writer).
pub fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

/// A declared line run from the copy of its program at a path of its own (the
/// running custodian, a built binary), its words and rules the line's.
#[derive(Debug, Clone)]
pub struct At<C> {
    program: OsString,
    line: C,
}

pub fn at<C>(program: impl AsRef<OsStr>, line: C) -> At<C> {
    At { program: program.as_ref().to_os_string(), line }
}

impl<C: CompositeShellCommand> CompositeShellCommand for At<C> {
    fn program(&self) -> &OsStr {
        &self.program
    }

    fn push_args(&self, out: &mut Vec<OsString>) {
        self.line.push_args(out);
    }

    fn ok_codes(&self) -> &'static [i32] {
        self.line.ok_codes()
    }

    fn exit_statuses(&self) -> &'static [xccute_contract::ExitStatusWord] {
        self.line.exit_statuses()
    }
}

impl<C: ValidatedCommand> ValidatedCommand for At<C> {
    fn validate(&self) -> CommandValidationResult {
        self.line.validate()
    }
}

/// A line ready to run: checked, previewed, its statuses as policy.
#[derive(Debug, Clone)]
pub struct Line {
    pub id: String,
    pub preview: CommandPreview,
    pub policy: ExitStatusPolicy,
    /// The codes the line's program accepts.
    pub ok_codes: &'static [i32],
    /// Extra environment for this line.
    pub env: Vec<(String, String)>,
}

/// A declared command's rules checked, then its preview and policy.
pub fn line(id: impl Into<String>, command: &(impl CompositeShellCommand + ValidatedCommand)) -> Result<Line> {
    command.validate().map_err(|why| ReadingRefusal::LineRefused.refuse(format!("{}: {why}", command.build_display())))?;
    Ok(Line { id: id.into(), preview: command.preview(), policy: ExitStatusPolicy::of(command), ok_codes: command.ok_codes(), env: Vec::new() })
}

impl Line {
    /// The line with every status going on to the next line, each still with
    /// its word as the reason (a test crate that fails does not stop the others).
    pub fn going_on(mut self) -> Line {
        self.policy.rules.iter_mut().for_each(|r| r.disposition = ExitDisposition::Continue);
        self.policy = self.policy.with_rule(xccute_runtime::ExitStatusRule { code: None, accepted: None, disposition: ExitDisposition::Continue, reason: "went on".into() });
        self
    }
}

/// How a line's output is seen while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watch {
    /// Kept, nothing shown.
    Quiet,
    /// Passed through as it comes, and kept.
    Stream,
    /// Each output line handed to the callback as it comes, and kept.
    Lines,
}

/// One line walked.
#[derive(Debug, Clone)]
pub struct Walked {
    pub operation: RuntimeOperation,
    pub receipt: RuntimeOperationReceipt,
    pub decision: ExitStatusDecision,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub millis: i64,
}

/// A plan of declared lines and the policy each one's status is read by.
pub struct Plan {
    pub plan: RuntimeOperationPlan,
    lines: Vec<Line>,
}

impl Plan {
    pub fn new(id: impl Into<String>) -> Plan {
        Plan { plan: RuntimeOperationPlan::new(id), lines: Vec::new() }
    }

    pub fn then(mut self, l: Line) -> Plan {
        self.plan = self.plan.then(RuntimeOperation::from_preview(l.id.clone(), &l.preview));
        self.lines.push(l);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The plan's first line answered without running it, as if it had exited
    /// `code` with `stdout` (the reading answered in this process): the same
    /// operation, the same decision by the line's policy, the same receipt.
    pub fn answered(&self, code: i32, stdout: String, stderr: String, millis: i64) -> Result<Walked> {
        let op = self.plan.operations.first().cloned().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse(format!("{}: an empty plan", self.plan.plan_id)))?;
        let l = self.lines.iter().find(|l| l.id == op.logical_id).expect("a plan's line is its own");
        let accepted = l.ok_codes.contains(&code);
        let decision = l.policy.decide(RuntimeExitStatus::new(Some(code), accepted));
        let receipt = RuntimeOperationReceipt::new(&self.plan, &op, decision.clone());
        Ok(Walked { operation: op, receipt, decision, code: Some(code), stdout, stderr, millis })
    }

    /// The plan walked from its first line; each line's status decided by its
    /// policy, the plan's transition naming the next.
    pub fn walk(&self, watch: Watch, mut on_line: impl FnMut(&str, &str)) -> Result<Vec<Walked>> {
        let mut walked = Vec::new();
        let mut next = self.plan.operations.first().map(|op| op.logical_id.clone());
        while let Some(id) = next {
            if walked.len() > self.plan.operations.len() * 4 {
                return Err(ReadingRefusal::LineRefused.refuse(format!("{}: the plan walked {} lines and did not end", self.plan.plan_id, walked.len())));
            }
            let op = self.plan.operation_by_logical_id(&id).cloned().ok_or_else(|| ReadingRefusal::LineRefused.refuse(format!("{}: no line {id}", self.plan.plan_id)))?;
            let l = self.lines.iter().find(|l| l.id == id).expect("a plan's line is its own");
            let started = Instant::now();
            let mut cmd = std::process::Command::new(&op.program);
            cmd.args(&op.argv);
            for (k, v) in &l.env {
                cmd.env(k, v);
            }
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = cmd.spawn().map_err(|e| ReadingRefusal::LineNotStarted.refuse(format!("{}: {e}", op.display)))?;
            let mut err_pipe = child.stderr.take();
            let err_thread = std::thread::spawn(move || {
                let mut s = String::new();
                if let Some(p) = err_pipe.as_mut() {
                    let _ = p.read_to_string(&mut s);
                }
                s
            });
            let mut stdout = String::new();
            if let Some(out) = child.stdout.take() {
                for text in BufReader::new(out).lines().map_while(|l| l.ok()) {
                    match watch {
                        Watch::Stream => println!("{text}"),
                        Watch::Lines => on_line(&id, &text),
                        Watch::Quiet => {}
                    }
                    stdout.push_str(&text);
                    stdout.push('\n');
                }
            }
            let status = child.wait().map_err(|e| ReadingRefusal::LineNotStarted.refuse(format!("{}: {e}", op.display)))?;
            let stderr = err_thread.join().unwrap_or_default();
            if watch == Watch::Stream && !stderr.is_empty() {
                eprint!("{stderr}");
            }
            let code = status.code();
            let accepted = code.is_some_and(|c| l.ok_codes.contains(&c));
            let decision = l.policy.decide(RuntimeExitStatus::new(code, accepted));
            let transition = self.plan.transition_after(&id, &decision).map_err(|e| ReadingRefusal::LineRefused.refuse(format!("{}: {e:?}", self.plan.plan_id)))?;
            next = if transition.disposition == ExitDisposition::Stop { None } else { transition.next_logical_id };
            let receipt = RuntimeOperationReceipt::new(&self.plan, &op, decision.clone());
            walked.push(Walked { operation: op, receipt, decision, code, stdout, stderr, millis: started.elapsed().as_millis() as i64 });
        }
        Ok(walked)
    }
}
