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
    // the refusal is a change already made: its receipt says so, and its top line is what is in place, the rule beneath
    assert_eq!(twice.in_place, ["its cfg already needs json: nothing to gate"]);
    assert_eq!(twice.reason(), preflight::ALREADY_IN_PLACE);
    assert_eq!(twice.headline_why().unwrap(), "already in place: its cfg already needs json: nothing to gate");
    let page = twice.markdown();
    let decision = page.lines().find(|l| l.starts_with("- decision:")).unwrap();
    assert!(decision.contains("already in place") && !decision.contains("`facts`"), "{decision}");
    assert!(page.lines().any(|l| l.starts_with("- the rules it breaks:") && l.contains("`facts`=authorized requires `facts`=preconditions")), "{page}");
    assert!(once.in_place.is_empty() && once.reason() == preflight::AUTHORIZED);
}

/// A refusal's top line is the reason in words: the precondition that does not hold and why, never the word's rule.
#[test]
fn a_refusals_top_line_is_the_reason_and_the_rule_is_beneath() {
    let (store, ctx, w) = project("why");
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "e", "name = \"e\"\nobjective = \"o\"\noperation = \"inline\"\ntarget = \"shop::early\"\n"), None, Runner::InProcess(&w)).unwrap();
    let pf = &p.preflight;
    assert!(!pf.authorized && pf.in_place.is_empty() && pf.reason() == preflight::NOT_AUTHORIZED);
    let top = p.said.iter().find(|l| l.starts_with("preflight e:")).unwrap();
    assert!(top.starts_with("preflight e: not authorized — the function can be inlined: ") && !top.contains("`facts`"), "{top}");
    assert!(p.said.iter().any(|l| l.trim_start().starts_with("the rules it breaks: `facts`=authorized requires `facts`=preconditions")), "{:?}", p.said);
}

/// shop's `list` is called inside the arguments of macros klepto does not expand: through the test module's
/// `use super::*` (a call of it), and through a path that resolves to nothing (`nowhere::list`, which no one can
/// tell is it). Two more macros write the name and call nothing: a declaration (`fn list`) and a value.
fn macro_calls(tag: &str, nowhere: bool) -> (Store, Ctx, Workspace) {
    let (store, ctx, _) = project(tag);
    let lib = std::fs::read_to_string(store.root.join("shop/src/lib.rs")).unwrap();
    let extra = if nowhere { "        wrapped!(nowhere::list(2), \"what nowhere is, no one says\");\n" } else { "" };
    write(&store.root, "shop/src/lib.rs", &format!("{lib}\npub fn list(n: u32) -> u32 {{\n    n\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn lists() {{\n        wrapped!(list(1) == 1, \"one\");\n        wrapped!(\n            crate::list(3) == 3,\n            \"three\"\n        );\n        wrapped!(fn list(), \"a declaration, not a call\");\n        wrapped!(list_of(list), \"a value named list, not a call\");\n{extra}    }}\n}}\n"));
    let w = Workspace::open_with(&store.root, &Scope { tests: true, ..Scope::default() }, Resolving::Free).unwrap();
    (store, ctx, w)
}

const LIST_WIDER: &str = "name = \"s\"\nobjective = \"o\"\noperation = \"change_signature\"\ntarget = \"shop::list\"\n\n[function]\nname = \"list\"\nvis = \"pub\"\nreturns = \"u32\"\nparams = [{ name = \"n\", ty = \"u32\" }, { name = \"m\", ty = \"u32\" }]\n\n[call_sites]\npass = \"0\"\n";

#[test]
fn change_signature_counts_the_calls_inside_macro_arguments_and_refuses_while_one_is_unresolved() {
    // every call resolves: the two inside macro arguments are calls of it, through `use super::*` and `crate::`
    let (store, ctx, w) = macro_calls("sig-macros", false);
    let calls = custodian_kit::change::macro_calls(w.klepto(), "shop::list");
    assert_eq!(calls.iter().map(|c| (c.line, c.paths.clone())).collect::<Vec<_>>(), [(47, vec!["list".to_string()]), (48, vec!["crate::list".to_string()])], "{calls:?}");
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "s", LIST_WIDER), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?} {:?}", p.why, p.preconditions);
    assert!(p.preconditions.iter().any(|c| c.what == "every call site gets the new argument" && c.detail == "2 call sites (0 resolved by klepto, 2 inside a macro's arguments)"), "{:?}", p.preconditions);
    assert!(p.preconditions.iter().any(|c| c.what == "every call site resolves" && c.holds && c.detail == "unresolved: 0"), "{:?}", p.preconditions);
    // a path no one resolves: refused, `unresolved: 1`, until the spec allows it
    let (store, ctx, w) = macro_calls("sig-nowhere", true);
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "s", LIST_WIDER), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized, "{:?}", p.preconditions);
    let c = p.preconditions.iter().find(|c| c.what == "every call site resolves").unwrap();
    assert!(!c.holds && c.detail.starts_with("unresolved: 1 — shop/src/lib.rs:54 (nowhere::list in wrapped!)") && c.detail.contains("call_sites.allow_unresolved = true"), "{}", c.detail);
    assert!(p.headline_why().unwrap().contains("every call site resolves: unresolved: 1"), "{:?}", p.headline_why());
    let allowed = format!("{LIST_WIDER}allow_unresolved = true\n");
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "s2", &allowed.replace("name = \"s\"", "name = \"s2\"")), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?}", p.why);
    assert!(p.listed.iter().any(|l| l.starts_with("unresolved call site shop/src/lib.rs:54 (nowhere::list in wrapped!)") && l.ends_with("not changed (call_sites.allow_unresolved)")), "{:?}", p.listed);
}

#[test]
fn code_naming_an_optional_dependency_needs_a_feature_that_turns_it_on() {
    let (store, ctx, w) = forms("optional-use");
    let bare = "name = \"m\"\nobjective = \"o\"\noperation = \"make_fn\"\ntarget = \"crate:p\"\n\n[function]\nname = \"from_b\"\nvis = \"pub\"\nreturns = \"u8\"\nbody = [{ piece = \"value\", value = \"b::b()\" }]\n";
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "m", bare), None, Runner::InProcess(&w)).unwrap().preflight;
    let c = p.preconditions.iter().find(|c| c.what == "the code names the optional dependency b only under a feature that turns it on").unwrap();
    assert!(!p.authorized && !c.holds, "{:?}", p.preconditions);
    assert_eq!(c.detail, "b is an optional dependency of p (turned on by b): give the code `attrs = ['#[cfg(feature = \"b\")]']`, or gate the place it lands");
    let gated = bare.replace("returns = \"u8\"\n", "returns = \"u8\"\nattrs = ['#[cfg(feature = \"b\")]']\n").replace("name = \"m\"", "name = \"m2\"");
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "m2", &gated), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?} {:?}", p.why, p.preconditions);
    assert!(p.preconditions.iter().any(|c| c.holds && c.detail == "gated by b"), "{:?}", p.preconditions);
    // a derive inside a cfg_attr on the feature is gated by it; a required dependency's path needs nothing
    let spec_of = |text: &str| -> custodian_kit::change::ChangeSpec { toml::from_str(text).unwrap() };
    let land = custodian_kit::change::landing(w.klepto(), &store.root, &spec_of(bare)).unwrap();
    let derive = spec_of("name = \"d\"\nobjective = \"o\"\noperation = \"make_struct\"\ntarget = \"crate:p\"\n\n[item]\nname = \"S\"\nattrs = ['#[cfg_attr(feature = \"b\", derive(b::B))]']\n");
    assert!(custodian_kit::change::optional_uses(w.klepto(), &derive, &land).iter().all(|u| u.gated));
    let required = spec_of(&bare.replace("b::b()", "a::a()"));
    assert!(custodian_kit::change::optional_uses(w.klepto(), &required, &land).is_empty());
}

#[test]
fn the_preflight_counts_the_configurations_the_change_leaves() {
    let (store, ctx, _) = forms("configs");
    // p's optional b and c are cargo's implicit features b and c; one feature naming both `dep:` takes both away:
    // the matrix the transform checks is the one with f and without b and c, smaller than the one before
    write(&store.root, "p/Cargo.toml", "[package]\nname = \"p\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\na = { path = \"../a\" }\nb = { path = \"../b\", optional = true }\nc = { path = \"../c\", optional = true }\n\n[features]\ng = []\n");
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let text = "name = \"f\"\nobjective = \"o\"\noperation = \"declare_feature\"\ntarget = \"crate:p\"\nfeature = \"f\"\nenables = [\"dep:b\", \"dep:c\"]\n";
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "f", text), None, Runner::InProcess(&w)).unwrap().preflight;
    let spec_v: custodian_kit::change::ChangeSpec = toml::from_str(text).unwrap();
    let after = klepto::check_matrix(&custodian_kit::change::model_after(w.klepto(), &store.root, &spec_v, "p").unwrap()).len();
    let before = klepto::check_matrix(&w.klepto().features["p"]).len();
    let i = p.impact.as_ref().unwrap();
    assert_eq!(i.feature_configurations, after, "{}", i.line());
    assert!(after < before, "{after} after, {before} before");
    let d = i.dimensions.iter().find(|d| d.name == "feature configurations to check").unwrap();
    assert_eq!((d.value, d.note.clone()), (Some(after as u64), format!("klepto's check matrix over each affected crate's features as the change leaves them ({before} before it)")));
    assert!(i.line().contains(&format!("{after} configurations to check")), "{}", i.line());
}

/// Every `enables` form the feature graph of p offers is a case of its own, and `optional` is one more case of
/// add_dependency and change_dependency.
#[test]
fn the_exercise_makes_a_case_for_each_form_the_feature_graph_offers() {
    let (store, _, w) = forms("exercise-forms");
    let k = w.klepto();
    let forms = custodian_kit::exercise::feature_forms(k, "p");
    assert_eq!(forms.iter().map(|(l, e)| (l.as_str(), e.clone())).collect::<Vec<_>>(), [("dep:b", vec!["dep:b".to_string()]), ("a/x", vec!["a/x".to_string()]), ("b?/y", vec!["b?/y".to_string()])]);
    let t = custodian_kit::exercise::Target { unit: custodian_kit::exercise::Unit::Crate, route: "crate:p".into(), package: "p".into() };
    let crates = vec!["p".to_string()];
    let specs = custodian_kit::exercise::specs_for(k, &store.root, custodian_kit::change::Operation::DeclareFeature, &t, 5, &crates);
    assert_eq!(specs.iter().map(|(n, f, s)| (*n, f.clone(), s.enables.clone(), s.name.clone())).collect::<Vec<_>>(), [
        (5, None, vec![], "exercise-005-declare-feature".to_string()),
        (6, Some("enables dep:b".into()), vec!["dep:b".to_string()], "exercise-006-declare-feature".into()),
        (7, Some("enables a/x".into()), vec!["a/x".to_string()], "exercise-007-declare-feature".into()),
        (8, Some("enables b?/y".into()), vec!["b?/y".to_string()], "exercise-008-declare-feature".into()),
    ]);
    // each form's spec is one the preflight authorizes
    let ctx = custodian_kit::runs::Ctx { root: store.root.clone(), store: Some(store.clone()), exe: PathBuf::from("custodian") };
    for (_, form, s) in &specs {
        let path = spec(&store.root, &s.name, &s.to_toml().unwrap());
        let p = preflight::run_preflight(&ctx, &path, None, Runner::InProcess(&w)).unwrap().preflight;
        assert!(p.authorized, "{form:?}: {:?}", p.why);
    }
    let adds = custodian_kit::exercise::specs_for(k, &store.root, custodian_kit::change::Operation::AddDependency, &t, 1, &crates);
    assert_eq!(adds.iter().map(|(_, f, s)| (f.clone(), s.dependency.as_ref().and_then(|d| d.optional))).collect::<Vec<_>>(), [(None, None), (Some("optional".into()), Some(true))]);
}

/// p depends on a (and a on c), and optionally on b; a declares x = ["c/z"], c declares z, b declares y.
fn forms(tag: &str) -> (Store, Ctx, Workspace) {
    let root = std::env::temp_dir().join(format!("custodian-kit-forms-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    write(&root, "Cargo.toml", "[workspace]\nresolver = \"2\"\nmembers = [\"p\", \"a\", \"b\", \"c\"]\n");
    write(&root, "p/Cargo.toml", "[package]\nname = \"p\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\na = { path = \"../a\" }\nb = { path = \"../b\", optional = true }\n\n[features]\ng = []\n");
    write(&root, "p/src/lib.rs", "pub fn p() -> u8 {\n    a::a()\n}\n");
    write(&root, "a/Cargo.toml", "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nc = { path = \"../c\" }\n\n[features]\nx = [\"c/z\"]\n");
    write(&root, "a/src/lib.rs", "pub fn a() -> u8 {\n    c::c()\n}\n");
    write(&root, "b/Cargo.toml", "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[features]\ny = []\n");
    write(&root, "b/src/lib.rs", "pub fn b() -> u8 {\n    2\n}\n");
    write(&root, "c/Cargo.toml", "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[features]\nz = []\n");
    write(&root, "c/src/lib.rs", "pub fn c() -> u8 {\n    1\n}\n");
    let (store, _) = Store::init(&root, Mode::Agent).unwrap();
    let ctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: PathBuf::from("custodian") };
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    (store, ctx, w)
}

#[test]
fn declare_feature_takes_every_cargo_form_and_reads_it_through_the_feature_graph() {
    let (store, ctx, w) = forms("all");
    let text = "name = \"f\"\nobjective = \"o\"\noperation = \"declare_feature\"\ntarget = \"crate:p\"\nfeature = \"f\"\nenables = [\"g\", \"a/x\", \"b?/y\", \"dep:b\"]\n";
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "f", text), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?} {:?}", p.reasons, p.preconditions);
    let listed = p.listed.join("\n");
    // a/x brings a's x, and a second level down c's z
    assert!(listed.contains("f brings a:x (a/x in p)") && listed.contains("f brings c:z (c/z in a)"), "{listed}");
    // dep:b turns b on, so b?/y brings y
    assert!(listed.contains("f brings the dependency b (dep:b in p)") && listed.contains("f brings b:y (b?/y in p, weak)"), "{listed}");
    // the weak entry alone: pending, said as weak
    let w2 = preflight::run_preflight(&ctx, &spec(&store.root, "w", "name = \"w\"\nobjective = \"o\"\noperation = \"declare_feature\"\ntarget = \"crate:p\"\nfeature = \"w\"\nenables = [\"b?/y\"]\n"), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(w2.authorized && w2.listed.iter().any(|l| l == "w weak: b?/y in p: brings it only when b is on"), "{:?}", w2.listed);
}

#[test]
fn declare_feature_refuses_a_feature_the_dependency_does_not_have_and_names_its_features() {
    let (store, ctx, w) = forms("nope");
    for (enables, why) in [("[\"a/nope\"]", "`a/nope`: a has no feature nope (its features: x)"), ("[\"b?/nope\"]", "`b?/nope`: b has no feature nope (its features: y)"), ("[\"q/x\"]", "`q/x`: p has no dependency q")] {
        let text = format!("name = \"n\"\nobjective = \"o\"\noperation = \"declare_feature\"\ntarget = \"crate:p\"\nfeature = \"n\"\nenables = {enables}\n");
        let p = preflight::run_preflight(&ctx, &spec(&store.root, "n", &text), None, Runner::InProcess(&w)).unwrap().preflight;
        assert!(!p.authorized && p.preconditions.iter().any(|c| !c.holds && c.detail.contains(why)), "{enables}: {:?}", p.preconditions);
    }
}

#[test]
fn a_method_generators_docs_are_written_inside_its_call() {
    let f: custodian_kit::change::FnSpec = toml::from_str("name = \"can_cook\"\nvis = \"pub\"\nreceiver = \"&self\"\nparams = [{ name = \"r\", ty = \"&Recipe\" }]\nreturns = \"bool\"\ndoc = \"Whether the shelves hold\\neverything the recipe needs.\"\ngenerator = { piece = \"simple_method\", value = \"self.missing(r).is_empty()\" }\n").unwrap();
    let text = custodian_kit::change::render_fn(&f, "    ").unwrap();
    assert_eq!(text, "    simple_impl::simple_method!(#[doc = \" Whether the shelves hold\"] #[doc = \" everything the recipe needs.\"] pub fn can_cook(&self, r: &Recipe) -> bool => self.missing(r).is_empty());\n");
    assert!(!text.contains("///"), "a `///` above a macro call documents nothing");
}

#[test]
fn a_value_step_with_a_closure_or_a_macro_call_is_reported_not_refused() {
    let (store, ctx, w) = project("values");
    let text = "name = \"v\"\nobjective = \"o\"\noperation = \"make_fn\"\ntarget = \"crate:shop\"\n\n[function]\nname = \"listed\"\nvis = \"pub\"\nreturns = \"Vec<u32>\"\nbody = [\n  { piece = \"let\", bind = \"xs\", value = \"vec![1, 2, 3]\" },\n  { piece = \"value\", value = \"xs.into_iter().map(|x| x * 2).collect()\" },\n]\n";
    let p = preflight::run_preflight(&ctx, &spec(&store.root, "v", text), None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "flagged, not refused: {:?} {:?}", p.reasons, p.preconditions);
    assert!(p.raw.contains(&"let with a macro call (vec!): vec![1, 2, 3]".to_string()), "{:?}", p.raw);
    assert!(p.raw.contains(&"value with a closure: xs.into_iter().map(|x| x * 2).collect()".to_string()), "{:?}", p.raw);
    assert!(p.flags.contains(&"raw".to_string()), "{:?}", p.flags);
    assert_eq!(custodian_kit::change::code_in_value("println!(\"{}\", f(|y| y))"), (1, vec!["println".to_string()]));
    assert_eq!(custodian_kit::change::code_in_value("a + b.len()"), (0, vec![]));
}

/// p depends on a, a on c: a body swapped in c is compiled in c (its tests too) and in a, never in p, a dependent's
/// dependent; a rename reaches the readers of what it renames as well.
#[test]
fn a_proof_compiles_the_crate_and_its_direct_dependents_never_a_dependents_dependent() {
    let (store, _ctx, w) = forms("proof");
    let spec_of = |text: &str| -> custodian_kit::change::ChangeSpec { toml::from_str(text).unwrap() };
    let body = spec_of("name = \"b\"\nobjective = \"o\"\noperation = \"replace_body\"\ntarget = \"c::c\"\nsame_expansion = true\n\n[function]\nname = \"c\"\nbody = [{ piece = \"value\", value = \"1\" }]\n");
    assert!(body.same_expansion);
    let land = custodian_kit::change::landing(w.klepto(), &store.root, &body).unwrap();
    let got = |s: &custodian_kit::change::ChangeSpec| custodian_kit::change::proof_packages(w.klepto(), s, &land).into_iter().map(|p| (p.package, p.tests, p.why)).collect::<Vec<_>>();
    assert_eq!(got(&body), [("a".to_string(), false, "direct dependent".to_string()), ("c".to_string(), true, "lands".to_string())]);
    assert!(!custodian_kit::change::reaches_readers(custodian_kit::change::Operation::ReplaceBody));
    let rename = spec_of("name = \"r\"\nobjective = \"o\"\noperation = \"rename\"\ntarget = \"c::c\"\nnew_name = \"cc\"\n");
    assert!(custodian_kit::change::reaches_readers(rename.op()));
    assert!(got(&rename).iter().all(|(p, ..)| p != "p"), "{:?}", got(&rename));
}
