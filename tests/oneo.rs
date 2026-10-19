//! The 1o forms: one internal form for a spec written in TOML or 1o, the
//! shipped schemas agree with the Rust forms and the vocabularies, and
//! core-1o's validator refuses what the schema refuses.

use custodian_kit::api::{DeclaredRole, ReadingKind, ReadingSpec};
use custodian_kit::oneo::{self, Doc};
use serde_json::Value;

const TOML: &str = r#"
name = "fleet-env"
question = "Who reads each NODEPLAN key, and where is it declared?"
kind = "readers_of_env"
target = "NODEPLAN_*"

[scope]
exclude = ["crates/old/**", "attempts/**"]
tests = true

[[declared_in]]
files = ["decl:marketplace"]
key = "env.*"
role = "env"
entry = "id"

[[declared_in]]
files = ["decl:slices", "decl:deploys"]
key = "binary"
role = "binary"

[readers]
fns = ["std::env::var", "nodeplan_sdk::env_or"]
follow = false

[duplicates]
level = "renamed"
min_tokens = 24

[change]
files = ["crates/a/src/lib.rs"]
git_ref = "main"

[bundle]
filters = ["no_tests", "only_src"]
no_features = ["metrics"]
"#;

const ONEO: &str = r#"{
  reading {
    name "fleet-env"
    question "Who reads each NODEPLAN key, and where is it declared?"
    kind "readers_of_env"
    target "NODEPLAN_*"
    scope {
      exclude "crates/old/**"
      exclude "attempts/**"
      tests true
    }
    declared_in {
      files "decl:marketplace"
      key "env.*"
      role "env"
      entry "id"
    }
    declared_in {
      files "decl:slices"
      files "decl:deploys"
      key "binary"
      role "binary"
    }
    readers {
      fns "std::env::var"
      fns "nodeplan_sdk::env_or"
      follow false
    }
    duplicates {
      level "renamed"
      min_tokens 24
    }
    change {
      files "crates/a/src/lib.rs"
      git_ref "main"
    }
    bundle {
      filters "no_tests"
      filters "only_src"
      no_features "metrics"
    }
  }
}
"#;

#[test]
fn a_toml_spec_and_its_1o_twin_are_one_spec_with_one_digest() {
    let t = ReadingSpec::from_toml(TOML).unwrap();
    let o = ReadingSpec::from_1o(ONEO).unwrap();
    assert_eq!(t, o);
    assert_eq!(custodian_kit::runs::spec_hash(&t), custodian_kit::runs::spec_hash(&o));
    // the digest is over the form, not the text: spacing changes nothing
    let squeezed = ONEO.split_whitespace().collect::<Vec<_>>().join(" ").replace("\" ", "\"\n");
    assert_eq!(custodian_kit::runs::spec_hash(&ReadingSpec::from_1o(&squeezed).unwrap()), custodian_kit::runs::spec_hash(&t));
}

#[test]
fn a_spec_written_in_1o_reads_back_the_same() {
    let t = ReadingSpec::from_toml(TOML).unwrap();
    let text = t.to_1o().unwrap();
    assert_eq!(ReadingSpec::from_1o(&text).unwrap(), t, "{text}");
}

#[test]
fn a_minimal_1o_spec_takes_the_defaults() {
    let o = ReadingSpec::from_1o("{ reading { name \"m\" question \"q\" kind \"use_sites\" target \"serve_metrics\" } }").unwrap();
    let t = ReadingSpec::from_toml("name = \"m\"\nquestion = \"q\"\nkind = \"use_sites\"\ntarget = \"serve_metrics\"\n").unwrap();
    assert_eq!(o, t);
}

#[test]
fn the_schema_refuses_what_the_spec_refuses() {
    let base = |body: &str| format!("{{ reading {{ name \"m\" question \"q\" target \"x\" {body} }} }}");
    let code = |text: &str| match ReadingSpec::from_1o(text) {
        Err(custodian_kit::CustodianError::Reading { code, message }) => (code, message),
        other => panic!("not refused: {other:?}"),
    };
    // a kind not in the vocabulary: core-1o's enum check
    let (c, m) = code(&base("kind \"nope\""));
    assert_eq!(c, "CUST0029");
    assert!(m.contains("allowed enum values"), "{m}");
    // an unknown key: core-1o's schema check
    let (_, m) = code(&base("kind \"reach\" mystery \"1\""));
    assert!(m.contains("Unknown key 'mystery'"), "{m}");
    // a missing required key
    let (_, m) = code("{ reading { name \"m\" question \"q\" kind \"reach\" } }");
    assert!(m.contains("Missing required key 'target'"), "{m}");
    // a single key written twice: the Rust form refuses it
    let (_, m) = code(&base("kind \"reach\" kind \"use_sites\""));
    assert!(m.contains("`kind` is written 2 times"), "{m}");
    // a wrong type
    let (_, m) = code(&base("kind \"reach\" scope { tests \"yes\" }"));
    assert!(m.contains("Expected boolean"), "{m}");
}

fn enum_of<'a>(schema: &'a Value, path: &[&str]) -> Vec<&'a str> {
    let mut v = &schema["properties"]["reading"];
    for p in path {
        v = &v["properties"][*p];
    }
    v["enum"].as_array().unwrap_or_else(|| panic!("{path:?} has no enum")).iter().map(|x| x.as_str().unwrap()).collect()
}

#[test]
fn the_reading_schema_carries_the_vocabularies() {
    let s = Doc::Reading.schema();
    assert_eq!(enum_of(&s, &["kind"]), ReadingKind::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>());
    assert_eq!(enum_of(&s, &["declared_in", "role"]), DeclaredRole::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>());
    assert_eq!(enum_of(&s, &["duplicates", "level"]), klepto::SHAPE_LEVELS.iter().map(|l| l.name()).collect::<Vec<_>>());
    let filters = enum_of(&s, &["bundle", "filters"]);
    custodian_kit::bundle::FiltersWord::from_names(filters.iter().copied()).expect("every schema filter is a filter of the word");
}

/// Every key of a serialized form, nested, as `a.b` paths.
fn keys(v: &Value, at: &str, out: &mut Vec<String>) {
    if let Value::Object(m) = v {
        for (k, x) in m {
            let p = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
            out.push(p.clone());
            match x {
                Value::Array(items) => items.iter().for_each(|i| keys(i, &p, out)),
                other => keys(other, &p, out),
            }
        }
    }
}

fn schema_keys(v: &Value, at: &str, out: &mut Vec<String>) {
    if let Some(Value::Object(m)) = v.get("properties") {
        for (k, x) in m {
            let p = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
            out.push(p.clone());
            schema_keys(x, &p, out);
        }
    }
}

#[test]
fn the_schemas_name_exactly_the_fields_of_the_rust_forms() {
    let mut have = Vec::new();
    keys(&serde_json::to_value(ReadingSpec::from_toml(TOML).unwrap()).unwrap(), "", &mut have);
    let mut declared = Vec::new();
    schema_keys(&Doc::Reading.schema()["properties"]["reading"], "", &mut declared);
    have.sort();
    have.dedup();
    declared.sort();
    assert_eq!(have, declared);

    let job = custodian_kit::jobs::JobFile { id: Some("1".into()), name: Some("n".into()), kind: Some("specs".into()), target: Some("t".into()), origin: Some("o".into()), count: Some(true), routes: vec!["*".into()] };
    let mut have = Vec::new();
    keys(&serde_json::to_value(&job).unwrap(), "", &mut have);
    let mut declared = Vec::new();
    schema_keys(&Doc::Job.schema()["properties"]["job"], "", &mut declared);
    have.sort();
    declared.sort();
    assert_eq!(have, declared);

    let mut declared = Vec::new();
    schema_keys(&Doc::Paths.schema()["properties"]["paths"], "", &mut declared);
    declared.sort();
    assert_eq!(declared, vec!["decl", "ignore", "path"]);
}

#[test]
fn the_job_file_and_the_paths_table_read_from_1o() {
    let j: custodian_kit::jobs::JobFile = oneo::parse(Doc::Job, "job", "{ job { name \"first-reading\" kind \"specs\" } }").unwrap();
    assert_eq!(j.name.as_deref(), Some("first-reading"));
    let p: custodian_kit::paths::PathsFile = oneo::parse(Doc::Paths, "paths", "{ paths { decl { marketplace \"marketplace/*.toml\" slices \"slices/*.toml\" slices \"slices/extra/*.toml\" } path \"docs\" } }").unwrap();
    assert_eq!(p.decl["marketplace"], vec!["marketplace/*.toml"]);
    assert_eq!(p.decl["slices"], vec!["slices/*.toml", "slices/extra/*.toml"]);
    assert_eq!(p.path, vec!["docs"]);
    let back: custodian_kit::paths::PathsFile = oneo::parse(Doc::Paths, "paths", &oneo::render(Doc::Paths, &p).unwrap()).unwrap();
    assert_eq!(back, p);
}

#[test]
fn a_double_quote_cannot_be_written_and_says_so() {
    let mut s = ReadingSpec::from_toml(TOML).unwrap();
    s.question = "Who reads \"NODEPLAN_*\"?".into();
    match s.to_1o() {
        Err(custodian_kit::CustodianError::Reading { code, .. }) => assert_eq!(code, "CUST0030"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn file_names_are_the_1o_convention() {
    assert_eq!(Doc::Reading.file_name("serve-metrics-use-sites"), "v1.serve-metrics-use-sites.1o.reading");
    let (d, n) = Doc::of_file(std::path::Path::new("x/v1.serve-metrics-use-sites.1o.reading")).unwrap();
    assert_eq!((d, n.as_str()), (Doc::Reading, "serve-metrics-use-sites"));
    assert_eq!(custodian_kit::reading::spec::SpecForm::stem(std::path::Path::new("v1.a.1o.reading")).as_deref(), Some("a"));
    assert_eq!(custodian_kit::reading::spec::SpecForm::stem(std::path::Path::new("a.toml")).as_deref(), Some("a"));
}

#[test]
fn init_puts_the_schemas_where_the_language_server_looks() {
    let root = std::env::temp_dir().join(format!("custodian-kit-oneo-init-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // a project manifest of its own is kept, the folder added to it
    std::fs::write(root.join("oneo.project.json"), r#"{"formatVersion":1,"schemaDirectories":["schemas"],"name":"kept"}"#).unwrap();
    let (store, made) = custodian_kit::store::Store::init(&root, custodian_kit::store::Mode::Agent).unwrap();
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(store.root.join("oneo.project.json")).unwrap()).unwrap();
    assert_eq!(manifest["name"], "kept");
    let dirs: Vec<&str> = manifest["schemaDirectories"].as_array().unwrap().iter().map(|d| d.as_str().unwrap()).collect();
    assert_eq!(dirs, vec!["schemas", ".custodian/config/schema"]);
    assert!(made.iter().any(|p| p.ends_with("config/v1.paths.1o.paths")));
    // core-1o's workspace (what language-server-1o opens) finds custodian's schemas
    let ws = core_1o::engine::Workspace::new(store.root.clone());
    for d in Doc::ALL {
        assert_eq!(ws.schema_for_file(d.modifier(), "v1"), Some(d.schema()), "{}", d.modifier());
    }
    // and validates a spec live, as an editor would see it
    let bad = core_1o::engine::diagnostics_for_text_workspace(&ws, "reading", "v1", "{ reading { name \"m\" question \"q\" kind \"nope\" target \"x\" } }");
    assert!(bad.iter().any(|d| d.message.contains("allowed enum values")), "{bad:?}");
    let good = core_1o::engine::diagnostics_for_text_workspace(&ws, "reading", "v1", ONEO);
    assert!(good.is_empty(), "{good:?}");
    // the paths table init wrote reads back
    let table = custodian_kit::paths::PathsTable::from_packages(&store.root, &Default::default(), Some(&store)).unwrap();
    assert!(table.file.decl.is_empty());
}
