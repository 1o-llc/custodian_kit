// Copyright © 2026 1o. All rights reserved.

//! The impact reading and its record, on the fixture workspaces: what a change to a type, an enum, a crate and a
//! feature-gated item entails; the cost by dimension with its basis; the linkage records that bind a target, stale
//! once a bound declaration is gone.

use custodian_kit::api::{self, ReadingSpec, Resolving, Workspace};
use custodian_kit::linkage::{self, LinkKind, Status};
use custodian_kit::reading::facts::Facts;
use custodian_kit::reading::impact::{Basis, ImpactRecord};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn impact(root: &Path, target: &str) -> api::Reading {
    let s = ReadingSpec::from_toml(&format!("name = \"i\"\nquestion = \"what does a change to it entail?\"\nkind = \"impact\"\ntarget = {}\n", toml::Value::String(target.into()))).unwrap();
    assert!(s.scope.tests, "impact reads the tests too");
    let ws = Workspace::open_for(root, &s, Resolving::Free).unwrap();
    api::run(&ws, &s).unwrap_or_else(|e| panic!("impact {target}: {e}"))
}

fn count(r: &api::Reading, what: &str) -> usize {
    r.counts().iter().find(|c| c.what == what).unwrap_or_else(|| panic!("no count {what}: {:?}", r.counts())).n
}

fn rows<'a>(r: &'a api::Reading, kind: &str) -> Vec<&'a api::Row> {
    r.rows().iter().filter(|x| x.kind == kind).collect()
}

#[test]
fn a_struct_is_read_with_its_uses_constructions_impls_dependents_and_tests() {
    let r = impact(&fixture("ws"), "core_lib::Config");
    assert_eq!(count(&r, "definitions"), 1);
    assert!(count(&r, "use sites") >= 2, "{:?}", r.counts());
    assert!(count(&r, "constructions") >= 1, "Config {{ name }} in new");
    assert_eq!(count(&r, "impl blocks"), 1);
    assert!(count(&r, "uses from other crates") >= 1, "app uses it");
    assert_eq!(count(&r, "binaries"), 1, "appd links it: {:?}", r.counts());
    assert!(count(&r, "tests covering it") >= 1, "config_names");
    assert!(rows(&r, "impl").iter().any(|x| x.label == "impl Config"));
    assert!(rows(&r, "test-line").iter().any(|x| x.label.contains("config_names")));
    let rec = ImpactRecord::of(&r.report);
    assert_eq!(rec.risk_class, "binary-facing");
    assert!(rec.packages.contains(&"app".to_string()) && rec.packages.contains(&"core_lib".to_string()), "{:?}", rec.packages);
    // the cost says how each number is known, and writes no number it cannot know
    let runtime = rec.dimensions.iter().find(|d| d.name == "runtime cost").unwrap();
    assert_eq!((runtime.value, runtime.basis), (None, Basis::Unknown));
    let uses = rec.dimensions.iter().find(|d| d.name == "use sites").unwrap();
    assert_eq!(uses.basis, Basis::Measured);
    assert!(rec.dimensions.iter().any(|d| d.basis == Basis::Heuristic));
    // as a task history diagnosis record
    let d = rec.to_record(Some(7));
    assert_eq!(d.kind().unwrap(), task_history::DiffRecordKind::Diagnosis);
    assert!(d.one("diagnosis").unwrap().starts_with("impact: core_lib::Config (item)"), "{:?}", d.one("diagnosis"));
    assert!(d.many("dimension").iter().any(|x| x == "runtime cost=unknown (unknown)"));
}

#[test]
fn an_enum_lists_the_matches_a_new_variant_breaks() {
    let r = impact(&fixture("ws"), "core_lib::Mode");
    // label() names every variant with no catch-all; serving() has one
    assert_eq!(count(&r, "matches without a catch-all"), 1, "{:?}", rows(&r, "match"));
    assert!(rows(&r, "match").len() >= 2);
}

#[test]
fn a_crate_is_read_by_its_public_items_and_their_uses_elsewhere() {
    let r = impact(&fixture("ws"), "crate:core_lib");
    assert!(count(&r, "definitions") >= 5, "public items");
    assert!(count(&r, "uses from other crates") >= 2);
    assert!(count(&r, "dependents") >= 1);
}

#[test]
fn a_change_under_several_feature_configurations_counts_them() {
    let root = fixture("features_ws");
    let r = impact(&root, "store::Store");
    let gates: Vec<&str> = rows(&r, "impact-feature").iter().map(|x| x.label.as_str()).collect();
    assert!(gates.contains(&"store: feature log"), "log_line reaches it: {gates:?}");
    assert!(gates.contains(&"store: feature metrics"), "metrics(&Store) reaches it: {gates:?}");
    let configs: Vec<(&str, &str)> = rows(&r, "impact-configurations").iter().map(|x| (x.label.as_str(), x.attributes["configurations"].as_str())).collect();
    let store = configs.iter().find(|(p, _)| *p == "store").expect("store's check matrix").1.parse::<usize>().unwrap();
    assert!(store >= 4, "two features: at least none, log, metrics, both: {configs:?}");
    assert!(count(&r, "feature configurations to check") >= store);
    assert!(configs.iter().any(|(p, _)| *p == "api"), "a dependent crate is checked too: {configs:?}");
    let literals = rows(&r, "construct").iter().filter(|x| x.attributes["how"] == "struct literal").count();
    assert_eq!(literals, 2, "Store::new's literal and api::sized's");
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
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

#[test]
fn a_linkage_record_binds_two_declarations_and_goes_stale_when_one_is_gone() {
    let dir = std::env::temp_dir().join(format!("custodian-kit-linkage-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    copy(&fixture("ws"), &dir);
    let (store, _) = custodian_kit::store::Store::init(&dir, custodian_kit::store::Mode::Plain).unwrap();
    let facts = Facts::build(&store.root, &Default::default(), Resolving::Free).unwrap();
    // a bind that does not resolve is refused when recorded
    let e = linkage::record(&facts, "x", LinkKind::Synchronized, "w", &["core_lib::Config".into(), "core_lib::Nope".into()], None).unwrap_err();
    assert!(e.to_string().contains("CUST0035"), "{e}");
    let rec = linkage::record(&facts, "config-and-the-app", LinkKind::Synchronized, "the app prints the config's name; the two change together", &["core_lib::Config".into(), "crate:app".into()], Some("the fixture")).unwrap();
    assert_eq!(rec.evidence.len(), 2, "both bound files' digests: {:?}", rec.evidence);
    let file = linkage::write(&store, &rec).unwrap();
    assert!(file.ends_with("v1.config-and-the-app.1o.linkage"), "{}", file.display());
    // the impact of Config names it, and why
    let r = impact(&store.root, "core_lib::Config");
    let l = rows(&r, "linkage");
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].attributes["status"], "ok");
    assert_eq!(count(&r, "linkage records"), 1);
    // Config's file changes: the record is to be reviewed
    let lib = store.root.join("crates/core_lib/src/lib.rs");
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\n// touched\n").unwrap();
    let facts = Facts::build(&store.root, &Default::default(), Resolving::Free).unwrap();
    let (_, r1, b1) = linkage::checked(&facts).unwrap().into_iter().next().unwrap();
    assert_eq!(r1.name, "config-and-the-app");
    assert_eq!(linkage::status(&b1), Status::Review);
    // Config is renamed away: stale
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap().replace("pub struct Config", "pub struct Settings").replace("impl Config", "impl Settings")).unwrap();
    let facts = Facts::build(&store.root, &Default::default(), Resolving::Free).unwrap();
    let (_, _, b2) = linkage::checked(&facts).unwrap().into_iter().next().unwrap();
    assert_eq!(linkage::status(&b2), Status::Stale);
    // an anchor naming no record is listed, not trusted
    std::fs::write(&lib, std::fs::read_to_string(&lib).unwrap() + "\n// linkage: nobody-wrote-this\npub fn anchored() {}\n").unwrap();
    let s = ReadingSpec::from_toml("name = \"l\"\nquestion = \"q\"\nkind = \"linkage\"\ntarget = \"*\"\n").unwrap();
    let ws = Workspace::open_for(&store.root, &s, Resolving::Free).unwrap();
    let r = api::run(&ws, &s).unwrap();
    assert_eq!(count(&r, "comment anchors with no record"), 1);
    assert_eq!(count(&r, "stale (a bound declaration no longer resolves)"), 1);
}
