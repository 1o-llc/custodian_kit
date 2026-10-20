// Copyright © 2026 1o. All rights reserved.

//! The parsed index held once per code digest (the second scan of the same code reads it back; a changed file
//! parses again; a kind that walks the trees parses), a reading stored once with its folder's index naming the
//! latest, the impact of the whole workspace (`*`), and the tests a change to a manifest or a non-Rust file reaches.

use custodian_kit::api::{Resolving, Scope, Workspace};
use custodian_kit::reading::facts::{Facts, IndexSource};
use custodian_kit::runs::{Ctx, Runner};
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
    let dir = std::env::temp_dir().join(format!("custodian-kit-index-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ws"), &dir);
    let (store, _) = Store::init(&dir, Mode::Agent).unwrap();
    let ctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: PathBuf::from("custodian") };
    (store, ctx)
}

#[test]
fn the_second_scan_of_the_same_code_reads_the_index_back_and_a_change_parses_again() {
    let (store, _) = project("cache");
    let first = Facts::build(&store.root, &Scope::default(), Resolving::Free).unwrap();
    assert!(matches!(first.index, IndexSource::Parsed { .. }) && first.trees, "{:?}", first.index);
    let started = std::time::Instant::now();
    let second = Facts::build(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let took = started.elapsed();
    assert!(matches!(second.index, IndexSource::FromCache { .. }) && !second.trees, "{:?}", second.index);
    assert!(took < std::time::Duration::from_secs(1), "from the cache in {took:?}");
    // the same answers
    assert_eq!(second.k.items.len(), first.k.items.len());
    assert_eq!(serde_json::to_value(&second.k.graph).unwrap(), serde_json::to_value(&first.k.graph).unwrap());
    // one model on disk, under the folder's state, compressed; 0.3.3's uncompressed file beside it is let go
    let index = store.root.join(".custodian/state/index");
    assert_eq!(std::fs::read_dir(&index).unwrap().count(), 1);
    let folder = std::fs::read_dir(&index).unwrap().next().unwrap().unwrap().path();
    let kept = folder.join("model.cbor.zst");
    let size = std::fs::metadata(&kept).unwrap().len();
    assert!(size > 0 && size < custodian_kit::reading::facts::INDEX_CACHE_MAX_BYTES, "{size}");
    std::fs::write(folder.join("model.cbor"), b"0.3.3's form").unwrap();
    // a changed file: parsed again, kept beside the first
    let lib = store.root.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn changed() {}\n").unwrap();
    let third = Facts::build(&store.root, &Scope::default(), Resolving::Free).unwrap();
    assert!(matches!(third.index, IndexSource::Parsed { .. }), "{:?}", third.index);
    assert!(third.k.items.iter().any(|d| d.id == "core_lib::changed"));
    assert_eq!(std::fs::read_dir(&index).unwrap().count(), 2);
    assert!(!folder.join("model.cbor").exists() && kept.exists(), "the uncompressed form goes, the kept model stays");
    // a third code: the oldest is let go, two kept
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\npub fn changed_again() {}\n").unwrap();
    Facts::build(&store.root, &Scope::default(), Resolving::Free).unwrap();
    assert_eq!(std::fs::read_dir(&index).unwrap().count(), custodian_kit::reading::facts::INDEX_CACHE_KEEP);
}

#[test]
fn a_reading_from_the_cache_says_so_and_a_count_parses_for_its_trees() {
    let (store, ctx) = project("says");
    Facts::build(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    assert!(matches!(w.facts().index, IndexSource::FromCache { .. }));
    let spec = store.root.join("reach.toml");
    std::fs::write(&spec, "name = \"reach-core\"\nquestion = \"q\"\nkind = \"reach\"\ntarget = \"crate:core_lib\"\n").unwrap();
    let ran = custodian_kit::runs::run_reading_by(&ctx, &spec, true, None, Runner::InProcess(&w)).unwrap();
    let r = ran.receipt.unwrap();
    assert!(r.reason.contains("index from cache ("), "{}", r.reason);
    // a count walks the syntax trees: answered over a parse, and says so
    let count = store.root.join("count.toml");
    std::fs::write(&count, "name = \"count-core\"\nquestion = \"q\"\nkind = \"count\"\ntarget = \"crate:core_lib\"\n").unwrap();
    let ran = custodian_kit::runs::run_reading_by(&ctx, &count, true, None, Runner::InProcess(&w)).unwrap();
    assert!(ran.receipt.unwrap().reason.contains("index parsed ("));
    // and so do the duplicates: their shapes are the function bodies
    let dup = store.root.join("dup.toml");
    std::fs::write(&dup, "name = \"dup\"\nquestion = \"q\"\nkind = \"duplicates\"\ntarget = \"*\"\n").unwrap();
    let ran = custodian_kit::runs::run_reading_by(&ctx, &dup, true, None, Runner::InProcess(&w)).unwrap();
    assert!(ran.receipt.unwrap().reason.contains("index parsed ("));
    // a model over what the cache holds is not kept, and the line says so
    let line = IndexSource::ParsedNotKept { millis: 35901, bytes: 300_000_000 }.line();
    assert_eq!(line, "parsed (35901 ms; not kept: 300 MB, over the 256 MB the cache holds)");
}

#[test]
fn a_reading_is_stored_once_and_its_folder_names_the_latest() {
    let (store, ctx) = project("once");
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let spec = store.root.join("reach.toml");
    std::fs::write(&spec, "name = \"reach-core\"\nquestion = \"q\"\nkind = \"reach\"\ntarget = \"crate:core_lib\"\n").unwrap();
    custodian_kit::runs::run_reading_by(&ctx, &spec, true, None, Runner::InProcess(&w)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    custodian_kit::runs::run_reading_by(&ctx, &spec, true, None, Runner::InProcess(&w)).unwrap();
    let dir = store.reading_dir("reach-core").unwrap();
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(!names.iter().any(|n| n.starts_with("latest")), "no copies: {names:?}");
    let idx: custodian_kit::reading::ReadingsIndex = serde_json::from_str(&std::fs::read_to_string(dir.join(custodian_kit::reading::READINGS_INDEX)).unwrap()).unwrap();
    assert_eq!(idx.readings.len(), 2);
    assert_eq!(idx.latest, idx.readings[1]);
    assert!(dir.join(format!("{}.json", idx.latest)).is_file() && dir.join(format!("{}.md", idx.latest)).is_file());
    assert_eq!(names.len(), 5, "two readings (page and record each) and the index: {names:?}");
}

#[test]
fn the_impact_of_the_whole_workspace_is_read() {
    let (store, ctx) = project("star");
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let spec = custodian_kit::api::ReadingSpec::search(custodian_kit::api::ReadingKind::Impact, "*").unwrap();
    let file = store.root.join("impact-star.toml");
    std::fs::write(&file, toml::to_string(&spec).unwrap()).unwrap();
    let ran = custodian_kit::runs::run_reading_by(&ctx, &file, false, None, Runner::InProcess(&w)).unwrap();
    let rec = custodian_kit::reading::load(&store.root.join(ran.receipt.unwrap().outputs.iter().find(|o| o.ends_with(".json")).unwrap())).unwrap();
    let packages = rec.counts.iter().find(|c| c.what == "packages").unwrap().n;
    assert_eq!(packages, w.klepto().packages.len(), "{:?}", rec.counts);
    assert!(rec.counts.iter().find(|c| c.what == "definitions").unwrap().n > 0);
}

#[test]
fn a_changed_manifest_or_other_file_reaches_every_test_of_its_crate() {
    let (store, _) = project("affected");
    let w = Workspace::open_with(&store.root, &Scope { tests: true, ..Default::default() }, Resolving::Free).unwrap();
    for file in ["crates/core_lib/Cargo.toml", "crates/core_lib/tests/ui/case.stderr"] {
        let spec = custodian_kit::api::ReadingSpec::from_toml(&format!("name = \"aff\"\nquestion = \"q\"\nkind = \"affected_tests\"\ntarget = \"*\"\nscope = {{ tests = true }}\n\n[change]\nfiles = [\"{file}\"]\n")).unwrap();
        let r = custodian_kit::api::run(&w, &spec).unwrap();
        let tests: Vec<String> = r.report.query.results.iter().filter(|x| x.kind == "test").map(|x| x.label.clone()).collect();
        assert!(tests.iter().any(|t| t.ends_with("metrics_say_where")) && tests.iter().any(|t| t.ends_with("modes_parse")), "{file}: {tests:?}");
        assert!(r.report.query.results.iter().any(|x| x.kind == "test-line" && x.label.contains("-p core_lib --lib")), "{file}");
    }
}
