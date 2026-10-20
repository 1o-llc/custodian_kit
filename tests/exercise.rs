// Copyright © 2026 1o. All rights reserved.

//! The exercise job kind on copies of the fixture workspace: its picks and specs come from the index, its cases run
//! on copies in the job's data folder (the project's tree is never written), and what it finds is receipts, a
//! summary, and diagnosis records in the job's task history.

use custodian_kit::api::Workspace;
use custodian_kit::change::{ChangeSpec, Operation};
use custodian_kit::exercise::{self, Options, Unit};
use custodian_kit::runs::Ctx;
use custodian_kit::store::{Mode, Store};
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

fn project(tag: &str) -> (Store, Ctx) {
    project_of("ws", tag)
}

fn project_of(fixture: &str, tag: &str) -> (Store, Ctx) {
    let dir = std::env::temp_dir().join(format!("custodian-kit-exercise-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture), &dir);
    let (store, _) = Store::init(&dir, Mode::Agent).unwrap();
    let ctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: PathBuf::from("custodian") };
    (store, ctx)
}

fn passenger_here() -> bool {
    custodian_kit::exec::on_path("code-passenger")
}

/// Every spec the exercise writes is a change spec its own checks take, and the fixture gives the operations their
/// targets: a struct with named fields for `add_field`, an enum for `add_variant`, a function for `replace_body`, a
/// crate with a dependency for `change_dependency`.
#[test]
fn the_picks_and_their_specs_come_from_the_index() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ws");
    let ws = Workspace::open(&root).unwrap();
    let k = ws.klepto();
    let crates: Vec<String> = k.packages.keys().cloned().collect();
    let mut made = 0;
    for op in Operation::ALL {
        for unit in exercise::units_of(*op) {
            for (n, t) in exercise::targets(k, &root, *op, *unit, None).into_iter().take(3).enumerate() {
                let Some(spec) = exercise::spec_for(k, &root, *op, &t, n as u64 + 1, &crates) else { continue };
                let text = spec.to_toml().unwrap();
                let back = ChangeSpec::from_toml(&text).unwrap_or_else(|e| panic!("{} on {}: {e}\n{text}", op.as_str(), t.route));
                assert_eq!((back.op(), back.target.as_str()), (*op, t.route.as_str()));
                assert_eq!(back.scope.write.len(), crates.len(), "every crate of the copy is in its scope");
                made += 1;
            }
        }
    }
    assert!(made > 30, "{made} specs");
    let has = |op: Operation, unit: Unit, route: &str| exercise::targets(k, &root, op, unit, None).iter().any(|t| t.route == route);
    assert!(has(Operation::AddField, Unit::Item, "core_lib::Config"));
    assert!(has(Operation::ChangeDependency, Unit::Crate, "crate:app"));
    assert!(!has(Operation::ChangeDependency, Unit::Crate, "crate:core_lib"), "core_lib names no dependency");
    assert!(has(Operation::MakeStruct, Unit::File, "crate:core_lib/src/lib.rs"));
    // a crate restricts the picks to its own units
    assert!(exercise::targets(k, &root, Operation::Rename, Unit::Function, Some("tool")).iter().all(|t| t.package == "tool"));
}

/// An exercise job from queue to close: its cases on copies in its data folder, the project's own code untouched,
/// each line a receipt of the job, each case a verification of its task, each finding a diagnosis naming its
/// receipt, the summary and the `check` receipt that verifies the job.
#[test]
fn an_exercise_runs_its_cases_on_copies_and_keeps_what_it_found() {
    if !passenger_here() {
        eprintln!("code-passenger is not on PATH: the exercise's transforms need it");
        return;
    }
    let (store, ctx) = project("job");
    let before = custodian_kit::code_digest(&store.root, &Default::default()).unwrap();
    let options = Options::from_words(Some("add_field,add_variant,rename"), Some("item"), Some("1"), false, Some("core_lib"), None, Some("3")).unwrap();
    let (started, closing, done, _) = exercise::run_job(&ctx, &options).unwrap();
    assert_eq!(custodian_kit::code_digest(&store.root, &Default::default()).unwrap(), before, "the project's tree is never written");
    let s = exercise::summary_of(&done).expect("the summary is kept in the job's data folder");
    assert_eq!(s.job, started.id);
    assert_eq!(s.cases.len(), 3, "{:#?}", s.cases);
    assert!(s.cases.iter().all(|c| c.tree.as_deref().is_some_and(|t| t.starts_with("cases/") && done.join("data/exercise").join(t).join("Cargo.toml").is_file())), "each case on a copy of its own");
    let field = s.cases.iter().find(|c| c.operation == "add_field").unwrap();
    assert_eq!((field.preflight.as_deref(), field.compile.as_deref(), field.status.as_str()), (Some("authorized"), Some("compiles"), "passed"), "{field:#?}");
    assert!(field.lines_touched > 0 && field.uncovered.is_empty(), "{field:#?}");
    // every case's lines are the job's receipts; the check receipt verifies it
    let journal = store.journal().unwrap();
    let runs = journal.runs_of_job(&started.id).unwrap();
    for c in &s.cases {
        for seq in &c.receipts {
            assert!(runs.iter().any(|r| r.seq == *seq), "case {} receipt #{seq} is the job's", c.n);
        }
    }
    let check = runs.iter().find(|r| r.command == "check" && r.name == "exercise").expect("the exercise's check receipt");
    assert_eq!(check.accepted, s.findings.is_empty());
    assert_eq!(closing.verify, [("check:exercise".to_string(), if s.findings.is_empty() { "satisfied" } else { "failed" }.to_string())]);
    assert!(closing.guidance.as_deref().is_some_and(|g| g.starts_with(&format!("exercise {}:", started.id))), "{:?}", closing.guidance);
    // each finding a diagnosis record of the job's task, with its receipt
    let task = custodian_kit::task_records::read(&store, &custodian_kit::task_records::task_id(&closing.name)).unwrap().unwrap();
    let diagnoses: Vec<_> = task.revisions.iter().flat_map(|r| r.documents.iter()).filter(|d| d.kind().ok() == Some(task_history::DiffRecordKind::Diagnosis)).collect();
    assert_eq!(diagnoses.len(), s.findings.len());
    for (d, f) in diagnoses.iter().zip(&s.findings) {
        assert_eq!(d.one("code").unwrap(), f.code.as_str());
        assert_eq!(d.one("receipt").ok(), f.receipt.as_deref());
    }
    let page = exercise::markdown(&s);
    assert!(page.contains("## operation × unit") && page.contains("| add_field | item | 1 | 1 | 0 |"), "{page}");
    // and the report reads it back by the job's id
    let (again, _) = exercise::report(&ctx, Some(&started.id)).unwrap();
    assert_eq!(again, s);
}

#[test]
fn the_readme_carries_the_exercise_s_vocabularies() {
    let readme = include_str!("../README.md");
    assert!(readme.contains(&Unit::doc_table()), "README.md must carry exercise::Unit::doc_table()");
    assert!(readme.contains(&exercise::FindingCode::doc_table()), "README.md must carry exercise::FindingCode::doc_table()");
}

/// What the exercise found on the features fixture, now the preflight's to refuse: a private field a struct literal in
/// another crate cannot name (`api::sized` builds `store::Store`), and an item gated while code outside it uses it.
#[test]
fn the_preflight_refuses_what_the_exercise_found_on_the_features_fixture() {
    let (store, ctx) = project_of("features_ws", "found");
    let ws = Workspace::open_with(&store.root, &custodian_kit::api::Scope { tests: true, ..Default::default() }, custodian_kit::api::Resolving::Free).unwrap();
    let dir = store.root.join(".custodian/data/specs/changes");
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, text: &str| {
        let p = dir.join(format!("{name}.toml"));
        std::fs::write(&p, text).unwrap();
        p
    };
    let field = "name = \"f\"\nobjective = \"o\"\noperation = \"add_field\"\ntarget = \"store::Store\"\n[scope]\nwrite = [\"crate:store\", \"crate:api\"]\n[field]\nname = \"tag\"\nty = \"u8\"\ndefault = \"0\"\n";
    let private = custodian_kit::preflight::run_preflight(&ctx, &write("f", field), None, custodian_kit::runs::Runner::InProcess(&ws)).unwrap().preflight;
    assert!(!private.authorized);
    let c = private.preconditions.iter().find(|c| c.what == "every struct literal can name the new field").unwrap();
    assert!(!c.holds && c.detail.contains("api/src/lib.rs:"), "{c:?}");
    let public = custodian_kit::preflight::run_preflight(&ctx, &write("g", &field.replace("name = \"f\"", "name = \"g\"").replace("default = \"0\"", "default = \"0\"\nvis = \"pub\"")), None, custodian_kit::runs::Runner::InProcess(&ws)).unwrap().preflight;
    assert!(public.authorized, "{:?} {:?}", public.reasons, public.preconditions);
    let gate = "name = \"h\"\nobjective = \"o\"\noperation = \"gate\"\ntarget = \"store::Store\"\nfeature = \"metrics\"\n";
    let p = custodian_kit::preflight::run_preflight(&ctx, &write("h", gate), None, custodian_kit::runs::Runner::InProcess(&ws)).unwrap().preflight;
    assert!(!p.authorized);
    let c = p.preconditions.iter().find(|c| c.what == "nothing outside store::Store uses it ungated").unwrap();
    assert!(!c.holds, "{c:?}");
    assert!(p.listed.iter().any(|l| l.starts_with("use api/src/lib.rs:") && l.contains("stops compiling without metrics")), "{:?}", p.listed);
    // a crate whose dependency turns the feature on keeps compiling (api enables store's `log`); the store's own impl
    // does not
    let p = custodian_kit::preflight::run_preflight(&ctx, &write("j", &gate.replace("name = \"h\"", "name = \"j\"").replace("metrics", "log")), None, custodian_kit::runs::Runner::InProcess(&ws)).unwrap().preflight;
    assert!(!p.listed.iter().any(|l| l.starts_with("use api/")), "{:?}", p.listed);
    assert!(p.listed.iter().any(|l| l.starts_with("use store/src/lib.rs:") && l.contains("<impl Store>")), "{:?}", p.listed);
    // a compatibility break gates it anyway
    let p = custodian_kit::preflight::run_preflight(&ctx, &write("i", &gate.replace("name = \"h\"", "name = \"i\"").replace("feature = ", "compatibility = \"break\"\nfeature = ")), None, custodian_kit::runs::Runner::InProcess(&ws)).unwrap().preflight;
    assert!(p.preconditions.iter().all(|c| c.holds), "{:?}", p.preconditions);
}
