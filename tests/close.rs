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
    job(&store, "x", "{ job { name \"x\" kind \"specs\" mode \"bounded\" target \"crate:core_lib\" verify \"test:vault_key_is_42\" } }\n");
    let started = jobs::start_with(&ctx, Some("x"), false, Some(&w)).unwrap();
    assert_eq!(jobs::job_file(&started.folder).mode, Some(JobMode::Bounded));
    std::fs::create_dir_all(stderr.parent().unwrap()).unwrap();
    std::fs::write(&stderr, "error: expected\n").unwrap();
    let (c, _, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert_eq!(c.result, "rejected");
    assert_eq!(c.refusal.as_ref().unwrap().code, "CUST0041", "{:?}", c.refusal);
    assert!(c.touched_files.iter().any(|(p, how)| p == "crates/core_lib/tests/ui/case.stderr" && how.starts_with("hand")), "{:?}", c.touched_files);
    // allowed: named, not refused
    job(&store, "y", "{ job { name \"y\" kind \"specs\" mode \"bounded\" target \"crate:core_lib\" verify \"test:vault_key_is_42\" } }\n");
    let started = jobs::start_with(&ctx, Some("y"), false, Some(&w)).unwrap();
    jobs::declare_allow(&ctx, Some("y"), "crates/*/tests/ui/*.stderr").unwrap();
    std::fs::write(&stderr, "error: expected again\n").unwrap();
    let (c, _, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert!(c.refusal.is_none(), "{:?}", c.refusal);
    assert_eq!(c.result, "pending", "the verification did not run: {:?}", c.result_reason);
    assert!(c.touched_files.iter().any(|(p, how)| p == "crates/core_lib/tests/ui/case.stderr" && how == "allow_file crates/*/tests/ui/*.stderr"), "{:?}", c.touched_files);
    // a TOML file that is no crate's manifest (a features document) is a file no operation writes: CUST0041, the
    // same code as the .stderr, never the hand-written code lines of CUST0031
    job(&store, "z", "{ job { name \"z\" kind \"specs\" mode \"bounded\" target \"crate:core_lib\" verify \"test:vault_key_is_42\" } }\n");
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

#[test]
fn a_verification_satisfied_by_a_test_receipt_names_it_and_the_close_is_accepted() {
    let (store, ctx) = project("verified-by");
    let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
    job(&store, "v", "{ job { name \"v\" kind \"specs\" target \"crate:core_lib\" verify \"test:vault_key_is_42\" } }\n");
    let started = jobs::start_with(&ctx, Some("v"), false, Some(&w)).unwrap();
    // before any test ran: the close would have to run it (`not run`, no receipt)
    let v = jobs::verifications(&ctx, &started.id).unwrap();
    assert_eq!((v[0].state.as_str(), v[0].receipt), ("not run", None), "{v:?}");
    let in_job = InJob { id: started.id.clone(), readings: started.folder.join(jobs::READINGS_DIR) };
    let line = TestLine { package: "core_lib".into(), target: Some(Some("gate".into())), filters: vec!["vault_key_is_42".into()] };
    let run = testrun::run(&ctx, &[line], Observe::Failed, Some(&in_job)).unwrap();
    let v = jobs::verifications(&ctx, &started.id).unwrap();
    assert_eq!(v[0].state, "satisfied", "{v:?} {run:?}");
    let seq = v[0].receipt.expect("the test receipt that satisfied it");
    let (c, _, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert_eq!(c.result, "accepted", "{:?}", c.result_reason);
    assert_eq!(c.verified_by, vec![format!("test:vault_key_is_42: receipt #{seq}")]);
    assert!(jobs::render_closing(&c).contains(&format!("- test:vault_key_is_42: satisfied (receipt #{seq})")));
}

fn receipt(name: &str, millis: i64) -> custodian_kit::receipts::Receipt {
    serde_json::from_value(serde_json::json!({"schema": "custodian.receipt.v1", "seq": 0, "utc": "2026-10-23T09:00:00Z", "command": "reading", "name": name, "spec": null, "spec_hash": null, "code_digest": null, "index_key": null, "plan_id": "p", "plan_digest": "d", "operation_digest": "d", "receipt_digest": "d", "line": "", "exit_code": 0, "accepted": true, "disposition": "Continue", "reason": "read", "outputs": [], "millis": millis})).unwrap()
}

#[test]
fn two_checkouts_numbering_receipts_the_same_merge_by_union_and_never_write_over_a_file() {
    let (a, _) = project("merge-a");
    let (b, _) = project("merge-b");
    // the history both carried before they parted, then each checkout's own receipts under the same numbers
    let shared = receipt("shared", 1);
    let (sa, _) = custodian_kit::receipts::keep(&a, shared.clone()).unwrap();
    let (sb, _) = custodian_kit::receipts::keep(&b, shared).unwrap();
    assert_eq!(sa.seq, sb.seq);
    let (ra, _) = custodian_kit::receipts::keep(&a, receipt("only-a", 2)).unwrap();
    let (rb, fb) = custodian_kit::receipts::keep(&b, receipt("only-b", 3)).unwrap();
    assert_eq!(ra.seq, rb.seq, "the same number for two different runs: the collision");
    let b_dir = fb.parent().unwrap().to_path_buf();
    assert_eq!(custodian_kit::receipts::collisions(&a, &b_dir).unwrap(), vec![rb.seq]);
    let m = custodian_kit::receipts::merge(&a, &b.root.join(".custodian")).unwrap();
    assert_eq!(m.same, 1, "the shared receipt is one receipt");
    assert_eq!(m.collided, vec![rb.seq]);
    assert_eq!(m.kept.len(), 1);
    let (was, now) = m.kept[0];
    assert_eq!(was, rb.seq);
    assert!(now > ra.seq, "kept under the next free number");
    assert!(m.map.as_deref().is_some_and(|p| Path::new(p).is_file()));
    // a second merge of the same folder keeps nothing
    assert!(custodian_kit::receipts::merge(&a, &b.root.join(".custodian")).unwrap().kept.is_empty());
    // a receipt file another checkout brought in is never written over: the next number is above it
    let a_dir = a.root.join(".custodian/data/receipts");
    let top = custodian_kit::receipts::receipts_in(&a_dir).iter().map(|r| r.seq).max().unwrap();
    let mut foreign = receipt("brought-in", 4);
    foreign.seq = top + 5;
    std::fs::write(a_dir.join(format!("{}.json", top + 5)), serde_json::to_string_pretty(&foreign).unwrap()).unwrap();
    let (next, _) = custodian_kit::receipts::keep(&a, receipt("after", 5)).unwrap();
    assert_eq!(next.seq, top + 6);
    let still: custodian_kit::receipts::Receipt = serde_json::from_str(&std::fs::read_to_string(a_dir.join(format!("{}.json", top + 5))).unwrap()).unwrap();
    assert_eq!(still.name, "brought-in");
}

/// A batch's member records cover its lines as single transforms do, and the receipts a line kept under the job's
/// NAME (`--job tb`, as custodian 0.3.11 kept them) are the job's: a bounded job with one batch over three files closes
/// accepted with no line by hand, its verification satisfied by the test run kept under the name, and its handoff
/// names the batch's specs and the test line.
#[test]
fn a_batch_over_three_files_closes_accepted_with_no_line_by_hand() {
    use custodian_kit::store::{Receipt, RECEIPT_SCHEMA};
    let dir = fake_writer();
    // SAFETY: the variable is set once, before any thread of this test reads it
    unsafe { std::env::set_var("PATH", format!("{}:{}", dir.display(), std::env::var("PATH").unwrap_or_default())) };
    let (store, ctx) = project("batch");
    let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
    job(&store, "tb", "{ job { name \"tb\" kind \"specs\" mode \"bounded\" target \"crate:core_lib\" verify \"test:vault_key_is_42\" } }\n");
    let started = jobs::start_with(&ctx, Some("tb"), false, Some(&w)).unwrap();
    assert_ne!(started.id, "tb");
    let hex = |b: &[u8]| stable_digest::StableDigest::sha256(b).as_str().to_string();
    // the files written before their records are kept: the close refuses them as lines by hand, and the job is done
    let files = ["crates/core_lib/src/lib.rs", "crates/core_lib/tests/gate.rs", "crates/core_lib/tests/metrics.rs"];
    let befores: Vec<Vec<u8>> = files.iter().map(|f| std::fs::read(store.root.join(f)).unwrap()).collect();
    for (i, file) in files.iter().enumerate() {
        let mut after = befores[i].clone();
        after.extend_from_slice(format!("\n// member {i}\n").as_bytes());
        std::fs::write(store.root.join(file), &after).unwrap();
    }
    let (c, refused, _) = jobs::close_with(&ctx, "tb", Some(&w)).unwrap();
    assert_eq!(c.refusal.as_ref().map(|r| r.code.as_str()), Some("CUST0031"), "{:?}", c.refusal);
    // its records kept after: `job close` closes the refused job again over the same receipts
    let batch = refused.join("data/transforms/batch-T");
    std::fs::create_dir_all(&batch).unwrap();
    let rel = |p: &Path| p.strip_prefix(&store.root).unwrap().to_string_lossy().into_owned();
    let receipt = |command: &str, name: &str, outputs: Vec<String>| Receipt {
        schema: RECEIPT_SCHEMA.into(),
        seq: 0,
        utc: "2026-10-23T18:19:49Z".into(),
        command: command.into(),
        name: name.into(),
        spec: None,
        spec_hash: None,
        code_digest: None,
        index_key: None,
        plan_id: "custodian.job.tb".into(),
        plan_digest: "d".into(),
        operation_digest: "d".into(),
        receipt_digest: format!("d-{name}"),
        line: "code-passenger transform --batch list --write".into(),
        exit_code: Some(0),
        accepted: true,
        disposition: "Continue".into(),
        reason: "done".into(),
        outputs,
        job: Some("tb".into()),
        reused_seq: None,
        millis: 0,
        runner: "line".into(),
    };
    custodian_kit::receipts::keep(&store, receipt("code-passenger.transform-batch", "batch of 3", vec![rel(&batch.join("list"))])).unwrap();
    // three members, three files, each written once by the batch
    for (i, file) in files.into_iter().enumerate() {
        let (before, after) = (&befores[i], std::fs::read(store.root.join(file)).unwrap());
        let name = format!("member-{i}");
        let record = serde_json::json!({
            "schema": "custodian.transform.v1", "spec_name": name, "spec": format!("{name}.toml"), "spec_digest": "d",
            "operation": "replace_body", "target": format!("core_lib::f{i}"),
            "files": [{ "path": file, "before": hex(before), "after": hex(&after), "lines": [[1, 1]] }],
            "checks": [], "postconditions": [], "written": true
        });
        let out = batch.join(format!("{name}.json"));
        std::fs::write(&out, record.to_string()).unwrap();
        custodian_kit::receipts::keep(&store, receipt("transform", &format!("{name} (batch of 3)"), vec![rel(&out)])).unwrap();
    }
    // the test run, kept under the job's name
    let line = TestLine { package: "core_lib".into(), target: Some(Some("gate".into())), filters: vec!["vault_key_is_42".into()] };
    testrun::run(&ctx, &[line], Observe::Failed, Some(&InJob { id: "tb".into(), readings: refused.join("readings") })).unwrap();
    let (c, done, _) = jobs::close_with(&ctx, "tb", Some(&w)).unwrap();
    assert!(c.refusal.is_none(), "{:?}", c.refusal);
    assert_eq!(c.result, "accepted", "{:?}", c.result_reason);
    assert!(!c.touched_files.iter().any(|(_, how)| how.starts_with("hand")), "{:?}", c.touched_files);
    assert_eq!(c.touched_files.iter().filter(|(_, how)| how.starts_with("transform #")).count(), 3, "{:?}", c.touched_files);
    assert_eq!(c.verify, [("test:vault_key_is_42".to_string(), "satisfied".to_string())]);
    let receipts = store.journal().unwrap().runs().unwrap();
    let h = custodian_kit::handoff::of_job(&store.root, &store, &done, &receipts, None).unwrap();
    let said = custodian_kit::handoff::render(&h, custodian_kit::handoff::Reader::James);
    assert!(said.contains("3 change specs were written through transforms") && said.contains("member-0") && said.contains("member-2"), "{said}");
    assert!(!said.contains("No test line ran") && said.contains("core_lib"), "{said}");
}

/// A transform code-passenger wrote on its own line (`code-passenger transform --preflight N --write`, its receipt
/// `code-passenger` / `transform`) is the job's as custodian's `transform` line is: the close counts it and names the
/// file it wrote as its, so an experiment's provenance names the change.
#[test]
fn a_transform_code_passenger_wrote_on_its_own_line_is_the_jobs() {
    let (store, ctx) = project("own-line");
    let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
    job(&store, "o", "{ job { name \"o\" kind \"specs\" target \"crate:core_lib\" } }\n");
    let started = jobs::start_with(&ctx, Some("o"), false, Some(&w)).unwrap();
    let file = "crates/core_lib/src/lib.rs";
    let before = std::fs::read(store.root.join(file)).unwrap();
    let mut after = before.clone();
    after.extend_from_slice(b"\n// written\n");
    std::fs::write(store.root.join(file), &after).unwrap();
    let hex = |b: &[u8]| stable_digest::StableDigest::sha256(b).as_str().to_string();
    let record = serde_json::json!({
        "schema": "custodian.transform.v1", "spec_name": "one", "spec": "one.toml", "spec_digest": "d", "operation": "replace_body",
        "target": "core_lib::f", "preflight_seq": 1, "files": [{ "path": file, "before": hex(&before), "after": hex(&after), "lines": [[1, 1]] }],
        "checks": [], "postconditions": [], "written": true, "writer": "code_passenger 0.5.0 (custodian_kit 0.4.1)"
    });
    // the record where the experimenter's --out put it: outside the tree
    let out = std::env::temp_dir().join(format!("custodian-kit-own-line-{}.json", std::process::id()));
    std::fs::write(&out, record.to_string()).unwrap();
    let mut r = receipt("transform", 0);
    (r.command, r.line, r.job, r.outputs) = ("code-passenger".into(), "code-passenger transform --spec one.toml --preflight 1 --write".into(), Some(started.id.clone()), vec![format!("./{file}"), out.display().to_string()]);
    let (kept, _) = custodian_kit::receipts::keep(&store, r).unwrap();
    let (c, _, _) = jobs::close_with(&ctx, &started.id, Some(&w)).unwrap();
    assert_eq!(c.transforms, [kept.seq], "{c:?}");
    assert!(c.touched_files.iter().any(|(p, how)| p == file && *how == format!("transform #{}", kept.seq)), "{:?}", c.touched_files);
}

/// A `test:` verification that names no test of the workspace is refused when the job starts and when it is declared.
#[test]
fn a_test_verification_that_matches_no_test_is_refused_at_start() {
    let (store, ctx) = project("nomatch");
    job(&store, "n", "{ job { name \"n\" kind \"specs\" target \"crate:core_lib\" verify \"test:chain::tests::*\" } }\n");
    let e = jobs::start_with(&ctx, Some("n"), false, None).unwrap_err().to_string();
    assert!(e.contains("no test matches"), "{e}");
    assert!(store.job_dir(JobState::Queued, "n").unwrap().exists(), "the job stays queued");
    job(&store, "m", "{ job { name \"m\" kind \"specs\" target \"crate:core_lib\" verify \"test:vault_key_is_42\" } }\n");
    jobs::start_with(&ctx, Some("m"), false, None).unwrap();
    assert!(jobs::declare_verify(&ctx, Some("m"), "test:chain::tests::*").unwrap_err().to_string().contains("no test matches"));
    jobs::declare_verify(&ctx, Some("m"), "test:core_lib").unwrap();
}

/// A file whose base has no final newline is carried by its record from its exact base bytes, and the timing records
/// a test run writes (`.maintenance/timings/`) are the test line's, never a hand's.
#[test]
fn a_base_without_a_final_newline_is_covered_and_a_test_runs_timings_are_its_own() {
    use custodian_kit::store::{Receipt, RECEIPT_SCHEMA};
    let dir = fake_writer();
    // SAFETY: the variable is set once, before any thread of this test reads it
    unsafe { std::env::set_var("PATH", format!("{}:{}", dir.display(), std::env::var("PATH").unwrap_or_default())) };
    let (store, ctx) = project("nonl");
    let manifest = "crates/core_lib/Cargo.toml";
    let before = std::fs::read_to_string(store.root.join(manifest)).unwrap().trim_end().to_string();
    std::fs::write(store.root.join(manifest), &before).unwrap();
    git(&store.root, &["commit", "-qam", "no final newline"]);
    let w = Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap();
    job(&store, "nl", "{ job { name \"nl\" kind \"specs\" mode \"bounded\" target \"crate:core_lib\" verify \"test:vault_key_is_42\" } }\n");
    let started = jobs::start_with(&ctx, Some("nl"), false, Some(&w)).unwrap();
    let after = format!("{before}\n# a line\n");
    std::fs::write(store.root.join(manifest), &after).unwrap();
    let timings = store.root.join(".maintenance/timings/custodian-1.jsonl");
    std::fs::create_dir_all(timings.parent().unwrap()).unwrap();
    std::fs::write(&timings, "{}\n").unwrap();
    let hex = |b: &[u8]| stable_digest::StableDigest::sha256(b).as_str().to_string();
    let out = started.folder.join("data/transforms/one.json");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let record = serde_json::json!({"schema": "custodian.transform.v1", "spec_name": "one", "spec": "one.toml", "spec_digest": "d", "operation": "add_dependency", "target": "core_lib", "files": [{ "path": manifest, "before": hex(before.as_bytes()), "after": hex(after.as_bytes()), "lines": [[2, 2]] }], "checks": [], "postconditions": [], "written": true});
    std::fs::write(&out, record.to_string()).unwrap();
    let rel = out.strip_prefix(&store.root).unwrap().to_string_lossy().into_owned();
    let r = Receipt { schema: RECEIPT_SCHEMA.into(), seq: 0, utc: "2026-10-23T00:00:00Z".into(), command: "transform".into(), name: "one".into(), spec: None, spec_hash: None, code_digest: None, index_key: None, plan_id: "p".into(), plan_digest: "d".into(), operation_digest: "d".into(), receipt_digest: "r-one".into(), line: "l".into(), exit_code: Some(0), accepted: true, disposition: "Continue".into(), reason: "done".into(), outputs: vec![rel], job: Some(started.id.clone()), reused_seq: None, millis: 0, runner: "line".into() };
    custodian_kit::receipts::keep(&store, r).unwrap();
    let line = TestLine { package: "core_lib".into(), target: Some(Some("gate".into())), filters: vec!["vault_key_is_42".into()] };
    testrun::run(&ctx, &[line], Observe::Failed, Some(&InJob { id: started.id.clone(), readings: started.folder.join("readings") })).unwrap();
    let (c, _, _) = jobs::close_with(&ctx, "nl", Some(&w)).unwrap();
    assert!(c.refusal.is_none(), "{:?} {:?}", c.refusal, c.touched_files);
    assert!(c.touched_files.iter().any(|(p, how)| p == manifest && how.starts_with("transform #")), "{:?}", c.touched_files);
    assert!(c.touched_files.iter().any(|(p, how)| p == ".maintenance/timings/custodian-1.jsonl" && how == "test run"), "{:?}", c.touched_files);
    assert_eq!(c.result, "accepted", "{:?}", c.result_reason);
}
