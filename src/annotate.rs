//! `custodian annotate [--from <file>] [--apply]`: a plan of the maintenance
//! attributes custodian's receipts call for, and a dry-run reading of where
//! each line would go. For each measured test, `#[timed_test(max = ..)]` at
//! the measured time times the margin, rounded up to the threshold table's
//! step; for each item a check flagged, the attribute the finding names
//! (`#[reads_env("KEY")]` at a bare env read; `#[mechanism]` and
//! `#[supersedes]` candidates listed, never applied). The margin and the steps
//! are declared data (`.custodian/config/annotate.toml`).
//!
//! `--apply` would hand the plan to code_passenger to place: code_passenger
//! has no line that places an attribute from a plan (its writers are
//! `annotate`, file headers from a compiled header plan, and `gate --write`,
//! `#[cfg]` gates), and custodian never edits source itself, so `--apply`
//! refuses with its code until code_passenger has that line.

use crate::reading::refusal::ReadingRefusal;
use crate::runs::{stamp, Ctx};
use crate::store::{named, write_whole};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SHIPPED_ANNOTATE: &str = r#"# custodian annotate: how a measured time becomes a #[timed_test] threshold.
# the measured time is multiplied by the margin, then rounded up to the first step at or above it
margin = 3.0
steps = ["50ms", "100ms", "250ms", "500ms", "1s", "2s", "5s", "10s", "30s", "1m", "5m"]
"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotateRule {
    pub margin: f64,
    pub steps: Vec<String>,
}

fn ms_of(step: &str) -> Option<u64> {
    let s = step.trim();
    if let Some(v) = s.strip_suffix("ms") {
        return v.parse().ok();
    }
    if let Some(v) = s.strip_suffix('s') {
        return v.parse::<u64>().ok().map(|n| n * 1000);
    }
    if let Some(v) = s.strip_suffix('m') {
        return v.parse::<u64>().ok().map(|n| n * 60_000);
    }
    None
}

impl AnnotateRule {
    pub fn parse(text: &str) -> Result<Self> {
        let r: AnnotateRule = toml::from_str(text).map_err(|e| ReadingRefusal::Thresholds.refuse(e))?;
        if r.steps.iter().any(|s| ms_of(s).is_none()) {
            return Err(ReadingRefusal::Thresholds.refuse("a step is not 50ms, 2s or 1m"));
        }
        Ok(r)
    }

    /// The threshold for a measured time: measured × margin, rounded up to a step.
    pub fn threshold(&self, measured_ms: u64) -> String {
        let want = (measured_ms as f64 * self.margin).ceil() as u64;
        self.steps.iter().find(|s| ms_of(s).is_some_and(|m| m >= want)).cloned().unwrap_or_else(|| self.steps.last().cloned().unwrap_or_else(|| "5m".into()))
    }
}

/// One line the plan would insert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Insert {
    /// Relative to the root.
    pub file: String,
    /// The line the attribute goes above.
    pub line: u32,
    pub text: String,
    pub reason: String,
    /// Applied by `--apply` (when there is a writer line); candidates are not.
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotatePlan {
    pub schema: String,
    pub inserts: Vec<Insert>,
    pub candidates: Vec<Insert>,
}

/// The plan from maintenance's timing report and a check's JSON (its `CheckRun`).
pub fn plan(root: &Path, rule: &AnnotateRule, check_json: Option<&str>) -> Result<AnnotatePlan> {
    let mut inserts = Vec::new();
    let mut candidates = Vec::new();
    if let Ok(rep) = maintenance::timed::report(root) {
        for r in &rep.tests {
            let max = rule.threshold(r.ms);
            inserts.push(Insert { file: r.file.clone(), line: r.line, text: format!("#[timed_test(max = \"{max}\")]"), reason: format!("measured {} ms × {} → {max} (was {} ms)", r.ms, rule.margin, r.max_ms), apply: true });
        }
    }
    if let Some(text) = check_json {
        let run: maintenance::runner::CheckRun = serde_json::from_str(text).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("not a check report: {e}")))?;
        for report in &run.reports {
            for f in report.findings.iter().chain(&report.notes) {
                let (Some(file), Some(line)) = (f.file.clone(), f.line) else { continue };
                let (text, apply) = match report.check.as_str() {
                    "reads_env" => match &f.target {
                        Some(key) => (format!("#[reads_env(\"{key}\")]"), true),
                        None => continue,
                    },
                    "mechanism" => (format!("#[mechanism(\"{}\")]  // candidate", f.target.clone().unwrap_or_default()), false),
                    "supersedes" => (format!("#[supersedes(\"{}\")]  // candidate", f.target.clone().unwrap_or_default()), false),
                    _ => continue,
                };
                let ins = Insert { file, line, text, reason: f.message.clone(), apply };
                if apply {
                    inserts.push(ins);
                } else {
                    candidates.push(ins);
                }
            }
        }
    }
    inserts.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    inserts.dedup();
    Ok(AnnotatePlan { schema: "custodian.annotate_plan.v1".into(), inserts, candidates })
}

pub fn page(p: &AnnotatePlan) -> String {
    let mut s = format!("# annotate — what would change (dry run)\n\n- lines to insert: {}\n- candidates (listed, never applied): {}\n\n## inserts\n\n", p.inserts.len(), p.candidates.len());
    for i in &p.inserts {
        s.push_str(&format!("- `{}:{}` · insert above: `{}` — {}\n", i.file, i.line, i.text, i.reason));
    }
    s.push_str("\n## candidates\n\n");
    for i in &p.candidates {
        s.push_str(&format!("- `{}:{}` · `{}` — {}\n", i.file, i.line, i.text, i.reason));
    }
    s
}

/// What `annotate` did.
#[derive(Debug)]
pub struct Annotated {
    pub plan: AnnotatePlan,
    pub plan_file: Option<PathBuf>,
    pub page: Option<PathBuf>,
    pub said: Vec<String>,
}

pub fn run(ctx: &Ctx, from: Option<&Path>, apply: bool) -> Result<Annotated> {
    let rule = match &ctx.store {
        Some(s) => {
            let p = s.path(&named::THRESHOLDS, &[("name", "annotate")])?;
            AnnotateRule::parse(&std::fs::read_to_string(&p).unwrap_or_else(|_| SHIPPED_ANNOTATE.to_string()))?
        }
        None => AnnotateRule::parse(SHIPPED_ANNOTATE)?,
    };
    let check_json = match from {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", p.display())))?),
        None => None,
    };
    let plan = plan(&ctx.root, &rule, check_json.as_deref())?;
    let mut said = Vec::new();
    let (plan_file, page_file) = match &ctx.store {
        Some(store) => {
            let pf = store.path(&named::PLAN, &[("name", "annotate")])?;
            write_whole(&pf, toml::to_string(&plan).map_err(|e| ReadingRefusal::Write.refuse(e))?.as_bytes())?;
            let dir = store.reading_dir("annotate")?;
            let page_path = dir.join(format!("{}.md", stamp()));
            write_whole(&page_path, page(&plan).as_bytes())?;
            write_whole(&dir.join("latest.md"), page(&plan).as_bytes())?;
            said.push(format!("plan: {}", ctx.rel(&pf)));
            said.push(format!("dry-run reading: {}", ctx.rel(&page_path)));
            (Some(pf), Some(page_path))
        }
        None => {
            print!("{}", page(&plan));
            said.push("free mode: nothing stored".into());
            (None, None)
        }
    };
    if apply {
        return Err(ReadingRefusal::NoWriterLine.refuse("annotate --apply: the plan is written; placing its lines needs a code_passenger line that does not exist yet"));
    }
    Ok(Annotated { plan, plan_file, page: page_file, said })
}
