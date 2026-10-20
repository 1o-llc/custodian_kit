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
