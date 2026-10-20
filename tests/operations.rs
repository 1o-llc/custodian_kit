// Copyright © 2026 1o. All rights reserved.

//! The preflight of `inline` and `wrap`, and of an operation on an item an attribute macro rewrites, on a workspace
//! made here: `marks` (a proc-macro crate with the attribute `keep`) and `shop` (a struct under `#[keep]`, a private
//! function its callers can take in, one that returns early, one to wrap).

use custodian_kit::api::{Resolving, Scope, Workspace};
use custodian_kit::preflight;
use custodian_kit::runs::{Ctx, Runner};
use custodian_kit::store::{Mode, Store};
use std::path::{Path, PathBuf};

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn project(tag: &str) -> (Store, Ctx, Workspace) {
    let root = std::env::temp_dir().join(format!("custodian-kit-ops-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    write(&root, "Cargo.toml", "[workspace]\nresolver = \"2\"\nmembers = [\"marks\", \"shop\", \"app\"]\n");
    write(&root, "app/Cargo.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nshop = { path = \"../shop\" }\n");
    write(&root, "app/src/lib.rs", "pub fn shop_version() -> u32 {\n    shop::version()\n}\n");
    write(&root, "marks/Cargo.toml", "[package]\nname = \"marks\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\nproc-macro = true\n");
    write(&root, "marks/src/lib.rs", "use proc_macro::TokenStream;\n\n#[proc_macro_attribute]\npub fn keep(_a: TokenStream, item: TokenStream) -> TokenStream {\n    item\n}\n");
    write(&root, "shop/Cargo.toml", "[package]\nname = \"shop\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmarks = { path = \"../marks\" }\n");
    write(
        &root,
        "shop/src/lib.rs",
        "use marks::keep;\n\n#[keep]\npub struct Packed {\n    pub bits: u8,\n}\n\nfn double(n: u32) -> u32 {\n    n * 2\n}\n\npub fn twice(o: u32) -> u32 {\n    double(o) + double(1)\n}\n\nfn early(n: u32) -> u32 {\n    if n > 3 {\n        return 3;\n    }\n    n\n}\n\npub fn capped() -> u32 {\n    early(9)\n}\n\npub fn version() -> u32 {\n    1\n}\n\nimpl Packed {\n    pub fn zero() -> Self {\n        Self { bits: 0 }\n    }\n}\n",
    );
    let (store, _) = Store::init(&root, Mode::Agent).unwrap();
    let ctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: PathBuf::from("custodian") };
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    (store, ctx, w)
}

fn spec(root: &Path, name: &str, text: &str) -> PathBuf {
    let p = root.join(".custodian/data/specs/changes").join(format!("{name}.toml"));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p
}

#[test]
fn a_struct_an_attribute_macro_rewrites_is_refused_by_the_macros_name() {
    let (store, ctx, w) = project("macro");
    assert_eq!(custodian_kit::change::attribute_macros(w.klepto(), "shop::Packed"), ["#[keep] (marks::keep)"]);
    for (name, text) in [
        ("wide", "name = \"wide\"\nobjective = \"o\"\noperation = \"add_field\"\ntarget = \"shop::Packed\"\n\n[field]\nname = \"more\"\nty = \"u8\"\ndefault = \"0\"\n"),
        ("ctor", "name = \"ctor\"\nobjective = \"o\"\noperation = \"add_constructor\"\ntarget = \"shop::Packed\"\n\n[function]\nname = \"of_bits\"\nvis = \"pub\"\nparams = [{ name = \"bits\", ty = \"u8\" }]\n"),
    ] {
        let p = preflight::run_preflight(&ctx, &spec(&store.root, name, text), None, Runner::InProcess(&w)).unwrap().preflight;
        assert!(!p.authorized, "{name}");
        let c = p.preconditions.iter().find(|c| c.what == "no attribute macro rewrites the item").unwrap_or_else(|| panic!("{:?}", p.preconditions));
        assert!(!c.holds && c.detail.contains("[CUST0040]") && c.detail.contains("#[keep] (marks::keep)"), "{}", c.detail);
    }
    // a method is not written into the fields: authorized
    let m = preflight::run_preflight(&ctx, &spec(&store.root, "m", "name = \"m\"\nobjective = \"o\"\noperation = \"add_method\"\ntarget = \"shop::Packed\"\n\n[function]\nname = \"bits\"\nreceiver = \"&self\"\nreturns = \"u8\"\n\n[[function.body]]\npiece = \"value\"\nvalue = \"self.bits\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(m.authorized, "{:?}", m.reasons);
}

#[test]
fn inline_is_authorized_for_a_function_whose_body_can_sit_in_its_callers() {
    let (store, ctx, w) = project("inline");
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "in", "name = \"in\"\nobjective = \"o\"\noperation = \"inline\"\ntarget = \"shop::double\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?} {:?}", p.reasons, p.preconditions);
    assert_eq!(p.write_set, ["shop/src/lib.rs"]);
    assert!(p.listed.iter().any(|l| l == "shop::double is removed: not pub, and every use of it is a call"), "{:?}", p.listed);
    let e = preflight::run_preflight(&ctx, &spec(&store.root, "e", "name = \"e\"\nobjective = \"o\"\noperation = \"inline\"\ntarget = \"shop::early\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!e.authorized);
    assert!(e.preconditions.iter().any(|c| c.what == "the function can be inlined" && !c.holds && c.detail.contains("returns early")), "{:?}", e.preconditions);
    assert!(custodian_kit::change::inline_blockers(w.klepto(), "shop::Packed").iter().any(|b| b.contains("not a function")));
    // a call in another crate: the body's names do not resolve there; `Self` in the body is the caller's
    assert!(custodian_kit::change::inline_blockers(w.klepto(), "shop::version").iter().any(|b| b.contains("called from another file")), "{:?}", custodian_kit::change::inline_blockers(w.klepto(), "shop::version"));
    assert!(custodian_kit::change::inline_blockers(w.klepto(), "shop::Packed::zero").iter().any(|b| b.contains("names `Self`")), "{:?}", custodian_kit::change::inline_blockers(w.klepto(), "shop::Packed::zero"));
}

#[test]
fn wrap_is_authorized_when_its_pieces_run_first_and_refused_when_one_gives_the_value() {
    let (store, ctx, w) = project("wrap");
    let ok = preflight::run_preflight(&ctx, &spec(&store.root, "w", "name = \"w\"\nobjective = \"o\"\noperation = \"wrap\"\ntarget = \"shop::version\"\n\n[function]\nname = \"version\"\n\n[[function.body]]\npiece = \"let\"\nbind = \"_seen\"\nvalue = \"1u8\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(ok.authorized, "{:?} {:?}", ok.reasons, ok.preconditions);
    let no = preflight::run_preflight(&ctx, &spec(&store.root, "w2", "name = \"w2\"\nobjective = \"o\"\noperation = \"wrap\"\ntarget = \"shop::version\"\n\n[function]\nname = \"version\"\n\n[[function.body]]\npiece = \"value\"\nvalue = \"2\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!no.authorized && no.preconditions.iter().any(|c| !c.holds && c.detail.contains("a wrapped body's value is the body it wraps")), "{:?}", no.preconditions);
    // the rendering: the pieces, then the body as it was
    let f = custodian_kit::change::ChangeSpec::from_path(&store.root.join(".custodian/data/specs/changes/w.toml")).unwrap().function.unwrap();
    assert_eq!(custodian_kit::change::render_wrapped(&f, "", "{\n    1\n}").unwrap(), "{\n    let _seen = 1u8;\n    {\n    1\n}\n}");
    assert_eq!(custodian_kit::change::render_inlined(&[("n".into(), "u32".into())], &["o".into()], "{\n    n * 2\n}").unwrap(), "{ let n: u32 = o; n * 2 }");
}

#[test]
fn declare_feature_reads_its_list_and_makes_a_dep_entrys_dependency_optional() {
    let (store, ctx, w) = project("feature");
    // `dep:marks` on a required dependency: authorized, and the dependency becomes optional in the same step
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "f", "name = \"f\"\nobjective = \"o\"\noperation = \"declare_feature\"\ntarget = \"crate:shop\"\nfeature = \"marks\"\nenables = [\"dep:marks\"]\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?} {:?}", p.reasons, p.preconditions);
    assert_eq!(p.write_set, ["shop/Cargo.toml"]);
    assert!(p.listed.iter().any(|l| l.starts_with("marks becomes optional")), "{:?}", p.listed);
    let s = custodian_kit::change::ChangeSpec::from_path(&store.root.join(".custodian/data/specs/changes/f.toml")).unwrap();
    let d = custodian_kit::change::feature_declaration(w.klepto(), &store.root, &s).unwrap();
    assert_eq!((d.made_optional.clone(), d.was.clone(), d.problems.is_empty()), (vec!["marks".to_string()], None, true));
    // refused: a dependency the crate does not have, a feature it does not declare, a name cargo does not take
    for (name, enables, why) in [("json", "[\"dep:serde\"]", "has no dependency serde"), ("both", "[\"json\"]", "declares no feature json"), ("no way", "[]", "not a feature name")] {
        let text = format!("name = \"x\"\nobjective = \"o\"\noperation = \"declare_feature\"\ntarget = \"crate:shop\"\nfeature = \"{name}\"\nenables = {enables}\n");
        let p = preflight::run_preflight(&ctx, &spec(&store.root, "x", &text), None, Runner::InProcess(&w)).unwrap().preflight;
        assert!(!p.authorized && p.preconditions.iter().any(|c| !c.holds && c.detail.contains(why)), "{name}: {:?}", p.preconditions);
    }
}

#[test]
fn make_lib_is_for_a_crate_with_only_a_binary_and_a_module_named_lib_is_refused() {
    let (store, ctx, _) = project("lib");
    write(&store.root, "Cargo.toml", "[workspace]\nresolver = \"2\"\nmembers = [\"marks\", \"shop\", \"app\", \"tool\"]\n");
    write(&store.root, "tool/Cargo.toml", "[package]\nname = \"tool\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    write(&store.root, "tool/src/main.rs", "fn main() {}\n");
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let ok = preflight::run_preflight(&ctx, &spec(&store.root, "l", "name = \"l\"\nobjective = \"o\"\noperation = \"make_lib\"\ntarget = \"crate:tool\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(ok.authorized, "{:?} {:?}", ok.reasons, ok.preconditions);
    assert_eq!(ok.write_set, ["tool/src/lib.rs"]);
    let has = preflight::run_preflight(&ctx, &spec(&store.root, "l2", "name = \"l2\"\nobjective = \"o\"\noperation = \"make_lib\"\ntarget = \"crate:shop\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!has.authorized && has.preconditions.iter().any(|c| !c.holds && c.detail.starts_with("it has one")), "{:?}", has.preconditions);
    // the pantry's 00-lib: a module named lib is no library
    let m = preflight::run_preflight(&ctx, &spec(&store.root, "m", "name = \"m\"\nobjective = \"o\"\noperation = \"add_module\"\ntarget = \"crate:tool\"\n\n[module]\nname = \"lib\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!m.authorized && m.preconditions.iter().any(|c| !c.holds && c.detail.contains("make_lib")), "{:?}", m.preconditions);
}

#[test]
fn a_target_unseen_while_the_lock_file_is_behind_the_manifests_says_why() {
    let (store, ctx, _) = project("lock");
    let ok = std::process::Command::new("cargo").args(["generate-lockfile", "--offline"]).current_dir(&store.root).output().unwrap();
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
    // a dependency the manifest names and the lock does not: the frozen scan reads no dependency macro
    write(&store.root, "app/Cargo.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nshop = { path = \"../shop\" }\nmarks = { path = \"../marks\" }\n");
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    assert!(w.klepto().dep_macro_note.is_some());
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "t", "name = \"t\"\nobjective = \"o\"\noperation = \"add_test\"\ntarget = \"shop::made_by_a_generator\"\n\n[function]\nname = \"t\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized);
    assert!(p.preconditions.iter().any(|c| !c.holds && c.detail.contains("dependency macros not harvested") && c.detail.contains("lock file")), "{:?}", p.preconditions);
}

#[test]
fn change_signature_with_a_body_says_both_are_proved_together() {
    let (store, ctx, w) = project("sig");
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "s", "name = \"s\"\nobjective = \"o\"\noperation = \"change_signature\"\ntarget = \"shop::version\"\n\n[function]\nname = \"version\"\nvis = \"pub\"\nreturns = \"u64\"\nbody = [{ piece = \"value\", value = \"1u64\" }]\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?} {:?}", p.reasons, p.preconditions);
    assert!(p.listed.iter().any(|l| l == "its body is written with the signature: 1 pieces, proved together"), "{:?}", p.listed);
}

#[test]
fn add_test_into_a_module_the_file_does_not_have_says_it_is_made() {
    let (store, ctx, w) = project("tomod");
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "t", "name = \"t\"\nobjective = \"o\"\noperation = \"add_test\"\ntarget = \"shop::version\"\nto = \"pantry_tests\"\n\n[function]\nname = \"one\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?} {:?}", p.reasons, p.preconditions);
    assert!(p.listed.iter().any(|l| l == "the test module shop::pantry_tests is made: #[cfg(test)] mod pantry_tests at the end of shop/src/lib.rs"), "{:?}", p.listed);
}

#[test]
fn gate_refuses_an_item_its_feature_gates_already() {
    let (store, ctx, _) = project("gated");
    write(&store.root, "shop/Cargo.toml", "[package]\nname = \"shop\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmarks = { path = \"../marks\" }\n\n[features]\njson = []\n");
    let lib = std::fs::read_to_string(store.root.join("shop/src/lib.rs")).unwrap();
    write(&store.root, "shop/src/lib.rs", &(lib + "\n#[cfg(feature = \"json\")]\npub fn to_json() -> u8 {\n    1\n}\n\npub fn plain() -> u8 {\n    2\n}\n"));
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let twice = preflight::run_preflight(&ctx, &spec(&store.root, "g", "name = \"g\"\nobjective = \"o\"\noperation = \"gate\"\ntarget = \"shop::to_json\"\nfeature = \"json\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!twice.authorized && twice.preconditions.iter().any(|c| !c.holds && c.detail == "its cfg already needs json: nothing to gate"), "{:?}", twice.preconditions);
    let once = preflight::run_preflight(&ctx, &spec(&store.root, "g2", "name = \"g2\"\nobjective = \"o\"\noperation = \"gate\"\ntarget = \"shop::plain\"\nfeature = \"json\"\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(once.authorized, "{:?} {:?}", once.reasons, once.preconditions);
}
