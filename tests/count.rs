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
