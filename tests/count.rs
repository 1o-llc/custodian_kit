//! The `count` kind on the fixture workspace: tokei's numbers per language and
//! crate, klepto's test share, the digest of the same tree, what moved between
//! two trees, and a route's history from the journal.

use custodian_kit::api::{self, ReadingSpec, Resolving, Workspace};
use custodian_kit::reading::count::{self, CountReport, Lines};
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ws")
}

fn spec(target: &str) -> ReadingSpec {
    ReadingSpec::from_toml(&format!("name = \"count\"\nquestion = \"how much\"\nkind = \"count\"\ntarget = {}\n", toml::Value::String(target.into()))).unwrap()
}

fn read_at(root: &Path, target: &str) -> api::Reading {
    let s = spec(target);
    let ws = Workspace::open_for(root, &s, Resolving::Free).unwrap();
    api::run(&ws, &s).unwrap_or_else(|e| panic!("{target}: {e}"))
}

fn count_at(root: &Path, target: &str) -> CountReport {
    read_at(root, target).report.count.expect("a count")
}

fn lines(lines: usize, code: usize, comments: usize, blanks: usize) -> Lines {
    Lines { lines, code, comments, blanks }
}

/// A copy of the fixture in a fresh folder of its own (nothing removed, ever).
fn copy_fixture(tag: &str) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                copy(&p, &to.join(e.file_name()));
            } else {
                std::fs::copy(&p, to.join(e.file_name())).unwrap();
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("custodian-kit-count-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    copy(&fixture(), &dir);
    dir
}

#[test]
fn a_crate_is_counted_per_language() {
    let c = count_at(&fixture(), "crate:copy_a");
    assert_eq!(c.route, "crate:copy_a");
    let rust = c.languages.iter().find(|l| l.language == "Rust").unwrap();
    assert_eq!((rust.files, rust.lines), (1, lines(7, 7, 0, 0)));
    let toml = c.languages.iter().find(|l| l.language == "TOML").unwrap();
    assert_eq!(toml.files, 1);
    assert_eq!(c.tests, Lines::default());
    assert_eq!(c.crates.len(), 1);
    assert_eq!(c.crates[0].name, "copy_a");
}

#[test]
fn the_test_share_is_klepto_s() {
    let c = count_at(&fixture(), "crate:core_lib");
    let rust = c.languages.iter().find(|l| l.language == "Rust").unwrap();
    assert_eq!(rust.files, 2, "src/lib.rs and tests/metrics.rs");
    // the #[cfg(test)] module (lines 70-83: 12 code, 2 blank) and tests/metrics.rs (4 code)
    assert_eq!(rust.tests, lines(18, 16, 0, 2));
    assert_eq!(c.test_share, c.tests.lines * 1000 / c.lines.lines);
}

#[test]
fn a_module_is_its_file_or_its_block() {
    let whole = count_at(&fixture(), "layered::whole");
    assert_eq!(whole.route, "module:layered::whole");
    assert_eq!(whole.lines, lines(7, 6, 0, 1));
    let wide = count_at(&fixture(), "layered::wide");
    assert_eq!(wide.crates[0].file_counts[0].path, "crates/layered/src/lib.rs#layered::wide");
    assert_eq!(wide.lines, lines(9, 8, 0, 1));
}

#[test]
fn the_workspace_is_counted_per_crate_and_the_reading_carries_the_numbers() {
    let r = read_at(&fixture(), "*");
    let c = r.report.count.clone().unwrap();
    assert_eq!(c.route, "*");
    let names: Vec<&str> = c.crates.iter().map(|k| k.name.as_str()).collect();
    for n in ["app", "copy_a", "core_lib", "layered", "tool"] {
        assert!(names.contains(&n), "{names:?}");
    }
    // the workspace's own Cargo.toml and the marketplace files sit in no crate
    assert!(names.contains(&"-"));
    assert_eq!(c.files, c.crates.iter().map(|k| k.files).sum::<usize>());
    // the reading's counts and JSON hold the numbers (diff reads them)
    assert_eq!(r.counts().iter().find(|x| x.what == "lines").unwrap().n, c.lines.lines);
    assert!(r.json().contains("\"custodian.count.v1\""));
    assert!(r.rows().iter().any(|x| x.kind == "count-crate" && x.label == "layered"));
}

#[test]
fn the_same_tree_gives_the_same_digest() {
    let (a, b) = (copy_fixture("same-a"), copy_fixture("same-b"));
    let (ca, cb) = (count_at(&a, "*"), count_at(&b, "*"));
    assert_eq!(ca.digest, cb.digest);
    assert!(ca.digest.starts_with("sha256:"));
    assert_eq!(count_at(&fixture(), "*").digest, ca.digest, "the fixture in place counts the same");
    let d = count::compare_counts(&ca, &cb);
    assert!(d.is_empty() && d.moved == count::Moved::default());
}

#[test]
fn compare_says_what_moved_between_two_trees() {
    let a = copy_fixture("before");
    let b = copy_fixture("after");
    // after: copy_a grows by a 4-line fn and a blank; a test file is added; copy_b's lib loses its last 3 lines
    let lib = b.join("crates/copy_a/src/lib.rs");
    let mut t = std::fs::read_to_string(&lib).unwrap();
    t.push_str("\n// doubled\npub fn twice(x: u32) -> u32 {\n    x * 2\n}\n");
    std::fs::write(&lib, t).unwrap();
    std::fs::create_dir_all(b.join("crates/copy_a/tests")).unwrap();
    std::fs::write(b.join("crates/copy_a/tests/twice.rs"), "#[test]\nfn twice() {\n    assert_eq!(copy_a::twice(2), 4);\n}\n").unwrap();
    let lib_b = b.join("crates/copy_b/src/lib.rs");
    let text = std::fs::read_to_string(&lib_b).unwrap();
    let kept = text.lines().take(4).collect::<Vec<_>>().join("\n") + "\n";
    std::fs::write(&lib_b, kept).unwrap();

    let (ca, cb) = (count_at(&a, "*"), count_at(&b, "*"));
    assert_ne!(ca.digest, cb.digest);
    let d = count::compare_counts(&ca, &cb);
    assert_eq!(d.files_added, vec!["crates/copy_a/tests/twice.rs"]);
    assert!(d.files_removed.is_empty());
    // copy_a: +5 in lib.rs (1 blank, 1 comment, 3 code) and +4 code in the test file
    let copy_a = d.crates.iter().find(|c| c.name == "copy_a").unwrap();
    assert_eq!(copy_a.moved.added, lines(9, 7, 1, 1));
    assert_eq!(copy_a.moved.removed, Lines::default());
    assert!(copy_a.test_share_after > copy_a.test_share_before);
    // copy_b: 3 code lines removed
    let copy_b = d.crates.iter().find(|c| c.name == "copy_b").unwrap();
    assert_eq!(copy_b.moved.removed, lines(3, 3, 0, 0));
    assert_eq!(d.moved.added.lines, 9);
    assert_eq!(d.moved.removed.lines, 3);
    assert!(d.test_share_delta > 0);
    assert_eq!(d.crates.len(), 2, "nothing else moved");
    // the diff's digest is over the diff: the same two counts give the same digest
    assert_eq!(count::compare_counts(&ca, &cb).digest, d.digest);
    // and through the readings
    let (ra, rb) = (read_at(&a, "*"), read_at(&b, "*"));
    assert_eq!(count::compare(&ra.report, &rb.report).unwrap().digest, d.digest);
}

#[test]
fn history_lists_a_routes_counts_in_journal_order() {
    let root = copy_fixture("history");
    let (store, _) = custodian_kit::store::Store::init(&root, custodian_kit::store::Mode::Plain).unwrap();
    let journal = store.journal().unwrap();
    let base = store.reading_dir("x").unwrap().parent().unwrap().to_path_buf();
    let mut digests = Vec::new();
    for (i, target) in ["*", "crate:copy_a", "*"].iter().enumerate() {
        if i == 2 {
            std::fs::write(root.join("crates/copy_a/src/extra.rs"), "pub fn more() {}\n").unwrap();
        }
        let mut r = read_at(&store.root, target);
        r.report.stamp = format!("20261019T00000{i}Z");
        let (_, json, _) = r.write(&base).unwrap();
        let rel = json.strip_prefix(&store.root).unwrap().to_string_lossy().into_owned();
        let receipt: custodian_kit::api::Receipt = serde_json::from_value(serde_json::json!({
            "schema": "custodian.receipt.v1", "seq": 0, "utc": format!("2026-10-19T00:00:0{i}Z"), "command": "reading", "name": "count",
            "spec": null, "spec_hash": null, "code_digest": null, "index_key": null, "plan_id": "p", "plan_digest": "d", "operation_digest": "d",
            "receipt_digest": "d", "line": "custodian exec-reading", "exit_code": 0, "accepted": true, "disposition": "Stop", "reason": "done",
            "outputs": [rel], "job": null, "reused_seq": null, "millis": 1
        }))
        .unwrap();
        journal.record(receipt).unwrap();
        digests.push(r.report.count.unwrap().digest);
    }
    let h = count::history(&store, "*").unwrap();
    assert_eq!(h.len(), 2, "the crate's count is another route");
    assert!(h[0].seq < h[1].seq);
    assert_eq!((h[0].digest.as_str(), h[1].digest.as_str()), (digests[0].as_str(), digests[2].as_str()));
    assert_eq!(h[1].files, h[0].files + 1);
    assert_eq!(count::history(&store, "crate:copy_a").unwrap().len(), 1);
}

#[test]
fn the_spec_excludes_and_the_route_tables_ignore_set_hold() {
    let c = count_at(&fixture(), "*");
    let s = ReadingSpec::from_toml("name = \"c\"\nquestion = \"q\"\nkind = \"count\"\ntarget = \"*\"\n[scope]\nexclude = [\"crates/copy_b/**\"]\n").unwrap();
    let ws = Workspace::open_for(&fixture(), &s, Resolving::Free).unwrap();
    let without = api::run(&ws, &s).unwrap().report.count.unwrap();
    assert!(c.crates.iter().any(|k| k.name == "copy_b"));
    assert!(!without.crates.iter().any(|k| k.name == "copy_b"), "{:?}", without.crates.iter().map(|k| &k.name).collect::<Vec<_>>());
}

#[test]
fn a_jobs_counts_give_lines_touched_per_step_and_for_the_job_and_sum_across_jobs() {
    use custodian_kit::jobs::{self, Taken};
    // three states of one tree: open, after a step that changed code, close
    let open = copy_fixture("job-open");
    let step = copy_fixture("job-step");
    let lib = step.join("crates/copy_a/src/lib.rs");
    let mut t = std::fs::read_to_string(&lib).unwrap();
    t.push_str("pub fn one() -> u32 {\n    1\n}\n");
    std::fs::write(&lib, &t).unwrap();
    let close = copy_fixture("job-close");
    std::fs::write(close.join("crates/copy_a/src/lib.rs"), &t).unwrap();
    std::fs::write(close.join("crates/copy_b/src/extra.rs"), "pub fn two() -> u32 {\n    2\n}\n").unwrap();

    // the store sits in a tree of its own (init writes oneo.project.json, which a count would see)
    let (store, _) = custodian_kit::store::Store::init(&copy_fixture("job-store"), custodian_kit::store::Mode::Agent).unwrap();
    let folder = store.job_dir(custodian_kit::store::JobState::Done, "j").unwrap();
    let mut counts = Vec::new();
    for (label, root) in [("open", &open), ("step 1 apply", &step), ("close", &close)] {
        let mut r = read_at(root, "*");
        r.report.name = format!("count-{}", label.replace(' ', "-"));
        let (_, json, _) = r.write(&folder.join("readings")).unwrap();
        counts.push(Taken { label: label.into(), route: "*".into(), reading: json.strip_prefix(&folder).unwrap().to_string_lossy().into_owned() });
    }
    let touched = jobs::lines_touched_of(&folder, &counts).unwrap();
    let lines: Vec<&str> = touched.iter().map(|t| t.line.as_str()).collect();
    assert_eq!(lines, vec!["step 1 apply: +3 \u{2212}0 across 1 files in 1 crates", "job: +6 \u{2212}0 across 2 files in 2 crates"]);
    assert!(folder.join(&touched[1].diff).is_file());

    // a closing receipt carrying them, and the sum over jobs
    let closing = jobs::Closing {
        schema: jobs::CLOSING_SCHEMA.into(),
        id: "j-1".into(),
        name: "j".into(),
        kind: "specs".into(),
        opened_utc: None,
        closed_utc: "2026-10-19T00:00:00Z".into(),
        specs: vec![],
        runs: vec![],
        reused: 0,
        tests: vec![],
        sensitive: vec![],
        touched: vec![],
        guidance: None,
        findings: vec![],
        lines_touched: touched,
        ..Default::default()
    };
    std::fs::write(folder.join("closing.json"), serde_json::to_string(&closing).unwrap()).unwrap();
    let journal = store.journal().unwrap();
    journal.put_job(&custodian_kit::store::JobRow { id: "j-1".into(), name: "j".into(), kind: "specs".into(), state: "done".into(), folder: folder.strip_prefix(&store.root).unwrap().to_string_lossy().into_owned(), opened_utc: None, closed_utc: None }).unwrap();
    let w = jobs::lines_within(&store, &["j-1"]).unwrap();
    assert_eq!((w.diffs, w.moved.added.lines, w.moved.removed.lines), (1, 6, 0));
    assert_eq!(w.files, vec!["crates/copy_a/src/lib.rs", "crates/copy_b/src/extra.rs"]);
    assert_eq!(w.crates, vec!["copy_a", "copy_b"]);
}

#[test]
fn the_triage_and_features_templates_count() {
    for (kind, text) in custodian_kit::jobs::SHIPPED_TEMPLATES {
        let t: custodian_kit::jobs::Template = toml::from_str(text).unwrap();
        assert!(t.count, "{kind}");
        assert!(t.step.iter().all(|s| ReadingSpec::from_toml(&format!("name = \"s\"\nquestion = \"q\"\nkind = \"{}\"\ntarget = \"x\"\n", s.kind)).is_ok()), "{kind}: every step is a reading");
    }
}

#[test]
fn a_job_is_a_task_history_that_round_trips_and_holds_against_the_tree() {
    use custodian_kit::task_records;
    use task_history::{find_records_by_kind, load_task_history, verify_terminal_tree_changes, DiffRecordKind};
    let root = copy_fixture("task-history");
    let (store, _) = custodian_kit::store::Store::init(&root, custodian_kit::store::Mode::Agent).unwrap();
    // job start: task and intent; the open count
    let task = task_records::open(&store, "2026-10-19-first-reading", "specs", "job-1", "read the job's specs").unwrap();
    assert_eq!((task.task_id.as_str(), task.revision.as_str()), ("2026-10-19-first-reading", "base"));
    let before = count_at(&store.root, "*");
    task_records::solution(&task, "use-sites (use_sites serve_metrics): run #3").unwrap();
    // a step that changes code: one file modified, one added
    let lib = store.root.join("crates/copy_a/src/lib.rs");
    let mut t = std::fs::read_to_string(&lib).unwrap();
    t.push_str("pub fn three() -> u32 {\n    3\n}\n");
    std::fs::write(&lib, t).unwrap();
    std::fs::write(store.root.join("crates/copy_a/src/more.rs"), "pub fn four() -> u32 {\n    4\n}\n").unwrap();
    let after = count_at(&store.root, "*");
    let changes = task_records::changes(&task, &store.root, &before, &after).unwrap();
    let ops: Vec<(String, String)> = changes.iter().map(|d| (d.one("path").unwrap().to_string(), d.one("operation").unwrap().to_string())).collect();
    assert_eq!(ops, vec![("crates/copy_a/src/lib.rs".to_string(), "modify".to_string()), ("crates/copy_a/src/more.rs".to_string(), "add".to_string())]);
    task_records::verification(&task, "cargo test -p copy_a", Some(true), None).unwrap();
    let key = custodian_kit::api::IndexKey::of_root(&store.root, &Default::default()).unwrap();
    task_records::close(&task, "accepted", Some(&key.0)).unwrap();

    // read back through load_task_history
    let index = load_task_history(task_records::root(&store).unwrap()).unwrap();
    assert_eq!((index.task_count, index.revision_count, index.change_count, index.accepted_revision_count), (1, 1, 2, 1));
    let kinds: Vec<&str> = index.tasks[0].revisions[0].documents.iter().map(|d| d.one("record_kind").unwrap()).collect();
    assert_eq!(kinds, vec!["task", "intent", "solution", "change", "change", "verification", "result"]);
    let result = find_records_by_kind(&index, DiffRecordKind::Result)[0];
    assert_eq!(result.one("resulting_tree_digest").unwrap(), key.0.trim_start_matches("sha256:"));
    // the history's last changes hold in the tree
    assert_eq!(verify_terminal_tree_changes(&index, &store.root).unwrap(), 2);
    // the brief reads them
    let lines = task_records::brief_lines(&task_records::read(&store, &task.task_id).unwrap().unwrap()).join("\n");
    assert!(lines.contains("add crates/copy_a/src/more.rs") && lines.contains("result accepted"), "{lines}");
    // a second run of the job is its next revision
    let again = task_records::open(&store, "2026-10-19-first-reading", "specs", "job-2", "again").unwrap();
    assert_eq!(again.revision, "fix1");
    assert_eq!(load_task_history(task_records::root(&store).unwrap()).unwrap().revision_count, 2);
}

#[test]
fn a_failed_test_in_cargo_s_words_fails_the_jobs_result() {
    use custodian_kit::store::{JobState, Mode, Store, TestRow};
    use task_history::DiffRecordKind;
    let (store, _) = Store::init(&copy_fixture("job-failed-test"), Mode::Agent).unwrap();
    let dir = store.job_dir(JobState::Queued, "t").unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("v1.job.1o.job"), "{ job { name \"t\" kind \"specs\" } }\n").unwrap();
    let ctx = custodian_kit::runs::Ctx { root: store.root.clone(), store: Some(store.clone()), exe: line_program() };
    let started = custodian_kit::jobs::start(&ctx, Some("t"), false).unwrap();
    // a test line run inside the job, its outcomes as cargo prints them (what the journal's tests table keeps)
    let receipt: custodian_kit::api::Receipt = serde_json::from_value(serde_json::json!({
        "schema": "custodian.receipt.v1", "seq": 0, "utc": "2026-10-19T00:00:00Z", "command": "test", "name": "copy_a",
        "spec": null, "spec_hash": null, "code_digest": null, "index_key": null, "plan_id": "p", "plan_digest": "d", "operation_digest": "d",
        "receipt_digest": "d", "line": "cargo test -p copy_a", "exit_code": 101, "accepted": false, "disposition": "Stop", "reason": "failed",
        "outputs": [], "job": started.id, "reused_seq": null, "millis": 1
    }))
    .unwrap();
    let journal = store.journal().unwrap();
    let kept = journal.record(receipt).unwrap();
    for (test, outcome) in [("a", "ok"), ("b", "FAILED")] {
        journal.put_test(&TestRow { seq: kept.seq, package: "copy_a".into(), target: "lib".into(), test: test.into(), outcome: outcome.into(), millis: None }).unwrap();
    }
    let (closing, _, _) = custodian_kit::jobs::close(&ctx, &started.id).unwrap();
    assert_eq!(closing.tests.len(), 2);
    let task = custodian_kit::task_records::read(&store, "t").unwrap().unwrap();
    let docs = &task.revisions[0].documents;
    let outcomes: Vec<&str> = docs.iter().filter(|d| d.kind().ok() == Some(DiffRecordKind::Verification)).map(|d| d.one("outcome").unwrap()).collect();
    assert_eq!(outcomes, vec!["pass", "fail"], "FAILED is a failure, not pending");
    let result = docs.iter().find(|d| d.kind().ok() == Some(DiffRecordKind::Result)).unwrap();
    assert_eq!(result.one("status").unwrap(), "failed_test");
}

#[test]
fn a_folder_of_specs_lists_only_its_specs_never_the_jobs_own_data() {
    let dir = copy_fixture("job-specs").join("job");
    std::fs::create_dir_all(dir.join("data/counts")).unwrap();
    for f in ["v1.a.1o.reading", "b.toml", "job.toml", "v1.job.1o.job", "counts.json", "notes.json", "data/counts.json", "data/counts/job-workspace.diff.json"] {
        std::fs::write(dir.join(f), "{}").unwrap();
    }
    let specs: Vec<String> = custodian_kit::jobs::job_specs(&dir).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(specs, vec!["b.toml", "v1.a.1o.reading"]);
}

#[test]
fn a_reading_kept_in_a_job_resolves_after_the_job_moves_to_done() {
    use custodian_kit::store::{JobState, Mode, Store};
    let (store, _) = Store::init(&copy_fixture("job-move"), Mode::Agent).unwrap();
    let running = store.job_dir(JobState::Running, "j").unwrap();
    let r = read_at(&store.root, "crate:copy_a");
    let (_, json, _) = r.write(&running.join("readings")).unwrap();
    let rel = json.strip_prefix(&store.root).unwrap().to_string_lossy().into_owned();
    std::fs::write(running.join("closing.md"), format!("reading: {rel}\n")).unwrap();
    let journal = store.journal().unwrap();
    let receipt: custodian_kit::api::Receipt = serde_json::from_value(serde_json::json!({
        "schema": "custodian.receipt.v1", "seq": 0, "utc": "2026-10-19T00:00:00Z", "command": "reading", "name": "count",
        "spec": null, "spec_hash": "sha256:spec", "code_digest": "sha256:code", "index_key": null, "plan_id": "p", "plan_digest": "d", "operation_digest": "d",
        "receipt_digest": "d", "line": "custodian exec-reading", "exit_code": 0, "accepted": true, "disposition": "Stop", "reason": "done",
        "outputs": [rel], "job": "j-1", "reused_seq": null, "millis": 1
    }))
    .unwrap();
    let kept = journal.record(receipt).unwrap();
    let done = store.job_dir(JobState::Done, "j").unwrap();
    let moved = custodian_kit::jobs::move_job(&store, &running, &done).unwrap();
    assert_eq!(moved, vec![kept.seq]);
    // the same search finds the reading where it now is
    let prev = journal.same_search("sha256:spec", "sha256:code").unwrap().expect("the earlier search");
    let out = prev.outputs.first().unwrap();
    assert!(out.contains("/jobs/done/j/"), "{out}");
    assert!(store.root.join(out).is_file(), "{out} resolves");
    // the receipt's file and the closing receipt say the same
    let file = store.path(&custodian_kit::store::named::RECEIPT, &[("seq", &kept.seq.to_string())]).unwrap();
    assert!(std::fs::read_to_string(file).unwrap().contains("/jobs/done/j/"));
    assert!(std::fs::read_to_string(done.join("closing.md")).unwrap().contains("/jobs/done/j/"));
}

#[test]
fn the_steps_run_on_scope_independent_ones_together_a_dependent_one_after() {
    use custodian_kit::jobs::runner::run_steps;
    use custodian_kit::jobs::{From, Step};
    let step = |name: &str, kind: &str, target: Option<&str>, for_each: Option<From>| Step { name: name.into(), kind: kind.into(), target: target.map(str::to_string), for_each, change_items: None, with: None, reads: None };
    let steps = vec![
        // c reads a's use sites: it waits for a
        step("c", "dependents", None, Some(From { step: "a".into(), row: "use-site".into(), field: "full_path".into() })),
        step("a", "use_sites", Some("serve_metrics"), None),
        step("b", "dependents", Some("{target}"), None),
    ];
    // one workspace, opened once and borrowed by every step
    let ws = Workspace::open(&fixture()).unwrap();
    let run = || {
        let mut exec = Answer;
        run_steps(&steps, "core_lib", Some(&ws), &mut exec).unwrap()
    };
    let (done, events, _) = run();
    assert_eq!(
        events,
        vec!["start a", "start b", "read a serve_metrics", "read b core_lib", "done a", "done b", "start c", "read c core_lib::serve_metrics", "done c"],
        "a and b interleave; c starts only when a is done"
    );
    let order: Vec<&str> = done.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(order, vec!["a", "b", "c"]);
    assert!(done[2].1[0].query.results.iter().any(|r| r.kind == "dependent"), "c read what a found");
    // the same job runs the same way every time
    assert_eq!(run().1, events);
}

#[test]
fn a_step_reading_a_step_the_job_lacks_is_refused() {
    use custodian_kit::jobs::runner::run_steps;
    use custodian_kit::jobs::{From, Step};
    let steps = vec![Step { name: "c".into(), kind: "reach".into(), target: None, for_each: Some(From { step: "nope".into(), row: "x".into(), field: "label".into() }), change_items: None, with: None, reads: None }];
    let mut exec = Answer;
    assert!(run_steps(&steps, "x", None, &mut exec).is_err());
}

/// An executor that keeps a receipt file per run (`receipts/<n>.json`) and
/// records what each step was given, for the `reads` tests.
#[derive(Default)]
struct Kept {
    n: usize,
    given: Vec<(String, String)>,
}

impl custodian_kit::jobs::runner::RunStep for Kept {
    fn run(&mut self, _ws: Option<&Workspace>, step: &custodian_kit::jobs::Step, _n: usize, target: &str, _items: &[String], _said: &mut Vec<String>) -> custodian_kit::Result<Option<custodian_kit::reading::ReadingReport>> {
        self.n += 1;
        self.given.push((step.name.clone(), target.to_string()));
        Ok(None)
    }

    fn receipt(&self) -> Option<String> {
        // the step named `silent` keeps no receipt
        (self.given.last().map(|(s, _)| s.as_str()) != Some("silent")).then(|| format!(".custodian/data/receipts/{}.json", self.n))
    }
}

#[test]
fn a_step_that_reads_another_waits_for_it_and_takes_its_receipts_as_targets() {
    use custodian_kit::jobs::runner::run_steps;
    use custodian_kit::jobs::Template;
    // code_passenger's shape: plan → apply → annotate, each reading the step before it; declared in the
    // job template as data, out of order, so only `reads` puts them in order
    let t: Template = toml::from_str(
        r#"kind = "chain"
[[step]]
name = "annotate"
kind = "use_sites"
reads = "apply"
target = "--after {receipt}"

[[step]]
name = "apply"
kind = "use_sites"
reads = "plan"

[[step]]
name = "plan"
kind = "use_sites"
target = "{target}"

[[step]]
name = "after-silent"
kind = "use_sites"
reads = "silent"

[[step]]
name = "silent"
kind = "use_sites"
"#,
    )
    .unwrap();
    assert_eq!(t.step[1].reads.as_deref(), Some("plan"));
    assert_eq!(custodian_kit::jobs::runner::dependencies(&t.step[0]), vec!["apply".to_string()]);
    let mut exec = Kept::default();
    let (done, events, said) = run_steps(&t.step, "core_lib", None, &mut exec).unwrap();
    let given: Vec<(&str, &str)> = exec.given.iter().map(|(s, t)| (s.as_str(), t.as_str())).collect();
    assert_eq!(
        given,
        vec![
            ("plan", "core_lib"),
            ("silent", "core_lib"),
            ("apply", ".custodian/data/receipts/1.json"),
            ("annotate", "--after .custodian/data/receipts/3.json"),
        ],
        "each step after the one it reads, its target that step's receipt: {events:?}"
    );
    // a step that left no receipt gives nothing: the step reading it is skipped, by the step-input decision
    assert!(events.contains(&"skip after-silent".to_string()), "{events:?}");
    assert!(said.iter().any(|s| s.contains("did step silent give step after-silent its input?")), "{said:?}");
    let order: Vec<&str> = done.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(order, vec!["plan", "silent", "after-silent", "apply", "annotate"]);
    // reading a step the job does not have is refused, as for for_each
    let lacking: Template = toml::from_str("kind = \"x\"\n[[step]]\nname = \"a\"\nkind = \"reach\"\nreads = \"nope\"\n").unwrap();
    assert!(run_steps(&lacking.step, "x", None, &mut Kept::default()).is_err());
}

#[test]
fn templates_load_through_the_kit_shipped_or_as_the_project_has_them() {
    use custodian_kit::store::{Mode, Store};
    // no project: the shipped templates
    let bare = std::env::temp_dir().join(format!("custodian-kit-templates-bare-{}", std::process::id()));
    std::fs::create_dir_all(&bare).unwrap();
    let t = custodian_kit::jobs::template(&bare, "triage").unwrap();
    assert_eq!((t.kind.as_str(), t.step.len()), ("triage", 4));
    assert_eq!(custodian_kit::jobs::template(&bare, "nope").unwrap_err().code, "CUST0015", "no job kind");
    let kinds: Vec<String> = custodian_kit::jobs::templates(&bare).unwrap().into_iter().map(|t| t.kind).collect();
    assert_eq!(kinds, vec!["features", "triage"]);
    // a project's own: an edited shipped one and one of its own kind, both through the same loader
    let root = copy_fixture("templates");
    let (store, _) = Store::init(&root, Mode::Agent).unwrap();
    let dir = store.path(&custodian_kit::store::named::JOB_TEMPLATE, &[("kind", "x")]).unwrap().parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("chain.toml"), "kind = \"chain\"\nsummary = \"a chain\"\n[[step]]\nname = \"a\"\nkind = \"reach\"\n[[step]]\nname = \"b\"\nkind = \"reach\"\nreads = \"a\"\n").unwrap();
    std::fs::write(dir.join("triage.toml"), "kind = \"triage\"\nsummary = \"edited\"\n[[step]]\nname = \"codes\"\nkind = \"diagnostics\"\ntarget = \"{target}\"\n").unwrap();
    let all = custodian_kit::jobs::templates(&root.join("crates")).unwrap();
    let by: Vec<(&str, usize)> = all.iter().map(|t| (t.kind.as_str(), t.step.len())).collect();
    assert_eq!(by, vec![("chain", 2), ("features", 3), ("triage", 1)], "found from below the root, the project's own over the shipped");
    assert_eq!(custodian_kit::jobs::template(&root, "chain").unwrap().step[1].reads.as_deref(), Some("a"));
    assert_eq!(custodian_kit::jobs::template(&root, "triage").unwrap().summary, "edited");
}

/// A step executor for the runner's own tests: each step answered over the
/// scope's workspace with `api::run`, nothing kept.
struct Answer;

impl custodian_kit::jobs::runner::RunStep for Answer {
    fn run(&mut self, ws: Option<&Workspace>, step: &custodian_kit::jobs::Step, _n: usize, target: &str, _items: &[String], _said: &mut Vec<String>) -> custodian_kit::Result<Option<custodian_kit::reading::ReadingReport>> {
        let ws = ws.expect("the scope's workspace");
        let s = ReadingSpec::from_toml(&format!("name = \"{}\"\nquestion = \"q\"\nkind = \"{}\"\ntarget = {}\n", step.name, step.kind, toml::Value::String(target.into()))).unwrap();
        Ok(Some(api::run(ws, &s).map_err(custodian_kit::CustodianError::from)?.report))
    }
}

/// The line runner's program: the `exec_reading_line` example cargo builds beside the tests.
fn line_program() -> PathBuf {
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    deps.parent().unwrap().join("examples").join("exec_reading_line")
}

fn queue_specs_job(root: &Path) -> custodian_kit::store::Store {
    use custodian_kit::store::{JobState, Mode, Store};
    let (store, _) = Store::init(root, Mode::Agent).unwrap();
    let dir = store.job_dir(JobState::Queued, "j").unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("v1.job.1o.job"), "{ job { name \"j\" kind \"specs\" } }\n").unwrap();
    std::fs::write(dir.join("v1.a-use-sites.1o.reading"), "{ reading { name \"a-use-sites\" question \"who calls serve_metrics\" kind \"use_sites\" target \"serve_metrics\" } }\n").unwrap();
    std::fs::write(dir.join("v1.b-dependents.1o.reading"), "{ reading { name \"b-dependents\" question \"what depends on core_lib\" kind \"dependents\" target \"crate:core_lib\" } }\n").unwrap();
    store
}

/// A receipt without what differs by when and how it ran: seq, time, runner,
/// the job's id, and the digests of a line that carries the stamp.
fn comparable(r: &custodian_kit::api::Receipt, root: &Path, job: &str) -> serde_json::Value {
    let stamp = regex_free_stamp;
    let norm = |s: &str| stamp(&s.replace(&root.display().to_string(), "<root>").replace(job, "<job>"));
    serde_json::json!({
        "command": r.command, "name": r.name, "spec": r.spec, "spec_hash": r.spec_hash, "code_digest": r.code_digest, "index_key": r.index_key,
        "plan_id": r.plan_id, "line": norm(&r.line), "exit_code": r.exit_code, "accepted": r.accepted, "disposition": r.disposition,
        // the reason without where the index came from (parsed, from the cache, held): each runner's own
        "reason": r.reason.split(" · index ").next().unwrap_or_default(),
        "outputs": r.outputs.iter().map(|o| norm(o)).collect::<Vec<_>>(), "schema": r.schema,
    })
}

/// `20261019T083829Z` (and `-2`) as `<stamp>`, without a regex crate.
fn regex_free_stamp(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let is_stamp = i + 16 <= b.len() && b[i..i + 8].iter().all(u8::is_ascii_digit) && b[i + 8] == b'T' && b[i + 9..i + 15].iter().all(u8::is_ascii_digit) && b[i + 15] == b'Z';
        if is_stamp {
            out.push_str("<stamp>");
            i += 16;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

#[test]
fn a_job_leaves_the_same_receipts_and_records_whichever_runner_answers() {
    use custodian_kit::runs::Ctx;
    let exe = line_program();
    assert!(exe.is_file(), "{} (cargo test builds the examples)", exe.display());
    let mut left = Vec::new();
    // both runs at one path (the registry's index key carries the root): the line run's tree is moved aside after
    let root = copy_fixture("runner");
    for in_process in [false, true] {
        if in_process {
            std::fs::rename(&root, root.with_extension("line-run")).unwrap();
            let fresh = copy_fixture("runner-second");
            std::fs::rename(&fresh, &root).unwrap();
        }
        let store = queue_specs_job(&root);
        let ctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: exe.clone() };
        let ws = in_process.then(|| Workspace::open_with(&store.root, &Default::default(), Resolving::Job).unwrap());
        let started = custodian_kit::jobs::start_with(&ctx, Some("j"), true, ws.as_ref()).unwrap();
        custodian_kit::jobs::close_with(&ctx, &started.id, ws.as_ref()).unwrap();
        let runs = store.journal().unwrap().runs().unwrap();
        let readings: Vec<&custodian_kit::api::Receipt> = runs.iter().filter(|r| r.command == "reading").collect();
        assert_eq!(readings.len(), 2);
        assert!(readings.iter().all(|r| r.runner == if in_process { "in_process" } else { "line" }));
        // the line field holds the line it ran, or would have run
        assert!(readings.iter().all(|r| r.line.contains("exec-reading --spec")));
        let receipts: Vec<serde_json::Value> = readings.iter().map(|r| comparable(r, &store.root, &started.id)).collect();
        let index = task_history::load_task_history(custodian_kit::task_records::root(&store).unwrap()).unwrap();
        let records: Vec<(String, String)> = index.tasks[0].revisions[0]
            .documents
            .iter()
            .map(|d| (d.one("record_kind").unwrap().to_string(), regex_free_stamp(&d.fields.iter().filter(|(k, _)| !matches!(k.as_str(), "sequence" | "tag" | "resulting_tree_digest")).map(|(k, v)| format!("{k}={}", v.join("|"))).collect::<Vec<_>>().join(";").replace(&started.id, "<job>"))))
            .collect();
        left.push((receipts, records, readings.iter().map(|r| r.seq).collect::<Vec<_>>()));
    }
    assert_eq!(left[0].0, left[1].0, "receipts");
    assert_eq!(left[0].1, left[1].1, "solution and other records");
    assert_eq!(left[0].2, left[1].2, "the journal's counter numbers them alike");
}
