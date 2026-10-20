// Copyright © 2026 1o. All rights reserved.

//! `why ROUTE`: who introduced a `crate:` or `decl:` route and for what, from
//! the records only, on one screen.
//!
//! - the task histories whose change records touch its files: the task and
//!   revision, its intent (the revision's last intent record), its result
//!   and its job (the task's `job-...` tag);
//! - the linkage records that bind it, and what else each binds;
//! - the readings about it (their receipts: a reading whose target is the
//!   route, its crate, or an item of the crate);
//! - git's first and last commit touching its files, read from a declared
//!   `git log` line ([`crate::exec::git_log_paths`]) the caller runs with a
//!   receipt and hands back as text ([`commits_of`]).
//!
//! A section with nothing in it says "no record": the records have no answer
//! there, and nothing is guessed.

use crate::linkage::LinkageRecord;
use crate::paths::{PathsTable, Resolving};
use crate::reading::refusal::ReadingRefusal;
use crate::store::Store;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use task_history::DiffRecordKind;

/// How many entries a section shows before "… and N more".
pub const SHOWN: usize = 6;

/// A reading about the route, as its receipt and the readings index name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingAbout {
    pub seq: i64,
    pub utc: String,
    pub name: String,
    pub kind: String,
    pub target: String,
    pub job: Option<String>,
}

/// A task revision whose change records touch the route's files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskTouch {
    pub task: String,
    pub revision: String,
    /// The revision's last intent record.
    pub intent: Option<String>,
    /// Its result record's status.
    pub result: Option<String>,
    /// The job the task was opened for (its `job-...` tag).
    pub job: Option<String>,
    /// The files of the route it changed, each with its operation.
    pub files: Vec<String>,
}

/// A linkage record that binds the route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkTouch {
    pub name: String,
    pub kind: String,
    pub why: String,
    pub author: Option<String>,
    pub recorded_utc: Option<String>,
    /// Its binds that name the route.
    pub binds_it: Vec<String>,
    /// What else it binds the route to.
    pub others: Vec<String>,
}

/// One commit, as `git log` wrote it ([`crate::exec::GIT_LOG_FORMAT`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

/// git's account of the route: how many commits touch its files, the first and the last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitEnds {
    pub commits: usize,
    pub first: Commit,
    pub last: Commit,
}

/// What the records say about a route.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Why {
    pub route: String,
    /// The crate a `crate:` route names.
    pub crate_name: Option<String>,
    /// The route's files (a crate's directory, a crate's file, a declared set's files), relative to the root.
    pub paths: Vec<String>,
    pub tasks: Vec<TaskTouch>,
    pub linkage: Vec<LinkTouch>,
    pub readings: Vec<ReadingAbout>,
    /// Filled by the caller from the `git log` line it ran.
    pub git: Option<GitEnds>,
    /// The `git log` line's receipt, and what it said when it found nothing.
    pub git_receipt: Option<i64>,
    pub git_note: Option<String>,
}

/// The route's place: its crate (for `crate:`) and its files relative to the root. Only `crate:` and `decl:` routes.
pub fn place(root: &Path, store: Option<&Store>, route: &str) -> Result<(Option<String>, Vec<String>)> {
    if !(route.starts_with("crate:") || route.starts_with("decl:")) {
        return Err(ReadingRefusal::UndeclaredRoute.refuse(format!("`{route}`: why reads a crate: or decl: route")));
    }
    let table = PathsTable::load(root, store)?;
    let r = table.resolve(route, Resolving::Job)?;
    let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/");
    if route.starts_with("decl:") {
        return Ok((None, r.files(root)?.iter().map(|p| rel(p)).collect()));
    }
    let paths = match r.crate_file(root) {
        Some(f) => vec![f],
        None => r.walk_root(root).map(|p| vec![rel(&p)]).unwrap_or_default(),
    };
    let krate = route.trim_start_matches("crate:").split('/').next().map(|n| n.replace('-', "_"));
    Ok((krate, paths))
}

/// Whether a file (relative) is one of the route's paths, or under one.
fn under(paths: &[String], file: &str) -> bool {
    paths.iter().any(|p| file == p || (!p.is_empty() && file.starts_with(&format!("{}/", p.trim_end_matches('/')))) || p.is_empty())
}

/// Whether a bind, a reading's target or an item names the route.
fn names(route: &str, krate: Option<&str>, word: &str) -> bool {
    if word == route || word.starts_with(&format!("{route}/")) {
        return true;
    }
    match krate {
        Some(k) => word == k || word.starts_with(&format!("{k}::")) || word == format!("dep:{k}") || word == format!("crate:{k}") || word.starts_with(&format!("crate:{k}/")),
        None => false,
    }
}

/// The task revisions whose change records touch `paths`, in history order.
pub fn tasks_touching(store: &Store, paths: &[String]) -> Result<Vec<TaskTouch>> {
    let root = crate::task_records::root(store)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let index = task_history::load_task_history(&root).map_err(|e| ReadingRefusal::ReadingUnreadable.refuse(format!("task history: {e}")))?;
    let mut out = Vec::new();
    for t in &index.tasks {
        let job = t.revisions.iter().flat_map(|r| &r.documents).filter(|d| d.kind().ok() == Some(DiffRecordKind::Task)).flat_map(|d| d.many("tag")).find(|tag| tag.starts_with("job-")).cloned();
        for r in &t.revisions {
            let files: Vec<String> = r
                .documents
                .iter()
                .filter(|d| d.kind().ok() == Some(DiffRecordKind::Change))
                .filter_map(|d| d.one("path").ok().filter(|p| under(paths, p)).map(|p| format!("{} {p}", d.one("operation").unwrap_or("modify"))))
                .collect();
            if files.is_empty() {
                continue;
            }
            let last = |kind: DiffRecordKind, key: &str| r.documents.iter().filter(|d| d.kind().ok() == Some(kind)).filter_map(|d| d.one(key).ok()).next_back().map(str::to_string);
            out.push(TaskTouch { task: t.task_directory.clone(), revision: r.name.clone(), intent: last(DiffRecordKind::Intent, "intent"), result: last(DiffRecordKind::Result, "status"), job: job.clone(), files });
        }
    }
    Ok(out)
}

/// The linkage records binding the route.
pub fn linkage_binding(records: &[(std::path::PathBuf, LinkageRecord)], route: &str, krate: Option<&str>) -> Vec<LinkTouch> {
    records
        .iter()
        .filter_map(|(_, rec)| {
            let (it, others): (Vec<&String>, Vec<&String>) = rec.binds.iter().partition(|b| names(route, krate, b));
            (!it.is_empty()).then(|| LinkTouch {
                name: rec.name.clone(),
                kind: rec.kind.as_str().to_string(),
                why: rec.why.clone(),
                author: rec.author.clone(),
                recorded_utc: rec.recorded_utc.clone(),
                binds_it: it.into_iter().cloned().collect(),
                others: others.into_iter().cloned().collect(),
            })
        })
        .collect()
}

/// What the records say about `route` (git's part is the caller's: [`commits_of`], [`ends`]).
pub fn of(root: &Path, store: &Store, route: &str, readings: &[ReadingAbout]) -> Result<Why> {
    let (krate, paths) = place(root, Some(store), route)?;
    let tasks = tasks_touching(store, &paths)?;
    let linkage = linkage_binding(&crate::linkage::load_all(store)?, route, krate.as_deref());
    let readings = readings.iter().filter(|r| names(route, krate.as_deref(), &r.target)).cloned().collect();
    Ok(Why { route: route.to_string(), crate_name: krate, paths, tasks, linkage, readings, ..Why::default() })
}

/// The commits a `git log` line printed in [`crate::exec::GIT_LOG_FORMAT`], newest first.
pub fn commits_of(stdout: &str) -> Vec<Commit> {
    stdout
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(4, '\t');
            Some(Commit { hash: f.next()?.to_string(), author: f.next()?.to_string(), date: f.next()?.to_string(), subject: f.next().unwrap_or_default().to_string() })
        })
        .filter(|c| c.hash.len() >= 7)
        .collect()
}

/// The first and the last of the commits (newest first, as `git log` lists them).
pub fn ends(commits: &[Commit]) -> Option<GitEnds> {
    Some(GitEnds { commits: commits.len(), first: commits.last()?.clone(), last: commits.first()?.clone() })
}

fn shown<T>(v: &[T], each: impl Fn(&T) -> String) -> String {
    let mut s: String = v.iter().take(SHOWN).map(|x| format!("- {}\n", each(x))).collect();
    if v.len() > SHOWN {
        s.push_str(&format!("- … and {} more\n", v.len() - SHOWN));
    }
    s
}

/// A long text cut to `n` characters, `…` where it was cut: one entry stays one line on a screen.
fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n).collect::<String>().trim_end().to_string() + "…"
}

fn commit_line(c: &Commit) -> String {
    format!("{} by {}, {}: {}", c.hash.get(..10).unwrap_or(&c.hash), c.author, c.date, c.subject)
}

/// The screen: what the route is, git's first and last commit, then the task histories, the linkage records and
/// the readings; "no record" where the records say nothing.
pub fn render(w: &Why) -> String {
    let mut s = format!("# why {}\n\n", w.route);
    let at = if w.paths.is_empty() { "no files".to_string() } else { w.paths.iter().take(SHOWN).cloned().collect::<Vec<_>>().join(", ") + &if w.paths.len() > SHOWN { format!(" … and {} more", w.paths.len() - SHOWN) } else { String::new() } };
    match &w.crate_name {
        Some(k) => s.push_str(&format!("The crate {k}, at {at}.\n")),
        None => s.push_str(&format!("The declared set {}: {at}.\n", w.route.trim_start_matches("decl:"))),
    }
    let receipt = w.git_receipt.map(|n| format!(" (git log, receipt #{n})")).unwrap_or_default();
    match &w.git {
        Some(g) => {
            s.push_str(&format!("It came in with {}{receipt}.\n", commit_line(&g.first)));
            if g.commits > 1 {
                s.push_str(&format!("It last changed in {}; {} commits touch it.\n", commit_line(&g.last), g.commits));
            }
        }
        None => s.push_str(&format!("git: no record{}{receipt}.\n", w.git_note.as_ref().map(|n| format!(" ({n})")).unwrap_or_default())),
    }
    s.push_str(&format!("\n## task histories that changed it ({})\n\n", w.tasks.len()));
    if w.tasks.is_empty() {
        s.push_str("No record: no task history has a change record in it.\n");
    }
    s.push_str(&shown(&w.tasks, |t| {
        format!(
            "{}/{}{}: {} — result {} — {}",
            t.task,
            t.revision,
            t.job.as_ref().map(|j| format!(" (job {j})")).unwrap_or_default(),
            t.intent.as_deref().map(|i| clip(i, 160)).unwrap_or_else(|| "no intent recorded".into()),
            t.result.clone().unwrap_or_else(|| "not closed".into()),
            t.files.join(", ")
        )
    }));
    s.push_str(&format!("\n## linkage records that bind it ({})\n\n", w.linkage.len()));
    if w.linkage.is_empty() {
        s.push_str("No record: no linkage record binds it.\n");
    }
    s.push_str(&shown(&w.linkage, |l| {
        format!(
            "{} ({}): {}{}{} — binds {}{}",
            l.name,
            l.kind,
            l.why,
            l.author.as_ref().map(|a| format!(", by {a}")).unwrap_or_default(),
            l.recorded_utc.as_ref().map(|u| format!(", {u}")).unwrap_or_default(),
            l.binds_it.join(", "),
            if l.others.is_empty() { String::new() } else { format!(" to {}", l.others.join(", ")) }
        )
    }));
    s.push_str(&format!("\n## readings about it ({})\n\n", w.readings.len()));
    if w.readings.is_empty() {
        s.push_str("No record: no reading names it.\n");
    }
    let newest: Vec<&ReadingAbout> = w.readings.iter().rev().collect();
    s.push_str(&shown(&newest, |r| format!("#{} {} {} `{}` ({}){}", r.seq, r.utc, r.kind, r.name, r.target, r.job.as_ref().map(|j| format!(" in job {j}")).unwrap_or_default())));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_names_its_crate_its_items_and_its_dependency() {
        assert!(names("crate:core_lib", Some("core_lib"), "core_lib::Config"));
        assert!(names("crate:core_lib", Some("core_lib"), "dep:core_lib"));
        assert!(names("crate:core_lib", Some("core_lib"), "crate:core_lib/src/lib.rs"));
        assert!(!names("crate:core_lib", Some("core_lib"), "core_libx::Config"));
        assert!(names("decl:market", None, "decl:market") && !names("decl:market", None, "market"));
        assert!(under(&["crates/core_lib".into()], "crates/core_lib/src/lib.rs") && !under(&["crates/core_lib".into()], "crates/core_libx/src/lib.rs"));
    }

    #[test]
    fn git_logs_lines_read_back_as_commits_and_their_ends() {
        let out = "b".repeat(40) + "\tJames Hunt\t2026-10-19T10:00:00+00:00\tthe second\n" + &"a".repeat(40) + "\tJames Hunt\t2026-09-01T09:00:00+00:00\tthe first: a\ttab\n";
        let c = commits_of(&out);
        assert_eq!(c.len(), 2);
        let e = ends(&c).unwrap();
        assert_eq!((e.commits, e.first.subject.as_str(), e.last.subject.as_str()), (2, "the first: a\ttab", "the second"));
        assert!(ends(&[]).is_none());
        assert_eq!((clip("short", 9), clip("a long line here", 6)), ("short".to_string(), "a long…".to_string()));
    }
}
