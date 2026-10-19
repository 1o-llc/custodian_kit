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
use std::path::{Path, PathBuf};
use tokei::{Config, LanguageType, Languages};

/// What a count is (files by crate, then language, each with its digest, and a
/// digest over the whole), two counts compared, a run of diffs summed and a
/// route's counts listed: tree_history's.
pub use tree_history::count::{compare_counts, share, within, CountDiff, CountReport, CountSummary, CrateCount, CrateMoved, FileCount, LanguageCount, LanguageMoved, Lines, Moved, Side, Within, COUNT_DIFF_SCHEMA, COUNT_SCHEMA};

/// The commit a reading line was started at (the reading line's caller runs
/// the declared git line and passes it); without it the count runs that line itself.
pub const COMMIT_ENV: &str = "CUSTODIAN_COMMIT";

/// tokei's lines of a file or a block.
fn lines_of(s: &tokei::CodeStats) -> Lines {
    let s = s.summarise();
    Lines { lines: s.code + s.comments + s.blanks, code: s.code, comments: s.comments, blanks: s.blanks }
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
            out.add(lines_of(&LanguageType::Rust.parse_from_str(part, config)));
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
                let whole = lines_of(&r.stats);
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
        let whole = lines_of(&LanguageType::Rust.parse_from_str(&part, &config));
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
// compare and history (tree_history's, over the kit's readings and journal)

/// Two count readings compared; a reading that is not a count is refused.
pub fn compare(a: &ReadingReport, b: &ReadingReport) -> Result<CountDiff> {
    let (Some(x), Some(y)) = (&a.count, &b.count) else {
        return Err(ReadingRefusal::MissingInput.refuse(format!("{} / {}: compare takes two count readings", a.name, b.name)));
    };
    Ok(compare_counts(x, y))
}

/// Every count of `route` the journal holds (reuses left out), oldest first.
/// `route` is spelled as the count resolves it: `*`, `crate:core_lib`, `module:core_lib::wide`.
pub fn history(store: &Store, route: &str) -> Result<Vec<CountSummary>> {
    Ok(tree_history::count::history(&store.root, &store.journal()?.runs()?, route))
}
