//! `count`: how much there is in a route, by tokei (as a library), and how
//! much of it is tests, by klepto.
//!
//! The route is the workspace (`*`), a crate (`crate:<name>`, or its name), a
//! crate's file, a declared set (`decl:<name>`), a declared raw path, or a
//! module (`core_lib::wide`). tokei walks inside the route's own root (a
//! crate's directory, the workspace root) with the route table's ignore set
//! ([`crate::paths::PathsTable::ignored`]) and the spec's excludes; a declared
//! set and a module are the files they name, never a walk. tokei's own
//! conventions hold: `.gitignore`, `.ignore` and `.tokeignore` are honoured,
//! hidden folders are skipped.
//!
//! Each file's lines, code, comments and blanks are tokei's. The part that is
//! tests is klepto's, as the bundle filters read it: a file under `tests/` or
//! `benches/` (or a test or bench target), a module whose cfg holds only under
//! `test`, and in any other file each `#[cfg(test)]` item, its lines counted
//! by tokei's Rust parser.
//!
//! A count is a [`CountReport`]: files by crate (then language), sorted, and
//! its digest over that canonical form, with no time inside, so the same tree
//! gives the same digest. Its header carries the index key and the git commit.
//! [`compare`] says what moved between two; [`history`] lists a route's counts
//! from the journal.

use super::facts::Facts;
use super::refusal::ReadingRefusal;
use super::spec::ReadingSpec;
use super::{Answer, ReadingReport};
use crate::bundle::{place_of, requires_test, walk, Place};
use crate::store::Store;
use crate::{CodeQueryResult, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokei::{Config, LanguageType, Languages};

pub const COUNT_SCHEMA: &str = "custodian.count.v1";
pub const COUNT_DIFF_SCHEMA: &str = "custodian.count_diff.v1";
/// The commit a reading line was started at (the reading line's caller runs
/// the declared git line and passes it); without it the count runs that line itself.
pub const COMMIT_ENV: &str = "CUSTODIAN_COMMIT";

/// Lines as tokei counts them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Lines {
    pub lines: usize,
    pub code: usize,
    pub comments: usize,
    pub blanks: usize,
}

impl Lines {
    fn of(s: &tokei::CodeStats) -> Lines {
        let s = s.summarise();
        Lines { lines: s.code + s.comments + s.blanks, code: s.code, comments: s.comments, blanks: s.blanks }
    }

    fn add(&mut self, o: Lines) {
        self.lines += o.lines;
        self.code += o.code;
        self.comments += o.comments;
        self.blanks += o.blanks;
    }
}

/// One file: its language, its lines, the part of them that is tests.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FileCount {
    /// Relative to the root; a module's inline block is `<file>#<module>`.
    pub path: String,
    pub language: String,
    pub lines: Lines,
    pub tests: Lines,
    /// sha256 of the file's bytes (an inline block's: of its text), so a
    /// change that keeps the line counts still shows, and a task's change
    /// records carry the digest before.
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageCount {
    pub language: String,
    pub files: usize,
    pub lines: Lines,
    pub tests: Lines,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrateCount {
    /// The crate's name; `-` for files in no crate.
    pub name: String,
    pub files: usize,
    pub lines: Lines,
    pub tests: Lines,
    pub languages: Vec<LanguageCount>,
    pub file_counts: Vec<FileCount>,
}

/// A count of a route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountReport {
    pub schema: String,
    /// The route as resolved: `*`, `crate:core_lib`, `decl:market`, `module:core_lib::wide`.
    pub route: String,
    pub index_key: Option<String>,
    pub commit: Option<String>,
    /// `sha256:` over the canonical counts (schema and crates; no route, key, commit or time).
    pub digest: String,
    pub files: usize,
    pub lines: Lines,
    pub tests: Lines,
    /// Test lines per thousand lines.
    pub test_share: usize,
    pub languages: Vec<LanguageCount>,
    pub crates: Vec<CrateCount>,
}

fn share(tests: &Lines, all: &Lines) -> usize {
    (tests.lines * 1000).checked_div(all.lines).unwrap_or(0)
}

fn languages_of<'a>(files: impl Iterator<Item = &'a FileCount>) -> Vec<LanguageCount> {
    let mut by: BTreeMap<String, LanguageCount> = BTreeMap::new();
    for f in files {
        let l = by.entry(f.language.clone()).or_insert_with(|| LanguageCount { language: f.language.clone(), files: 0, lines: Lines::default(), tests: Lines::default() });
        l.files += 1;
        l.lines.add(f.lines);
        l.tests.add(f.tests);
    }
    by.into_values().collect()
}

impl CountReport {
    /// Files grouped by crate, then language, sorted; the digest over that form.
    pub fn from_files(route: String, index_key: Option<String>, commit: Option<String>, files: Vec<(String, FileCount)>) -> CountReport {
        let mut by: BTreeMap<String, Vec<FileCount>> = BTreeMap::new();
        for (krate, f) in files {
            by.entry(krate).or_default().push(f);
        }
        let crates: Vec<CrateCount> = by
            .into_iter()
            .map(|(name, mut file_counts)| {
                file_counts.sort();
                let mut lines = Lines::default();
                let mut tests = Lines::default();
                for f in &file_counts {
                    lines.add(f.lines);
                    tests.add(f.tests);
                }
                CrateCount { name, files: file_counts.len(), lines, tests, languages: languages_of(file_counts.iter()), file_counts }
            })
            .collect();
        let digest = crate::digest::of_value(&(COUNT_SCHEMA, &crates));
        let mut lines = Lines::default();
        let mut tests = Lines::default();
        for c in &crates {
            lines.add(c.lines);
            tests.add(c.tests);
        }
        CountReport {
            schema: COUNT_SCHEMA.into(),
            route,
            index_key,
            commit,
            digest,
            files: crates.iter().map(|c| c.files).sum(),
            test_share: share(&tests, &lines),
            lines,
            tests,
            languages: languages_of(crates.iter().flat_map(|c| c.file_counts.iter())),
            crates,
        }
    }
}

/// The git commit at `root`, by the declared `git rev-parse HEAD` line; `None` outside git.
pub fn commit_at(root: &Path) -> Option<String> {
    if let Ok(c) = std::env::var(COMMIT_ENV) {
        return Some(c).filter(|c| !c.is_empty());
    }
    let plan = crate::exec::Plan::new("git-head").then(crate::exec::line("git-head", &crate::exec::git_head(root)).ok()?);
    let walked = plan.walk(crate::exec::Watch::Quiet, |_, _| {}).ok()?;
    let w = walked.first()?;
    w.decision.status.accepted.then(|| w.stdout.trim().to_string()).filter(|s| s.len() >= 7)
}

/// What a count reads: roots tokei walks, or files it is handed, and a module's inline blocks.
struct Plan {
    route: String,
    walk: Vec<PathBuf>,
    inline: Vec<(PathBuf, String, usize, usize)>,
}

fn plan(f: &Facts, target: &str) -> Result<Plan> {
    let target = target.trim();
    if target == "*" {
        return Ok(Plan { route: "*".into(), walk: vec![f.root.clone()], inline: vec![] });
    }
    // a crate by its name is the crate (its directory), not its root module
    if !target.contains(':') {
        if let Some(p) = f.package_named(target) {
            return plan(f, &format!("crate:{}", p.name));
        }
    }
    // a module: its own file, its file children, its inline block when it is one
    if let Some(m) = f.k.modules.get(target) {
        let mut walk: Vec<PathBuf> = f.k.modules.modules.values().filter(|x| !x.inline && (x.path == m.path || x.path.starts_with(&format!("{}::", m.path)))).map(|x| x.file.clone()).collect();
        walk.sort();
        walk.dedup();
        let mut inline = Vec::new();
        if m.inline {
            let (start, end) = inline_block(f, m).ok_or_else(|| ReadingRefusal::TargetUnknown.refuse(format!("{target}: the inline module's block is not in its file")))?;
            inline.push((m.file.clone(), m.path.clone(), start, end));
        }
        return Ok(Plan { route: format!("module:{}", m.path), walk, inline });
    }
    let spelled = if target.contains(':') {
        target.to_string()
    } else {
        return Err(ReadingRefusal::TargetUnknown.refuse(format!("{target}: a count takes *, a route (crate:, decl:, path:) or a module")));
    };
    let route = f.paths.resolve(&spelled, f.how)?;
    let walk = match route.walk_root(&f.root) {
        Some(r) => vec![r],
        None => route.files(&f.root)?,
    };
    Ok(Plan { route: route.name().to_string(), walk, inline: vec![] })
}

/// The lines of an inline module's `mod x { }` block: from the file's own
/// module, down the inline modules its path names, in klepto's parse of the file.
fn inline_block(f: &Facts, m: &klepto::ModuleInfo) -> Option<(usize, usize)> {
    use syn::spanned::Spanned;
    let host = f.k.modules.for_file(&m.file)?;
    let rest = m.segments.get(host.segments.len()..)?;
    let parsed = f.k.files.iter().find(|x| x.path == m.file)?;
    let mut items: &[syn::Item] = &parsed.ast.items;
    let mut found = None;
    for seg in rest {
        let module = items.iter().find_map(|i| match i {
            syn::Item::Mod(x) if x.ident == seg => Some(x),
            _ => None,
        })?;
        let start = module.attrs.iter().map(|a| a.pound_token.span.start().line).chain([module.span().start().line]).min()?;
        found = Some((start, module.span().end().line));
        items = &module.content.as_ref()?.1;
    }
    found
}

/// A spec's excludes in tokei's ignore syntax: a pattern with a `/` is
/// anchored under the root by a leading `**/` (tokei matches against the walked path).
fn ignore_set(f: &Facts, spec: &ReadingSpec) -> Vec<String> {
    let mut out = f.paths.ignored();
    for g in &spec.scope.exclude {
        let g = g.trim_start_matches("./");
        out.push(if g.contains('/') && !g.starts_with("**/") { format!("**/{g}") } else { g.to_string() });
    }
    out
}

/// The test lines of a Rust file, by klepto: all of it under tests/ or
/// benches/ or in a test-only module, else each `#[cfg(test)]` item.
fn test_lines(f: &Facts, file: &Path, rel: &str, text: &str, whole: Lines, config: &Config) -> Lines {
    let place = place_of(f, rel, file);
    let module_cfg = f.k.modules.for_file(file).map(|m| m.cfg.clone()).unwrap_or_default();
    if matches!(place, Place::Tests | Place::Benches) || requires_test(&module_cfg) {
        return whole;
    }
    let Some(parsed) = f.k.files.iter().find(|x| x.path == file) else { return Lines::default() };
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = Lines::default();
    walk(&parsed.ast.items, &mut |s, e, cfg| {
        if requires_test(cfg) {
            let part: String = lines.get(s.saturating_sub(1)..e.min(lines.len())).map(|l| l.concat()).unwrap_or_default();
            out.add(Lines::of(&LanguageType::Rust.parse_from_str(part, config)));
            return false;
        }
        true
    });
    out
}

/// The count of `spec.target` over `f`.
pub fn count(f: &Facts, spec: &ReadingSpec) -> Result<CountReport> {
    let plan = plan(f, &spec.target)?;
    let config = Config::default();
    let ignored = ignore_set(f, spec);
    let ignored_refs: Vec<&str> = ignored.iter().map(String::as_str).collect();
    let mut files: Vec<(String, FileCount)> = Vec::new();
    // the crate whose directory holds the file (the deepest), read or not
    let krate = |p: &Path| f.package_of_rel(&f.rel(p)).map(|k| k.name.clone()).unwrap_or_else(|| "-".to_string());
    if !plan.walk.is_empty() {
        let mut langs = Languages::new();
        langs.get_statistics(&plan.walk, &ignored_refs, &config);
        for (ty, lang) in &langs {
            for r in &lang.reports {
                let rel = f.rel(&r.name);
                let whole = Lines::of(&r.stats);
                let tests = if *ty == LanguageType::Rust {
                    let text = f.k.files.iter().find(|x| x.path == r.name).map(|x| x.source.clone()).or_else(|| std::fs::read_to_string(&r.name).ok()).unwrap_or_default();
                    test_lines(f, &r.name, &rel, &text, whole, &config)
                } else {
                    Lines::default()
                };
                let digest = std::fs::read(&r.name).map(|b| crate::digest::of(b)).unwrap_or_default();
                files.push((krate(&r.name), FileCount { path: rel, language: ty.name().to_string(), lines: whole, tests, digest }));
            }
        }
    }
    for (file, module, s, e) in &plan.inline {
        let text = f.k.files.iter().find(|x| &x.path == file).map(|x| x.source.clone()).unwrap_or_default();
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let part: String = lines.get(s.saturating_sub(1)..(*e).min(lines.len())).map(|l| l.concat()).unwrap_or_default();
        let whole = Lines::of(&LanguageType::Rust.parse_from_str(&part, &config));
        let tests = if requires_test(&f.k.modules.get(module).map(|m| m.cfg.clone()).unwrap_or_default()) { whole } else { Lines::default() };
        let digest = crate::digest::of(part.as_bytes());
        files.push((krate(file), FileCount { path: format!("{}#{module}", f.rel(file)), language: LanguageType::Rust.name().to_string(), lines: whole, tests, digest }));
    }
    let index_key = crate::api::IndexKey::of_root(&f.root, &spec.scope).ok().map(|k| k.0);
    Ok(CountReport::from_files(plan.route, index_key, commit_at(&f.root), files))
}

fn row(kind: &str, label: &str, krate: Option<String>, files: usize, l: &Lines, t: &Lines) -> CodeQueryResult {
    let mut r = super::kinds::named_row(kind, label, krate, "counted by tokei; tests by klepto");
    for (k, v) in [("files", files), ("lines", l.lines), ("code", l.code), ("comments", l.comments), ("blanks", l.blanks), ("test_lines", t.lines), ("test_share", share(t, l))] {
        r.attributes.insert(k.to_string(), v.to_string());
    }
    r
}

/// The `count` kind: the count, its rows (per language, per crate) and its counts.
pub fn rows(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let c = count(f, spec)?;
    let mut ans = Answer::default();
    ans.count("files", c.files);
    ans.count("lines", c.lines.lines);
    ans.count("code", c.lines.code);
    ans.count("comments", c.lines.comments);
    ans.count("blanks", c.lines.blanks);
    ans.count("test lines", c.tests.lines);
    ans.count("test share (per thousand lines)", c.test_share);
    for l in &c.languages {
        ans.count(format!("{} lines", l.language), l.lines.lines);
    }
    ans.rows.extend(c.languages.iter().map(|l| row("count-language", &l.language, None, l.files, &l.lines, &l.tests)));
    ans.rows.extend(c.crates.iter().map(|k| row("count-crate", &k.name, Some(k.name.clone()), k.files, &k.lines, &k.tests)));
    ans.count = Some(c);
    Ok(ans)
}

// ---------------------------------------------------------------------------
// compare and history

/// Lines that came and went.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Moved {
    pub added: Lines,
    pub removed: Lines,
}

impl Moved {
    fn between(before: Lines, after: Lines) -> Moved {
        let mut m = Moved::default();
        for (b, a, add, rem) in [
            (before.lines, after.lines, &mut m.added.lines, &mut m.removed.lines),
            (before.code, after.code, &mut m.added.code, &mut m.removed.code),
            (before.comments, after.comments, &mut m.added.comments, &mut m.removed.comments),
            (before.blanks, after.blanks, &mut m.added.blanks, &mut m.removed.blanks),
        ] {
            if a > b {
                *add = a - b;
            } else {
                *rem = b - a;
            }
        }
        m
    }

    fn add(&mut self, o: Moved) {
        self.added.add(o.added);
        self.removed.add(o.removed);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Side {
    pub digest: String,
    pub commit: Option<String>,
    pub index_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageMoved {
    pub language: String,
    pub moved: Moved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrateMoved {
    pub name: String,
    pub moved: Moved,
    pub languages: Vec<LanguageMoved>,
    pub files_added: Vec<String>,
    pub files_removed: Vec<String>,
    /// Files in both whose lines moved.
    pub files_changed: Vec<String>,
    pub test_share_before: usize,
    pub test_share_after: usize,
}

/// What moved between two counts, file by file: a line count that grew is
/// added, one that shrank is removed; a file only after is added whole, only
/// before is removed whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountDiff {
    pub schema: String,
    pub route: String,
    pub before: Side,
    pub after: Side,
    /// `sha256:` over the canonical diff (the two digests and what moved; no commits).
    pub digest: String,
    pub moved: Moved,
    pub files_added: Vec<String>,
    pub files_removed: Vec<String>,
    pub files_changed: Vec<String>,
    pub test_share_before: usize,
    pub test_share_after: usize,
    /// After minus before, per thousand lines.
    pub test_share_delta: i64,
    pub crates: Vec<CrateMoved>,
}

impl CountDiff {
    pub fn is_empty(&self) -> bool {
        self.before.digest == self.after.digest
    }

    /// Every file the diff names: added, removed, changed.
    pub fn files(&self) -> Vec<String> {
        let mut v: Vec<String> = self.files_added.iter().chain(&self.files_removed).chain(&self.files_changed).cloned().collect();
        v.sort();
        v
    }

    /// `step 3 apply: +412 −38 across 6 files in 2 crates`.
    pub fn line(&self, label: &str) -> String {
        format!("{label}: +{} \u{2212}{} across {} files in {} crates", self.moved.added.lines, self.moved.removed.lines, self.files().len(), self.crates.len())
    }
}

/// What a run of diffs moved together: lines added and removed, the files and crates touched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Within {
    pub diffs: usize,
    pub moved: Moved,
    pub files: Vec<String>,
    pub crates: Vec<String>,
}

/// The diffs summed (a range of steps, or of jobs).
pub fn within<'a>(diffs: impl IntoIterator<Item = &'a CountDiff>) -> Within {
    let mut w = Within::default();
    let mut files = std::collections::BTreeSet::new();
    let mut crates = std::collections::BTreeSet::new();
    for d in diffs {
        w.diffs += 1;
        w.moved.add(d.moved);
        files.extend(d.files());
        crates.extend(d.crates.iter().map(|c| c.name.clone()));
    }
    w.files = files.into_iter().collect();
    w.crates = crates.into_iter().collect();
    w
}

/// Two counts compared.
pub fn compare_counts(a: &CountReport, b: &CountReport) -> CountDiff {
    let files_of = |c: &CountReport| -> BTreeMap<String, (String, FileCount)> { c.crates.iter().flat_map(|k| k.file_counts.iter().map(move |f| (f.path.clone(), (k.name.clone(), f.clone())))).collect() };
    let (fa, fb) = (files_of(a), files_of(b));
    #[allow(clippy::type_complexity)]
    let mut crates: BTreeMap<String, (Moved, BTreeMap<String, Moved>, Vec<String>, Vec<String>, Vec<String>)> = BTreeMap::new();
    let mut paths: Vec<&String> = fa.keys().chain(fb.keys()).collect();
    paths.sort();
    paths.dedup();
    for p in paths {
        let (before, after) = (fa.get(p), fb.get(p));
        let (krate, language) = after.or(before).map(|(k, f)| (k.clone(), f.language.clone())).unwrap_or_default();
        let m = Moved::between(before.map(|x| x.1.lines).unwrap_or_default(), after.map(|x| x.1.lines).unwrap_or_default());
        let e = crates.entry(krate).or_default();
        e.0.add(m);
        e.1.entry(language).or_default().add(m);
        match (before, after) {
            (None, Some(_)) => e.2.push(p.clone()),
            (Some(_), None) => e.3.push(p.clone()),
            (Some(x), Some(y)) if m != Moved::default() || x.1.digest != y.1.digest => e.4.push(p.clone()),
            _ => {}
        }
    }
    let share_of = |c: &CountReport, name: &str| c.crates.iter().find(|k| k.name == name).map(|k| share(&k.tests, &k.lines)).unwrap_or(0);
    let crates: Vec<CrateMoved> = crates
        .into_iter()
        .filter(|(_, (m, _, added, removed, changed))| *m != Moved::default() || !added.is_empty() || !removed.is_empty() || !changed.is_empty())
        .map(|(name, (moved, langs, added, removed, changed))| CrateMoved {
            test_share_before: share_of(a, &name),
            test_share_after: share_of(b, &name),
            languages: langs.into_iter().filter(|(_, m)| *m != Moved::default()).map(|(language, moved)| LanguageMoved { language, moved }).collect(),
            files_added: added,
            files_removed: removed,
            files_changed: changed,
            moved,
            name,
        })
        .collect();
    let mut moved = Moved::default();
    for c in &crates {
        moved.add(c.moved);
    }
    let files_added: Vec<String> = crates.iter().flat_map(|c| c.files_added.iter().cloned()).collect();
    let files_removed: Vec<String> = crates.iter().flat_map(|c| c.files_removed.iter().cloned()).collect();
    let files_changed: Vec<String> = crates.iter().flat_map(|c| c.files_changed.iter().cloned()).collect();
    let test_share_delta = b.test_share as i64 - a.test_share as i64;
    let digest = crate::digest::of_value(&(COUNT_DIFF_SCHEMA, &a.digest, &b.digest, &moved, &files_added, &files_removed, &files_changed, a.test_share, b.test_share, &crates));
    CountDiff {
        schema: COUNT_DIFF_SCHEMA.into(),
        route: b.route.clone(),
        before: Side { digest: a.digest.clone(), commit: a.commit.clone(), index_key: a.index_key.clone() },
        after: Side { digest: b.digest.clone(), commit: b.commit.clone(), index_key: b.index_key.clone() },
        digest,
        moved,
        files_added,
        files_removed,
        files_changed,
        test_share_before: a.test_share,
        test_share_after: b.test_share,
        test_share_delta,
        crates,
    }
}

/// Two count readings compared; a reading that is not a count is refused.
pub fn compare(a: &ReadingReport, b: &ReadingReport) -> Result<CountDiff> {
    let (Some(x), Some(y)) = (&a.count, &b.count) else {
        return Err(ReadingRefusal::MissingInput.refuse(format!("{} / {}: compare takes two count readings", a.name, b.name)));
    };
    Ok(compare_counts(x, y))
}

/// One count of a route, as the journal keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountSummary {
    pub seq: i64,
    pub utc: String,
    pub name: String,
    pub commit: Option<String>,
    pub digest: String,
    pub files: usize,
    pub lines: Lines,
    pub tests: Lines,
    pub test_share: usize,
}

/// Every count of `route` the journal holds (reuses left out), oldest first.
/// `route` is spelled as the count resolves it: `*`, `crate:core_lib`, `module:core_lib::wide`.
pub fn history(store: &Store, route: &str) -> Result<Vec<CountSummary>> {
    let mut out = Vec::new();
    for r in store.journal()?.runs()? {
        if r.command != "reading" || r.reused_seq.is_some() {
            continue;
        }
        let Some(json) = r.outputs.iter().find(|o| o.ends_with(".json")) else { continue };
        let Ok(rep) = super::load(&store.root.join(json)) else { continue };
        let Some(c) = rep.count.filter(|c| c.route == route) else { continue };
        out.push(CountSummary { seq: r.seq, utc: r.utc.clone(), name: r.name.clone(), commit: c.commit, digest: c.digest, files: c.files, lines: c.lines, tests: c.tests, test_share: c.test_share });
    }
    out.sort_by_key(|s| s.seq);
    Ok(out)
}
