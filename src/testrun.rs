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
