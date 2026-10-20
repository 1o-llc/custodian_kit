// Copyright © 2026 1o. All rights reserved.

//! A job's close on a git repository: a test failing the same way at the job's base is pre-existing, named with its
//! first failing line, and the job is accepted (judged at close on a worktree of the base, or from the baseline
//! `job start` ran); a test the job breaks is the job's; every file the job touched is listed, and in a bounded job a
//! file no operation writes needs `allow_file`; the job's lines touched are git's +added −removed.

use custodian_kit::api::{Resolving, Workspace};
use custodian_kit::jobs::{self, JobMode};
use custodian_kit::runs::{Ctx, InJob};
use custodian_kit::store::{JobState, Mode, Store};
use custodian_kit::testrun::{self, Observe, TestLine};
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn git(root: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(root).args(args).env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t").output().unwrap();
    assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
}

/// A copy of the fixture with a test that always fails (the gate it reads is never set) and one that passes, in a
/// git repository of its own with one commit, initialized (agent mode).
fn project(tag: &str) -> (Store, Ctx) {
    let dir = std::env::temp_dir().join(format!("custodian-kit-close-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ws"), &dir);
    std::fs::write(
        dir.join("crates/core_lib/tests/gate.rs"),
        "#[test]\nfn needs_the_gate() {\n    assert!(std::env::var(\"CUSTODIAN_KIT_GATE_NEVER_SET\").is_ok(), \"the gate is not set\");\n}\n\n#[test]\nfn vault_key_is_42() {\n    assert_eq!(core_lib::vault_key(), 42);\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join(".gitignore"), "target/\n.custodian/state/\n").unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    let (store, _) = Store::init(&dir, Mode::Agent).unwrap();
    let ctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: PathBuf::from("custodian") };
    (store, ctx)
}

fn job(store: &Store, name: &str, job_file: &str) {
    let dir = store.job_dir(JobState::Queued, name).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("v1.job.1o.job"), job_file).unwrap();
}

fn gate_tests(ctx: &Ctx, in_job: &InJob) {
    let line = TestLine { package: "core_lib".into(), target: Some(Some("gate".into())), filters: vec!["needs_the_gate".into(), "vault_key_is_42".into()] };
    testrun::run(ctx, &[line], Observe::Failed, Some(in_job)).unwrap();
}

#[test]
fn a_test_failing_the_same_way_at_the_base_is_pre_existing_and_the_job_is_accepted() {
    let (store, ctx) = project("pre");
    let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
    job(&store, "j", "{ job { name \"j\" kind \"specs\" target \"crate:core_lib\" count true verify \"test:vault_key_is_42\" } }\n");
    let started = jobs::start_with(&ctx, Some("j"), false, Some(&w)).unwrap();
    let base = jobs::job_file(&started.folder).base_commit.expect("the job opens at a commit");
    assert!(started.said.iter().any(|l| l.starts_with(&format!("base: {base}"))), "{:?}", started.said);
    // the job's change: a line by hand (unbounded), the vault key kept
    let lib = store.root.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn added() -> u8 {\n    1\n}\n").unwrap();
    let in_job = InJob { id: started.id.clone(), readings: started.folder.join(jobs::READINGS_DIR) };
    gate_tests(&ctx, &in_job);
    let (c, _, said) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert_eq!(c.result, "accepted", "{:?} {:?} {said:?}", c.result_reason, c.failing);
    assert_eq!(c.pre_existing.len(), 1, "{:?}", c.pre_existing);
    let p = &c.pre_existing[0];
    assert_eq!((p.package.as_str(), p.test.as_str(), p.first_line.as_str(), p.from.as_str()), ("core_lib", "needs_the_gate", "the gate is not set", format!("base {base}").as_str()));
    assert!(c.failing.is_empty());
    assert_eq!(c.base_commit.as_deref(), Some(base.as_str()));
    // every file touched, and git's exact lines for the job
    assert!(c.touched_files.iter().any(|(p, how)| p == "crates/core_lib/src/lib.rs" && how.starts_with("hand")), "{:?}", c.touched_files);
    let j = c.lines_touched.iter().find(|t| t.label == "job").expect("the job counted");
    assert!(j.line.starts_with("job: +4 \u{2212}0 across 1 files") && j.line.contains("git diff --numstat"), "{}", j.line);
    let page = jobs::render_closing(&c);
    assert!(page.contains("## pre-existing (1)") && page.contains("`the gate is not set`"), "{page}");
}

#[test]
fn a_test_the_job_breaks_is_the_jobs_and_the_baseline_answers_for_the_base() {
    let (store, ctx) = project("broken");
    let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
    job(&store, "b", "{ job { name \"b\" kind \"specs\" target \"crate:core_lib\" baseline true verify \"test:vault_key_is_42\" } }\n");
    let started = jobs::start_with(&ctx, Some("b"), false, Some(&w)).unwrap();
    let baseline = custodian_kit::baseline::recorded(&started.folder).expect("the baseline is kept at open");
    assert!(baseline.outcomes.iter().any(|o| o.test == "needs_the_gate" && o.outcome == "FAILED" && o.first_line.as_deref() == Some("the gate is not set")), "{:?}", baseline.outcomes);
    assert!(baseline.outcomes.iter().any(|o| o.test == "vault_key_is_42" && o.outcome == "ok"));
    // the job breaks the vault key
    let lib = store.root.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap().replace("k + 1", "k + 2")).unwrap();
    let in_job = InJob { id: started.id.clone(), readings: started.folder.join(jobs::READINGS_DIR) };
    gate_tests(&ctx, &in_job);
    let (c, _, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert_eq!(c.result, "failed_test", "{:?}", c.result_reason);
    assert_eq!(c.pre_existing.iter().map(|p| (p.test.as_str(), p.from.as_str())).collect::<Vec<_>>(), [("needs_the_gate", "baseline")]);
    assert_eq!(c.failing.len(), 1, "{:?}", c.failing);
    assert!(c.failing[0].starts_with("core_lib gate vault_key_is_42: ok at the base"), "{:?}", c.failing);
}

/// A `code-passenger` on PATH that answers nothing: a bounded job needs its writer to be there.
fn fake_writer() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("custodian-kit-close-writer-{}", std::process::id()));
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
fn a_bounded_job_names_every_file_it_touched_and_a_file_no_operation_writes_needs_allow_file() {
    let dir = fake_writer();
    // SAFETY: the variable is set once, before any thread of this test reads it
    unsafe { std::env::set_var("PATH", format!("{}:{}", dir.display(), std::env::var("PATH").unwrap_or_default())) };
    let (store, ctx) = project("allow");
    let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
    // a test's expected output, written by hand
    let stderr = store.root.join("crates/core_lib/tests/ui/case.stderr");
    job(&store, "x", "{ job { name \"x\" kind \"specs\" mode \"bounded\" target \"crate:core_lib\" verify \"test:x\" } }\n");
    let started = jobs::start_with(&ctx, Some("x"), false, Some(&w)).unwrap();
    assert_eq!(jobs::job_file(&started.folder).mode, Some(JobMode::Bounded));
    std::fs::create_dir_all(stderr.parent().unwrap()).unwrap();
    std::fs::write(&stderr, "error: expected\n").unwrap();
    let (c, _, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert_eq!(c.result, "rejected");
    assert_eq!(c.refusal.as_ref().unwrap().code, "CUST0041", "{:?}", c.refusal);
    assert!(c.touched_files.iter().any(|(p, how)| p == "crates/core_lib/tests/ui/case.stderr" && how.starts_with("hand")), "{:?}", c.touched_files);
    // allowed: named, not refused
    job(&store, "y", "{ job { name \"y\" kind \"specs\" mode \"bounded\" target \"crate:core_lib\" verify \"test:x\" } }\n");
    let started = jobs::start_with(&ctx, Some("y"), false, Some(&w)).unwrap();
    jobs::declare_allow(&ctx, Some("y"), "crates/*/tests/ui/*.stderr").unwrap();
    std::fs::write(&stderr, "error: expected again\n").unwrap();
    let (c, _, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert!(c.refusal.is_none(), "{:?}", c.refusal);
    assert_eq!(c.result, "pending", "the verification did not run: {:?}", c.result_reason);
    assert!(c.touched_files.iter().any(|(p, how)| p == "crates/core_lib/tests/ui/case.stderr" && how == "allow_file crates/*/tests/ui/*.stderr"), "{:?}", c.touched_files);
    // a TOML file that is no crate's manifest (a features document) is a file no operation writes: CUST0041, the
    // same code as the .stderr, never the hand-written code lines of CUST0031
    job(&store, "z", "{ job { name \"z\" kind \"specs\" mode \"bounded\" target \"crate:core_lib\" verify \"test:x\" } }\n");
    let started = jobs::start_with(&ctx, Some("z"), false, Some(&w)).unwrap();
    std::fs::write(store.root.join("features.toml"), "default = []\n").unwrap();
    let (c, _, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert_eq!(c.refusal.as_ref().map(|r| r.code.as_str()), Some("CUST0041"), "{:?}", c.refusal);
    assert!(c.refusal.as_ref().unwrap().message.contains("features.toml"));
    assert!(jobs::is_code_file("crates/core_lib/Cargo.toml") && jobs::is_code_file("crates/core_lib/src/lib.rs") && !jobs::is_code_file("features.toml"));
}

#[test]
fn a_test_verification_names_a_test_or_every_test_of_a_module() {
    use custodian_kit::jobs::test_named;
    assert!(test_named("kitchen::tests::can_cook", "can_cook") && test_named("kitchen::tests::can_cook", "tests::can_cook"));
    assert!(test_named("kitchen::tests::can_cook", "tests::*") && test_named("kitchen::tests::can_cook", "kitchen::tests::*") && test_named("tests::pantry_tests", "tests::*"));
    assert!(!test_named("kitchen::tests::can_cook", "cook") && !test_named("kitchen::tests_more::can_cook", "tests::*") && !test_named("kitchen::tests::can_cook", "kitchen::*::x"));
    jobs::check_verify("test:pantry_tests::*").unwrap();
}

#[test]
fn a_closes_findings_are_each_said_once_and_the_lines_that_prove_a_fix_only_with_a_failing_test() {
    let close = |name: &str, break_it: bool| {
        let (store, ctx) = project(name);
        let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
        let spec = store.root.join("touch.toml");
        std::fs::write(&spec, "name = \"touch\"\nobjective = \"o\"\noperation = \"add_attribute\"\ntarget = \"core_lib::vault_key\"\nattribute = \"#[inline]\"\n").unwrap();
        job(&store, name, &format!("{{ job {{ name \"{name}\" kind \"specs\" target \"crate:core_lib\" verify \"test:vault_key_is_42\" }} }}\n"));
        let started = jobs::start_with(&ctx, Some(name), false, Some(&w)).unwrap();
        let in_job = InJob { id: started.id.clone(), readings: started.folder.join(jobs::READINGS_DIR) };
        if break_it {
            let lib = store.root.join("crates/core_lib/src/lib.rs");
            std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap().replace("k + 1", "k + 2")).unwrap();
        }
        // the same impact read three times: its test lines are findings of each reading
        for _ in 0..3 {
            custodian_kit::preflight::run_preflight(&ctx, &spec, Some(&in_job), custodian_kit::runs::Runner::InProcess(&w)).unwrap();
        }
        gate_tests(&ctx, &in_job);
        jobs::close_with(&ctx, &started.id, Some(&w)).unwrap().0
    };
    let quiet = close("quiet", false);
    assert!(!quiet.findings.iter().any(|f| f.starts_with(jobs::PROVE_A_FIX)), "no test of the job fails: {:?} {:?}", quiet.findings, quiet.failing);
    let failing = close("failing", true);
    let prove: Vec<&String> = failing.findings.iter().filter(|f| f.starts_with(jobs::PROVE_A_FIX)).collect();
    assert!(!prove.is_empty(), "a test fails: {:?} {:?}", failing.findings, failing.failing);
    let unique: std::collections::BTreeSet<&&String> = prove.iter().collect();
    assert_eq!(unique.len(), prove.len(), "each once: {prove:?}");
}
