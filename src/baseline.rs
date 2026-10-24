// Copyright © 2026 1o. All rights reserved.

//! A test that fails the same way at the job's base is not the job's failure.
//!
//! The base is the commit the job opened at (`job start` records it in the job
//! file as `base_commit`). At close, every test the job ran whose last outcome
//! is `FAILED` is run again on the job's tree, for its first failing line, and
//! looked up at the base: in the job's baseline when `job start` ran the tests
//! the target reaches at open (`baseline true` in the job file: kept in
//! `data/baseline.json`), else run on a scratch worktree of the repository at
//! the base commit (outside the project, kept for the next close at the same
//! commit). A test that fails at the base with the same first failing line is
//! `pre-existing`: the closing receipt names it with that line, and it does not
//! make the job `failed_test`. One that passes at the base, fails there with
//! another line, or passed when run again on the job's tree, is the job's.
//!
//! The first failing line is the panic's message (the line after `thread '..'
//! panicked at <file>:<line>:<col>:`), not its place: a job that edits the
//! file a failing test sits in moves the place, never the message. The trees'
//! own paths in it are written `<root>`, and a test's scratch folder under the
//! system's temporary directory `<temp>`, so the job's tree and the base's read
//! the same.

use crate::exec::{self, Plan, Watch};
use crate::reading::refusal::ReadingRefusal;
use crate::runs::{keep, receipt_of, Ctx, InJob};
use crate::store::{write_whole, TestRow};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const BASELINE_SCHEMA: &str = "custodian.baseline.v1";

/// The baseline's file in a job folder's data.
pub const BASELINE_FILE: &str = "baseline.json";

/// One test's outcome on a tree, with its first failing line when it failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub package: String,
    /// The target word the journal keeps: `lib`, `bin:<name>`, `test:<name>` (or a bare test target's name),
    /// `unit` and `lib+bins` (the library and binaries), `all`.
    pub target: String,
    pub test: String,
    /// cargo's word: `ok`, `FAILED`, `ignored`; `not run` when its line ran no such test.
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_line: Option<String>,
}

/// The tests run at open, on the job's base.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub schema: String,
    pub commit: Option<String>,
    pub outcomes: Vec<Outcome>,
}

/// A test the job ran that fails the same way at its base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreExisting {
    pub package: String,
    pub target: String,
    pub test: String,
    pub first_line: String,
    /// `baseline` (run at open) or `base <commit>` (run at close on the base's worktree).
    pub from: String,
}

/// What the close found of the job's failing tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Judged {
    pub pre_existing: Vec<PreExisting>,
    /// The job's own failures, each `package target test: why it is the job's`.
    pub own: Vec<String>,
    pub said: Vec<String>,
}

/// Whether two target words name one target: `test:gate` and `gate` (the journal keeps a test target bare), and
/// `lib`, `unit` and `lib+bins` (the library's unit tests).
fn same_target(a: &str, b: &str) -> bool {
    let norm = |t: &str| -> (String, crate::testrun::Configuration) {
        let (word, config) = crate::testrun::Configuration::of_target(t);
        let w = match word.as_str() {
            "unit" | "lib+bins" => "lib".to_string(),
            t => t.strip_prefix("test:").unwrap_or(t).to_string(),
        };
        (w, config)
    };
    norm(a) == norm(b)
}

/// The first failing line of `test` in a cargo test run's output: the panic's message, the tree's root written
/// `<root>`; else the first line of what the test printed.
pub fn first_failing_line(stdout: &str, test: &str, root: &Path) -> Option<String> {
    let head = format!("---- {test} stdout ----");
    let mut lines = stdout.lines().skip_while(|l| l.trim() != head).skip(1);
    let mut first = None;
    while let Some(l) = lines.next() {
        if l.starts_with("---- ") || l.trim() == "failures:" {
            break;
        }
        let t = l.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("thread '") && t.contains("panicked at") {
            // `panicked at 'message', src/x.rs:1:2` (older), else the message is the next line
            let after = t.split_once("panicked at ").map(|(_, r)| r).unwrap_or("");
            if let Some(m) = after.strip_prefix('\'').and_then(|m| m.rsplit_once("', ")).map(|(m, _)| m) {
                first = Some(m.to_string());
            } else {
                first = lines.by_ref().map(str::trim).find(|x| !x.is_empty()).map(str::to_string);
            }
            break;
        }
        if first.is_none() {
            first = Some(t.to_string());
        }
    }
    let root = root.to_string_lossy().into_owned();
    let canon = std::fs::canonicalize(&root).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    first.map(|f| {
        let f = if canon.is_empty() { f } else { f.replace(&canon, "<root>") };
        temp_written(&f.replace(&root, "<root>"))
    })
}

/// A test's own scratch folder written `<temp>`: the system's temporary directory and the one name under it
/// (`.tmpY3a7Dp`, made fresh by each run), so the same failure in a scratch folder reads the same on the job's tree
/// and at the base.
fn temp_written(line: &str) -> String {
    let temp = std::env::temp_dir();
    let mut prefixes: Vec<String> = [Some(temp.clone()), std::fs::canonicalize(&temp).ok()].into_iter().flatten().map(|p| p.to_string_lossy().trim_end_matches('/').to_string()).filter(|p| p.len() > 1).collect();
    // the longest first: `/private/var/folders/..` holds `/var/folders/..`
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    prefixes.dedup();
    let mut out = line.to_string();
    for prefix in &prefixes {
        let mut written = String::new();
        let mut rest = out.as_str();
        while let Some(at) = rest.find(&format!("{prefix}/")) {
            written.push_str(&rest[..at]);
            let after = &rest[at + prefix.len() + 1..];
            let name = after.find(|c: char| c == '/' || c.is_whitespace() || "'\"`:),".contains(c)).unwrap_or(after.len());
            written.push_str("<temp>");
            rest = &after[name..];
        }
        written.push_str(rest);
        out = written;
    }
    out
}

/// `test <name> ... ok|FAILED|ignored` from libtest's output.
fn outcomes(stdout: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("test ")?;
            let (name, outcome) = rest.rsplit_once(" ... ")?;
            let outcome = outcome.split_whitespace().next()?;
            matches!(outcome, "ok" | "FAILED" | "ignored").then(|| (name.trim().to_string(), outcome.to_string()))
        })
        .collect()
}

/// The packages with a library target, from cargo metadata of the tree at `root`.
fn libraries(root: &Path) -> Vec<String> {
    crate::klepto_builder(root, &crate::ScanOptions::graph()).map(|b| b.packages().values().filter(|p| p.targets.iter().any(|t| t.kind == "lib" || t.kind == "proc-macro" || t.kind == "rlib")).map(|p| p.name.clone()).collect()).unwrap_or_default()
}

/// The cargo test line that runs exactly `tests` of one package and target word, in the tree at `root`.
fn line_for(root: &Path, package: &str, target: &str, tests: &[String], libs: &[String]) -> exec::CargoTestRooted<exec::Cargo> {
    // a line run under a feature configuration carries it in its target word: run again the same way
    let (word, config) = crate::testrun::Configuration::of_target(target);
    let target = word.as_str();
    let mut line = config.on(exec::Cargo::test(root.join("Cargo.toml").as_os_str(), package));
    let has_lib = libs.iter().any(|l| *l == package.replace('-', "_"));
    line = match target.split_once(':') {
        Some(("bin", b)) => line.bin(b),
        Some(("test", t)) => line.test(t),
        _ => match target {
            "all" => line,
            "lib" => line.lib(),
            // the library and the binaries: never `--lib` for a package that has none
            "unit" | "lib+bins" => {
                if has_lib {
                    line.lib().bins()
                } else {
                    line.bins()
                }
            }
            t => line.test(t),
        },
    };
    if !tests.is_empty() {
        let mut rest: Vec<OsString> = vec!["--".into()];
        rest.extend(tests.iter().map(OsString::from));
        rest.push("--exact".into());
        line = line.rest(rest);
    }
    line
}

/// Runs `tests` (package, target word, test) in the tree at `tree`, one line per package and target, each with a
/// receipt named `command` under the job (never `test`: these runs are not the job's own tests); their outcomes and
/// first failing lines.
pub fn run_at(ctx: &Ctx, tree: &Path, tests: &[(String, String, String)], command: &str, job: Option<&InJob>) -> Result<Vec<Outcome>> {
    let mut by: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (p, t, n) in tests {
        by.entry((p.clone(), t.clone())).or_default().push(n.clone());
    }
    let libs = libraries(tree);
    let mut plan = Plan::new(format!("{command}:{}", tree.display()));
    let keys: Vec<(String, String)> = by.keys().cloned().collect();
    for (i, (p, t)) in keys.iter().enumerate() {
        let line = line_for(tree, p, t, &by[&(p.clone(), t.clone())], &libs);
        plan = plan.then(exec::line(format!("{command}-{i}:{p}"), &line)?.going_on());
    }
    let walked = plan.walk(Watch::Quiet, |_, _| {})?;
    let mut out = Vec::new();
    for (w, (p, t)) in walked.iter().zip(&keys) {
        if let Some(store) = &ctx.store {
            keep(store, receipt_of(&plan, w, command, &format!("{p} {t}"), None, None, None, None, vec![], job))?;
        }
        let found: BTreeMap<String, String> = outcomes(&w.stdout).into_iter().collect();
        for n in &by[&(p.clone(), t.clone())] {
            let outcome = found.get(n).cloned().unwrap_or_else(|| "not run".into());
            let first_line = (outcome == "FAILED").then(|| first_failing_line(&w.stdout, n, tree)).flatten();
            out.push(Outcome { package: p.clone(), target: t.clone(), test: n.clone(), outcome, first_line });
        }
    }
    Ok(out)
}

/// The commit the tree at `root` is at (`git rev-parse HEAD`, the kit's declared git line), when it is a repository.
pub fn head_commit(root: &Path) -> Option<String> {
    let plan = Plan::new("base-commit").then(exec::line("git-head", &exec::git_head(root)).ok()?);
    let w = plan.walk(Watch::Quiet, |_, _| {}).ok()?;
    let w = w.first()?;
    (w.code == Some(0)).then(|| w.stdout.trim().to_string()).filter(|c| !c.is_empty())
}

/// A worktree of the repository at `commit`, outside the project: made with `git worktree add --detach` the first
/// time, reused after.
pub fn base_tree(root: &Path, commit: &str) -> Result<PathBuf> {
    let key = crate::digest::short(&crate::digest::of(root.to_string_lossy().as_ref()), 12);
    let dir = std::env::temp_dir().join("custodian-base").join(format!("{key}-{}", &commit[..commit.len().min(12)]));
    if dir.join("Cargo.toml").is_file() && head_commit(&dir).as_deref() == Some(commit) {
        return Ok(dir);
    }
    if let Some(p) = dir.parent() {
        std::fs::create_dir_all(p).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", p.display())))?;
    }
    let line = exec::Git::in_dir(root.as_os_str()).words([OsString::from("worktree"), "add".into(), "--detach".into(), dir.as_os_str().to_os_string(), commit.into()]);
    let plan = Plan::new("base-tree").then(exec::line("git-worktree", &line)?);
    let w = plan.walk(Watch::Quiet, |_, _| {})?;
    let w = w.first().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse("git worktree add"))?;
    if w.code != Some(0) {
        return Err(ReadingRefusal::LineRefused.refuse(format!("git worktree add {} {commit}: {}", dir.display(), w.stderr.trim())));
    }
    Ok(dir)
}

/// The baseline at open: the tests the job's target reaches, run on the tree as it is (the base), kept in the job's
/// data. `tests` are (package, target word, test).
pub fn record_at_open(ctx: &Ctx, folder: &Path, in_job: &InJob, commit: Option<String>, tests: &[(String, String, String)]) -> Result<Baseline> {
    let outcomes = run_at(ctx, &ctx.root, tests, "baseline", Some(in_job))?;
    let b = Baseline { schema: BASELINE_SCHEMA.into(), commit, outcomes };
    write_whole(&folder.join(crate::jobs::JOB_DATA).join(BASELINE_FILE), (serde_json::to_string_pretty(&b)? + "\n").as_bytes())?;
    Ok(b)
}

/// The baseline a job folder keeps, when `job start` recorded one.
pub fn recorded(folder: &Path) -> Option<Baseline> {
    std::fs::read_to_string(folder.join(crate::jobs::JOB_DATA).join(BASELINE_FILE)).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// Two first failing lines are the same failure when they differ only in cargo's build hashes (`leg-eb350b6bf5583434`
/// in a path under the build folder): the base and the job's tree build the same test under another hash.
pub fn same_first_line(a: Option<&str>, b: Option<&str>) -> bool {
    a.map(masked) == b.map(masked)
}

/// A line with every `-<16 hex digits>` (cargo's metadata hash) written `-#`.
fn masked(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < bytes.len() {
        let hash = bytes[i] == b'-' && i + 17 <= bytes.len() && bytes[i + 1..i + 17].iter().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) && bytes.get(i + 17).is_none_or(|c| !c.is_ascii_alphanumeric());
        if hash {
            out.push_str("-#");
            i += 17;
        } else {
            let ch = line[i..].chars().next().unwrap_or_default();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// The job's failing tests judged against its base: each failing test's last outcome in the job, run again on the
/// job's tree for its first failing line, then looked up in the baseline, else run at the base commit.
pub fn judge(ctx: &Ctx, folder: &Path, in_job: &InJob, base_commit: Option<&str>, tests: &[TestRow]) -> Result<Judged> {
    let mut last: BTreeMap<(String, String, String), &TestRow> = BTreeMap::new();
    for t in tests {
        last.insert((t.package.clone(), t.target.clone(), t.test.clone()), t);
    }
    let failing: Vec<(String, String, String)> = last.into_iter().filter(|(_, t)| matches!(t.outcome.as_str(), "FAILED" | "failed" | "fail")).map(|(k, _)| k).collect();
    let mut j = Judged::default();
    if failing.is_empty() {
        return Ok(j);
    }
    // the job's own first lines
    let here = run_at(ctx, &ctx.root, &failing, "rerun", Some(in_job))?;
    let baseline = recorded(folder);
    let in_baseline = |p: &str, t: &str, n: &str| baseline.as_ref().and_then(|b| b.outcomes.iter().find(|o| o.package == p && same_target(&o.target, t) && o.test == n).cloned());
    let missing: Vec<(String, String, String)> = failing.iter().filter(|(p, t, n)| in_baseline(p, t, n).is_none()).cloned().collect();
    let at_base: Vec<Outcome> = match (missing.is_empty(), base_commit) {
        (true, _) => Vec::new(),
        (false, Some(c)) => {
            let tree = base_tree(&ctx.root, c)?;
            j.said.push(format!("base: {} failing tests run at {c} in {}", missing.len(), tree.display()));
            run_at(ctx, &tree, &missing, "base-test", Some(in_job))?
        }
        (false, None) => {
            j.said.push("base: the job has no base commit (not a git repository, or opened before custodian_kit 0.3.3): every failing test is the job's".into());
            Vec::new()
        }
    };
    for (p, t, n) in &failing {
        let mine = here.iter().find(|o| &o.package == p && &o.target == t && &o.test == n);
        let (base, from) = match in_baseline(p, t, n) {
            Some(b) => (Some(b), "baseline".to_string()),
            None => (at_base.iter().find(|o| &o.package == p && &o.target == t && &o.test == n).cloned(), format!("base {}", base_commit.unwrap_or("-"))),
        };
        let why = match (mine, &base) {
            (Some(m), _) if m.outcome != "FAILED" => format!("passed when run again ({}): flaky, and the job's", m.outcome),
            (_, None) => "not run at the base".to_string(),
            (_, Some(b)) if b.outcome != "FAILED" => format!("{} at the base", b.outcome),
            (Some(m), Some(b)) if same_first_line(m.first_line.as_deref(), b.first_line.as_deref()) => {
                j.pre_existing.push(PreExisting { package: p.clone(), target: t.clone(), test: n.clone(), first_line: m.first_line.clone().unwrap_or_default(), from });
                continue;
            }
            (Some(m), Some(b)) => format!("fails at the base another way: `{}` there, `{}` here", b.first_line.clone().unwrap_or_default(), m.first_line.clone().unwrap_or_default()),
            (None, Some(_)) => "not run again on the job's tree".to_string(),
        };
        j.own.push(format!("{p} {t} {n}: {why}"));
    }
    j.said.push(format!("failing tests: {} pre-existing (the same first failing line at the base), {} the job's", j.pre_existing.len(), j.own.len()));
    Ok(j)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_failing_line_is_the_panics_message_with_the_root_written_root() {
        let out = "running 2 tests\ntest a::x ... FAILED\ntest a::y ... ok\n\nfailures:\n\n---- a::x stdout ----\n\nthread 'a::x' panicked at src/lib.rs:10:9:\nno such file /w/tree/etc/x\nnote: run with `RUST_BACKTRACE=1`\n\n\nfailures:\n    a::x\n";
        assert_eq!(first_failing_line(out, "a::x", Path::new("/w/tree")).as_deref(), Some("no such file <root>/etc/x"));
        let old = "---- b stdout ----\nthread 'b' panicked at 'boom', src/lib.rs:3:1\n";
        assert_eq!(first_failing_line(old, "b", Path::new("/w")).as_deref(), Some("boom"));
        // a test's fresh scratch folder reads the same on both trees
        let tmp = std::env::temp_dir().join(".tmpY3a7Dp").join("ended");
        let other = std::env::temp_dir().join(".tmpbLm43K").join("ended");
        let says = |p: &Path| format!("---- c stdout ----\nthread 'c' panicked at src/lib.rs:1:1:\nchown root:root {} failed\n", p.display());
        assert_eq!(first_failing_line(&says(&tmp), "c", Path::new("/w")), first_failing_line(&says(&other), "c", Path::new("/w")));
        assert_eq!(first_failing_line(&says(&tmp), "c", Path::new("/w")).as_deref(), Some("chown root:root <temp>/ended failed"));
        assert_eq!(outcomes("test a::x ... FAILED\ntest result: FAILED. 1 passed"), vec![("a::x".to_string(), "FAILED".to_string())]);
    }
}

#[cfg(test)]
mod first_line_tests {
    #[test]
    fn a_first_line_that_differs_only_in_a_build_hash_is_the_same_failure() {
        let here = "no daemon binaries beside /t/debug/deps/leg-c91e7a82f1e05340 (build them)";
        let base = "no daemon binaries beside /t/debug/deps/leg-eb350b6bf5583434 (build them)";
        assert!(super::same_first_line(Some(here), Some(base)));
        assert!(!super::same_first_line(Some(here), Some("assertion failed: x == 2")));
        assert!(!super::same_first_line(Some("left: 1"), Some("left: 2")));
    }
}

