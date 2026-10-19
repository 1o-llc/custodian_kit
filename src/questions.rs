// Copyright © 2026 1o. All rights reserved.

//! Decisions only a person makes, found before the work and asked together,
//! and their answers, kept as the project's task history so no answered
//! question is asked again.
//!
//! A question asked is a `problem` record (`problem` the question, its
//! `decision_id`, each `option`, `why`, `source`); its answer is a `solution`
//! record with the same `decision_id`, the `answer` and the reason it was
//! given. The id is the digest of the question's words (case and spacing
//! aside), so the same question asked by another job, another plan or another
//! agent is the same decision: answered once, answered everywhere.

use crate::reading::refusal::ReadingRefusal;
use crate::store::Store;
use crate::task_records::TaskRef;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use task_history::{DiffDocument, DiffRecordKind};

/// An open decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    /// `q-<12 hex>`: the digest of the question's words.
    pub id: String,
    pub question: String,
    pub options: Vec<String>,
    pub why: Option<String>,
    /// What raised it: `plan:<name>`, `change:<name>`, `job:<id>`, `preflight:<spec>`.
    pub source: String,
    /// The task and revision that asked it.
    #[serde(default)]
    pub task: Option<String>,
}

/// A decision made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub why: Option<String>,
    pub by: Option<String>,
    /// `<task>/<revision>`.
    pub task: String,
}

/// The id of a question: the digest of its words, lowercased, spacing collapsed.
pub fn id_of(question: &str) -> String {
    let norm: String = question.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    format!("q-{}", crate::digest::short(&crate::digest::of(norm.trim_end_matches('?')), 12))
}

/// A question as a change spec, a plan or a job writes it.
pub fn question(text: &str, options: &[String], why: Option<&str>, source: &str) -> Question {
    Question { id: id_of(text), question: text.trim().to_string(), options: options.to_vec(), why: why.map(str::to_string), source: source.to_string(), task: None }
}

/// The question every job without a declared verification is asked first.
pub fn done_question(job: &str) -> Question {
    question(
        &format!("what does done look like for {job}?"),
        &["test:<test name>".into(), "reading:<spec name that must change>".into(), "check:<maintenance check>".into()],
        Some("a job closes `accepted` only on evidence; with no verification declared it closes `pending`"),
        &format!("job:{job}"),
    )
}

/// Every decision made in the project's task histories, by id (the latest answer wins).
pub fn decided(store: &Store) -> Result<BTreeMap<String, Decision>> {
    let mut out = BTreeMap::new();
    for (task, rev, d) in records(store)? {
        if d.kind().ok() != Some(DiffRecordKind::Solution) {
            continue;
        }
        let Ok(id) = d.one("decision_id") else { continue };
        out.insert(
            id.to_string(),
            Decision {
                id: id.to_string(),
                question: d.one("question").unwrap_or_default().to_string(),
                answer: d.one("answer").unwrap_or_default().to_string(),
                why: d.one("why").ok().map(str::to_string),
                by: d.one("by").ok().map(str::to_string),
                task: format!("{task}/{rev}"),
            },
        );
    }
    Ok(out)
}

/// Every question asked and not answered, in the order asked, each once.
pub fn open(store: &Store) -> Result<Vec<Question>> {
    let done = decided(store)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (task, rev, d) in records(store)? {
        if d.kind().ok() != Some(DiffRecordKind::Problem) {
            continue;
        }
        let Ok(id) = d.one("decision_id") else { continue };
        if done.contains_key(id) || !seen.insert(id.to_string()) {
            continue;
        }
        out.push(Question {
            id: id.to_string(),
            question: d.one("problem").unwrap_or_default().to_string(),
            options: d.many("option").to_vec(),
            why: d.one("why").ok().map(str::to_string),
            source: d.one("source").unwrap_or_default().to_string(),
            task: Some(format!("{task}/{rev}")),
        });
    }
    Ok(out)
}

/// What asking a question did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Written as a problem record: open now.
    Asked,
    /// Already open (asked before): not asked again.
    AlreadyOpen,
    /// Answered before: here is the answer, nothing asked.
    Answered(Decision),
}

/// Asks `q` in task `t`, unless it is answered or open already anywhere in the project.
pub fn ask(store: &Store, t: &TaskRef, q: &Question) -> Result<Asked> {
    if let Some(d) = decided(store)?.remove(&q.id) {
        return Ok(Asked::Answered(d));
    }
    if open(store)?.iter().any(|o| o.id == q.id) {
        return Ok(Asked::AlreadyOpen);
    }
    let mut d = DiffDocument::record(DiffRecordKind::Problem).with("problem", q.question.clone()).with("decision_id", q.id.clone()).with("source", q.source.clone());
    for o in &q.options {
        d = d.with("option", o.clone());
    }
    if let Some(w) = &q.why {
        d = d.with("why", w.clone());
    }
    t.write(d)?;
    Ok(Asked::Asked)
}

/// Records the answer to an open question in the task that asked it (its latest revision when that one is closed).
pub fn answer(store: &Store, id: &str, answer: &str, why: Option<&str>, by: Option<&str>) -> Result<Decision> {
    let q = open(store)?.into_iter().find(|q| q.id == id).ok_or_else(|| ReadingRefusal::NoDecision.refuse(id))?;
    let (task, rev) = q.task.as_deref().and_then(|t| t.split_once('/')).map(|(a, b)| (a.to_string(), b.to_string())).ok_or_else(|| ReadingRefusal::NoDecision.refuse(format!("{id}: no task asked it")))?;
    let t = TaskRef { root: crate::task_records::root(store)?, task_id: task.clone(), revision: rev.clone() };
    let mut d = DiffDocument::record(DiffRecordKind::Solution).with("solution", format!("decided: {} → {answer}", q.question)).with("decision_id", id).with("question", q.question.clone()).with("answer", answer);
    if let Some(w) = why {
        d = d.with("why", w);
    }
    if let Some(b) = by {
        d = d.with("by", b);
    }
    t.write(d)?;
    task_history::write_index(&t.root).map_err(|e| ReadingRefusal::Write.refuse(e))?;
    Ok(Decision { id: id.to_string(), question: q.question, answer: answer.to_string(), why: why.map(str::to_string), by: by.map(str::to_string), task: format!("{task}/{rev}") })
}

/// Every record of every task, with its task and revision.
fn records(store: &Store) -> Result<Vec<(String, String, DiffDocument)>> {
    let root = crate::task_records::root(store)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let index = task_history::load_task_history(&root).map_err(|e| ReadingRefusal::ReadingUnreadable.refuse(format!("task history: {e}")))?;
    let mut out = Vec::new();
    for t in index.tasks {
        for r in t.revisions {
            for d in r.documents {
                out.push((t.task_directory.clone(), r.name.clone(), d));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_is_the_same_question_whatever_its_spacing() {
        assert_eq!(id_of("What does  done look like for x?"), id_of("what does done look like for x"));
        assert_ne!(id_of("which store?"), id_of("which socket?"));
        assert!(id_of("q").starts_with("q-") && id_of("q").len() == 14);
    }
}
