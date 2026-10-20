//! The `.custodian/` folder, named once: every part's route and path, pinned. custodian and differences_kit take
//! these descriptors and test that their paths are these.

use custodian_kit::store::named::{self, ALL};
use pathgen::ArtifactOpDescriptor;
use std::path::Path;

/// Every part, as `route  path` with its template unfilled, in `ALL`'s order.
const GOLDEN: &[(&str, &str)] = &[
    ("custodian.settings", "config/custodian.toml"),
    ("custodian.paths", "config/v1.paths.1o.paths"),
    ("custodian.schema", "config/schema/{modifier}/v1.schema.json"),
    ("custodian.thresholds", "config/{name}.toml"),
    ("custodian.job_template", "config/jobs/{kind}.toml"),
    ("custodian.spec.queued", "data/specs/queued/{name}.toml"),
    ("custodian.spec.done", "data/specs/done/{name}.toml"),
    ("custodian.reading.page", "data/readings/{name}/{stamp}.md"),
    ("custodian.receipt", "data/receipts/{seq}.json"),
    ("custodian.inventory", "data/inventory.md"),
    ("custodian.plan", "data/plans/{name}.toml"),
    ("custodian.history", "data/history/v1.history_index.1o.diff"),
    ("custodian.journal", "state/journal.sqlite"),
    ("custodian.job", "state/jobs/{state}/{job}/v1.job.1o.job"),
    ("custodian.serve", "runtime/serve.sock"),
    ("custodian.reading.record", "data/readings/{name}/{stamp}.json"),
    ("custodian.check", "data/checks/{stamp}.json"),
    ("custodian.log", "data/logs/{name}.txt"),
    ("custodian.writer", "state/writer.sqlite"),
    ("custodian.change", "data/specs/changes/v1.{name}.1o.change"),
    ("custodian.preflight", "data/preflight/{name}/{stamp}.json"),
    ("custodian.transform", "data/transforms/{name}/{stamp}.json"),
    ("custodian.linkage", "data/linkage/v1.{name}.1o.linkage"),
    ("custodian.intent_plan", "data/plans/intent/{name}.json"),
    ("custodian.tree", "data/trees/{point}.json"),
    ("custodian.index", "state/index/{digest}/model.cbor.zst"),
];

#[test]
fn every_part_is_named_once_and_where_the_golden_says() {
    let got: Vec<(&str, String)> = ALL.iter().map(|d| (d.route_id, format!("{}/{}", d.base_role.label(), d.relative_template))).collect();
    let want: Vec<(&str, String)> = GOLDEN.iter().map(|(r, p)| (*r, p.to_string())).collect();
    assert_eq!(got, want);
    // one binding per route and endpoint
    let ops: &'static [ArtifactOpDescriptor] = Box::leak(ALL.iter().map(|d| ArtifactOpDescriptor::new(*d)).collect::<Vec<_>>().into_boxed_slice());
    pathgen::ArtifactOpRegistry::new(ops).validate_unique().unwrap();
}

#[test]
fn a_part_resolves_under_the_projects_folder_and_never_leaves_it() {
    let p = named::policy(Path::new("/p")).unwrap();
    assert_eq!(named::resolve(&p, &named::JOURNAL, &[]).unwrap(), Path::new("/p/.custodian/state/journal.sqlite"));
    assert_eq!(named::resolve(&p, &named::READING_RECORD, &[("name", "env"), ("stamp", "s")]).unwrap(), Path::new("/p/.custodian/data/readings/env/s.json"));
    assert_eq!(named::resolve(&p, &named::JOB_FILE, &[("state", "done"), ("job", "j")]).unwrap(), Path::new("/p/.custodian/state/jobs/done/j/v1.job.1o.job"));
    assert_eq!(named::resolve(&p, &named::TREE, &[("point", "one")]).unwrap(), Path::new("/p/.custodian/data/trees/one.json"));
    assert!(named::resolve(&p, &named::QUEUED_SPEC, &[("name", "../x")]).is_err(), "a template value cannot leave the folder");
    // the store resolves through the same two functions
    let d = std::env::temp_dir().join(format!("kit-layout-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let (store, _) = custodian_kit::store::Store::init(&d, custodian_kit::store::Mode::Plain).unwrap();
    for part in ALL {
        let vars: Vec<(&str, &str)> = ["name", "stamp", "seq", "state", "job", "kind", "modifier", "point", "digest"].iter().map(|k| (*k, "x")).collect();
        assert_eq!(store.path(part, &vars).unwrap(), named::resolve(&named::policy(&store.root).unwrap(), part, &vars).unwrap(), "{}", part.route_id);
    }
}
