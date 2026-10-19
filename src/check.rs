//! `custodian check <which>`: maintenance's own declared line
//! (`maintenance check <which> --workspace <root> --registry <file>`, from its
//! runner) run through custodian's executor like every other run, so its
//! receipt has the same shape; the JSON the line prints is written as a
//! reading. The checks stay maintenance's; custodian only runs the line.
//! `check all` in agent mode is a job: one line per check, under one id,
//! closed with its receipt.

use crate::exec::{self, Plan, Watch};
use crate::reading::refusal::ReadingRefusal;
use crate::runs::{keep, now_utc, receipt_of, stamp, Ctx, InJob};
use crate::store::{write_whole, JobRow, Receipt};
use crate::Result;
use maintenance::runner::{CheckName, CheckRun, Maintenance};
use std::path::PathBuf;

/// The checks `all` runs, one line each.
pub fn every_check() -> Vec<CheckName> {
    CheckName::ALL.iter().copied().filter(|c| *c != CheckName::All).collect()
}

pub fn check_named(word: &str) -> Result<CheckName> {
    CheckName::of_word(word).ok_or_else(|| ReadingRefusal::SpecInvalid.refuse(format!("`{word}` is not a check: {}", CheckName::ALL.iter().map(|c| c.word()).collect::<Vec<_>>().join(", "))))
}

/// What one check line did.
#[derive(Debug)]
pub struct Checked {
    pub run: Option<CheckRun>,
    pub receipt: Receipt,
    pub page: Option<PathBuf>,
    pub said: Vec<String>,
}

fn page(run: &CheckRun, line: &str) -> String {
    let mut s = format!("# check — {}\n\n- line: `{line}`\n- workspace: {}\n- registry: {}\n", run.reports.iter().map(|r| r.check.clone()).collect::<Vec<_>>().join(", "), run.workspace, run.registry.clone().unwrap_or_else(|| "-".into()));
    for r in &run.reports {
        s.push_str(&format!("\n## {} — {}\n\n", r.check, if r.passed { "passed" } else { "findings" }));
        for (k, v) in &r.counts {
            s.push_str(&format!("- {k}: {v}\n"));
        }
        if !r.findings.is_empty() {
            s.push_str(&format!("\n### findings ({})\n\n", r.findings.len()));
        }
        for f in &r.findings {
            let at = match (&f.file, f.line) {
                (Some(p), Some(l)) => format!("`{p}:{l}` · "),
                (Some(p), None) => format!("`{p}` · "),
                _ => String::new(),
            };
            s.push_str(&format!("- {at}{}{}\n", f.message, f.item.as_ref().map(|i| format!(" · item={i}")).unwrap_or_default()));
        }
        for n in &r.notes {
            s.push_str(&format!("- note: {}\n", n.message));
        }
    }
    s
}

/// One check, run: its line, its receipt, its reading.
pub fn run_check(ctx: &Ctx, which: CheckName, job: Option<&InJob>) -> Result<Checked> {
    let registry = maintenance::registry::path(&ctx.root);
    let line = Maintenance::check_of(which, ctx.root.as_os_str()).registry(registry.as_os_str());
    let plan = Plan::new(format!("check:{}", which.word())).then(exec::line(format!("check-{}", which.word()), &line)?);
    let walked = match plan.walk(Watch::Quiet, |_, _| {}) {
        Ok(w) => w,
        Err(e) if e.to_string().contains("CUST0014") => {
            return Err(ReadingRefusal::ProgramMissing.refuse(format!("maintenance (install it: custodian self install maintenance --from <the maintenance workspace>): {e}")))
        }
        Err(e) => return Err(e),
    };
    let w = &walked[0];
    let run: Option<CheckRun> = serde_json::from_str(w.stdout.trim()).ok();
    let mut said = Vec::new();
    let mut outputs = Vec::new();
    let mut page_file = None;
    if let (Some(run), Some(store)) = (&run, &ctx.store) {
        let dir = match job {
            Some(j) => j.readings.join(format!("check-{}", which.word())),
            None => store.reading_dir(&format!("check-{}", which.word()))?,
        };
        let stem = stamp();
        let p = dir.join(format!("{stem}.md"));
        let j = dir.join(format!("{stem}.json"));
        write_whole(&p, page(run, &w.operation.display).as_bytes())?;
        write_whole(&j, (serde_json::to_string_pretty(run)? + "\n").as_bytes())?;
        write_whole(&dir.join("latest.md"), page(run, &w.operation.display).as_bytes())?;
        outputs = vec![ctx.rel(&p), ctx.rel(&j)];
        page_file = Some(p);
    }
    let receipt = receipt_of(&plan, w, "check", which.word(), None, None, None, None, outputs, job);
    let receipt = match &ctx.store {
        Some(store) => {
            let (r, f) = keep(store, receipt)?;
            said.push(format!("check {}: {} (run #{}, receipt {})", which.word(), r.reason, r.seq, ctx.rel(&f)));
            r
        }
        None => {
            said.push(format!("check {}: {} (free mode, nothing stored)", which.word(), receipt.reason));
            receipt
        }
    };
    if let Some(p) = &page_file {
        said.push(format!("reading: {}", ctx.rel(p)));
    } else if run.is_none() {
        said.push(format!("the line printed no check report: {}", w.stderr.lines().last().unwrap_or("")));
    }
    Ok(Checked { run, receipt, page: page_file, said })
}

/// `check all` as a job: every check a line, one id, a closing receipt.
pub fn check_all_job(ctx: &Ctx) -> Result<(Vec<Checked>, Vec<String>)> {
    let store = ctx.store()?;
    let name = format!("check-all-{}", stamp());
    let id = format!("job-{}", crate::digest::short(&crate::digest::of(format!("{name}\n{}", now_utc())), 12));
    let folder = store.job_dir(crate::store::JobState::Done, &name)?;
    let journal = store.journal()?;
    journal.put_job(&JobRow { id: id.clone(), name: name.clone(), kind: "check-all".into(), state: "running".into(), folder: ctx.rel(&folder), opened_utc: Some(now_utc()), closed_utc: None })?;
    let in_job = InJob { id: id.clone(), readings: folder.join("readings") };
    let mut out = Vec::new();
    let mut said = vec![format!("job {id} (check all): {}", ctx.rel(&folder))];
    for c in every_check() {
        let checked = run_check(ctx, c, Some(&in_job))?;
        said.extend(checked.said.iter().cloned());
        out.push(checked);
    }
    let lines: Vec<String> = out
        .iter()
        .map(|c| format!("- {}: {} — {}", c.receipt.name, c.receipt.reason, c.run.as_ref().map(|r| r.reports.iter().map(|x| format!("{} findings", x.findings.len())).collect::<Vec<_>>().join(", ")).unwrap_or_default()))
        .collect();
    let closing = format!("# job {id} — check all\n\n{}\n", lines.join("\n"));
    write_whole(&folder.join("closing.md"), closing.as_bytes())?;
    journal.put_job(&JobRow { id: id.clone(), name, kind: "check-all".into(), state: "done".into(), folder: ctx.rel(&folder), opened_utc: None, closed_utc: Some(now_utc()) })?;
    said.push(format!("closing receipt: {}", ctx.rel(&folder.join("closing.md"))));
    Ok((out, said))
}
