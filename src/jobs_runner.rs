//! The job's step runner, on `scope` (deterministic structured concurrency).
//!
//! Every step of a template job is a task in one scope. A step that reads
//! another step's readings (its `for_each` or `change_items` names that step)
//! waits until that step is done; steps that depend on nothing run together,
//! one reading per poll, interleaved in the scope's fixed order, so the same
//! job runs and completes in the same order every time. The scope environment
//! holds what every step shares: the open [`Workspace`] (borrowed, never
//! cloned per step), the step executor, and the readings done so far.
//!
//! What a reading is, and what it leaves behind (its receipt, its task-history
//! solution record), is the executor's: [`RunStep`]. `job start` runs the
//! declared reading line through it, as before, receipts and records unchanged;
//! [`InProcess`] answers over the borrowed workspace with no line at all.

use super::{values, Step};
use crate::api::Workspace;
use crate::reading::ReadingReport;
use crate::Result;
use scope::{run_scoped, Cx, ScopedTask, TaskPoll};
use std::collections::BTreeSet;

/// One reading of one step, for one target.
pub trait RunStep {
    /// `n` is the target's place in the step's targets; `ws` is the scope's
    /// open workspace, when one was opened.
    fn run(&mut self, ws: Option<&Workspace>, step: &Step, n: usize, target: &str, items: &[String], said: &mut Vec<String>) -> Result<Option<ReadingReport>>;
}

/// What happened, in order: `start <step>`, `read <step> <target>`, `done <step>`, `skip <step>`.
pub type Events = Vec<String>;

/// The scope environment: shared by every step task, borrowed for each poll.
pub struct StepEnv<'a> {
    pub ws: Option<&'a Workspace>,
    pub exec: &'a mut dyn RunStep,
    /// The job's own target (`{target}` in a step's target).
    pub target: String,
    /// The readings of every step done, in completion order.
    pub done: Vec<(String, Vec<ReadingReport>)>,
    pub events: Events,
    pub said: Vec<String>,
    /// The first refusal; every step stops at its next poll.
    pub error: Option<crate::CustodianError>,
}

enum Phase {
    Waiting,
    Running { targets: Vec<String>, items: Vec<String>, next: usize, reports: Vec<ReadingReport> },
}

struct StepTask<'s> {
    step: &'s Step,
    deps: Vec<String>,
    phase: Phase,
}

/// The steps a step reads: the ones its `for_each` and `change_items` name.
pub fn dependencies(step: &Step) -> Vec<String> {
    let mut d: Vec<String> = step.for_each.iter().chain(step.change_items.iter()).map(|f| f.step.clone()).collect();
    d.sort();
    d.dedup();
    d
}

impl<'s, 'a> ScopedTask<'s, StepEnv<'a>> for StepTask<'s>
where
    'a: 's,
{
    fn poll(&mut self, cx: &mut Cx<'_, 's, StepEnv<'a>>) -> TaskPoll {
        let env = &mut *cx.resources;
        if env.error.is_some() {
            return TaskPoll::Ready;
        }
        let step = self.step;
        match &mut self.phase {
            Phase::Waiting => {
                let finished: BTreeSet<&str> = env.done.iter().map(|(n, _)| n.as_str()).collect();
                if !self.deps.iter().all(|d| finished.contains(d.as_str())) {
                    // a dependent step waits for the steps it reads
                    return cx.yield_now();
                }
                let items = step.change_items.as_ref().map(|f| values(&env.done, f)).unwrap_or_default();
                let targets: Vec<String> = match (&step.for_each, &step.target) {
                    (Some(f), _) => values(&env.done, f),
                    (None, Some(tt)) => vec![tt.replace("{target}", &env.target)],
                    (None, None) => vec![env.target.clone()],
                };
                let gate = match (&step.change_items, &step.for_each) {
                    (Some(f), _) => Some(crate::decisions::step_input(&step.name, &f.step, &items)),
                    (None, Some(f)) => Some(crate::decisions::step_input(&step.name, &f.step, &targets)),
                    _ => None,
                };
                if let Some(g) = &gate {
                    env.said.push(g.said());
                }
                if gate.as_ref().is_some_and(|g| !g.go) {
                    // the decision said there is nothing to read: the step is not run
                    env.events.push(format!("skip {}", step.name));
                    env.done.push((step.name.clone(), Vec::new()));
                    return TaskPoll::Ready;
                }
                env.events.push(format!("start {}", step.name));
                self.phase = Phase::Running { targets, items, next: 0, reports: Vec::new() };
                cx.yield_now()
            }
            Phase::Running { targets, items, next, reports } => {
                if *next < targets.len() {
                    let t = targets[*next].clone();
                    let ws = env.ws;
                    match env.exec.run(ws, step, *next, &t, items, &mut env.said) {
                        Ok(r) => reports.extend(r),
                        Err(e) => {
                            env.error = Some(e);
                            return TaskPoll::Ready;
                        }
                    }
                    env.events.push(format!("read {} {t}", step.name));
                    *next += 1;
                    return cx.yield_now();
                }
                env.events.push(format!("done {}", step.name));
                env.done.push((step.name.clone(), std::mem::take(reports)));
                TaskPoll::Ready
            }
        }
    }
}

/// The steps run in one scope: independent ones together, dependent ones after
/// what they read. Returns the environment's readings, events and said lines.
pub fn run_steps<'a>(steps: &[Step], target: &str, ws: Option<&'a Workspace>, exec: &'a mut dyn RunStep) -> Result<(Vec<(String, Vec<ReadingReport>)>, Events, Vec<String>)> {
    let names: BTreeSet<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    for s in steps {
        if let Some(d) = dependencies(s).iter().find(|d| !names.contains(d.as_str())) {
            return Err(crate::reading::refusal::ReadingRefusal::SpecInvalid.refuse(format!("step {} reads step {d}, which the job does not have", s.name)));
        }
    }
    let mut env = StepEnv { ws, exec, target: target.to_string(), done: Vec::new(), events: Vec::new(), said: Vec::new(), error: None };
    let report = run_scoped(&mut env, |s| {
        for step in steps {
            s.spawn_named(step.name.clone(), StepTask { step, deps: dependencies(step), phase: Phase::Waiting });
        }
    });
    if let Some(e) = env.error {
        return Err(e);
    }
    if !report.all_completed() {
        return Err(crate::reading::refusal::ReadingRefusal::LineNotStarted.refuse(format!("the job's steps did not all finish: {report:?}")));
    }
    Ok((env.done, env.events, env.said))
}

/// A step answered over the scope's borrowed workspace, in this process: no
/// line, no receipt; the reading is returned to the runner.
pub struct InProcess {
    pub question: String,
}

impl RunStep for InProcess {
    fn run(&mut self, ws: Option<&Workspace>, step: &Step, _n: usize, target: &str, items: &[String], _said: &mut Vec<String>) -> Result<Option<ReadingReport>> {
        let ws = ws.ok_or_else(|| crate::reading::refusal::ReadingRefusal::NoWorkspace.refuse("an in-process step needs the scope's workspace"))?;
        let mut t = format!("name = {}\nquestion = {}\nkind = {}\ntarget = {}\n", super::toml_str(&step.name), super::toml_str(&self.question), super::toml_str(&step.kind), super::toml_str(target));
        if let Some(w) = &step.with {
            t.push_str(w);
            t.push('\n');
        }
        if !items.is_empty() {
            t.push_str(&format!("\n[change]\nitems = [{}]\n", items.iter().map(|i| super::toml_str(i)).collect::<Vec<_>>().join(", ")));
        }
        let mut spec = crate::reading::spec::ReadingSpec::from_toml(&t)?;
        // the workspace was opened once, for its scope; a step reads under it
        spec.scope = ws.scope.clone();
        Ok(Some(crate::api::run(ws, &spec)?.report))
    }
}
