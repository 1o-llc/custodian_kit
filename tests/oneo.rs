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

[requirements]
intent = "add hex to one_db_kernel"
change_spec = "specs/changes/add-hex.toml"
faces = ["prerequisites", "rust"]

[[requirements.needs]]
member = "one_db_kernel"
dependency = "hex"
version = "0.4"
features = ["serde", "alloc"]
path = "../hex"
optional = false
new_crate = "crates/hex"
said = "intent"
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
    requirements {
      intent "add hex to one_db_kernel"
      needs {
        member "one_db_kernel"
        dependency "hex"
        version "0.4"
        features "serde"
        features "alloc"
        path "../hex"
        optional false
        new_crate "crates/hex"
        said "intent"
      }
      change_spec "specs/changes/add-hex.toml"
      faces "prerequisites"
      faces "rust"
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
    // a single key written twice: the schema refuses it (x-1o-single-keys), no longer the Rust form
    let (c, m) = code(&base("kind \"reach\" kind \"use_sites\""));
    assert_eq!(c, "CUST0029");
    assert!(m.contains("Key 'kind' is written 2 times in 'reading'; it takes one value"), "{m}");
    // a repeating key checks each value it is written with
    let (_, m) = code(&base("kind \"reach\" scope { exclude \"a/**\" exclude 7 }"));
    assert!(m.contains("Expected string for 'exclude'"), "{m}");
    // a wrong type
    let (_, m) = code(&base("kind \"reach\" scope { tests \"yes\" }"));
    assert!(m.contains("Expected boolean"), "{m}");
}

fn enum_of<'a>(schema: &'a Value, path: &[&str]) -> Vec<&'a str> {
    let mut v = &schema["properties"]["reading"];
    for p in path {
        v = &v["properties"][*p];
        // a repeating key's schema is its items'
        if v["type"] == "array" {
            v = &v["items"];
        }
    }
    v["enum"].as_array().unwrap_or_else(|| panic!("{path:?} has no enum")).iter().map(|x| x.as_str().unwrap()).collect()
}

#[test]
fn the_reading_schema_carries_the_vocabularies() {
    let s = Doc::Reading.schema();
    assert_eq!(enum_of(&s, &["kind"]), ReadingKind::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>());
    assert_eq!(enum_of(&s, &["declared_in", "role"]), DeclaredRole::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>());
    assert_eq!(enum_of(&s, &["duplicates", "level"]), klepto::SHAPE_LEVELS.iter().map(|l| l.name()).collect::<Vec<_>>());
    let faces = enum_of(&s, &["requirements", "faces"]);
    assert_eq!(faces, custodian_kit::reading::requirements::FacesWord::all_names());
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
    // a repeating key's keys are its items'
    if let Some(items) = v.get("items") {
        return schema_keys(items, at, out);
    }
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

    let job = custodian_kit::jobs::JobFile { id: Some("1".into()), name: Some("n".into()), kind: Some("specs".into()), target: Some("t".into()), origin: Some("o".into()), count: Some(true), routes: vec!["*".into()], task_id: Some("t".into()), revision: Some("base".into()), mode: Some(custodian_kit::jobs::JobMode::Bounded), verify: vec!["test:x".into()], target_known: Some(true), outcome_known: Some(true), base_commit: Some("c".into()), baseline: Some(true), allow_file: vec!["tests/ui/*.stderr".into()], budget: Some("hours=4,battery=20".into()) };
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

    // the change spec and the linkage record, every field set once
    let piece = |name: &str| {
        let mut v = serde_json::Map::new();
        v.insert("piece".into(), name.into());
        for s in custodian_kit::change::SLOTS {
            v.insert(s.to_string(), if ["conditions", "errors", "candidates", "fields", "flags", "args"].contains(s) { serde_json::json!(["x"]) } else { serde_json::json!("x") });
        }
        serde_json::Value::Object(v)
    };
    let f = serde_json::json!({"name": "f", "vis": "pub", "receiver": "&self", "params": [{"name": "a", "ty": "u8"}], "returns": "u8", "generics": "<T>", "where": "T: Clone", "async": true, "const": true, "unsafe": true, "attrs": ["#[inline]"], "doc": "d", "body": [piece("ensure")], "generator": piece("getter")});
    let field = serde_json::json!({"name": "n", "ty": "u8", "vis": "pub", "doc": "d", "attrs": ["#[x]"], "default": "0"});
    let variant = serde_json::json!({"name": "V", "doc": "d", "attrs": ["#[x]"], "tuple": ["u8"], "fields": [field.clone()], "discriminant": "1"});
    let change = serde_json::json!({
        "name": "c", "objective": "o", "operation": "make_struct", "target": "crate:x",
        "item": {"name": "S", "vis": "pub", "doc": "d", "generics": "<T>", "derives": ["Debug"], "attrs": ["#[x]"], "fields": [field.clone()], "tuple": ["u8"], "variants": [variant.clone()], "generator": piece("shell_command_builder")},
        "function": f.clone(), "functions": [f.clone()], "field": field.clone(), "variant": variant.clone(), "derive": ["Clone"], "attribute": "#[x]", "trait": "Display", "assoc": ["type O = u8;"],
        "generator": piece("impl_std_error"), "module": {"name": "m", "vis": "pub", "doc": "d"}, "dependency": {"name": "d", "version": "1", "path": "p", "features": ["f"], "optional": true},
        "dependencies": [{"name": "d", "version": "1", "path": "p", "features": ["f"], "optional": false}],
        "new_name": "n", "to": "crate:x/src/y.rs", "items": ["x::a"], "feature": "f", "enables": ["dep:d"], "linkage": "l", "call_sites": {"pass": "0", "allow_unresolved": true}, "compatibility": "preserve",
        "scope": {"read": ["crate:x"], "write": ["crate:x"]}, "features": ["default"], "validate": ["check"], "postconditions": ["exists x::S"], "decisions": [{"question": "q", "options": ["a"], "why": "w"}], "same_expansion": true
    });
    let spec: custodian_kit::change::ChangeSpec = serde_json::from_value(change).unwrap();
    let mut have = Vec::new();
    keys(&serde_json::to_value(&spec).unwrap(), "", &mut have);
    let mut declared = Vec::new();
    schema_keys(&Doc::Change.schema()["properties"]["change"], "", &mut declared);
    have.sort();
    have.dedup();
    declared.sort();
    declared.dedup();
    assert_eq!(have, declared);
    let rec = custodian_kit::linkage::LinkageRecord { name: "l".into(), kind: custodian_kit::linkage::LinkKind::Invariant, why: "w".into(), binds: vec!["a".into()], author: Some("a".into()), recorded_utc: Some("t".into()), evidence: vec!["e".into()] };
    let mut have = Vec::new();
    keys(&serde_json::to_value(&rec).unwrap(), "", &mut have);
    let mut declared = Vec::new();
    schema_keys(&Doc::Linkage.schema()["properties"]["linkage"], "", &mut declared);
    have.sort();
    declared.sort();
    assert_eq!(have, declared);
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

/// CUST0030 was a string holding a double quote; core-1o 0.2 reads escapes, so it is written as one and reads
/// back. The record is written at the top level, without the outer `{ }`.
#[test]
fn a_double_quote_is_written_as_an_escape_and_reads_back() {
    let mut s = ReadingSpec::from_toml(TOML).unwrap();
    s.question = "Who reads \"NODEPLAN_*\"?\nC:\\kit\tand a tab".into();
    let text = s.to_1o().unwrap();
    assert!(text.starts_with("reading {\n"), "{text}");
    assert!(text.contains(r#"question "Who reads \"NODEPLAN_*\"?\nC:\\kit\tand a tab""#), "{text}");
    assert_eq!(ReadingSpec::from_1o(&text).unwrap(), s);
    // the older wrapped form reads the same
    assert_eq!(ReadingSpec::from_1o(&format!("{{\n{text}}}\n")).unwrap(), s);
    // a negative number is still CUST0030: core-1o reads none
    #[derive(serde::Serialize)]
    struct Neg {
        n: i64,
    }
    match oneo::render(Doc::Reading, &Neg { n: -1 }) {
        Err(custodian_kit::CustodianError::Reading { code, .. }) => assert_eq!(code, "CUST0030"),
        other => panic!("{other:?}"),
    }
}

/// Each vocabulary in a schema carries its words' descriptions (`enumDescriptions`, what the language server
/// shows), and the change spec's operations the keys each needs (`x-1o-required-when`), as the Rust says them.
#[test]
fn the_schemas_carry_the_vocabularies_descriptions_and_each_operation_s_keys() {
    use custodian_kit::change::{Compatibility, Operation};
    fn said(v: &Value) -> (Vec<&str>, Vec<&str>) {
        let words = v["enum"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        let docs = v["enumDescriptions"].as_array().unwrap_or_else(|| panic!("no enumDescriptions: {v}")).iter().map(|x| x.as_str().unwrap()).collect();
        (words, docs)
    }
    macro_rules! same {
        ($v:expr, $vocab:ty) => {
            assert_eq!(said($v), (<$vocab>::ALL.iter().map(|k| k.as_str()).collect(), <$vocab>::ALL.iter().map(|k| k.doc()).collect()), stringify!($vocab))
        };
    }
    let reading = Doc::Reading.schema();
    same!(&reading["properties"]["reading"]["properties"]["kind"], ReadingKind);
    same!(&reading["properties"]["reading"]["properties"]["declared_in"]["items"]["properties"]["role"], DeclaredRole);
    same!(&Doc::Job.schema()["properties"]["job"]["properties"]["mode"], custodian_kit::jobs::JobMode);
    same!(&Doc::Linkage.schema()["properties"]["linkage"]["properties"]["kind"], custodian_kit::linkage::LinkKind);
    let change = Doc::Change.schema();
    same!(&change["properties"]["change"]["properties"]["operation"], Operation);
    same!(&change["properties"]["change"]["properties"]["compatibility"], Compatibility);
    // the keys an operation needs: needs() where one key says it (functions, trait and dependency each have another
    // way to be given, so they stay the Rust check's)
    let when = &change["properties"]["change"]["x-1o-required-when"]["operation"];
    for op in Operation::ALL {
        let want: Vec<&str> = op.needs().iter().copied().filter(|k| !["functions", "trait", "dependency"].contains(k)).collect();
        let have: Vec<&str> = when.get(op.as_str()).and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_str().unwrap()).collect()).unwrap_or_default();
        assert_eq!(have, want, "{}", op.as_str());
    }
    let e = custodian_kit::change::ChangeSpec::from_1o("change { name \"c\" objective \"o\" operation \"rename\" target \"x::a\" }").unwrap_err();
    assert!(e.to_string().contains("Missing required key 'new_name' for operation 'rename'"), "{e}");
    // every block says its single keys
    fn single(v: &Value, at: &str) {
        if v.get("properties").is_some() {
            assert_eq!(v["x-1o-single-keys"], true, "{at}");
            for (k, x) in v["properties"].as_object().unwrap() {
                single(x, &format!("{at}.{k}"));
            }
        }
        if let Some(i) = v.get("items") {
            single(i, &format!("{at}[]"));
        }
    }
    for d in Doc::ALL {
        single(&d.schema(), d.modifier());
    }
}

/// Every document under `CUSTODIAN_GOLDEN_CORPUS` (a copy of a project's `.custodian`) reads into its form: the
/// 1o documents, and the reading specs, change specs and job files still in TOML; and each, written as the 1o
/// every writer now writes, reads back as the same form.
#[test]
fn every_document_of_a_corpus_reads_and_its_1o_reads_back() {
    use custodian_kit::change::ChangeSpec;
    let Ok(root) = std::env::var("CUSTODIAN_GOLDEN_CORPUS") else { return };
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let at = p.display().to_string();
            let kind = match Doc::of_file(&p) {
                Some((Doc::Reading, _)) => {
                    let s = ReadingSpec::from_1o(&text).unwrap_or_else(|e| panic!("{at}: {e}"));
                    assert_eq!(ReadingSpec::from_1o(&s.to_1o().unwrap()).unwrap(), s, "{at}");
                    "1o reading"
                }
                Some((Doc::Job, _)) => {
                    let j: custodian_kit::jobs::JobFile = oneo::parse(Doc::Job, &at, &text).unwrap_or_else(|e| panic!("{at}: {e}"));
                    let back: custodian_kit::jobs::JobFile = oneo::parse(Doc::Job, &at, &oneo::render(Doc::Job, &j).unwrap()).unwrap();
                    assert_eq!(serde_json::to_value(&back).unwrap(), serde_json::to_value(&j).unwrap(), "{at}");
                    "1o job"
                }
                Some((Doc::Paths, _)) => {
                    let t: custodian_kit::paths::PathsFile = oneo::parse(Doc::Paths, &at, &text).unwrap_or_else(|e| panic!("{at}: {e}"));
                    assert_eq!(oneo::parse::<custodian_kit::paths::PathsFile>(Doc::Paths, &at, &oneo::render(Doc::Paths, &t).unwrap()).unwrap(), t, "{at}");
                    "1o paths"
                }
                Some((Doc::Change, _)) => {
                    let c = ChangeSpec::from_1o(&text).unwrap_or_else(|e| panic!("{at}: {e}"));
                    assert_eq!(ChangeSpec::from_1o(&c.to_1o().unwrap()).unwrap(), c, "{at}");
                    "1o change"
                }
                Some((Doc::Linkage, _)) => "1o linkage",
                None if name.ends_with(".toml") => {
                    if let Ok(c) = ChangeSpec::from_toml(&text) {
                        assert_eq!(ChangeSpec::from_1o(&c.to_1o().unwrap()).unwrap_or_else(|e| panic!("{at}: {e}")), c, "{at}");
                        "toml change spec"
                    } else if let Ok(s) = ReadingSpec::from_toml(&text) {
                        assert_eq!(ReadingSpec::from_1o(&s.to_1o().unwrap()).unwrap_or_else(|e| panic!("{at}: {e}")), s, "{at}");
                        "toml reading spec"
                    } else {
                        "toml, another document"
                    }
                }
                None => continue,
            };
            *counts.entry(kind.to_string()).or_default() += 1;
        }
    }
    eprintln!("{counts:?}");
    assert!(!counts.is_empty());
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
    let diff: Value = serde_json::from_str(task_history::DIFF_SCHEMA_JSON).unwrap();
    assert_eq!(ws.schema_for_file("diff", "v1"), Some(diff), "the task-history records' schema is found too");
    // and validates a spec live, as an editor would see it
    let bad = core_1o::engine::diagnostics_for_text_workspace(&ws, "reading", "v1", "{ reading { name \"m\" question \"q\" kind \"nope\" target \"x\" } }");
    assert!(bad.iter().any(|d| d.message.contains("allowed enum values")), "{bad:?}");
    let good = core_1o::engine::diagnostics_for_text_workspace(&ws, "reading", "v1", ONEO);
    assert!(good.is_empty(), "{good:?}");
    // the paths table init wrote reads back
    let table = custodian_kit::paths::PathsTable::from_packages(&store.root, &Default::default(), Some(&store)).unwrap();
    assert!(table.file.decl.is_empty());
}
