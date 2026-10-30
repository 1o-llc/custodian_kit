// Copyright © 2026 1o. All rights reserved.

//! A format or lint finding that is in the base already is not the candidate's.
//!
//! A historical specimen may carry formatting or lint findings of its own. The candidate is judged by the findings it
//! adds, not by every finding the tree holds: the same check runs on the base and on the candidate, each finding is
//! keyed by what it says and the code it is about, never by its line number (an edit above it moves the line, never
//! the finding), and the candidate's findings are counted against the base's. One the base has as many times or more
//! is `inherited`; each one more is `introduced`. The test baseline ([`crate::baseline`]) judges a failing test the
//! same way, by its first failing line.
//!
//! The checks are declared lines ([`crate::exec::CargoFmt`], [`crate::exec::CargoClippy`]). Neither tree is written:
//! rustfmt's `--check` reads only, and clippy, which builds, runs on a copy of each tree, its build kept in the copy.

use crate::exec::{self, Cargo, Plan, Watch};
use crate::reading::refusal::ReadingRefusal;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const DELTA_SCHEMA: &str = "custodian.delta.v1";

/// The checks a delta runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `cargo fmt --check`: a place rustfmt would write otherwise.
    Fmt,
    /// `cargo clippy`: a warning or an error clippy or rustc gives.
    Clippy,
}

impl Kind {
    pub const ALL: &'static [Kind] = &[Kind::Fmt, Kind::Clippy];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Fmt => "fmt",
            Kind::Clippy => "clippy",
        }
    }
}

impl std::str::FromStr for Kind {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Kind, String> {
        Kind::ALL.iter().copied().find(|k| k.as_str() == s).ok_or_else(|| format!("`{s}` is not a delta kind: fmt, clippy"))
    }
}

/// One finding, keyed by its file (relative to its tree) and what it says.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Finding {
    pub file: String,
    /// fmt: the lines rustfmt would take out and put in; clippy: the lint's code (or its message) and the code it
    /// points at.
    pub key: String,
    /// Where the check saw it in its tree (information: never part of the key).
    pub line: Option<u32>,
}

/// A delta: the check on the base and on the candidate, and the candidate's findings split by whether the base has them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delta {
    pub schema: String,
    pub kind: Kind,
    /// The base and the candidate as given.
    pub base: String,
    pub candidate: String,
    /// The lines that ran, with their exit codes.
    pub lines: Vec<(String, Option<i32>)>,
    pub base_findings: usize,
    pub candidate_findings: usize,
    /// The candidate's findings the base has too.
    pub inherited: Vec<Finding>,
    /// The candidate's findings the base does not have (as many times).
    pub introduced: Vec<Finding>,
}

impl Delta {
    /// No finding introduced.
    pub fn clean(&self) -> bool {
        self.introduced.is_empty()
    }
}

/// The delta of `kind` between `base` and `candidate`: the check run on each, the candidate's findings counted against
/// the base's.
pub fn delta(kind: Kind, base: &Path, candidate: &Path) -> Result<Delta> {
    let mut lines = Vec::new();
    let (b, c) = match kind {
        Kind::Fmt => (fmt_findings(base, &mut lines)?, fmt_findings(candidate, &mut lines)?),
        Kind::Clippy => (clippy_findings(base, &mut lines)?, clippy_findings(candidate, &mut lines)?),
    };
    let mut left: BTreeMap<(String, String), usize> = BTreeMap::new();
    for f in &b {
        *left.entry((f.file.clone(), f.key.clone())).or_default() += 1;
    }
    let (mut inherited, mut introduced) = (Vec::new(), Vec::new());
    for f in &c {
        match left.get_mut(&(f.file.clone(), f.key.clone())) {
            Some(n) if *n > 0 => {
                *n -= 1;
                inherited.push(f.clone());
            }
            _ => introduced.push(f.clone()),
        }
    }
    Ok(Delta { schema: DELTA_SCHEMA.into(), kind, base: base.display().to_string(), candidate: candidate.display().to_string(), lines, base_findings: b.len(), candidate_findings: c.len(), inherited, introduced })
}

/// rustfmt's `--check` diffs in the tree at `root`, one finding per hunk: the lines it would take out and put in.
pub fn fmt_findings(root: &Path, lines: &mut Vec<(String, Option<i32>)>) -> Result<Vec<Finding>> {
    let l = Cargo::fmt(root.join("Cargo.toml").as_os_str()).all().rest(["--", "--check"]);
    let w = run("delta-fmt", exec::line("fmt", &l)?.in_dir(root), lines)?;
    read_fmt(root, w.code, &w.stdout, &w.stderr)
}

/// `cargo fmt --check`'s answer: exit 0 is a formatted tree, exit 1 a tree with diffs, each one a finding. An exit 1
/// with no diff in it is cargo or rustup refusing before rustfmt ran (no rustfmt for the toolchain): refused, never a
/// formatted tree.
fn read_fmt(root: &Path, code: Option<i32>, stdout: &str, stderr: &str) -> Result<Vec<Finding>> {
    let found = parse_fmt(root, stdout);
    match code {
        Some(0) => Ok(found),
        Some(1) if !found.is_empty() => Ok(found),
        _ => Err(ReadingRefusal::LineRefused.refuse(format!("cargo fmt --check in {}: exit {code:?}: {}", root.display(), stderr.trim()))),
    }
}

/// `cargo fmt --check`'s output read into findings: `Diff in <file>:<line>:` (or `at line <n>:`) opens a hunk, its
/// `-` and `+` lines are what it would change.
pub fn parse_fmt(root: &Path, out: &str) -> Vec<Finding> {
    let mut found = Vec::new();
    let mut cur: Option<(String, Option<u32>, Vec<String>)> = None;
    let close = |cur: &mut Option<(String, Option<u32>, Vec<String>)>, found: &mut Vec<Finding>| {
        if let Some((file, line, body)) = cur.take() {
            found.push(Finding { file, key: body.join("\n"), line });
        }
    };
    for l in out.lines() {
        if let Some(rest) = l.strip_prefix("Diff in ") {
            close(&mut cur, &mut found);
            let rest = rest.trim_end_matches(':');
            let (file, line) = match rest.split_once(" at line ") {
                Some((f, n)) => (f.to_string(), n.trim().parse().ok()),
                None => match rest.rsplit_once(':') {
                    Some((f, n)) if n.chars().all(|c| c.is_ascii_digit()) => (f.to_string(), n.parse().ok()),
                    _ => (rest.to_string(), None),
                },
            };
            cur = Some((relative(root, Path::new(&file)), line, Vec::new()));
        } else if let Some((_, _, body)) = cur.as_mut() {
            if let Some(t) = l.strip_prefix('-').or_else(|| l.strip_prefix('+')) {
                body.push(format!("{}{}", &l[..1], t.trim_end()));
            }
        }
    }
    close(&mut cur, &mut found);
    found.sort();
    found
}

/// clippy's findings in the tree at `root`, run on a copy of it so neither its build nor a lock cargo would write
/// lands in the tree.
pub fn clippy_findings(root: &Path, lines: &mut Vec<(String, Option<i32>)>) -> Result<Vec<Finding>> {
    let copy = scratch_copy(root)?;
    let l = Cargo::clippy(copy.join("Cargo.toml").as_os_str()).workspace().all_targets().message_format("json");
    let w = run("delta-clippy", exec::line("clippy", &l)?.in_dir(&copy), lines);
    let _ = std::fs::remove_dir_all(&copy);
    let w = w?;
    if w.code != Some(0) {
        let first = w.stderr.lines().find(|l| l.starts_with("error")).unwrap_or("").to_string();
        return Err(ReadingRefusal::LineRefused.refuse(format!("cargo clippy on a copy of {}: exit {:?}: {first}", root.display(), w.code)));
    }
    Ok(parse_clippy(&w.stdout))
}

/// cargo's JSON messages read into findings: each compiler message with a primary span, keyed by its lint code (or
/// its message when it has none) and the code its primary span covers.
pub fn parse_clippy(out: &str) -> Vec<Finding> {
    let mut found = Vec::new();
    for l in out.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(l) else { continue };
        if v["reason"] != "compiler-message" {
            continue;
        }
        let m = &v["message"];
        if !matches!(m["level"].as_str(), Some("warning") | Some("error")) {
            continue;
        }
        let Some(span) = m["spans"].as_array().and_then(|s| s.iter().find(|s| s["is_primary"] == true)) else { continue };
        let code = m["code"]["code"].as_str().map(str::to_string).unwrap_or_else(|| m["message"].as_str().unwrap_or("").to_string());
        let text: Vec<String> = span["text"].as_array().map(|t| t.iter().filter_map(|x| x["text"].as_str()).map(|s| s.trim().to_string()).collect()).unwrap_or_default();
        found.push(Finding { file: span["file_name"].as_str().unwrap_or("").replace('\\', "/"), key: format!("{code}: {}", text.join(" ")), line: span["line_start"].as_u64().map(|n| n as u32) });
    }
    found.sort();
    found
}

fn run(id: &str, l: exec::Line, lines: &mut Vec<(String, Option<i32>)>) -> Result<exec::Walked> {
    let shown = l.preview.display().to_string();
    let w = Plan::new(id).then(l).walk(Watch::Quiet, |_, _| {})?.into_iter().next().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse(&shown))?;
    lines.push((shown, w.code));
    Ok(w)
}

fn relative(root: &Path, p: &Path) -> String {
    let canon = |x: &Path| x.canonicalize().unwrap_or_else(|_| x.to_path_buf());
    let (r, c) = (canon(root), canon(p));
    c.strip_prefix(&r).or_else(|_| p.strip_prefix(root)).map(|x| x.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| p.display().to_string())
}

/// A copy of the tree at `root` under the system's temporary directory: every file but `target`, `.git`, `.custodian`
/// and `.passenger`; a symlink is not followed.
fn scratch_copy(root: &Path) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("custodian-delta-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    let skip = |n: &std::ffi::OsStr| matches!(n.to_str(), Some("target" | ".git" | ".custodian" | ".passenger"));
    for e in walkdir::WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| e.depth() == 0 || !(e.file_type().is_dir() && skip(e.file_name()))) {
        let e = e.map_err(|e| ReadingRefusal::Write.refuse(e))?;
        if !e.file_type().is_file() {
            continue;
        }
        let rel = e.path().strip_prefix(root).unwrap_or(e.path());
        let to = dir.join(rel);
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|x| ReadingRefusal::Write.refuse(format!("{}: {x}", d.display())))?;
        }
        std::fs::copy(e.path(), &to).map_err(|x| ReadingRefusal::Write.refuse(format!("{}: {x}", to.display())))?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fmt_hunk_is_keyed_by_what_it_changes_never_by_its_line() {
        let root = Path::new("/w");
        let a = parse_fmt(root, "Diff in /w/src/lib.rs:3:\n fn a() {\n-    let x=1;\n+    let x = 1;\n }\n");
        let b = parse_fmt(root, "Diff in /w/src/lib.rs at line 40:\n fn a() {\n-    let x=1;\n+    let x = 1;\n }\n");
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].file.as_str(), a[0].key.as_str(), a[0].line), ("src/lib.rs", "-    let x=1;\n+    let x = 1;", Some(3)));
        assert_eq!(a[0].key, b[0].key, "the line moved; the finding is the same");
    }

    #[test]
    fn a_fmt_that_never_ran_is_refused_never_a_formatted_tree() {
        let root = Path::new("/w");
        let no_rustfmt = "error: 'cargo-fmt' is not installed for the toolchain '1.89-x86_64-unknown-linux-gnu'.\n";
        let e = read_fmt(root, Some(1), "", no_rustfmt).unwrap_err().to_string();
        assert!(e.contains("exit Some(1)") && e.contains("'cargo-fmt' is not installed"), "{e}");
        assert!(read_fmt(root, Some(0), "", "").unwrap().is_empty());
        assert_eq!(read_fmt(root, Some(1), "Diff in /w/src/lib.rs:1:\n-fn a( ) {}\n+fn a() {}\n", "").unwrap().len(), 1);
        assert!(read_fmt(root, None, "", "").is_err());
    }

    #[test]
    fn a_clippy_message_is_keyed_by_its_lint_and_its_code() {
        let line = r#"{"reason":"compiler-message","message":{"level":"warning","message":"unneeded `return` statement","code":{"code":"clippy::needless_return"},"spans":[{"file_name":"src/lib.rs","line_start":7,"is_primary":true,"text":[{"text":"    return x;"}]}]}}"#;
        let f = parse_clippy(&format!("{line}\n{{\"reason\":\"build-finished\",\"success\":true}}\n"));
        assert_eq!(f, [Finding { file: "src/lib.rs".into(), key: "clippy::needless_return: return x;".into(), line: Some(7) }]);
    }
}
