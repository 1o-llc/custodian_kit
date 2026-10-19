// Copyright © 2026 1o. All rights reserved.

//! An intent's plan (unbounded work): what an objective entails, found from
//! the code before anything is changed, and proposed as bounded steps.
//!
//! From the intent's words the scan's items and crates that name them are the
//! candidates (the place the work most likely is); the workspace's crates and
//! the dependencies it already uses that name them are what can be reused; a
//! word nothing names is something new to make, and where it goes is asked.
//! Each of the top candidates gets an `impact` reading (through the reading
//! path: a receipt, or a reuse), so the plan carries the affected packages,
//! use sites, feature gates and tests, and the cost by dimension, measured,
//! heuristic or unknown. Alternatives are written only where there is a real
//! choice (two candidates the words name equally; reuse a crate or make a new
//! module). The decisions the plan cannot make are asked together, before any
//! step, and an answered one is not asked again.
//!
//! The plan's steps are proposals: `custodian plan approve <plan> <step>`
//! writes the step as a change spec (`data/specs/changes/`), which its
//! preflight authorizes or not. A plan writes no code and grants no write.

use crate::change::{ChangeSpec, Operation};
use crate::questions::Question;
use crate::reading::impact::{Dimension, ImpactRecord};
use crate::reading::refusal::ReadingRefusal;
use crate::runs::{Ctx, InJob, Runner};
use crate::store::{named, write_whole, Receipt, Store, RECEIPT_SCHEMA};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const PLAN_SCHEMA: &str = "custodian.plan.v1";

/// Words an intent is written with that name nothing in code.
const STOP: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "into", "add", "make", "new", "support", "implement", "change", "what", "would", "need", "needed", "required", "every", "all", "any", "each", "its", "their", "our", "has", "have", "can", "should", "must", "will", "use", "using", "used", "how", "why", "when", "where", "which", "across", "between", "about", "also", "more", "less", "than", "then", "there", "here", "out", "get", "set", "let", "not", "but", "are", "was", "were", "been", "being", "does", "did", "done", "one", "two", "workflow", "capability", "behavior", "behaviour", "requested", "determine", "identify", "everything",
];

/// One place the work likely is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// An item path or a `crate:<name>` route.
    pub target: String,
    /// `crate`, or the item's kind (`struct`, `fn`, ...).
    pub kind: String,
    /// The intent's words it names.
    pub words: Vec<String>,
    pub file: Option<String>,
    /// The operations a bounded step on it would take.
    pub operations: Vec<String>,
}

/// Something that can be reused rather than written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reuse {
    pub name: String,
    /// `workspace crate` or `dependency of <crate>`.
    pub what: String,
    pub words: Vec<String>,
}

/// A real choice, with what each side costs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alternative {
    pub choice: String,
    pub options: Vec<String>,
    pub costs: Vec<String>,
}

/// One proposed step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub n: usize,
    pub operation: String,
    pub target: String,
    pub what: String,
    /// `proposed`, then `approved` with the change spec it became.
    pub status: String,
    #[serde(default)]
    pub spec: Option<String>,
}

/// A plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub schema: String,
    pub name: String,
    pub intent: String,
    pub utc: String,
    pub words: Vec<String>,
    /// The first [`KEPT`] candidates, in rank order.
    pub candidates: Vec<Candidate>,
    /// How many candidates the scan found (the plan keeps the first [`KEPT`]).
    #[serde(default)]
    pub candidates_found: usize,
    pub reuse: Vec<Reuse>,
    /// Words nothing in the scan names: what may need making.
    pub new: Vec<String>,
    /// Per candidate read: (target, its impact reading's receipt, the record).
    pub impacts: Vec<(String, Option<i64>, ImpactRecord)>,
    pub packages: Vec<String>,
    pub feature_gates: Vec<String>,
    pub tests: Vec<String>,
    /// The cost over every candidate read, by dimension.
    pub cost: Vec<Dimension>,
    pub risks: Vec<String>,
    pub alternatives: Vec<Alternative>,
    pub decisions: Vec<Question>,
    pub answered: Vec<crate::questions::Decision>,
    pub steps: Vec<PlanStep>,
}

fn words_of(intent: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in intent.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        let w = w.trim_matches('_').to_lowercase();
        if w.len() >= 3 && !STOP.contains(&w.as_str()) && !out.contains(&w) && !w.chars().all(|c| c.is_ascii_digit()) {
            out.push(w);
        }
    }
    out
}

/// A name's words: `JobRecord` → job, record; `serve_metrics` → serve, metrics.
fn parts(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for seg in name.split('_') {
        let mut cur = String::new();
        for c in seg.chars() {
            if c.is_uppercase() && !cur.is_empty() {
                out.push(cur.to_lowercase());
                cur.clear();
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur.to_lowercase());
        }
    }
    out
}

/// A candidate's place in the list, first to last: the most intent words named; then the longest run of them in
/// the intent's order inside the name (`uds route table` in `UdsRouteTable` is 3); then the largest share of the
/// name's own words the intent names; then code before tests (unless the intent says test); then the shortest path
/// (the outermost declaration); then the name.
fn rank(c: &Candidate, words: &[String]) -> (std::cmp::Reverse<usize>, std::cmp::Reverse<usize>, std::cmp::Reverse<usize>, bool, usize, String) {
    let last = c.target.rsplit("::").next().unwrap_or(&c.target).trim_start_matches("crate:");
    let ps = parts(last);
    let mut run = 0;
    for i in 0..words.len() {
        for j in 0..ps.len() {
            let n = words[i..].iter().zip(&ps[j..]).take_while(|(w, p)| w == p).count();
            run = run.max(n);
        }
    }
    let share = if ps.is_empty() { 0 } else { ps.iter().filter(|p| words.contains(p)).count() * 1000 / ps.len() };
    let test = !words.iter().any(|w| w == "test" || w == "tests") && (c.target.contains("::tests::") || c.file.as_deref().is_some_and(|f| f.starts_with("tests/") || f.contains("/tests/")));
    (std::cmp::Reverse(c.words.len()), std::cmp::Reverse(run), std::cmp::Reverse(share), test, c.target.matches("::").count(), c.target.clone())
}

fn operations_for(kind: &str) -> Vec<String> {
    let ops: &[Operation] = match kind {
        "struct" => &[Operation::AddField, Operation::AddMethod, Operation::AddTraitImpl],
        "enum" => &[Operation::AddVariant, Operation::AddMethod],
        "fn" | "method" => &[Operation::ChangeSignature, Operation::AddTest, Operation::Rename],
        "trait" => &[Operation::AddMethod],
        "crate" => &[Operation::MakeStruct, Operation::MakeFn, Operation::AddModule],
        _ => &[Operation::Rename, Operation::AddAttribute],
    };
    ops.iter().map(|o| o.as_str().to_string()).collect()
}

/// The scan's candidates and reuse for the intent's words.
pub fn investigate(f: &crate::reading::facts::Facts, words: &[String]) -> (Vec<Candidate>, Vec<Reuse>, Vec<String>) {
    let mut found: BTreeMap<String, Candidate> = BTreeMap::new();
    for d in &f.k.items {
        if d.generated.is_some() || matches!(d.kind, klepto::graph::NodeKind::Module | klepto::graph::NodeKind::Impl | klepto::graph::NodeKind::External) {
            continue;
        }
        let ps = parts(&d.name);
        let hit: Vec<String> = words.iter().filter(|w| ps.contains(w)).cloned().collect();
        if hit.is_empty() {
            continue;
        }
        let kind = format!("{:?}", d.kind).to_lowercase();
        found.insert(d.id.clone(), Candidate { target: d.id.clone(), operations: operations_for(&kind), kind, words: hit, file: Some(f.rel(&d.location.path)) });
    }
    for p in f.packages.values() {
        let ps = parts(&p.name);
        let hit: Vec<String> = words.iter().filter(|w| ps.contains(w)).cloned().collect();
        if !hit.is_empty() {
            found.insert(format!("crate:{}", p.name), Candidate { target: format!("crate:{}", p.name), kind: "crate".into(), words: hit, file: Some(format!("{}/Cargo.toml", p.dir).trim_start_matches('/').to_string()), operations: operations_for("crate") });
        }
    }
    let mut candidates: Vec<Candidate> = found.into_values().collect();
    candidates.sort_by_cached_key(|c| rank(c, words));
    let mut reuse = Vec::new();
    for p in f.packages.values() {
        let hit: Vec<String> = words.iter().filter(|w| parts(&p.name).contains(w)).cloned().collect();
        if !hit.is_empty() {
            reuse.push(Reuse { name: p.name.clone(), what: "workspace crate".into(), words: hit });
        }
    }
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (pkg, m) in &f.k.features {
        for d in &m.dependencies {
            if !f.packages.contains_key(&d.name.replace('-', "_")) {
                deps.entry(d.name.clone()).or_default().insert(pkg.clone());
            }
        }
    }
    for (dep, users) in deps {
        let hit: Vec<String> = words.iter().filter(|w| parts(&dep.replace('-', "_")).contains(w)).cloned().collect();
        if !hit.is_empty() {
            reuse.push(Reuse { name: dep, what: format!("dependency of {}", users.into_iter().collect::<Vec<_>>().join(", ")), words: hit });
        }
    }
    let named: BTreeSet<&String> = candidates.iter().flat_map(|c| c.words.iter()).chain(reuse.iter().flat_map(|r| r.words.iter())).collect();
    let new: Vec<String> = words.iter().filter(|w| !named.contains(w)).cloned().collect();
    (candidates, reuse, new)
}

/// How many candidates a plan keeps, in rank order: the rest are counted, not listed (a word as common as
/// `record` names thousands of items in a large workspace).
pub const KEPT: usize = 25;

/// How many candidates get an impact reading.
pub const READ: usize = 3;

/// What a plan run left.
#[derive(Debug, Clone)]
pub struct Ran {
    pub plan: Plan,
    pub receipt: Option<Receipt>,
    pub outputs: Vec<String>,
    pub said: Vec<String>,
}

/// The plan of an intent: investigated, its top candidates' impact read, its decisions asked, its steps proposed;
/// kept (`data/plans/intent/<name>.json` and `.md`) with a `plan` receipt; its records in the running job's task
/// history, or a task of its own (`plan-<name>`).
pub fn run_plan(ctx: &Ctx, name: &str, intent: &str, job: Option<&InJob>, runner: Runner<'_>) -> Result<Ran> {
    let mut said = Vec::new();
    let words = words_of(intent);
    if words.is_empty() {
        return Err(ReadingRefusal::SpecInvalid.refuse(format!("`{intent}`: no word in it names anything to look for")));
    }
    let scope = crate::reading::spec::Scope { tests: true, ..Default::default() };
    let facts_owned;
    let facts = match runner {
        Runner::InProcess(ws) if ws.scope == scope => ws.facts(),
        _ => {
            facts_owned = crate::reading::facts::Facts::build(&ctx.root, &scope, if job.is_some() { crate::paths::Resolving::Job } else { crate::paths::Resolving::Free })?;
            &facts_owned
        }
    };
    let (candidates, reuse, new) = investigate(facts, &words);
    said.push(format!("plan {name}: words {}; {} candidates, {} to reuse, {} named by nothing", words.join(", "), candidates.len(), reuse.len(), new.len()));
    // the top candidates' impact, through the reading path
    let mut impacts = Vec::new();
    let dir = match (&ctx.store, job) {
        (_, Some(j)) => j.readings.parent().map(|p| p.join("steps")).unwrap_or_else(|| j.readings.clone()),
        (Some(s), None) => s.dir_of(&named::INTENT_PLAN, &[("name", "x")])?.join(name),
        (None, None) => std::env::temp_dir().join(format!("custodian-plan-{}", std::process::id())),
    };
    for c in candidates.iter().take(READ) {
        // the same search `run TARGET --kind impact` and a preflight make: one key
        let spec = crate::reading::spec::ReadingSpec::search(crate::reading::spec::ReadingKind::Impact, &c.target)?;
        let iname = spec.name.clone();
        let rec = match &ctx.store {
            Some(_) => {
                let p = dir.join(crate::oneo::Doc::Reading.file_name(&iname));
                write_whole(&p, spec.to_1o().or_else(|_| toml::to_string(&spec).map_err(|e| ReadingRefusal::Write.refuse(e)))?.as_bytes())?;
                match crate::runs::run_reading_by(ctx, &p, false, job, runner) {
                    Ok(r) => {
                        said.extend(r.said.iter().cloned());
                        let seq = r.receipt.as_ref().map(|x| x.reused_seq.unwrap_or(x.seq));
                        let rep = r.receipt.as_ref().and_then(|x| x.outputs.iter().find(|o| o.ends_with(".json"))).and_then(|j| crate::reading::load(&ctx.root.join(j)).ok());
                        rep.map(|rep| (seq, ImpactRecord::of(&rep)))
                    }
                    Err(e) => {
                        said.push(format!("impact of {} not read: {e}", c.target));
                        None
                    }
                }
            }
            None => crate::reading::read_facts(facts, Path::new(&iname), &spec, &crate::runs::stamp()).ok().map(|rep| (None, ImpactRecord::of(&rep))),
        };
        if let Some((seq, r)) = rec {
            impacts.push((c.target.clone(), seq, r));
        }
    }
    let set = |f: fn(&ImpactRecord) -> &Vec<String>| -> Vec<String> { impacts.iter().flat_map(|(_, _, i)| f(i).iter().cloned()).collect::<BTreeSet<_>>().into_iter().collect() };
    let packages = set(|i| &i.packages);
    let feature_gates = set(|i| &i.feature_gates);
    let tests = set(|i| &i.test_lines);
    // the cost over every candidate read: the measured dimensions summed, the unknown ones kept unknown
    let mut cost: BTreeMap<String, Dimension> = BTreeMap::new();
    for (_, _, i) in &impacts {
        for d in &i.dimensions {
            let e = cost.entry(d.name.clone()).or_insert_with(|| Dimension { name: d.name.clone(), value: d.value.map(|_| 0), basis: d.basis, note: format!("summed over the {} candidates read; {}", impacts.len(), d.note) });
            if let (Some(a), Some(b)) = (e.value, d.value) {
                e.value = Some(a + b);
            }
        }
    }
    let risks: Vec<String> = impacts.iter().map(|(t, _, i)| format!("{t}: {}", i.risk_class)).collect();
    // real choices only
    let mut alternatives = Vec::new();
    // a tie is the same words named, the same run of them, the same share of the name: the facts cannot choose
    let tie = |c: &Candidate| {
        let r = rank(c, &words);
        (r.0, r.1, r.2)
    };
    let top = candidates.first().map(tie);
    let tied: Vec<&Candidate> = candidates.iter().filter(|c| Some(tie(c)) == top && !c.words.is_empty()).take(4).collect();
    if tied.len() > 1 {
        alternatives.push(Alternative {
            choice: format!("where the work is: {} candidates name the same words", tied.len()),
            options: tied.iter().map(|c| c.target.clone()).collect(),
            costs: tied.iter().map(|c| impacts.iter().find(|(t, _, _)| t == &c.target).map(|(_, _, i)| i.line()).unwrap_or_else(|| format!("{}: not read", c.target))).collect(),
        });
    }
    if !new.is_empty() && !reuse.is_empty() {
        alternatives.push(Alternative {
            choice: format!("{}: reuse a crate the workspace has, or make it new", new.join(", ")),
            options: reuse.iter().map(|r| format!("reuse {} ({})", r.name, r.what)).chain(std::iter::once("a new module".to_string())).collect(),
            costs: vec!["reuse: the impact of the crate reused (above)".into(), "new: a new module, no existing use site moves".into()],
        });
    }
    // the decisions it cannot make, asked together
    let source = format!("plan:{name}");
    let mut questions = Vec::new();
    for a in &alternatives {
        questions.push(crate::questions::question(&format!("{}: which?", a.choice), &a.options, Some("the plan cannot choose between them from the code"), &source));
    }
    for w in &new {
        let near = candidates.first().map(|c| c.target.clone()).unwrap_or_else(|| "a crate of the workspace".into());
        questions.push(crate::questions::question(&format!("nothing in the code is named {w}: where does it go?"), &[format!("near {near}"), "a new crate".into()], Some("a new module's place is a design decision"), &source));
    }
    if candidates.is_empty() {
        questions.push(crate::questions::question(&format!("nothing in the code names {}: is the intent about this workspace?", words.join(", ")), &[], None, &source));
    }
    let in_job_verified = ctx.store.as_ref().and_then(crate::preflight::job_view).is_some_and(|(v, _)| v.outcome_known);
    if !in_job_verified {
        questions.push(crate::questions::question(&format!("what does done look like for {name}?"), &["test:<test name>".into(), "reading:<spec name that must change>".into(), "check:<maintenance check>".into()], Some("a plan's steps close accepted only on evidence"), &source));
    }
    let decided = ctx.store.as_ref().map(crate::questions::decided).transpose()?.unwrap_or_default();
    let (answered, open): (Vec<Question>, Vec<Question>) = questions.into_iter().partition(|q| decided.contains_key(&q.id));
    let answered: Vec<crate::questions::Decision> = answered.iter().filter_map(|q| decided.get(&q.id).cloned()).collect();
    let steps: Vec<PlanStep> = candidates
        .iter()
        .take(READ)
        .enumerate()
        .map(|(i, c)| PlanStep { n: i + 1, operation: c.operations.first().cloned().unwrap_or_default(), target: c.target.clone(), what: format!("{} {} (it names {}); other operations: {}", c.operations.first().cloned().unwrap_or_default(), c.target, c.words.join(", "), c.operations.iter().skip(1).cloned().collect::<Vec<_>>().join(", ")), status: "proposed".into(), spec: None })
        .collect();
    let plan = Plan {
        schema: PLAN_SCHEMA.into(),
        name: name.into(),
        intent: intent.into(),
        utc: crate::runs::now_utc(),
        words,
        candidates_found: candidates.len(),
        candidates: candidates.into_iter().take(KEPT).collect(),
        reuse,
        new,
        impacts,
        packages,
        feature_gates,
        tests,
        cost: cost.into_values().collect(),
        risks,
        alternatives,
        decisions: open,
        answered,
        steps,
    };
    let Some(store) = &ctx.store else {
        return Ok(Ran { plan, receipt: None, outputs: Vec::new(), said });
    };
    let (receipt, outputs) = keep_plan(ctx, store, &plan, job, &mut said)?;
    Ok(Ran { plan, receipt: Some(receipt), outputs, said })
}

fn keep_plan(ctx: &Ctx, store: &Store, plan: &Plan, job: Option<&InJob>, said: &mut Vec<String>) -> Result<(Receipt, Vec<String>)> {
    let json_path = store.path(&named::INTENT_PLAN, &[("name", &plan.name)])?;
    let json = serde_json::to_string_pretty(plan)? + "\n";
    write_whole(&json_path, json.as_bytes())?;
    let md = json_path.with_extension("md");
    write_whole(&md, markdown(plan).as_bytes())?;
    // its records: the running job's task, else a task of its own
    let task = match crate::preflight::job_view(store) {
        Some((_, folder)) => {
            let jf = crate::jobs::job_file(&folder);
            crate::task_records::of_job(store, jf.task_id.as_deref(), jf.revision.as_deref())?
        }
        None => Some(crate::task_records::open(store, &format!("plan-{}", plan.name), "plan", &plan.name, &plan.intent)?),
    };
    if let Some(t) = &task {
        for (_, seq, i) in &plan.impacts {
            t.write(i.to_record(*seq))?;
        }
        for q in &plan.decisions {
            crate::questions::ask(store, t, q)?;
        }
        task_history::write_index(&t.root).map_err(|e| ReadingRefusal::Write.refuse(e))?;
        said.push(format!("task history: {}/{} ({} impacts, {} decisions asked)", t.task_id, t.revision, plan.impacts.len(), plan.decisions.len()));
    }
    let outputs = vec![ctx.rel(&json_path), ctx.rel(&md)];
    let digest = crate::digest::tagged(&json);
    let r = Receipt {
        schema: RECEIPT_SCHEMA.into(),
        seq: 0,
        utc: plan.utc.clone(),
        command: "plan".into(),
        name: plan.name.clone(),
        spec: None,
        spec_hash: Some(crate::digest::of_value(&(PLAN_SCHEMA, &plan.intent))),
        code_digest: None,
        index_key: None,
        plan_id: format!("plan:{}", plan.name),
        plan_digest: digest.clone(),
        operation_digest: digest.clone(),
        receipt_digest: digest,
        line: format!("custodian plan {}", plan.intent),
        exit_code: Some(0),
        accepted: true,
        disposition: "Continue".into(),
        reason: if plan.decisions.is_empty() { "planned".into() } else { format!("planned, {} decisions open", plan.decisions.len()) },
        outputs: outputs.clone(),
        job: job.map(|j| j.id.clone()),
        reused_seq: None,
        millis: 0,
        runner: crate::receipts::RUNNER_IN_PROCESS.into(),
    };
    let (r, f) = crate::receipts::keep(store, r)?;
    said.push(format!("plan: {} · receipt #{} {}", outputs.join(" + "), r.seq, ctx.rel(&f)));
    Ok((r, outputs))
}

/// The plan as a page: the decisions first, then the steps, then the evidence.
pub fn markdown(p: &Plan) -> String {
    let mut s = format!("# plan — {}\n\n- intent: {}\n- words looked for: {}\n", p.name, p.intent, p.words.join(", "));
    if !p.decisions.is_empty() {
        s.push_str("\n## decisions to make before any step\n\n");
        for q in &p.decisions {
            s.push_str(&format!("- `{}` {}{}\n", q.id, q.question, if q.options.is_empty() { String::new() } else { format!(" — {}", q.options.join(" | ")) }));
        }
    }
    if !p.answered.is_empty() {
        s.push_str("\n## answered before (not asked again)\n\n");
        for d in &p.answered {
            s.push_str(&format!("- {} → {} ({})\n", d.question, d.answer, d.task));
        }
    }
    s.push_str("\n## proposed steps (each becomes a change spec when approved: custodian plan approve)\n\n");
    for st in &p.steps {
        s.push_str(&format!("{}. [{}] {}{}\n", st.n, st.status, st.what, st.spec.as_ref().map(|x| format!(" → `{x}`")).unwrap_or_default()));
    }
    if !p.alternatives.is_empty() {
        s.push_str("\n## alternatives\n\n");
        for a in &p.alternatives {
            s.push_str(&format!("- {}\n", a.choice));
            for (o, c) in a.options.iter().zip(a.costs.iter().chain(std::iter::repeat(&String::new()))) {
                s.push_str(&format!("  - {o}{}\n", if c.is_empty() { String::new() } else { format!(": {c}") }));
            }
        }
    }
    s.push_str("\n## where the work is\n\n");
    for c in &p.candidates {
        s.push_str(&format!("- {} ({}) names {}{}\n", c.target, c.kind, c.words.join(", "), c.file.as_ref().map(|f| format!(" · `{f}`")).unwrap_or_default()));
    }
    if p.candidates_found > p.candidates.len() {
        s.push_str(&format!("- and {} more ranked below these (counted, not kept)\n", p.candidates_found - p.candidates.len()));
    }
    if !p.reuse.is_empty() {
        s.push_str("\n## to reuse\n\n");
        for r in &p.reuse {
            s.push_str(&format!("- {} ({}) names {}\n", r.name, r.what, r.words.join(", ")));
        }
    }
    if !p.new.is_empty() {
        s.push_str(&format!("\n## named by nothing in the code (to make)\n\n{}\n", p.new.iter().map(|w| format!("- {w}\n")).collect::<String>()));
    }
    s.push_str(&format!("\n## affected\n\n- packages: {}\n- feature gates: {}\n- tests: {}\n- risk: {}\n", p.packages.join(", "), p.feature_gates.join(", "), p.tests.len(), p.risks.join("; ")));
    if !p.cost.is_empty() {
        s.push_str("\n## cost\n\n| dimension | value | basis |\n|---|---|---|\n");
        for d in &p.cost {
            s.push_str(&format!("| {} | {} | {} |\n", d.name, d.value.map(|v| v.to_string()).unwrap_or_else(|| "unknown".into()), d.basis.as_str()));
        }
    }
    s
}

/// The placeholder an approved step's spec carries where a person or agent fills in the operation's details.
pub const TODO: &str = "TODO";

/// A plan, read back.
pub fn load(store: &Store, name: &str) -> Result<(Plan, PathBuf)> {
    let p = store.path(&named::INTENT_PLAN, &[("name", name)])?;
    let text = std::fs::read_to_string(&p).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", p.display())))?;
    Ok((serde_json::from_str(&text)?, p))
}

/// `plan approve <plan> <step> [--operation OP]`: the step as a change spec (`data/specs/changes/`), its operation's
/// details left `TODO` where the plan cannot know them; the plan's step marked approved. The spec is proposed until
/// its preflight authorizes it (a `TODO` left in it is a precondition that does not hold).
pub fn approve(store: &Store, name: &str, step: usize, operation: Option<&str>) -> Result<(PathBuf, Plan)> {
    let (mut plan, file) = load(store, name)?;
    let have: Vec<String> = plan.steps.iter().map(|s| s.n.to_string()).collect();
    let st = plan.steps.iter_mut().find(|s| s.n == step).ok_or_else(|| ReadingRefusal::NoPlanStep.refuse(format!("{name} step {step}: its steps are {}", have.join(", "))))?;
    let op: Operation = operation.unwrap_or(&st.operation).parse().map_err(|e: String| ReadingRefusal::ChangeInvalid.refuse(e))?;
    let spec_name = format!("{}-{step}", crate::jobs::slug_of(name));
    let mut spec = ChangeSpec { name: spec_name.clone(), objective: format!("{} (plan {name}, step {step})", plan.intent), operation: Some(op), target: st.target.clone(), ..ChangeSpec::default() };
    // the operation's sections, with what the plan cannot know left TODO
    for need in op.needs() {
        match *need {
            "item" => spec.item = Some(crate::change::ItemSpec { name: TODO.into(), vis: Some("pub".into()), ..Default::default() }),
            "function" => spec.function = Some(crate::change::FnSpec { name: TODO.into(), vis: Some("pub".into()), doc: Some(TODO.into()), ..Default::default() }),
            "functions" => spec.functions = vec![crate::change::FnSpec { name: TODO.into(), ..Default::default() }],
            "field" => spec.field = Some(crate::change::FieldSpec { name: TODO.into(), ty: TODO.into(), ..Default::default() }),
            "variant" => spec.variant = Some(crate::change::VariantSpec { name: TODO.into(), ..Default::default() }),
            "derive" => spec.derive = vec![TODO.into()],
            "attribute" => spec.attribute = Some(format!("#[{TODO}]")),
            "module" => spec.module = Some(crate::change::ModuleSpec { name: TODO.to_lowercase(), ..Default::default() }),
            "dependency" => spec.dependency = Some(crate::change::DependencySpec { name: TODO.into(), ..Default::default() }),
            "trait" => spec.trait_name = Some(TODO.into()),
            "new_name" => spec.new_name = Some(TODO.into()),
            "to" => spec.to = Some(TODO.into()),
            "items" => spec.items = vec![TODO.into()],
            "feature" => spec.feature = Some(TODO.into()),
            "linkage" => spec.linkage = Some(TODO.into()),
            _ => {}
        }
    }
    spec.decisions = plan.decisions.iter().map(|q| crate::change::Question { question: q.question.clone(), options: q.options.clone(), why: q.why.clone() }).collect();
    let out = store.path(&named::CHANGE_SPEC, &[("name", &spec_name)])?.with_file_name(format!("{spec_name}.toml"));
    write_whole(&out, spec.to_toml()?.as_bytes())?;
    st.status = "approved".into();
    st.spec = Some(out.strip_prefix(&store.root).unwrap_or(&out).to_string_lossy().into_owned());
    write_whole(&file, (serde_json::to_string_pretty(&plan)? + "\n").as_bytes())?;
    write_whole(&file.with_extension("md"), markdown(&plan).as_bytes())?;
    Ok((out, plan))
}

/// Whether a spec still carries a placeholder an approval left.
pub fn has_todo(spec: &ChangeSpec) -> bool {
    serde_json::to_string(spec).is_ok_and(|t| t.contains(&format!("\"{TODO}\"")) || t.contains(&format!("\"{}\"", TODO.to_lowercase())) || t.contains(&format!("#[{TODO}]")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_named_thing_ranks_before_names_that_share_its_words() {
        let words: Vec<String> = ["record", "contract", "artifact", "uds", "route", "table", "read"].iter().map(|s| s.to_string()).collect();
        let c = |t: &str, f: &str, hit: &[&str]| Candidate { target: t.into(), kind: "struct".into(), words: hit.iter().map(|s| s.to_string()).collect(), file: Some(f.into()), operations: vec![] };
        let mut cs = [
            c("nodeplan_macros::expand_nodeplan_daemon_uds_contract_from_artifact", "registry/nodeplan_macros/src/lib.rs", &["contract", "artifact", "uds"]),
            c("nodeplan_package::tests::uds_function_route_binding_blocks_when_artifact_is_not_ready", "registry/nodeplan_package/src/lib.rs", &["artifact", "uds", "route"]),
            c("nodeplan_sdk::uds_contract::UdsRouteTable", "registry/nodeplan_sdk/src/uds_contract.rs", &["uds", "route", "table"]),
        ];
        cs.sort_by_cached_key(|x| rank(x, &words));
        assert_eq!(cs[0].target, "nodeplan_sdk::uds_contract::UdsRouteTable");
        assert!(cs[2].target.contains("::tests::"), "a test item is last when the intent does not say test");
    }

    #[test]
    fn an_intent_is_read_as_the_words_code_is_named_with() {
        assert_eq!(words_of("Add a digest to the job record and store it"), ["digest", "job", "record", "store"]);
        assert_eq!(parts("JobRecord"), ["job", "record"]);
        assert_eq!(parts("serve_metrics"), ["serve", "metrics"]);
    }
}
