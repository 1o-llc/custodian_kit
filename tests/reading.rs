//! Every reading kind on the fixture workspace (tests/fixtures/ws): known use
//! sites, env reads, duplicates, dependents, declarations, sockets, reach,
//! constructors, matches, public API, tests, sensitivity, routes, the diff.

use custodian_kit::api::{self, ReadingKind, ReadingSpec, Resolving, Scope, Workspace};
use custodian_kit::diff;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ws")
}

fn spec(kind: &str, target: &str, extra: &str) -> ReadingSpec {
    ReadingSpec::from_toml(&format!("name = \"t\"\nquestion = \"q\"\nkind = \"{kind}\"\ntarget = {}\n{extra}", toml::Value::String(target.into()))).expect("spec")
}

fn read(kind: &str, target: &str, extra: &str) -> api::Reading {
    let s = spec(kind, target, extra);
    let ws = Workspace::open_for(&fixture(), &s, Resolving::Free).expect("workspace");
    api::run(&ws, &s).unwrap_or_else(|e| panic!("{kind} {target}: {e}"))
}

fn count(r: &api::Reading, what: &str) -> usize {
    r.counts().iter().find(|c| c.what.starts_with(what)).unwrap_or_else(|| panic!("no count {what}: {:?}", r.counts())).n
}

fn rows<'a>(r: &'a api::Reading, kind: &str) -> Vec<&'a api::Row> {
    r.rows().iter().filter(|x| x.kind == kind).collect()
}

const MARKET: &str = r#"
[[declared_in]]
files = ["marketplace/*.toml"]
key = "env.*"
role = "env"
entry = "id"

[[declared_in]]
files = ["marketplace/*.toml"]
key = "binary"
role = "binary"
entry = "id"

[[declared_in]]
files = ["marketplace/*.toml"]
key = "socket.path"
role = "socket"
entry = "id"

[[declared_in]]
files = ["marketplace/*.toml"]
key = "socket.path_env"
role = "socket_env"
entry = "id"
"#;

#[test]
fn use_sites_resolve_through_imports_and_calls() {
    let r = read("use_sites", "serve_metrics", "");
    assert_eq!(count(&r, "definitions"), 1);
    let uses = rows(&r, "use-site");
    // app/src/main.rs calls it, tests/metrics.rs is not read (no tests in the scope)
    assert!(uses.iter().any(|u| u.path.as_deref() == Some("crates/app/src/main.rs")), "{uses:?}");
    assert!(uses.iter().all(|u| u.attributes.get("full_path").map(String::as_str) == Some("core_lib::serve_metrics")));
    assert_eq!(count(&r, "crates using it"), 1);
    assert_eq!(count(&r, "binaries linking"), 1);
}

#[test]
fn readers_of_env_find_literals_constants_and_declarations() {
    let r = read("readers_of_env", "FIX_*", MARKET);
    let keys: Vec<&str> = rows(&r, "env-key").iter().map(|k| k.label.as_str()).collect();
    assert_eq!(keys, vec!["FIX_DECLARED_ONLY", "FIX_NAMED", "FIX_SOCKET", "FIX_TOKEN"]);
    let reads = rows(&r, "env-read");
    // FIX_TOKEN by std::env::var directly; FIX_SOCKET through the constant passed to env_or
    assert!(reads.iter().any(|x| x.label == "FIX_TOKEN" && x.attributes.get("reader").map(String::as_str) == Some("std::env::var")));
    assert!(reads.iter().any(|x| x.label == "FIX_SOCKET" && x.attributes.get("via").is_some_and(|v| v.starts_with("const core_lib::ENV_SOCKET")) && x.attributes.get("reader").map(String::as_str) == Some("core_lib::env_or")));
    assert_eq!(rows(&r, "env-const").len(), 1);
    // FIX_NAMED is a returned literal, read by nothing
    assert!(rows(&r, "env-named").iter().any(|x| x.label == "FIX_NAMED"));
    assert_eq!(count(&r, "keys read but declared nowhere"), 1, "FIX_TOKEN");
    assert_eq!(count(&r, "keys declared but read nowhere"), 1, "FIX_DECLARED_ONLY");
    // the item's marks ride on its rows (from .maintenance/registry.json)
    assert!(reads.iter().any(|x| x.label == "FIX_TOKEN" && x.attributes.get("sensitivity").map(String::as_str) == Some("confidential")));
}

#[test]
fn duplicates_find_the_copied_crates() {
    let r = read("duplicates", "*", "[duplicates]\nmin_tokens = 8\n");
    let pairs: Vec<&str> = rows(&r, "crate-pair").iter().map(|p| p.label.as_str()).collect();
    assert!(pairs.contains(&"copy_a ↔ copy_b"), "{pairs:?}");
    let dups = rows(&r, "duplicate");
    assert!(dups.iter().any(|d| d.label == "copy_a::checksum") && dups.iter().any(|d| d.label == "copy_b::checksum"));
}

#[test]
fn dependents_of_a_crate_and_of_an_item() {
    let r = read("dependents", "core_lib", "");
    let deps: Vec<&str> = rows(&r, "dependent").iter().map(|d| d.label.as_str()).collect();
    assert_eq!(deps, vec!["app", "tool"]);
    assert_eq!(count(&r, "binaries linking it"), 2);
    let r = read("dependents", "core_lib::env_or", "");
    assert!(rows(&r, "dependent").iter().any(|d| d.label == "core_lib::socket"));
}

#[test]
fn declared_in_reads_toml_at_key_paths() {
    let r = read("declared_in", "FIX_SOCKET", MARKET);
    assert_eq!(count(&r, "declarations"), 3, "two env tables and one path_env");
    assert_eq!(count(&r, "entries"), 2);
}

#[test]
fn sockets_follow_the_key_and_the_path() {
    let r = read("sockets", "appd", MARKET);
    assert_eq!(count(&r, "entries running the binary"), 1);
    assert_eq!(count(&r, "sockets declared"), 1);
    assert_eq!(count(&r, "socket env keys declared"), 1);
    assert!(count(&r, "reads of those keys") >= 1);
    // the socket path as a literal in code (the default in core_lib::socket)
    assert!(rows(&r, "dial-literal").iter().any(|d| d.path.as_deref() == Some("crates/core_lib/src/lib.rs")));
    // the tool's entry sets the same key: it dials it
    assert!(rows(&r, "dial-entry").iter().any(|d| d.attributes.get("entry").map(String::as_str) == Some("fix:tool")));
}

#[test]
fn reach_of_a_crate_a_file_and_an_item() {
    let r = read("reach", "core_lib", MARKET);
    assert_eq!(count(&r, "crates"), 3);
    assert_eq!(count(&r, "binaries"), 2);
    assert_eq!(count(&r, "entries naming those binaries"), 2);
    assert_eq!(count(&r, "items at level confidential or above"), 2);
    let r = read("reach", "core_lib::label", MARKET);
    // the bin's crate root is named after its target: toold
    assert!(rows(&r, "reach-item").iter().any(|x| x.label == "toold::main"), "{:?}", rows(&r, "reach-item"));
    assert_eq!(count(&r, "binaries"), 1);
    let r = read("reach", "crates/copy_a/src/lib.rs", "");
    assert_eq!(count(&r, "crates"), 1);
}

#[test]
fn constructs_and_matches_on() {
    let r = read("constructs", "Config", "");
    let built: Vec<(&str, &str)> = rows(&r, "construct").iter().map(|x| (x.path.as_deref().unwrap_or(""), x.attributes.get("how").map(String::as_str).unwrap_or(""))).collect();
    assert!(built.contains(&("crates/core_lib/src/lib.rs", "struct literal")), "{built:?}");
    assert!(built.contains(&("crates/app/src/main.rs", "constructor fn")), "{built:?}");
    let r = read("matches_on", "Mode", "");
    // parse_mode matches on a &str (its arms build Modes); serving and label match on a Mode
    assert_eq!(count(&r, "matches on it"), 2);
    assert_eq!(count(&r, "matches with a catch-all"), 1);
    let label = rows(&r, "match").into_iter().find(|m| m.attributes.get("catch_all").map(String::as_str) == Some("false")).expect("label's match");
    assert_eq!(label.attributes.get("names").map(String::as_str), Some("Maintenance, ReadOnly, Serving"));
}

#[test]
fn public_api_and_its_users() {
    let r = read("public_api", "core_lib", "");
    let label = rows(&r, "public-item").into_iter().find(|x| x.label == "core_lib::label").expect("label");
    assert_eq!(label.attributes.get("used_by_crates").map(String::as_str), Some("tool"));
    assert!(count(&r, "never used from another crate") >= 1);
}

#[test]
fn tests_covering_and_affected_tests_name_exact_lines() {
    let r = read("tests_covering", "core_lib::parse_mode", "");
    let tests: Vec<&str> = rows(&r, "test").iter().map(|t| t.label.as_str()).collect();
    assert_eq!(tests, vec!["core_lib::tests::modes_parse"]);
    let lines: Vec<&str> = rows(&r, "test-line").iter().map(|t| t.label.as_str()).collect();
    assert_eq!(lines, vec!["cargo test -p core_lib --lib -- tests::modes_parse --exact"]);
    // a change to the file of serve_metrics reaches the integration test that calls it
    let r = read("affected_tests", "*", "[change]\nitems = [\"core_lib::serve_metrics\"]\n");
    let lines: Vec<&str> = rows(&r, "test-line").iter().map(|t| t.label.as_str()).collect();
    assert_eq!(lines, vec!["cargo test -p core_lib --test metrics -- metrics_say_where --exact"]);
}

#[test]
fn sensitivity_lists_marked_items_and_redacts_secrets() {
    let r = read("sensitivity", "*", "");
    assert_eq!(count(&r, "items marked"), 2);
    assert_eq!(count(&r, "at secret"), 1);
    let secret = rows(&r, "sensitive-item").into_iter().find(|x| x.label == "core_lib::vault_key").expect("vault_key");
    // the one marker: the bundles redaction line, level and digest of what is held
    let ctx = secret.attributes.get("context").expect("context");
    let marker = bundles::redaction::RedactionMarker::parse("#", ctx).unwrap_or_else(|| panic!("not a marker: {ctx}"));
    assert_eq!(marker.level, "secret");
    assert_eq!(secret.attributes.get("held").map(String::as_str), Some("true"));
    assert_eq!(secret.attributes.get("mechanism").map(String::as_str), Some("vault keys"));
}

#[test]
fn routes_resolve_and_jobs_refuse_walked_paths() {
    let s = spec("reach", "crate:core_lib", "");
    let ws = Workspace::open_for(&fixture(), &s, Resolving::Job).unwrap();
    let r = api::run(&ws, &s).unwrap();
    assert_eq!(count(&r, "crates"), 3);
    let s = spec("reach", "crate:core_lib/src/lib.rs", "");
    let r = api::run(&ws, &s).unwrap();
    assert!(count(&r, "items touched") > 5);
    // a raw path in a job: refused, with its code
    let s = spec("reach", "crates/core_lib/src/lib.rs", "");
    let e = api::run(&ws, &s).unwrap_err();
    assert_eq!(e.code, "CUST0023");
    // an undeclared route name: refused
    let s = spec("reach", "crate:nope", "");
    let e = api::run(&ws, &s).unwrap_err();
    assert_eq!(e.code, "CUST0022");
    let s = spec("declared_in", "FIX_SOCKET", "[[declared_in]]\nfiles = [\"decl:market\"]\nkey = \"env.*\"\n");
    assert_eq!(api::run(&ws, &s).unwrap_err().code, "CUST0022");
}

#[test]
fn the_vocabulary_table_in_the_readme_is_the_enum() {
    let readme = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md")).unwrap();
    assert!(readme.contains(&ReadingKind::doc_table()), "README.md must carry ReadingKind::doc_table()");
    assert!(readme.contains(&api::DeclaredRole::doc_table()), "README.md must carry DeclaredRole::doc_table()");
}

#[test]
fn a_spec_is_the_same_in_toml_and_json_and_refuses_unknown_keys() {
    let s = spec("reach", "core_lib", MARKET);
    let back = ReadingSpec::from_json(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
    assert!(ReadingSpec::from_toml("name = \"x\"\nquestion = \"q\"\nkind = \"reach\"\ntarget = \"a\"\nmystery = 1\n").is_err());
    assert!(ReadingSpec::from_toml("name = \"x\"\nquestion = \"q\"\nkind = \"nope\"\ntarget = \"a\"\n").is_err());
}

#[test]
fn the_diff_says_what_moved() {
    let a = read("use_sites", "serve_metrics", "");
    let b = read("use_sites", "env_or", "");
    let d = diff::compare(&a.report, &b.report);
    assert!(!d.added.is_empty() && !d.removed.is_empty());
    let same = diff::compare(&a.report, &a.report);
    assert!(same.is_empty());
}

#[test]
fn a_workspace_held_open_answers_many_specs() {
    let ws = Workspace::open(&fixture()).unwrap();
    for (k, t) in [("use_sites", "env_or"), ("dependents", "core_lib"), ("public_api", "consumer"), ("constructs", "Config")] {
        let r = api::run(&ws, &spec(k, t, "")).unwrap();
        assert!(!r.counts().is_empty());
    }
    assert!(api::IndexKey::of(&ws).0.starts_with("sha256:"));
    let s = spec("tests_covering", "core_lib::label", "");
    assert_eq!(api::run(&ws, &s).unwrap_err().code, "CUST0002", "tests read: another scope");
    let _ = Scope::default();
}

#[test]
fn the_bundle_kind_goes_through_bundle_rs_with_one_marker() {
    let r = read("bundle", "crate:core_lib", "[scope]\ntests = true\n[bundle]\nfilters = [\"no_tests\"]\n");
    assert!(count(&r, "files bundled") >= 2, "lib.rs and Cargo.toml");
    assert_eq!(count(&r, "windows held redacted (secret or above)"), 1, "vault_key");
    assert!(count(&r, "files left out by the filters") >= 1, "tests/metrics.rs");
    let lib = rows(&r, "bundled-file").into_iter().find(|x| x.label == "crates/core_lib/src/lib.rs").expect("lib.rs");
    assert!(lib.attributes.get("held").is_some_and(|h| h.ends_with("@secret")), "{:?}", lib.attributes);
    let bytes = r.report.artifact.clone().expect("the bundle's bytes");
    let text = String::from_utf8(bytes).unwrap();
    let markers = bundles::redaction::markers_in(&text, "#");
    assert_eq!(markers.len(), 1, "{text}");
    assert_eq!(markers[0].level, "secret");
    assert!(!text.contains("let k = 41;"), "the secret body is held");
    // the artifact is not part of the record
    assert!(!r.json().contains("artifact"));
}

#[test]
fn a_job_bundle_holds_a_secret_window_as_the_marker_and_unbundles() {
    use custodian_kit::job_bundle::{write_entries, Entry};
    let entries = vec![
        Entry { path: "job/job.toml".into(), text: "name = \"j\"\n".into(), held: None },
        Entry { path: "windows/a.L1-3.txt".into(), text: "// a.rs:1-3 (site x)\nlet k = 41;\nk + 1\n".into(), held: Some("secret".into()) },
    ];
    let (bytes, held) = write_entries(&entries, false).unwrap();
    assert_eq!(held, 1);
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(!text.contains("let k = 41;") && text.contains("// a.rs:1-3 (site x)"), "{text}");
    assert_eq!(bundles::redaction::markers_in(&text, "#").len(), 1);
    let (open, held) = write_entries(&entries, true).unwrap();
    assert_eq!(held, 0);
    assert!(String::from_utf8(open).unwrap().contains("let k = 41;"));
    let dir = std::env::temp_dir().join(format!("custodian-kit-unbundle-{}", std::process::id()));
    let file = dir.with_extension("bundle");
    std::fs::write(&file, &bytes).unwrap();
    bundles::unbundle_files(&file, &dir, "#").unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("job/job.toml")).unwrap(), "name = \"j\"\n");
}
