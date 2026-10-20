// Copyright © 2026 1o. All rights reserved.

//! `handoff` and `why` over a project's records, made here as a job leaves them: a bounded job that wrote two
//! change specs through transforms, had one refused by its preflight, answered one decision and left one open,
//! ran three test lines (two tests failed, one of them failed before the job's first change too, one line stopped
//! before any test reported), and moved lines in one crate; an earlier closed job whose test run is the evidence
//! for "pre-existing". The rendered handoff is pinned whole, for a person and for an agent.

use custodian_kit::jobs::{Closing, JobFile, JobMode, Touched};
use custodian_kit::store::{Mode, Receipt, Store, TestRow, RECEIPT_SCHEMA};
use std::path::{Path, PathBuf};

fn receipt(seq: i64, utc: &str, command: &str, name: &str, job: Option<&str>, accepted: bool, reason: &str, outputs: &[&str]) -> Receipt {
    Receipt {
        schema: RECEIPT_SCHEMA.into(),
        seq,
        utc: utc.into(),
        command: command.into(),
        name: name.into(),
        spec: None,
        spec_hash: (command == "reading").then(|| "sha256:s".into()),
        code_digest: None,
        index_key: None,
        plan_id: "p".into(),
        plan_digest: "p".into(),
        operation_digest: "o".into(),
        receipt_digest: "r".into(),
        line: format!("custodian {command} {name}"),
        exit_code: Some(if accepted { 0 } else if command == "test" { 101 } else { 1 }),
        accepted,
        disposition: "Continue".into(),
        reason: reason.into(),
        outputs: outputs.iter().map(|o| o.to_string()).collect(),
        job: job.map(str::to_string),
        reused_seq: None,
        millis: 1,
        runner: if command == "test" { "line".into() } else { "in_process".into() },
    }
}

fn row(seq: i64, package: &str, target: &str, test: &str, outcome: &str) -> TestRow {
    TestRow { seq, package: package.into(), target: target.into(), test: test.into(), outcome: outcome.into(), millis: None }
}

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn job_folder(store: &Store, name: &str, jf: &JobFile, closing: &Closing) -> PathBuf {
    let dir = store.jobs_dir(custodian_kit::store::JobState::Done).unwrap().join(name);
    write(&dir.join(custodian_kit::jobs::job_file_name()), &custodian_kit::oneo::render(custodian_kit::oneo::Doc::Job, jf).unwrap());
    write(&dir.join("closing.json"), &serde_json::to_string_pretty(closing).unwrap());
    dir
}

const TRANSFORM: &str = r#"{"schema": "custodian.transform.v1", "spec_name": "SPEC", "spec": ".custodian/data/specs/changes/SPEC.toml", "spec_digest": "d", "operation": "OP",
"target": "TARGET", "files": [{"path": "crates/core_lib/src/lib.rs", "before": "a", "after": "b", "lines": [[1, 4]]}], "checks": [], "postconditions": [], "written": true}"#;

const DIFF: &str = r#"{"schema": "custodian.count_diff.v1", "route": "*", "before": {"digest": "a", "commit": null, "index_key": null}, "after": {"digest": "b", "commit": null, "index_key": null},
"digest": "d", "moved": {"added": {"lines": 6, "code": 5, "comments": 0, "blanks": 1}, "removed": {"lines": 1, "code": 1, "comments": 0, "blanks": 0}}, "files_added": [], "files_removed": [],
"files_changed": ["crates/core_lib/src/lib.rs", "crates/app/Cargo.toml"], "test_share_before": 100, "test_share_after": 100, "test_share_delta": 0,
"crates": [{"name": "app", "moved": {"added": {"lines": 0, "code": 0, "comments": 0, "blanks": 0}, "removed": {"lines": 0, "code": 0, "comments": 0, "blanks": 0}}, "languages": [], "files_added": [], "files_removed": [], "files_changed": ["crates/app/Cargo.toml"], "test_share_before": 0, "test_share_after": 0},
{"name": "core_lib", "moved": {"added": {"lines": 6, "code": 5, "comments": 0, "blanks": 1}, "removed": {"lines": 1, "code": 1, "comments": 0, "blanks": 0}}, "languages": [], "files_added": [], "files_removed": [], "files_changed": ["crates/core_lib/src/lib.rs"], "test_share_before": 100, "test_share_after": 100}]}"#;

/// One project at a time: a store's writer is one per process (uh-tom-mick's admission of one).
fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ONE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The project: the earlier job, the bounded job and every receipt they ran.
fn project() -> (PathBuf, Store, Vec<Receipt>) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("kit-handoff-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (store, _) = Store::init(&root, Mode::Agent).unwrap();
    // the earlier job: its test run (#2) is the run before the change
    let earlier = JobFile { id: Some("job-000000000001".into()), name: Some("baseline".into()), kind: Some("specs".into()), ..JobFile::default() };
    job_folder(&store, "baseline", &earlier, &Closing { id: "job-000000000001".into(), name: "baseline".into(), closed_utc: "2026-10-19T20:00:00Z".into(), tests: vec![row(2, "core_lib", "lib", "old_failure", "FAILED")], result: "failed_test".into(), ..Closing::default() });
    // the bounded job's task: intent, a decision answered, one left open
    let t = custodian_kit::task_records::open(&store, "hold", "specs", "job-00000000000a", "read the job's specs: hold").unwrap();
    t.write(task_history::DiffDocument::record(task_history::DiffRecordKind::Intent).with("intent", "the restart holds its lock while it runs")).unwrap();
    let which = custodian_kit::questions::question("which lock file?", &["the anchor's".into(), "a new one".into()], Some("two files could hold it"), "plan:hold");
    custodian_kit::questions::ask(&store, &t, &which).unwrap();
    custodian_kit::questions::answer(&store, &which.id, "the anchor's", Some("the restart already waits on it"), None).unwrap();
    let open = custodian_kit::questions::question("does the stop wait too?", &["yes".into(), "no".into()], None, "plan:hold");
    custodian_kit::questions::ask(&store, &t, &open).unwrap();
    // the job's receipts
    let folder_rel = ".custodian/state/jobs/done/hold";
    let j = Some("job-00000000000a");
    let mut receipts = vec![
        receipt(2, "2026-10-19T20:00:00Z", "test", "core_lib lib", Some("job-000000000001"), false, "unaccepted status: stop", &[]),
        receipt(3, "2026-10-19T21:00:00Z", "decide", "decide", None, true, "done", &[]),
        receipt(4, "2026-10-19T21:05:00Z", "reading", "impact-core_lib_Config", j, true, "read", &[]),
        receipt(5, "2026-10-19T21:06:00Z", "preflight", "lock-by-hand", j, false, "not authorized", &[]),
        receipt(6, "2026-10-19T21:07:00Z", "preflight", "hold-lock", j, true, "authorized", &[]),
        receipt(7, "2026-10-19T21:10:00Z", "transform", "hold-lock", j, true, "nothing to do or converged", &[&format!("{folder_rel}/data/transforms/hold-lock.json")]),
        receipt(8, "2026-10-19T21:20:00Z", "transform", "hold-test", j, true, "nothing to do or converged", &[&format!("{folder_rel}/data/transforms/hold-test.json")]),
        receipt(9, "2026-10-19T21:30:00Z", "test", "core_lib lib", j, false, "unaccepted status: stop", &[]),
        receipt(10, "2026-10-19T21:31:00Z", "test", "app lib", j, false, "unaccepted status: stop", &[]),
        receipt(11, "2026-10-19T21:32:00Z", "test", "core_lib test:metrics", j, true, "accepted status: continue", &[]),
    ];
    receipts[1].line = format!("custodian decide {} the anchor's --why the restart already waits on it", which.id);
    // the bounded job's folder: its file, its transforms, its count diff, its closing receipt
    let jf = JobFile {
        id: Some("job-00000000000a".into()),
        name: Some("hold".into()),
        kind: Some("specs".into()),
        target: Some("core_lib::Config".into()),
        task_id: Some(t.task_id.clone()),
        revision: Some(t.revision.clone()),
        mode: Some(JobMode::Bounded),
        verify: vec!["test:holds".into(), "reading:impact-core_lib_Config".into()],
        ..JobFile::default()
    };
    let touched: Touched = serde_json::from_value(serde_json::json!({"label": "job", "route": "*", "before_digest": "a", "after_digest": "b", "diff_digest": "d",
        "moved": {"added": {"lines": 6, "code": 5, "comments": 0, "blanks": 1}, "removed": {"lines": 1, "code": 1, "comments": 0, "blanks": 0}},
        "files": 2, "crates": 2, "line": "job: +6 −1 across 2 files in 2 crates", "diff": "data/counts/job-workspace.diff.json"}))
    .unwrap();
    let closing = Closing {
        schema: "custodian.job_closing.v1".into(),
        id: "job-00000000000a".into(),
        name: "hold".into(),
        kind: "specs".into(),
        opened_utc: Some("2026-10-19T21:04:00Z".into()),
        closed_utc: "2026-10-19T21:40:00Z".into(),
        runs: (4..=11).collect(),
        tests: vec![row(9, "core_lib", "lib", "old_failure", "FAILED"), row(9, "core_lib", "lib", "holds", "ok"), row(9, "core_lib", "lib", "new_failure", "FAILED"), row(11, "core_lib", "test:metrics", "metrics_say_where", "ok"), row(11, "core_lib", "test:metrics", "slow", "ignored")],
        lines_touched: vec![touched],
        mode: Some(JobMode::Bounded),
        verify: vec![("test:holds".into(), "satisfied".into()), ("reading:impact-core_lib_Config".into(), "unchanged".into())],
        transforms: vec![7, 8],
        result: "failed_test".into(),
        result_reason: Some("a test or check the job ran failed".into()),
        ..Closing::default()
    };
    let dir = job_folder(&store, "hold", &jf, &closing);
    write(&dir.join("data/counts/job-workspace.diff.json"), DIFF);
    write(&dir.join("data/transforms/hold-lock.json"), &TRANSFORM.replace("SPEC", "hold-lock").replace("OP", "add_method").replace("TARGET", "core_lib::Config"));
    write(&dir.join("data/transforms/hold-test.json"), &TRANSFORM.replace("SPEC", "hold-test").replace("OP", "add_test").replace("TARGET", "core_lib::Config::hold"));
    (root, store, receipts)
}

const FOR_JAMES: &str = "# handoff: hold (job-00000000000a)

The job hold ran bounded from 2026-10-19 21:04Z to 21:40Z, 8 runs. It closed failed_test: a test or check the job ran failed.

## Waiting on a person
- Decision QOPEN is open: does the stop wait too? (yes | no)
- The change spec lock-by-hand was refused by its preflight at 2026-10-19 21:06Z (receipt #5: not authorized) and never ran.
- The verification reading:impact-core_lib_Config is unchanged: the reading it names did not change.

## What it was for
the restart holds its lock while it runs.
Its target was core_lib::Config.

## What changed
6 lines added and 1 removed, across 2 files in 2 crates.
- core_lib: +6 −1 in 1 file
- app: +0 −0 in 1 file
2 change specs were written through transforms, by 2 operations: add_method, add_test.
- hold-lock (add_method on core_lib::Config, 1 file) at 2026-10-19 21:10Z, receipt #7
- hold-test (add_test on core_lib::Config::hold, 1 file) at 2026-10-19 21:20Z, receipt #8

## What was decided
- QWHICH: which lock file? → the anchor's. Why: the restart already waits on it. By: the record names no one. At 2026-10-19 21:00Z, receipt #3.

## What was verified
The verification test:holds is satisfied.
5 tests ran in 3 test lines between 2026-10-19 21:30Z and 21:32Z: 2 passed, 2 failed, 1 ignored.
- core_lib lib: 2 failed (receipt #9)
- app lib: stopped with exit 101 before any test reported (receipt #10)
1 of the 2 failed before the job's first change too (a run before receipt #7); the rest are not known to be pre-existing.
1 reading was made and 0 reused.

## What runs next
- Decide QOPEN: does the stop wait too?
- Fix the change spec lock-by-hand until its preflight authorizes it, or drop it.
- Open the next revision of hold and finish it there.
- Read impact-core_lib_Config in the job: done is that reading changing from the one before it.
- Make the 2 failing tests pass, or show from a run before the change that they failed already.
- Build 1 test line again and read why it stopped.
";

const AGENT_TAIL: &str = "
The lines, in order:
    custodian decide QOPEN <answer> --why \"<the reason>\"
    custodian preflight .custodian/data/specs/changes/lock-by-hand.toml
    custodian job start hold
    custodian run impact-core_lib_Config
    cargo test -p core_lib --lib -- old_failure new_failure --exact
    custodian test run --affected   # app lib stopped at receipt #10
    custodian job close hold

The routes and targets it read:
- core_lib::Config
Its folder: .custodian/state/jobs/done/hold · its task history: hold/base
";

#[test]
fn a_handoff_is_made_from_the_records_in_its_order_and_pinned() {
    let _one = one_at_a_time();
    let (root, store, receipts) = project();
    let folder = custodian_kit::handoff::job_folder(&store, Some("hold")).unwrap();
    let h = custodian_kit::handoff::of_job(&root, &store, &folder, &receipts, None).unwrap();
    let ids = |s: &str| s.replace(&custodian_kit::questions::id_of("does the stop wait too?"), "QOPEN").replace(&custodian_kit::questions::id_of("which lock file?"), "QWHICH");
    let james = ids(&custodian_kit::handoff::render(&h, custodian_kit::handoff::Reader::James));
    assert_eq!(james, FOR_JAMES, "the handoff moved; made now:\n{james}");
    let agent = ids(&custodian_kit::handoff::render(&h, custodian_kit::handoff::Reader::Agent));
    assert_eq!(agent, format!("{FOR_JAMES}{AGENT_TAIL}"), "made now:\n{agent}");
    // the job closed last is the one named by default; --since keeps only what came after
    assert_eq!(custodian_kit::handoff::job_folder(&store, None).unwrap(), folder);
    let later = custodian_kit::handoff::of_job(&root, &store, &folder, &receipts, Some(8)).unwrap();
    assert!(later.written.is_empty() && later.refused.is_empty() && later.decided.is_empty() && later.tests.len() == 2, "{later:?}");
    assert_eq!(custodian_kit::handoff::jobs_since(&store, &receipts, 8).unwrap(), vec![folder]);
}

#[test]
fn why_names_the_task_history_and_the_linkage_record_of_a_route() {
    let _one = one_at_a_time();
    let (root, store, receipts) = project();
    // the fixture workspace: a crate whose change record a task history keeps, a linkage record binding it
    write(&root.join("Cargo.toml"), "[workspace]\nmembers = [\"crates/core_lib\"]\nresolver = \"2\"\n");
    write(&root.join("crates/core_lib/Cargo.toml"), "[package]\nname = \"core_lib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    write(&root.join("crates/core_lib/src/lib.rs"), "pub struct Config;\n");
    let t = custodian_kit::task_records::of_job(&store, Some("hold"), Some("base")).unwrap().unwrap();
    t.write(task_history::DiffDocument::record(task_history::DiffRecordKind::Change).with("path", "crates/core_lib/src/lib.rs").with("operation", "modify").with("before_digest", "a".repeat(64)).with("after_digest", "b".repeat(64))).unwrap();
    let rec = custodian_kit::linkage::LinkageRecord {
        name: "config-socket".into(),
        kind: "connected".parse().unwrap(),
        why: "the socket is read from the config".into(),
        binds: vec!["core_lib::Config".into(), "app::Settings".into()],
        author: Some("James Hunt".into()),
        recorded_utc: Some("2026-10-19T09:00:00Z".into()),
        evidence: vec![],
    };
    custodian_kit::linkage::write(&store, &rec).unwrap();
    let readings = vec![custodian_kit::why::ReadingAbout { seq: 4, utc: receipts[2].utc.clone(), name: "impact-core_lib_Config".into(), kind: "impact".into(), target: "core_lib::Config".into(), job: Some("job-00000000000a".into()) }];
    let w = custodian_kit::why::of(&root, &store, "crate:core_lib", &readings).unwrap();
    assert_eq!(w.paths, ["crates/core_lib"]);
    assert_eq!(w.tasks.len(), 1);
    assert_eq!((w.tasks[0].intent.as_deref(), w.tasks[0].job.as_deref(), w.tasks[0].files.as_slice()), (Some("the restart holds its lock while it runs"), Some("job-00000000000a"), &["modify crates/core_lib/src/lib.rs".to_string()][..]));
    assert_eq!((w.linkage.len(), w.linkage[0].others.as_slice()), (1, &["app::Settings".to_string()][..]));
    let text = custodian_kit::why::render(&w);
    assert!(text.contains("git: no record."), "the caller did not run git: {text}");
    assert!(text.contains("- config-socket (connected): the socket is read from the config, by James Hunt, 2026-10-19T09:00:00Z — binds core_lib::Config to app::Settings"), "{text}");
    assert!(text.contains("- #4 2026-10-19T21:05:00Z impact `impact-core_lib_Config` (core_lib::Config) in job job-00000000000a"), "{text}");
    // a route that names nothing in the records: "no record", never a guess
    let none = custodian_kit::why::render(&custodian_kit::why::Why { route: "decl:market".into(), ..Default::default() });
    assert!(none.contains("No record: no task history") && none.contains("No record: no linkage record") && none.contains("No record: no reading"), "{none}");
    assert!(custodian_kit::why::of(&root, &store, "path:crates", &[]).is_err(), "why reads crate: and decl: routes");
}
