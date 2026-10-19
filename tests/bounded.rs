// Copyright © 2026 1o. All rights reserved.

//! Change specs and their preflight, the gate a code-changing step passes, decisions asked once, the modes of a job
//! and the two facts it starts from, and an intent's plan, on copies of the fixture workspace.

use custodian_kit::api::{Resolving, Scope, Workspace};
use custodian_kit::jobs::{self, JobMode};
use custodian_kit::preflight;
use custodian_kit::questions;
use custodian_kit::runs::{Ctx, Runner};
use custodian_kit::store::{JobState, Mode, Store};
use std::path::{Path, PathBuf};

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        if e.file_name() == "target" {
            continue;
        }
        if e.path().is_dir() {
            copy(&e.path(), &to.join(e.file_name()));
        } else {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

/// An initialized copy of the fixture (agent mode), in a fresh folder of its own.
fn project(tag: &str) -> (Store, Ctx) {
    let dir = std::env::temp_dir().join(format!("custodian-kit-bounded-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ws"), &dir);
    let (store, _) = Store::init(&dir, Mode::Agent).unwrap();
    let ctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: PathBuf::from("custodian") };
    (store, ctx)
}

fn ws(root: &Path) -> Workspace {
    Workspace::open_with(root, &Scope { tests: true, ..Default::default() }, Resolving::Free).unwrap()
}

/// A change spec in custodian's spec folder (`data/specs/changes/`), where an agent writes them.
fn spec(root: &Path, name: &str, text: &str) -> PathBuf {
    let dir = root.join(".custodian/data/specs/changes");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("{name}.toml"));
    std::fs::write(&p, text).unwrap();
    p
}

const ADD_DIGEST: &str = "name = \"config-digest\"\nobjective = \"carry the config's digest\"\noperation = \"add_field\"\ntarget = \"core_lib::Config\"\n\n[field]\nname = \"digest\"\nty = \"String\"\nvis = \"pub\"\ndefault = \"String::new()\"\n";

#[test]
fn a_field_with_a_default_is_authorized_and_lands_inside_the_struct() {
    let (store, ctx) = project("field");
    let w = ws(&store.root);
    let s = spec(&store.root, "config-digest", ADD_DIGEST);
    let ran = preflight::run_preflight(&ctx, &s, None, Runner::InProcess(&w)).unwrap();
    let p = &ran.preflight;
    assert!(p.authorized, "{:?} {:?}", p.reasons, p.preconditions);
    assert_eq!(p.landing.as_ref().unwrap().file, "crates/core_lib/src/lib.rs");
    assert!(p.landing.as_ref().unwrap().line().starts_with("inside core_lib::Config at crates/core_lib/src/lib.rs:"));
    assert_eq!(p.write_set, ["crates/core_lib/src/lib.rs"], "the literal in Config::new is in the same file");
    assert!(p.impact.as_ref().unwrap().dimensions.iter().any(|d| d.name == "runtime cost" && d.value.is_none()));
    let r = ran.receipt.unwrap();
    assert_eq!((r.command.as_str(), r.reason.as_str()), ("preflight", "authorized"));
    assert!(r.outputs.iter().any(|o| o.ends_with(".json")) && r.outputs.iter().any(|o| o.ends_with(".md")));
    // the same spec again over the same code: the impact reading is a reuse
    let again = preflight::run_preflight(&ctx, &s, None, Runner::InProcess(&w)).unwrap();
    assert!(again.said.iter().any(|l| l.contains("reused, not read again")), "{:?}", again.said);
}

#[test]
fn a_change_that_breaks_uses_is_not_authorized_and_lists_them() {
    let (store, ctx) = project("break");
    let w = ws(&store.root);
    let no_default = spec(&store.root, "f", &ADD_DIGEST.replace("default = \"String::new()\"\n", ""));
    let p = preflight::run_preflight(&ctx, &no_default, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized);
    assert!(p.listed.iter().any(|l| l.starts_with("struct literal crates/core_lib/src/lib.rs:")), "{:?}", p.listed);
    assert!(p.reasons.iter().any(|r| r.contains("preconditions")), "{:?}", p.reasons);
    // compatibility break: the literal is listed and the change is authorized (the cargo check says which uses stop)
    let breaks = spec(&store.root, "f2", &ADD_DIGEST.replace("default = \"String::new()\"\n", "").replace("target = \"core_lib::Config\"\n", "target = \"core_lib::Config\"\ncompatibility = \"break\"\n"));
    let p = preflight::run_preflight(&ctx, &breaks, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?}", p.reasons);
    assert!(p.listed.iter().any(|l| l.starts_with("struct literal ")));
    let variant = spec(&store.root, "v", "name = \"mode-paused\"\nobjective = \"a paused mode\"\noperation = \"add_variant\"\ntarget = \"core_lib::Mode\"\n\n[variant]\nname = \"Paused\"\n");
    let p = preflight::run_preflight(&ctx, &variant, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized);
    assert!(p.listed.iter().any(|l| l.contains("match without a catch-all") && l.contains("Paused")), "{:?}", p.listed);
}

#[test]
fn a_write_outside_the_scope_is_asked_for_never_taken_and_a_taken_name_is_a_collision() {
    let (store, ctx) = project("scope");
    let w = ws(&store.root);
    // the field's default fills a literal in core_lib, but the spec may only write app
    let s = spec(&store.root, "s", &format!("{ADD_DIGEST}\n[scope]\nwrite = [\"crate:app\"]\n"));
    let p = preflight::run_preflight(&ctx, &s, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized);
    assert_eq!(p.outside, ["crates/core_lib/src/lib.rs"]);
    assert!(p.decisions.iter().any(|q| q.question.contains("widen its scope.write to crate:core_lib")), "{:?}", p.decisions);
    let taken = spec(&store.root, "t", "name = \"socket-again\"\nobjective = \"o\"\noperation = \"make_fn\"\ntarget = \"crate:core_lib\"\n\n[function]\nname = \"socket\"\n");
    let p = preflight::run_preflight(&ctx, &taken, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized && p.collisions.iter().any(|c| c.contains("core_lib::socket is already declared")), "{:?}", p.collisions);
}

#[test]
fn a_code_changing_step_needs_a_preflight_of_this_spec_over_this_code() {
    let (store, ctx) = project("gate");
    let w = ws(&store.root);
    let path = spec(&store.root, "config-digest", ADD_DIGEST);
    let s = custodian_kit::change::ChangeSpec::from_path(&path).unwrap();
    let code = |root: &Path| custodian_kit::api::SearchKey::of(root, &custodian_kit::api::ReadingSpec::from_toml("name = \"i\"\nquestion = \"q\"\nkind = \"impact\"\ntarget = \"x\"\n").unwrap()).unwrap().code_digest;
    let (d, _) = preflight::gate(&store, &s, &code(&store.root)).unwrap();
    assert!(!d.go && d.reason.contains("no preflight"), "{}", d.reason);
    preflight::run_preflight(&ctx, &path, None, Runner::InProcess(&w)).unwrap();
    let (d, r) = preflight::gate(&store, &s, &code(&store.root)).unwrap();
    assert!(d.go, "{}", d.reason);
    assert_eq!(r.unwrap().reason, "authorized");
    // the code moves: stale, preflight it again
    let lib = store.root.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn moved() {}\n").unwrap();
    let (d, _) = preflight::gate(&store, &s, &code(&store.root)).unwrap();
    assert!(!d.go && d.reason.contains("the code moved since preflight"), "{}", d.reason);
    // the spec is edited: the same
    std::fs::write(&path, ADD_DIGEST.replace("name = \"digest\"", "name = \"hash\"")).unwrap();
    let s2 = custodian_kit::change::ChangeSpec::from_path(&path).unwrap();
    let (d, _) = preflight::gate(&store, &s2, &code(&store.root)).unwrap();
    assert!(!d.go && d.reason.contains("the spec changed since preflight"), "{}", d.reason);
}

#[test]
fn a_decision_answered_once_is_never_asked_again_in_any_task() {
    let (store, _) = project("decide");
    let a = custodian_kit::task_records::open(&store, "first", "specs", "j-1", "the first").unwrap();
    let q = questions::question("Which store does the cache use?", &["sqlite".into(), "files".into()], Some("both work"), "job:j-1");
    assert_eq!(questions::ask(&store, &a, &q).unwrap(), questions::Asked::Asked);
    assert_eq!(questions::ask(&store, &a, &q).unwrap(), questions::Asked::AlreadyOpen);
    assert_eq!(questions::open(&store).unwrap().len(), 1);
    questions::answer(&store, &q.id, "sqlite", Some("the journal is sqlite already"), Some("James")).unwrap();
    assert!(questions::open(&store).unwrap().is_empty());
    // another task, another agent, the same words: answered, not asked
    let b = custodian_kit::task_records::open(&store, "second", "specs", "j-2", "the second").unwrap();
    let again = questions::question("which store does the cache  use", &[], None, "job:j-2");
    match questions::ask(&store, &b, &again).unwrap() {
        questions::Asked::Answered(d) => assert_eq!((d.answer.as_str(), d.why.as_deref()), ("sqlite", Some("the journal is sqlite already"))),
        other => panic!("{other:?}"),
    }
}

/// A queued folder-of-specs job with the given job file.
fn job(store: &Store, name: &str, job_file: &str, specs: &[(&str, &str)]) {
    let dir = store.job_dir(JobState::Queued, name).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("v1.job.1o.job"), job_file).unwrap();
    for (n, t) in specs {
        std::fs::write(dir.join(format!("{n}.toml")), t).unwrap();
    }
}

#[test]
fn the_four_combinations_of_target_and_outcome_known() {
    for (target, verify, tk, ok) in [(Some("crate:core_lib"), Some("check:all"), true, true), (Some("crate:core_lib"), None, true, false), (None, Some("check:all"), false, true), (None, None, false, false)] {
        let (store, ctx) = project(&format!("known-{tk}-{ok}"));
        let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
        let mut jf = "{ job { name \"j\" kind \"specs\"".to_string();
        if let Some(t) = target {
            jf.push_str(&format!(" target \"{t}\""));
        }
        if let Some(v) = verify {
            jf.push_str(&format!(" verify \"{v}\""));
        }
        jf.push_str(" } }\n");
        job(&store, "j", &jf, &[]);
        let started = jobs::start_with(&ctx, Some("j"), false, Some(&w)).unwrap();
        let f = jobs::job_file(&started.folder);
        assert_eq!((f.target_known, f.outcome_known, f.mode), (Some(tk), Some(ok), Some(JobMode::Unbounded)));
        assert!(started.said.iter().any(|l| l.starts_with(if tk { "target known" } else { "target unknown" })), "{:?}", started.said);
        assert!(started.said.iter().any(|l| l.starts_with(if ok { "outcome known" } else { "outcome unknown" })), "{:?}", started.said);
        if !tk {
            // the job's target becomes known at its first preflight
            let s = spec(&store.root, "config-digest", ADD_DIGEST);
            let in_job = custodian_kit::runs::InJob { id: started.id.clone(), readings: started.folder.join("readings") };
            let wt = ws(&store.root);
            preflight::run_preflight(&ctx, &s, Some(&in_job), Runner::InProcess(&wt)).unwrap();
        }
        let (c, _, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
        assert_eq!(c.target_known_at.as_deref().map(|a| a == "open"), Some(tk), "{:?}", c.target_known_at);
        assert_eq!(c.outcome_known_at.is_some(), ok, "{:?}", c.outcome_known_at);
        // no accepted without evidence: unknown outcome is pending, the done question first among the decisions
        assert_eq!(c.result, "pending", "{:?}", c.result_reason);
        if !ok {
            assert!(c.decisions_open.iter().any(|q| q.question == "what does done look like for j?"), "{:?}", c.decisions_open);
            assert!(c.result_reason.as_deref().unwrap().contains("no verification declared"));
        } else {
            assert!(c.result_reason.as_deref().unwrap().contains("not verified yet: check:all"), "{:?}", c.result_reason);
        }
    }
}

/// A `code-passenger` on PATH that answers nothing: a bounded job needs its writer to be there.
fn fake_writer() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("custodian-kit-fake-writer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("code-passenger");
    std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

#[test]
fn a_bounded_job_refuses_hand_edits_at_close_and_needs_its_facts_before_a_preflight_authorizes() {
    let dir = fake_writer();
    let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap_or_default());
    // SAFETY: the variable is set once, before any thread of this test reads it
    unsafe { std::env::set_var("PATH", &path) };
    let (store, ctx) = project("bounded");
    let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
    job(&store, "b", "{ job { name \"b\" kind \"specs\" mode \"bounded\" target \"crate:core_lib\" } }\n", &[]);
    let started = jobs::start_with(&ctx, Some("b"), false, Some(&w)).unwrap();
    assert_eq!(jobs::job_file(&started.folder).mode, Some(JobMode::Bounded));
    // no verification declared: the preflight refuses, naming the flag
    let s = spec(&store.root, "config-digest", ADD_DIGEST);
    let in_job = custodian_kit::runs::InJob { id: started.id.clone(), readings: started.folder.join("readings") };
    let wt = ws(&store.root);
    let p = preflight::run_preflight(&ctx, &s, Some(&in_job), Runner::InProcess(&wt)).unwrap().preflight;
    assert!(!p.authorized && p.reasons.iter().any(|r| r.contains("outcome_known")), "{:?}", p.reasons);
    // declared: authorized
    jobs::declare_verify(&ctx, Some("b"), "test:config_names").unwrap();
    let p = preflight::run_preflight(&ctx, &s, Some(&in_job), Runner::InProcess(&wt)).unwrap().preflight;
    assert!(p.authorized, "{:?}", p.reasons);
    // the agent writes code by hand: the close is rejected, the file listed, CUST0031
    let lib = store.root.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn by_hand() {}\n").unwrap();
    let (c, done, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert_eq!(c.result, "rejected");
    assert_eq!(c.uncovered.iter().map(|u| u.path.as_str()).collect::<Vec<_>>(), ["crates/core_lib/src/lib.rs"]);
    assert_eq!(c.refusal.as_ref().unwrap().code, "CUST0031");
    assert!(done.join("closing.md").is_file(), "the job is closed, its record terminal");
    assert_eq!(c.outcome_known_at.as_deref().map(|a| a.starts_with('#')), Some(true), "known at the verify receipt");
}

#[test]
fn an_intent_is_planned_from_the_code_and_a_step_becomes_a_change_spec() {
    let (store, ctx) = project("plan");
    let w = ws(&store.root);
    let ran = custodian_kit::plan::run_plan(&ctx, "config-digest", "carry a digest in the config and show it in the mode label", None, Runner::InProcess(&w)).unwrap();
    let p = &ran.plan;
    assert!(p.candidates.iter().any(|c| c.target == "core_lib::Config"), "{:?}", p.candidates.iter().map(|c| &c.target).collect::<Vec<_>>());
    assert!(p.candidates.iter().any(|c| c.target == "core_lib::label" || c.target == "core_lib::Mode"));
    assert!(p.new.contains(&"digest".to_string()), "nothing names digest: {:?}", p.new);
    assert!(p.decisions.iter().any(|q| q.question == "nothing in the code is named digest: where does it go?"));
    assert!(p.decisions.iter().any(|q| q.question == "what does done look like for config-digest?"));
    assert!(!p.impacts.is_empty() && p.cost.iter().any(|d| d.name == "runtime cost" && d.value.is_none()));
    assert_eq!(ran.receipt.unwrap().command, "plan");
    // the decisions are in the plan's own task history, asked once
    assert!(questions::open(&store).unwrap().iter().any(|q| q.source == "plan:config-digest"));
    let n = p.steps.iter().find(|s| s.target == "core_lib::Config").unwrap().n;
    let (spec_path, plan) = custodian_kit::plan::approve(&store, "config-digest", n, Some("add_field")).unwrap();
    assert_eq!(plan.steps.iter().find(|s| s.n == n).unwrap().status, "approved");
    let spec = custodian_kit::change::ChangeSpec::from_path(&spec_path).unwrap();
    assert_eq!((spec.op(), spec.target.as_str()), (custodian_kit::change::Operation::AddField, "core_lib::Config"));
    // a TODO left by the approval is a precondition that does not hold
    let p = preflight::run_preflight(&ctx, &spec_path, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized && p.preconditions.iter().any(|c| c.what == "no TODO is left in the spec" && !c.holds));
}

#[test]
fn a_preflight_finds_the_impact_the_command_line_already_read() {
    let (store, ctx) = project("one-key");
    let w = ws(&store.root);
    // `custodian run core_lib::Config --kind impact` makes this spec; the preflight makes the same one
    let line = custodian_kit::api::ReadingSpec::search(custodian_kit::api::ReadingKind::Impact, "core_lib::Config").unwrap();
    assert_eq!(line.name, "impact-core_lib_Config");
    let file = store.root.join("impact-line.toml");
    std::fs::write(&file, toml::to_string(&line).unwrap()).unwrap();
    let first = custodian_kit::runs::run_reading_by(&ctx, &file, false, None, Runner::InProcess(&w)).unwrap();
    let n = first.receipt.as_ref().unwrap().seq;
    let s = spec(&store.root, "config-digest", ADD_DIGEST);
    let ran = preflight::run_preflight(&ctx, &s, None, Runner::InProcess(&w)).unwrap();
    assert!(ran.said.iter().any(|l| l.contains("reused, not read again") && l.contains(&format!("#{n}"))), "{:?}", ran.said);
}
