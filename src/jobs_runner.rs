//! The job's step runner, on `scope` (deterministic structured concurrency).
//!
//! Every step of a template job is a task in one scope. A step that reads
//! another step's readings (its `for_each` or `change_items` names that step)
//! or its receipts (`reads`) waits until that step is done; steps that depend on nothing run together,
//! one reading per poll, interleaved in the scope's fixed order, so the same
//! job runs and completes in the same order every time. The scope environment
//! holds what every step shares: the open [`Workspace`] (borrowed, never
//! cloned per step), the step executor, and the readings done so far.
//!
//! What a reading is, and what it leaves behind (its receipt, its task-history
//! solution record), is the executor's: [`RunStep`]. `job start`'s executor
//! builds the declared reading line for each step and either runs it or, when
//! the scope holds a workspace, answers it in this process over that
//! workspace (`runs::Runner`): the same receipt and record either way, the
//! receipt naming its runner.

use super::{values, Step};
use crate::api::Workspace;
use crate::reading::ReadingReport;
use crate::Result;
use scope::{run_scoped, Cx, ScopedTask, TaskPoll};
use std::collections::{BTreeMap, BTreeSet};

/// One reading of one step, for one target.
pub trait RunStep {
    /// `n` is the target's place in the step's targets; `ws` is the scope's
    /// open workspace, when one was opened.
    fn run(&mut self, ws: Option<&Workspace>, step: &Step, n: usize, target: &str, items: &[String], said: &mut Vec<String>) -> Result<Option<ReadingReport>>;

    /// The receipt file (relative to the root) the last `run` kept, for a
    /// step that `reads` this one. An executor that keeps none says none.
    fn receipt(&self) -> Option<String> {
        None
    }
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
    /// The receipt files each step's runs kept, by step, in run order: what a
    /// step that `reads` it takes as its targets.
    pub receipts: BTreeMap<String, Vec<String>>,
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

/// The steps a step reads: the ones its `for_each`, `change_items` and `reads` name.
pub fn dependencies(step: &Step) -> Vec<String> {
    let mut d: Vec<String> = step.for_each.iter().chain(step.change_items.iter()).map(|f| f.step.clone()).chain(step.reads.iter().cloned()).collect();
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
                // the receipts of the step this one reads: each one a target, or `{receipt}` in its target
                let read = step.reads.as_ref().map(|r| env.receipts.get(r).cloned().unwrap_or_default());
                let targets: Vec<String> = match (&step.for_each, &read, &step.target) {
                    (Some(f), _, _) => values(&env.done, f),
                    (None, Some(rs), Some(tt)) => rs.iter().map(|r| tt.replace("{target}", &env.target).replace("{receipt}", r)).collect(),
                    (None, Some(rs), None) => rs.clone(),
                    (None, None, Some(tt)) => vec![tt.replace("{target}", &env.target)],
                    (None, None, None) => vec![env.target.clone()],
                };
                let gate = match (&step.change_items, &step.for_each, &step.reads) {
                    (Some(f), _, _) => Some(crate::decisions::step_input(&step.name, &f.step, &items)),
                    (None, Some(f), _) => Some(crate::decisions::step_input(&step.name, &f.step, &targets)),
                    (None, None, Some(r)) => Some(crate::decisions::step_input(&step.name, r, &targets)),
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
                        Ok(r) => {
                            reports.extend(r);
                            if let Some(f) = env.exec.receipt() {
                                env.receipts.entry(step.name.clone()).or_default().push(f);
                            }
                        }
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
    let mut env = StepEnv { ws, exec, target: target.to_string(), done: Vec::new(), events: Vec::new(), said: Vec::new(), error: None, receipts: BTreeMap::new() };
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
