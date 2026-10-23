//! `custodian test`: cargo's test lines, declared once (`exec::Cargo`), run
//! as a plan through the executor with a receipt per crate. `--affected` runs
//! exactly the tests an affected_tests reading names; `--all` one line per
//! workspace package. What a run shows is chosen: `failed` prints only the
//! failures, `progress` one line updated in place, `all` everything. Each
//! test's outcome goes into the journal, and `test report` reads the journal
//! (and maintenance's timing report, when the project has one) for the
//! slowest and the flakiest.

use crate::exec::{self, Plan, Watch};
use crate::reading::refusal::ReadingRefusal;
use crate::reading::ReadingReport;
use crate::runs::{keep, receipt_of, Ctx, InJob};
use crate::store::{Receipt, TestRow};
use crate::{klepto_builder, Result, ScanOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observe {
    Failed,
    Progress,
    All,
}

impl std::str::FromStr for Observe {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "failed" => Ok(Observe::Failed),
            "progress" => Ok(Observe::Progress),
            "all" => Ok(Observe::All),
            o => Err(format!("--observe {o}: failed, progress or all")),
        }
    }
}

/// A feature configuration a test line runs under: cargo's default features (`Configuration::default()`), with
/// listed features on top, with every feature, or without the defaults. A line run under another configuration
/// than the default carries it in its target word, `lib [features json]`, so its outcomes are told apart from the
/// default's and a rerun (a job's close) runs it the same way.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct Configuration {
    pub no_default: bool,
    pub all: bool,
    pub features: Vec<String>,
}

impl Configuration {
    pub fn is_default(&self) -> bool {
        *self == Configuration::default()
    }

    /// cargo's words for it: `--no-default-features`, `--all-features`, `--features a,b`.
    pub fn args(&self) -> Vec<String> {
        let mut a = Vec::new();
        if self.no_default {
            a.push("--no-default-features".to_string());
        }
        if self.all {
            a.push("--all-features".to_string());
        }
        if !self.features.is_empty() {
            a.push("--features".to_string());
            a.push(self.features.join(","));
        }
        a
    }

    /// `no-default-features, features json,chrono`; empty for the default.
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.no_default {
            parts.push("no-default-features".to_string());
        }
        if self.all {
            parts.push("all-features".to_string());
        }
        if !self.features.is_empty() {
            parts.push(format!("features {}", self.features.join(",")));
        }
        parts.join(", ")
    }

    /// A target word with this configuration: the word itself for the default, else `<word> [<label>]`.
    pub fn target_with(&self, target: &str) -> String {
        if self.is_default() {
            target.to_string()
        } else {
            format!("{target} [{}]", self.label())
        }
    }

    /// A target word read back: the word and its configuration.
    pub fn of_target(target: &str) -> (String, Configuration) {
        let Some((word, rest)) = target.split_once(" [") else { return (target.to_string(), Configuration::default()) };
        let mut c = Configuration::default();
        for part in rest.trim_end_matches(']').split(", ") {
            match part {
                "no-default-features" => c.no_default = true,
                "all-features" => c.all = true,
                p => {
                    if let Some(f) = p.strip_prefix("features ") {
                        c.features = f.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect();
                    }
                }
            }
        }
        (word.to_string(), c)
    }

    /// Every configuration klepto's check matrix asks of a package (the transform's N of N): each feature set, with
    /// and without the defaults, one per distinct set of features on.
    pub fn matrix(model: &klepto::FeatureModel) -> Vec<Configuration> {
        klepto::check_matrix(model).into_iter().map(|j| Configuration { no_default: j.no_default, all: false, features: j.features }).collect()
    }

    /// The configuration on a cargo test line.
    pub fn on(&self, mut line: exec::CargoTestRooted<exec::Cargo>) -> exec::CargoTestRooted<exec::Cargo> {
        if self.no_default {
            line = line.no_default_features();
        }
        if self.all {
            line = line.all_features();
        }
        if !self.features.is_empty() {
            line = line.features(self.features.join(","));
        }
        line
    }
}

/// One cargo test line to run: a package, a target, the exact filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestLine {
    pub package: String,
    /// `None`: every target (`--all`); `Some(None)`: the lib and bins; `Some(Some(t))`: `--test t`.
    pub target: Option<Option<String>>,
    pub filters: Vec<String>,
}

/// The lines an affected_tests (or tests_covering) reading names.
pub fn lines_of_reading(r: &ReadingReport) -> Vec<TestLine> {
    let mut by: BTreeMap<(String, Option<String>), Vec<String>> = BTreeMap::new();
    for row in r.query.results.iter().filter(|x| x.kind == "test") {
        let (Some(pkg), Some(filter)) = (row.crate_name.clone(), row.attributes.get("filter").cloned()) else { continue };
        let target = row.attributes.get("test_target").filter(|t| *t != "unit").cloned();
        by.entry((pkg, target)).or_default().push(filter);
    }
    by.into_iter().map(|((package, t), filters)| TestLine { package, target: Some(t), filters }).collect()
}

/// One line per workspace package (cargo metadata, as klepto reads it).
pub fn lines_all(ctx: &Ctx) -> Result<Vec<TestLine>> {
    let b = klepto_builder(&ctx.root, &ScanOptions::graph()).map_err(|e| ReadingRefusal::NoWorkspace.refuse(e))?;
    Ok(b.packages().values().map(|p| TestLine { package: p.cargo_name.clone(), target: None, filters: Vec::new() }).collect())
}

/// What a test run did.
#[derive(Debug, Default)]
pub struct TestRun {
    pub receipts: Vec<(Receipt, Option<PathBuf>)>,
    pub results: Vec<TestRow>,
    pub said: Vec<String>,
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

/// The first lines of each failure's captured output.
fn failure_text(stdout: &str, test: &str) -> Vec<String> {
    let head = format!("---- {test} stdout ----");
    let mut lines = stdout.lines().skip_while(|l| l.trim() != head).skip(1);
    let mut out = Vec::new();
    for l in lines.by_ref() {
        if l.starts_with("---- ") || l.trim() == "failures:" || out.len() >= 6 {
            break;
        }
        out.push(l.to_string());
    }
    out
}

pub fn run(ctx: &Ctx, lines: &[TestLine], observe: Observe, job: Option<&InJob>) -> Result<TestRun> {
    let mut plan = Plan::new(format!("test:{}", lines.len()));
    for (i, t) in lines.iter().enumerate() {
        let mut cmd = exec::Cargo::test(ctx.root.join("Cargo.toml").as_os_str(), t.package.as_str());
        match &t.target {
            None => {}
            Some(None) => cmd = cmd.lib().bins(),
            Some(Some(name)) => cmd = cmd.test(name.as_str()),
        }
        if !t.filters.is_empty() {
            let mut rest: Vec<std::ffi::OsString> = vec!["--".into()];
            rest.extend(t.filters.iter().map(Into::into));
            rest.push("--exact".into());
            cmd = cmd.rest(rest);
        }
        let mut l = exec::line(format!("test-{i}:{}", t.package), &cmd)?.going_on();
        // maintenance's #[timed_test] writes its timings under this run's name
        l.env.push(("MAINTENANCE_RUN".into(), format!("custodian-{}-{i}", crate::runs::stamp())));
        plan = plan.then(l);
    }
    let watch = match observe {
        Observe::All => Watch::Stream,
        _ => Watch::Lines,
    };
    let mut counts = (0usize, 0usize, 0usize);
    let walked = plan.walk(watch, |id, text| {
        if let Some((name, outcome)) = outcomes(text).into_iter().next() {
            match outcome.as_str() {
                "ok" => counts.0 += 1,
                "FAILED" => counts.1 += 1,
                _ => counts.2 += 1,
            }
            if observe == Observe::Progress {
                let mut e = std::io::stderr();
                let _ = write!(e, "\r\x1b[2K[{id}] ok {} failed {} ignored {} — {}", counts.0, counts.1, counts.2, name.chars().take(80).collect::<String>());
                let _ = e.flush();
            }
            if observe == Observe::Failed && outcome == "FAILED" {
                eprintln!("FAILED {name} ({id})");
            }
        }
    })?;
    if observe == Observe::Progress {
        eprintln!();
    }
    let mut out = TestRun::default();
    for (w, t) in walked.iter().zip(lines) {
        let name = match &t.target {
            Some(Some(tt)) => format!("{} --test {tt}", t.package),
            _ => t.package.clone(),
        };
        let r = receipt_of(&plan, w, "test", &name, None, None, None, None, vec![], job);
        let found = outcomes(&w.stdout);
        if observe == Observe::Failed {
            for (test, o) in found.iter().filter(|(_, o)| o == "FAILED") {
                for l in failure_text(&w.stdout, test) {
                    eprintln!("    {l}");
                }
                let _ = o;
            }
            if !w.decision.status.accepted && found.iter().all(|(_, o)| o != "FAILED") {
                eprintln!("{name}: {} (no test failed: the build or the run did)", w.decision.reason);
                for l in w.stderr.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev() {
                    eprintln!("    {l}");
                }
            }
        }
        let (r, f) = match &ctx.store {
            Some(store) => {
                let (r, f) = keep(store, r)?;
                (r, Some(f))
            }
            None => (r, None),
        };
        let target = match &t.target {
            None => "all".to_string(),
            Some(None) => "unit".to_string(),
            Some(Some(tt)) => tt.clone(),
        };
        for (test, outcome) in found {
            let row = TestRow { seq: r.seq, package: t.package.clone(), target: target.clone(), test, outcome, millis: None };
            if let Some(store) = &ctx.store {
                store.journal()?.put_test(&row)?;
            }
            out.results.push(row);
        }
        out.said.push(format!(
            "{}: {} in {} ms ({}){}",
            r.line,
            r.reason,
            r.millis,
            if r.seq > 0 { format!("run #{}", r.seq) } else { "free mode".into() },
            f.as_ref().map(|f| format!(", receipt {}", ctx.rel(f))).unwrap_or_default()
        ));
        out.receipts.push((r, f));
    }
    let failed = out.results.iter().filter(|t| t.outcome == "FAILED").count();
    out.said.push(format!("tests: {} ran, {} failed, {} lines", out.results.len(), failed, lines.len()));
    Ok(out)
}

/// `test report`: the slowest crates by their last run, the flakiest tests
/// (more than one outcome across runs), and maintenance's slowest timed tests.
pub fn report(ctx: &Ctx) -> Result<String> {
    let store = ctx.store()?;
    let journal = store.journal()?;
    let runs: Vec<Receipt> = journal.runs()?.into_iter().filter(|r| r.command == "test").collect();
    let mut last: BTreeMap<String, &Receipt> = BTreeMap::new();
    for r in &runs {
        last.insert(r.name.clone(), r);
    }
    let mut slow: Vec<&&Receipt> = last.values().collect();
    slow.sort_by_key(|r| std::cmp::Reverse(r.millis));
    let mut s = format!("# test report\n\n{} test runs in the journal.\n\n## slowest crates (last run)\n\n", runs.len());
    for r in slow.iter().take(15) {
        s.push_str(&format!("- {}: {} ms ({}, run #{})\n", r.name, r.millis, r.reason, r.seq));
    }
    let mut seen: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for t in journal.tests()? {
        seen.entry((t.package.clone(), t.test.clone())).or_default().insert(t.outcome.clone());
    }
    let flaky: Vec<_> = seen.iter().filter(|(_, o)| o.contains("ok") && o.contains("FAILED")).collect();
    s.push_str(&format!("\n## flakiest ({} tests that both passed and failed)\n\n", flaky.len()));
    for ((p, t), _) in &flaky {
        s.push_str(&format!("- {p} {t}\n"));
    }
    match maintenance::timed::report(&ctx.root) {
        Ok(rep) if !rep.tests.is_empty() => {
            s.push_str(&format!("\n## slowest timed tests (maintenance's report, {})\n\n", rep.source));
            for r in rep.tests.iter().take(15) {
                s.push_str(&format!("- {} ({}:{}): {} ms of {} ms, {:?}\n", r.test, r.file, r.line, r.ms, r.max_ms, r.outcome));
            }
        }
        _ => s.push_str("\nno #[timed_test] timings in .maintenance/timings.\n"),
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configuration_is_carried_in_the_target_word_and_read_back() {
        let c = Configuration { no_default: true, all: false, features: vec!["json".into(), "chrono".into()] };
        assert_eq!(c.args(), ["--no-default-features", "--features", "json,chrono"]);
        let word = c.target_with("lib");
        assert_eq!(word, "lib [no-default-features, features json,chrono]");
        assert_eq!(Configuration::of_target(&word), ("lib".to_string(), c));
        assert_eq!(Configuration::default().target_with("test:gate"), "test:gate");
        assert_eq!(Configuration::of_target("test:gate"), ("test:gate".to_string(), Configuration::default()));
        let all = Configuration { all: true, ..Default::default() };
        assert_eq!(Configuration::of_target(&all.target_with("bin:tool")), ("bin:tool".to_string(), all));
    }
}

/// The public surface of one Rust file, each item's signature as tokens, by its path in the file: a function's
/// signature (not its body), a type's definition, a trait's items' signatures, a constant's or static's type (not its
/// value), a re-export; a method of an inherent impl under its type's path (`Type`), its signature added to the
/// type's. Doc comments are left out: they change no reader. A file that does not parse has none.
pub fn public_signatures(text: &str) -> BTreeMap<String, String> {
    use quote::ToTokens;
    fn is_pub(v: &syn::Visibility) -> bool {
        matches!(v, syn::Visibility::Public(_))
    }
    fn attrs(a: &[syn::Attribute]) -> String {
        a.iter().filter(|x| !x.path().is_ident("doc")).map(|x| x.to_token_stream().to_string()).collect::<Vec<_>>().join(" ")
    }
    fn walk(items: &[syn::Item], prefix: &str, out: &mut BTreeMap<String, String>) {
        let key = |name: &str| if prefix.is_empty() { name.to_string() } else { format!("{prefix}::{name}") };
        for item in items {
            match item {
                syn::Item::Fn(x) if is_pub(&x.vis) => {
                    out.insert(key(&x.sig.ident.to_string()), format!("{} {}", attrs(&x.attrs), x.sig.to_token_stream()));
                }
                syn::Item::Struct(x) if is_pub(&x.vis) => {
                    let mut y = x.clone();
                    y.attrs.retain(|a| !a.path().is_ident("doc"));
                    y.fields.iter_mut().for_each(|f| f.attrs.retain(|a| !a.path().is_ident("doc")));
                    out.insert(key(&x.ident.to_string()), y.to_token_stream().to_string());
                }
                syn::Item::Enum(x) if is_pub(&x.vis) => {
                    let mut y = x.clone();
                    y.attrs.retain(|a| !a.path().is_ident("doc"));
                    y.variants.iter_mut().for_each(|v| v.attrs.retain(|a| !a.path().is_ident("doc")));
                    out.insert(key(&x.ident.to_string()), y.to_token_stream().to_string());
                }
                syn::Item::Union(x) if is_pub(&x.vis) => {
                    out.insert(key(&x.ident.to_string()), format!("{} union {} {}", attrs(&x.attrs), x.ident, x.fields.to_token_stream()));
                }
                syn::Item::Trait(x) if is_pub(&x.vis) => {
                    let sigs: Vec<String> = x
                        .items
                        .iter()
                        .map(|i| match i {
                            syn::TraitItem::Fn(f) => f.sig.to_token_stream().to_string(),
                            syn::TraitItem::Type(t) => format!("type {} {}", t.ident, t.bounds.to_token_stream()),
                            syn::TraitItem::Const(c) => format!("const {}: {}", c.ident, c.ty.to_token_stream()),
                            other => other.to_token_stream().to_string(),
                        })
                        .collect();
                    out.insert(key(&x.ident.to_string()), format!("{} trait {} {} : {} {{ {} }}", attrs(&x.attrs), x.ident, x.generics.to_token_stream(), x.supertraits.to_token_stream(), sigs.join("; ")));
                }
                syn::Item::Type(x) if is_pub(&x.vis) => {
                    out.insert(key(&x.ident.to_string()), format!("type {} {} = {}", x.ident, x.generics.to_token_stream(), x.ty.to_token_stream()));
                }
                syn::Item::Const(x) if is_pub(&x.vis) => {
                    out.insert(key(&x.ident.to_string()), format!("const {}: {}", x.ident, x.ty.to_token_stream()));
                }
                syn::Item::Static(x) if is_pub(&x.vis) => {
                    out.insert(key(&x.ident.to_string()), format!("static {} {}: {}", x.mutability.to_token_stream(), x.ident, x.ty.to_token_stream()));
                }
                syn::Item::Use(x) if is_pub(&x.vis) => {
                    let t = x.tree.to_token_stream().to_string();
                    out.insert(key(&format!("use {t}")), t);
                }
                syn::Item::Mod(x) if is_pub(&x.vis) => {
                    if let Some((_, inner)) = &x.content {
                        walk(inner, &key(&x.ident.to_string()), out);
                    }
                }
                syn::Item::Impl(x) if x.trait_.is_none() => {
                    let ty = match &*x.self_ty {
                        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
                        _ => None,
                    };
                    let Some(ty) = ty else { continue };
                    for i in &x.items {
                        if let syn::ImplItem::Fn(f) = i {
                            if is_pub(&f.vis) {
                                let e = out.entry(key(&ty)).or_default();
                                e.push_str(&format!(" | {} {}", attrs(&f.attrs), f.sig.to_token_stream()));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = BTreeMap::new();
    if let Ok(file) = syn::parse_file(text) {
        walk(&file.items, "", &mut out);
    }
    out
}

/// The public items whose signature a change alters or removes, as an affected_tests reading's
/// `change.signatures` (`<file>#<path in the file>`): each changed Rust file read at `git_ref` (a declared `git
/// show` line; a file new since has no public item to alter) and as it is now (a file gone has every one removed),
/// their [`public_signatures`] compared. An item added is no reader's change.
pub fn signature_changes(root: &std::path::Path, git_ref: &str, files: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for file in files.iter().filter(|f| f.ends_with(".rs")) {
        let before = Plan::new("git-show")
            .then(match exec::line("git-show", &exec::git_show(root, git_ref, file)) {
                Ok(l) => l,
                Err(_) => continue,
            })
            .walk(Watch::Quiet, |_, _| {})
            .ok()
            .and_then(|w| w.into_iter().next())
            .filter(|w| w.code == Some(0))
            .map(|w| public_signatures(&w.stdout))
            .unwrap_or_default();
        let now = std::fs::read_to_string(root.join(file)).map(|t| public_signatures(&t)).unwrap_or_default();
        for (path, sig) in &before {
            if now.get(path) != Some(sig) {
                let path = path.strip_prefix("use ").map(|_| "*").unwrap_or(path);
                out.push(format!("{file}#{path}"));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod signature_tests {
    use super::public_signatures;

    #[test]
    fn a_body_or_a_doc_changes_no_signature_and_a_parameter_or_a_field_does() {
        let before = "/// one\npub fn level() -> u32 {\n    1\n}\npub struct Config {\n    pub name: String,\n}\nimpl Config {\n    pub fn new() -> Self { Config { name: String::new() } }\n    fn hidden(&self) {}\n}\nfn private() {}\npub const N: u32 = 1;\n";
        let body = before.replace("    1\n", "    { 1 }\n").replace("/// one", "/// the level").replace("fn hidden(&self) {}", "fn hidden(&self) { let _ = 1; }").replace("fn private() {}", "fn private(x: u8) {}").replace("= 1;", "= 2;");
        assert_eq!(public_signatures(before), public_signatures(&body), "bodies, docs, private items and a constant's value are no reader's change");
        let a = public_signatures(before);
        let param = public_signatures(&before.replace("pub fn level() -> u32", "pub fn level(at: u8) -> u32"));
        assert_ne!(a["level"], param["level"]);
        let field = public_signatures(&before.replace("pub name: String,", "pub name: String,\n    pub digest: String,"));
        assert_ne!(a["Config"], field["Config"]);
        let method = public_signatures(&before.replace("pub fn new() -> Self", "pub fn new(name: &str) -> Self"));
        assert_ne!(a["Config"], method["Config"], "a method's signature is its type's");
        assert!(!a.contains_key("private"));
    }
}

/// The libtest words after `--` that make every test say its own time (`test m::t ... ok <0.012s>`):
/// `-Z unstable-options --report-time`, which stable libtest takes under [`REPORT_TIME_ENV`] as the kit's expansion
/// does.
pub const REPORT_TIME: &[&str] = &["-Z", "unstable-options", "--report-time"];

/// The variable that lets stable libtest take [`REPORT_TIME`].
pub const REPORT_TIME_ENV: (&str, &str) = ("RUSTC_BOOTSTRAP", "1");

/// A libtest line with its time (`test m::t ... ok <0.012s>`): the test, `ok`/`FAILED`/`ignored`, and its time in
/// milliseconds when the line says one.
pub fn timed_outcome(line: &str) -> Option<(String, String, Option<u64>)> {
    let rest = line.strip_prefix("test ")?;
    let (name, tail) = rest.rsplit_once(" ... ")?;
    let mut words = tail.split_whitespace();
    let outcome = words.next()?;
    if !matches!(outcome, "ok" | "FAILED" | "ignored") {
        return None;
    }
    let ms = words.next().and_then(|t| t.strip_prefix('<')).and_then(|t| t.strip_suffix("s>")).and_then(|s| s.parse::<f64>().ok()).map(|s| (s * 1000.0).round() as u64);
    Some((name.trim().to_string(), outcome.to_string(), ms))
}

/// maintenance's classes, by the time a test took: `unit` within 500 ms, `integration` within 10 s, `slow` within
/// 1 min; past that, `slow` at the time itself.
pub fn timing_class(ms: u64) -> (&'static str, u64) {
    [("unit", 500u64), ("integration", 10_000), ("slow", 60_000)].into_iter().find(|(_, max)| ms <= *max).unwrap_or(("slow", ms))
}

/// Where a test is written: the file (from `root`) and the line of its `#[test]` (else of its `fn`), found in the
/// `.rs` files under `package_dir` by its name, the file whose path holds most of its module path chosen.
pub fn test_place(root: &std::path::Path, package_dir: &std::path::Path, test: &str) -> Option<(String, u32)> {
    let segs: Vec<&str> = test.split("::").collect();
    let (name, modules) = segs.split_last()?;
    let mut best: Option<(usize, String, u32)> = None;
    for e in walkdir::WalkDir::new(package_dir).into_iter().filter_entry(|e| e.file_name() != "target").flatten() {
        if e.path().extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim_start();
            let t = t.strip_prefix("pub ").unwrap_or(t);
            let t = t.strip_prefix("async ").unwrap_or(t);
            if !t.starts_with(&format!("fn {name}(")) {
                continue;
            }
            let line = if i > 0 && lines[i - 1].trim_start().starts_with("#[test") { i } else { i + 1 };
            let rel = e.path().strip_prefix(root).unwrap_or(e.path()).to_string_lossy().replace('\\', "/");
            let score = modules.iter().filter(|m| rel.contains(*m)).count();
            if best.as_ref().is_none_or(|(s, ..)| score > *s) {
                best = Some((score, rel, line as u32));
            }
        }
    }
    best.map(|(_, f, l)| (f, l))
}

/// The tests' times as maintenance's timing records (`.maintenance/timings`, its `Record`): each test with a time,
/// placed by [`test_place`], its class and threshold by [`timing_class`], `passed` or `panicked` by its outcome.
pub fn timing_records(root: &std::path::Path, package_dir: &std::path::Path, results: &[(String, String, Option<u64>)], at_unix_ms: u64) -> Vec<maintenance::timed::Record> {
    results
        .iter()
        .filter(|(_, o, _)| o != "ignored")
        .filter_map(|(test, outcome, ms)| {
            let ms = (*ms)?;
            let (file, line) = test_place(root, package_dir, test)?;
            let (class, max_ms) = timing_class(ms);
            let outcome = if outcome == "ok" { maintenance::timed::Outcome::Passed } else { maintenance::timed::Outcome::Panicked };
            Some(maintenance::timed::Record { test: test.clone(), file, line, ms, max_ms, class: Some(class.into()), threshold_from: "class".into(), outcome, at_unix_ms })
        })
        .collect()
}

/// The records written as one run of maintenance's timings (`.maintenance/timings/<run>.jsonl` under `root`), so
/// `annotate` places a `#[timed_test]` for each.
pub fn write_timings(root: &std::path::Path, run: &str, records: &[maintenance::timed::Record]) -> Result<Option<PathBuf>> {
    if records.is_empty() {
        return Ok(None);
    }
    let file = root.join(maintenance::registry::DIR).join(maintenance::timed::TIMINGS).join(format!("{run}.jsonl"));
    let mut text = String::new();
    for r in records {
        text.push_str(&serde_json::to_string(r)?);
        text.push('\n');
    }
    crate::store::write_whole(&file, text.as_bytes())?;
    Ok(Some(file))
}

#[cfg(test)]
mod timed_tests {
    #[test]
    fn a_report_time_line_gives_the_test_its_time_and_its_class() {
        assert_eq!(super::timed_outcome("test tests::a ... ok <0.012s>"), Some(("tests::a".into(), "ok".into(), Some(12))));
        assert_eq!(super::timed_outcome("test tests::b ... FAILED <1.500s>"), Some(("tests::b".into(), "FAILED".into(), Some(1500))));
        assert_eq!(super::timed_outcome("test tests::c ... ok"), Some(("tests::c".into(), "ok".into(), None)));
        assert_eq!(super::timing_class(12), ("unit", 500));
        assert_eq!(super::timing_class(1500), ("integration", 10_000));
        assert_eq!(super::timing_class(70_000), ("slow", 70_000));
    }
}
