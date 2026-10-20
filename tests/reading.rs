//! Every reading kind on the fixture workspace (tests/fixtures/ws): known use
//! sites, env reads, duplicates, dependents, declarations, sockets, reach,
//! constructors, matches, public API, tests, sensitivity, routes, the diff, the toolkit's patterns.

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
fn patterns_shape_every_function_and_find_the_shapes_repeated_across_crates() {
    let r = read("patterns", "*", "[patterns]\nmin_repeats = 2\n");
    let fns = rows(&r, "function");
    assert_eq!(fns.len(), count(&r, "functions shaped"), "a route for every function");
    assert!(fns.iter().any(|f| f.label == "copy_a::checksum") && fns.iter().all(|f| f.attributes.contains_key("status")));
    // the same loop in copy_a and copy_b, which no piece writes
    let cands = rows(&r, "candidate-shape");
    let lp = cands.iter().find(|c| c.attributes["frame"].starts_with("for $b in $ {")).unwrap_or_else(|| panic!("{:?}", cands.iter().map(|c| &c.attributes["frame"]).collect::<Vec<_>>()));
    assert_eq!(lp.attributes["crates"], "2");
    assert!(lp.attributes["proposed"].contains('!'));
    assert!(!rows(&r, "crate").is_empty());
    // each exact match by a named piece is a change spec file, scoped to its crate, that keeps the expansion
    let files = custodian_kit::reading::patterns::spec_files(&r.report);
    let exact = fns.iter().filter(|f| f.attributes["status"] == "exact").count();
    assert!(exact > 0 && files.len() == exact, "{} files for {exact} exact matches", files.len());
    for (name, text) in &files {
        let s = custodian_kit::change::ChangeSpec::from_toml(text).unwrap_or_else(|e| panic!("{name}: {e}\n{text}"));
        assert!(name == &format!("{}.toml", s.name) && s.same_expansion && s.scope.write.len() == 1 && s.scope.write[0].starts_with("crate:"), "{text}");
    }
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
    assert!(readme.contains(&api::Depth::doc_table()), "README.md must carry Depth::doc_table()");
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

fn scope_of(dep: &str) -> custodian_kit::reading::scope::Unavailable {
    let ws = Workspace::open(&fixture()).unwrap();
    let j = custodian_kit::features::judge(ws_facts(&ws), &custodian_kit::features::Thresholds::parse(custodian_kit::features::SHIPPED_THRESHOLDS).unwrap());
    j.into_iter().find(|j| j.package == "layered" && j.dependency == dep).unwrap_or_else(|| panic!("layered/{dep} judged")).unavailable
}

fn ws_facts(ws: &Workspace) -> &custodian_kit::reading::facts::Facts {
    ws.facts()
}

#[test]
fn scope_depth_of_a_dependency_used_in_one_function() {
    use custodian_kit::api::Depth;
    let u = scope_of("helper_a");
    assert_eq!(u.depth, Some(Depth::Function));
    assert_eq!(u.direct, vec!["layered::uses_a"]);
    assert_eq!(u.items, vec!["layered::calls_uses_a", "layered::uses_a"]);
    assert!(u.units.iter().all(|x| x.depth == Depth::Function));
    assert!(u.files.iter().all(|p| !p.wholly), "lib.rs keeps plain and the rest");
    // the caller in another crate loses uses_a; the bystander loses nothing
    assert_eq!(u.callers_elsewhere.len(), 1);
    assert_eq!(u.callers_elsewhere[0].caller, "layered_user::user");
    assert_eq!(u.callers_elsewhere[0].loses, vec!["layered::uses_a"]);
    assert!(u.header().iter().any(|l| l.contains("layered_user::user (layered_user) loses layered::uses_a")), "{:?}", u.header());
}

#[test]
fn scope_depth_of_a_dependency_used_everywhere_in_a_module() {
    use custodian_kit::api::Depth;
    let u = scope_of("helper_b");
    assert_eq!(u.depth, Some(Depth::Module));
    assert_eq!(u.units.len(), 1, "{:?}", u.units);
    assert_eq!((u.units[0].name.as_str(), u.units[0].items), ("layered::wide", 2));
    assert!(u.modules.iter().any(|m| m.name == "layered::wide" && m.wholly));
    assert!(u.files.iter().all(|p| !p.wholly));
}

#[test]
fn scope_depth_of_a_dependency_used_in_a_whole_file() {
    use custodian_kit::api::Depth;
    let u = scope_of("helper_c");
    assert_eq!(u.depth, Some(Depth::File));
    assert_eq!(u.units.len(), 1, "{:?}", u.units);
    assert_eq!(u.units[0].name, "crates/layered/src/whole.rs");
    assert!(u.header().iter().any(|l| l.contains("wholly gated: crates/layered/src/whole.rs")));
}

#[test]
fn reach_of_a_crate_says_how_deep_it_sits_in_each_user() {
    let r = read("reach", "helper_c", "");
    let scope = rows(&r, "scope").into_iter().find(|x| x.crate_name.as_deref() == Some("layered")).expect("layered's scope");
    assert_eq!(scope.attributes.get("depth").map(String::as_str), Some("file"));
    assert_eq!(count(&r, "files wholly behind it"), 1);
    let r = read("reach", "layered::uses_a", "");
    assert_eq!(count(&r, "callers in other crates losing items"), 1);
    assert!(rows(&r, "loses").iter().any(|x| x.label == "layered_user::user"));
}

/// A reading that reads declarations and has none named refuses (sockets), or says it read none (reach, readers of
/// an env key): never zeros as if it had read them. The workspace's one default (`config/declared.toml`) is read
/// when a spec names none.
#[test]
fn a_reading_with_no_declaration_source_refuses_or_says_so_never_zeros() {
    let e = {
        let s = spec("sockets", "appd", "");
        let ws = Workspace::open_for(&fixture(), &s, Resolving::Free).unwrap();
        api::run(&ws, &s).unwrap_err()
    };
    assert_eq!(e.code, "CUST0039", "{e}");
    let r = read("reach", "crate:core_lib", "");
    assert!(r.counts().iter().all(|c| !c.what.starts_with("entries naming")), "no entries count without a source: {:?}", r.counts());
    assert!(r.report.not_read.iter().any(|n| n.contains("declarations were not read")), "{:?}", r.report.not_read);
    // the workspace's default source, in an initialized copy
    let dir = std::env::temp_dir().join(format!("custodian-kit-declared-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    copy_tree(&fixture(), &dir);
    let (store, _) = custodian_kit::store::Store::init(&dir, custodian_kit::store::Mode::Plain).unwrap();
    let decl = store.path(&custodian_kit::store::named::THRESHOLDS, &[("name", "declared")]).unwrap();
    std::fs::write(&decl, MARKET).unwrap();
    let s = spec("sockets", "appd", "");
    let ws = Workspace::open_for(&store.root, &s, Resolving::Free).unwrap();
    let r = api::run(&ws, &s).unwrap();
    assert!(r.counts().iter().find(|c| c.what == "entries running the binary").unwrap().n >= 1, "{:?}", r.counts());
}

#[test]
fn the_duplicates_summary_names_the_top_crate_pairs() {
    let r = read("duplicates", "*", "[duplicates]\nmin_tokens = 8\n");
    assert!(!r.report.headline.is_empty());
    assert!(r.report.headline[0].contains("copy_a ↔ copy_b") && r.report.headline[0].contains("shared bodies"), "{:?}", r.report.headline);
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_name() == "target" {
            continue;
        }
        if e.path().is_dir() {
            copy_tree(&e.path(), &to.join(e.file_name()));
        } else {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}
