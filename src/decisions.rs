//! The questions custodian asks before it acts, declared once as xccute
//! decisions: each question names the observation tool that answers it, and
//! each answer comes back as facts and a go / no-go with its reason, said in
//! the run's output and kept in its receipt.
//!
//! - same search: is there an earlier reading of this spec over this code? (reuse it)
//! - writer: does another custodian run hold this project's folder? (refuse)
//! - job running: is a job already running? (a second start waits for the user)
//! - step input: did the earlier step give this step its input? (skip it)

use crate::store::{Journal, Receipt, Store};
use crate::Result;
use xccute_decisions::{DecisionObservationTool, DecisionQuestionSpec};
use xccute_runtime::RuntimeObservationKind;

/// custodian's journal as an observation tool: it answers from receipts.
pub fn journal_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "custodian.journal",
        program: "custodian",
        kind: RuntimeObservationKind::Custom("journal_query".into()),
        responsibility: "answers from the project's journal: the runs made here and their receipts",
        default_question: "has this been answered here before?",
        compact_fact_shape: "#seq utc command name reason",
        success_meaning: "an earlier run answers",
        empty_meaning: "no earlier run answers",
    }
}

/// The project folder's writer admission as an observation tool.
pub fn admission_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "custodian.admission",
        program: "custodian",
        kind: RuntimeObservationKind::Custom("writer_admission".into()),
        responsibility: "answers whether this run may write the project's .custodian folder",
        default_question: "does another custodian run hold the writer?",
        compact_fact_shape: "admitted | held by another run",
        success_meaning: "this run holds the writer",
        empty_meaning: "another run holds it",
    }
}

/// A job folder's own readings as an observation tool.
pub fn steps_tool() -> DecisionObservationTool {
    DecisionObservationTool {
        tool_id: "custodian.job_step",
        program: "custodian",
        kind: RuntimeObservationKind::Custom("job_step_input".into()),
        responsibility: "answers from a job's earlier readings what a step takes as its input",
        default_question: "did the earlier step give this step its input?",
        compact_fact_shape: "N values from step S rows R field F",
        success_meaning: "the step has its input",
        empty_meaning: "the step has nothing to read",
    }
}

/// An answered question.
#[derive(Debug, Clone)]
pub struct Decided {
    pub question: DecisionQuestionSpec,
    pub facts: Vec<String>,
    /// Go on with the run as asked.
    pub go: bool,
    pub reason: String,
}

impl Decided {
    pub fn said(&self) -> String {
        format!("decided: {} — {} ({})", self.question.question, self.reason, if self.facts.is_empty() { "no facts".to_string() } else { self.facts.join("; ") })
    }
}

/// Same search: the newest accepted reading of this spec over this code.
pub fn same_search(journal: &Journal, spec_hash: &str, code_digest: &str) -> Result<(Decided, Option<Receipt>)> {
    let q = journal_tool().required_question_spec("same_search", "read", "is there an earlier reading of this spec over this code?");
    let prev = journal.same_search(spec_hash, code_digest)?;
    let decided = match &prev {
        Some(p) => Decided { question: q, facts: vec![format!("#{} {} {} {}", p.seq, p.utc, p.command, p.name)], go: false, reason: format!("search #{} answers: reuse it", p.seq) },
        None => Decided { question: q, facts: Vec::new(), go: true, reason: "no earlier reading: read".into() },
    };
    Ok((decided, prev))
}

/// Job running: a job already in `jobs/running`.
pub fn job_running(store: &Store) -> Result<Decided> {
    let q = journal_tool().required_question_spec("job_running", "job-start", "is a job already running?");
    let dir = store.jobs_dir(crate::store::JobState::Running)?;
    let running: Vec<String> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    Ok(if running.is_empty() {
        Decided { question: q, facts: Vec::new(), go: true, reason: "none running: start".into() }
    } else {
        Decided { question: q, facts: running.clone(), go: false, reason: format!("{} running: close it first (job close)", running.join(", ")) }
    })
}

/// Step input: the values an earlier step gave.
pub fn step_input(step: &str, from: &str, values: &[String]) -> Decided {
    let q = steps_tool().required_question_spec(format!("step_input:{step}"), step.to_string(), format!("did step {from} give step {step} its input?"));
    if values.is_empty() {
        Decided { question: q, facts: Vec::new(), go: false, reason: format!("step {from} gave nothing: step {step} is not run") }
    } else {
        Decided { question: q, facts: vec![format!("{} values", values.len())], go: true, reason: format!("step {from} gave {} values", values.len()) }
    }
}

/// Writer admission, answered by the store's admission.
pub fn writer(admitted: std::result::Result<(), String>) -> Decided {
    let q = admission_tool().required_question_spec("writer", "run", "does another custodian run hold this project's folder?");
    match admitted {
        Ok(()) => Decided { question: q, facts: vec!["admitted".into()], go: true, reason: "this run holds the writer".into() },
        Err(holder) => Decided { question: q, facts: vec![holder.clone()], go: false, reason: format!("held by another run ({holder})") },
    }
}
