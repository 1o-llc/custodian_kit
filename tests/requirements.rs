//! The requirements reading on its fixtures (tests/fixtures/requirements/): the brief's acceptance cases that are the
//! library's (1-7 and 9), the three Rust-version disagreements, and a package in two versions. Each fixture is a
//! workspace of path packages, so cargo reads it offline; the ones cargo must refuse have no lock.

use custodian_kit::api::{ReadingSpec, Resolving};
use custodian_kit::reading::requirements::{self, Basis, Check, Finding, RequirementsReport, Severity, Status};
use custodian_kit::reading::ReadingReport;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/requirements").join(name)
}

fn spec(target: &str, extra: &str) -> ReadingSpec {
    ReadingSpec::from_toml(&format!("name = \"requirements\"\nquestion = \"what does the change need\"\nkind = \"requirements\"\ntarget = {}\n{extra}", toml::Value::String(target.into()))).expect("spec")
}

fn read_at(root: &Path, target: &str, extra: &str) -> (ReadingReport, RequirementsReport) {
    let s = spec(target, extra);
    let r = custodian_kit::reading::read(root, Path::new("requirements.toml"), &s, "20261023T000000Z", Resolving::Free).unwrap_or_else(|e| panic!("{}: {e}", root.display()));
    let req = r.requirements.clone().expect("a requirements report");
    (r, req)
}

fn read(name: &str, target: &str, extra: &str) -> (ReadingReport, RequirementsReport) {
    read_at(&fixture(name), target, extra)
}

fn of(r: &RequirementsReport, check: Check) -> Vec<&Finding> {
    r.findings.iter().filter(|f| f.check == check).collect()
}

fn one(r: &RequirementsReport, check: Check) -> &Finding {
    let v = of(r, check);
    assert_eq!(v.len(), 1, "one {check}: {:#?}", r.findings);
    v[0]
}

/// Every file under a directory and its bytes' digest.
fn tree(dir: &Path) -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    let mut work = vec![dir.to_path_buf()];
    while let Some(d) = work.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                work.push(p);
            } else {
                out.insert(p.clone(), custodian_kit::digest::of(std::fs::read(&p).unwrap()));
            }
        }
    }
    out
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

// 1
#[test]
fn a_workspace_dependency_with_no_root_entry_names_the_manifest_and_its_governing_root() {
    let (_, r) = read("inherit", ".", "");
    let f = one(&r, Check::WorkspaceDependencyMissing);
    assert_eq!((f.krate.as_deref(), f.severity, f.basis, f.status), (Some("one_db_kernel"), Severity::Error, Basis::Observed, Status::Unsatisfied));
    let at = f.at.as_ref().unwrap();
    assert_eq!((at.file.as_str(), at.line, at.section.as_str(), at.field.as_str()), ("crates/one_db_kernel/Cargo.toml", Some(7), "dependencies", "hex"));
    assert!(f.what.contains("governing root Cargo.toml has no [workspace.dependencies].hex"), "{}", f.what);
    assert!(f.action.as_deref().unwrap().contains("add `hex` to [workspace.dependencies] in Cargo.toml"), "{:?}", f.action);
    assert!(f.action.as_deref().unwrap().contains("this reading does not choose one"));
    // util is inherited from the same root, and found
    assert_eq!(r.state.satisfied["`workspace = true` dependencies found in their root"], 2);
    assert_eq!(r.state.satisfied["inherited [package] fields found in the root"], 2);
    // cargo refuses on the same name
    assert!(r.state.metadata.starts_with("--frozen refused") || r.state.metadata.starts_with("refused"), "{}", r.state.metadata);
    assert!(r.state.agreement.starts_with("agree: cargo refused naming `hex`"), "{}", r.state.agreement);
    // the first step is the root's declaration, required
    let s = &r.steps[0];
    assert!(s.required && s.action.contains("[workspace.dependencies] in Cargo.toml"), "{:#?}", r.steps);
    assert_eq!(s.answers, vec![f.id.clone()]);
}

// 2
#[test]
fn a_nested_independent_workspace_is_its_own_root_and_no_declaration_goes_in_the_outer_one() {
    let (_, r) = read("nested", ".", "");
    let roots: Vec<(&str, bool)> = r.state.roots.iter().map(|x| (x.manifest.as_str(), x.nested)).collect();
    // hexish is excluded by inner/ and holds an empty [workspace]: a root of its own too
    assert_eq!(roots, [("Cargo.toml", false), ("inner/Cargo.toml", true), ("inner/hexish/Cargo.toml", true)]);
    let f = one(&r, Check::WorkspaceDependencyMissing);
    assert_eq!(f.krate.as_deref(), Some("core"));
    assert_eq!(f.at.as_ref().unwrap().file, "inner/core/Cargo.toml");
    assert!(f.what.contains("governing root inner/Cargo.toml has no [workspace.dependencies].bytesish"), "{}", f.what);
    // the outer root declares bytesish, and is not proposed
    assert!(f.action.as_deref().unwrap().contains("in inner/Cargo.toml"), "{:?}", f.action);
    assert!(of(&r, Check::NotAMember).is_empty(), "{:#?}", of(&r, Check::NotAMember));
    // app's bytesish and core's hexish resolve, each in its own root
    assert_eq!(r.state.satisfied["`workspace = true` dependencies found in their root"], 2);
}

// 3
#[test]
fn a_path_dependency_chain_is_followed_to_the_directory_that_is_not_there() {
    let (_, r) = read("paths", ".", "");
    let f = one(&r, Check::PathMissing);
    let at = f.at.as_ref().unwrap();
    assert_eq!((at.file.as_str(), at.field.as_str(), f.krate.as_deref()), ("vendor/a/Cargo.toml", "b", Some("a")));
    assert!(f.what.contains("vendor/b"), "{}", f.what);
    assert_eq!(r.state.satisfied["path dependencies found with their package"], 1, "app → vendor/a is there");
    assert!(of(&r, Check::MetadataRefused).len() == 1, "cargo refuses the chain too");
}

// 4
#[test]
fn features_are_read_in_their_four_forms_and_untested_combinations_are_unknown() {
    let (_, r) = read("features", "crate:app", "[requirements]\nintent = \"app needs tool/x and lib/slow and lib/quick\"\n");
    assert_eq!(one(&r, Check::MetadataRead).status, Status::Satisfied);
    let feats = of(&r, Check::NeedFeature);
    let about = |f: &str| feats.iter().find(|x| x.what.contains(f)).unwrap_or_else(|| panic!("{f}: {feats:#?}"));
    // tool's x: only behind app's weak forward
    let x = about("`x`");
    assert_eq!((x.status, x.basis), (Status::Informational, Basis::Inferred));
    assert!(x.what.contains("`weakx` (`tool?/x`: weak"), "{}", x.what);
    // lib's slow: nothing turns it on
    let slow = about("`slow`");
    assert_eq!((slow.status, slow.severity), (Status::Unsatisfied, Severity::Error));
    assert!(slow.action.as_deref().unwrap().contains("add \"slow\" to the features of `lib`"), "{:?}", slow.action);
    // lib's quick: on through app's default feature `fast` (forwarded), not app's own entry
    let quick = about("`quick`");
    assert_eq!(quick.status, Status::Informational, "{quick:#?}");
    assert!(quick.what.contains("`fast` (`lib/quick`"), "{}", quick.what);
    // the combinations nothing resolved or built
    let c = one(&r, Check::FeatureCombinations);
    assert_eq!((c.basis, c.krate.as_deref()), (Basis::Unknown, Some("app")));
    assert!(c.what.contains("3 features; 7 combinations"), "{}", c.what);
    for form in ["fast = [lib/quick]", "extra = [dep:tool]", "weakx = [tool?/x]", "default turns on [default, fast]", "cargo's resolve: [default, fast]"] {
        assert!(c.evidence.contains(form), "{form} in {}", c.evidence);
    }
}

// 5
#[test]
fn a_real_version_conflict_is_reported_without_a_guessed_repair() {
    let (_, r) = read("conflict", ".", "");
    let f = one(&r, Check::PathVersionMismatch);
    assert_eq!((f.status, f.basis), (Status::Unsatisfied, Basis::Observed));
    assert!(f.what.contains("requires 0.2") && f.what.contains("is 0.1.0"), "{}", f.what);
    let action = f.action.as_deref().unwrap();
    assert!(!action.chars().any(|c| c.is_ascii_digit()), "no version guessed: {action}");
    assert!(action.contains("which one is a decision"));
    let cargo = one(&r, Check::MetadataRefused);
    assert!(cargo.what.contains("dup"), "cargo names it too: {}", cargo.what);
    assert!(r.steps.iter().all(|s| !s.action.contains("0.1.0") && !s.action.contains("\"0.2")), "{:#?}", r.steps);
}

// 6
#[test]
fn an_intent_to_add_a_crate_gives_its_prerequisites_impact_and_an_ordered_plan() {
    let (_, r) = read("add", ".", "[requirements]\nintent = \"add util/fmt to app\"\n");
    assert_eq!(r.needs.len(), 1);
    assert_eq!((r.needs[0].member.as_deref(), r.needs[0].dependency.as_deref(), r.needs[0].features.clone()), (Some("app"), Some("util"), vec!["fmt".to_string()]));
    let d = of(&r, Check::NeedDeclared);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].status, Status::Unsatisfied);
    assert!(d[0].action.as_deref().unwrap().contains("util 0.3.1 meets (the version the lock already holds"), "{:?}", d[0].action);
    assert!(r.reuse.iter().any(|x| x.contains("the lock holds util 0.3.1") && x.contains("lib (0.3")), "{:#?}", r.reuse);
    // util has fmt; nothing turns it on yet
    let f = one(&r, Check::NeedFeature);
    assert_eq!(f.status, Status::Unsatisfied);
    // impact from cargo's resolve: what app depends on and what depends on it
    let i = r.impact.iter().find(|i| i.krate == "app").expect("app's impact");
    assert_eq!(i.depends_on, ["lib 0.1.0"]);
    assert_eq!(i.direct_dependents, ["cli 0.1.0"]);
    assert!(i.readings.contains(&"custodian run crate:app --kind impact".to_string()));
    // the order: declare, then the feature, then read again, then the checks this reading never runs
    let actions: Vec<(bool, &str)> = r.steps.iter().map(|s| (s.required, s.action.as_str())).collect();
    assert!(actions[0].0 && actions[0].1.starts_with("add `util` to [dependencies] of app/Cargo.toml"), "{actions:#?}");
    assert!(actions[1].0 && actions[1].1.contains("add \"fmt\" to the features of `util`"), "{actions:#?}");
    assert!(actions[2].0 && actions[2].1.starts_with("read the requirements again"), "{actions:#?}");
    assert!(!actions[3].0 && actions[3].1.starts_with("cargo check -p app"), "{actions:#?}");
}

// 7: nothing is written; the key moves with the manifests and the lock, not with the code
#[test]
fn the_reading_writes_nothing_and_its_key_moves_with_the_manifests_only() {
    let root = fixture("add");
    let before = tree(&root);
    let _ = read("add", ".", "[requirements]\nintent = \"add util to app\"\n");
    assert_eq!(tree(&root), before, "no file of the workspace was written");
    let dir = std::env::temp_dir().join(format!("custodian-kit-requirements-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    copy(&root, &dir);
    let s = spec(".", "[requirements]\nintent = \"add util to app\"\n");
    let k0 = requirements::input_digest(&dir, &s).unwrap();
    assert_eq!(requirements::input_digest(&dir, &s).unwrap(), k0, "the same inputs, the same key");
    std::fs::write(dir.join("app/src/lib.rs"), "//! fixture, changed\npub fn f() {}\n").unwrap();
    assert_eq!(requirements::input_digest(&dir, &s).unwrap(), k0, "a source edit is not an input");
    let other = spec(".", "[requirements]\nintent = \"add lib to cli\"\n");
    assert_ne!(requirements::input_digest(&dir, &other).unwrap(), k0, "another intent is another reading");
    let m = dir.join("app/Cargo.toml");
    std::fs::write(&m, std::fs::read_to_string(&m).unwrap() + "util = { path = \"../util\" }\n").unwrap();
    let k1 = requirements::input_digest(&dir, &s).unwrap();
    assert_ne!(k1, k0, "a manifest edit invalidates it");
    let lock = dir.join("Cargo.lock");
    std::fs::write(&lock, std::fs::read_to_string(&lock).unwrap() + "\n").unwrap();
    assert_ne!(requirements::input_digest(&dir, &s).unwrap(), k1, "a lock edit invalidates it");
    // and the search key the journal reuses by is this digest
    let key = custodian_kit::api::SearchKey::of(&dir, &s).unwrap();
    assert_eq!(key.code_digest, requirements::input_digest(&dir, &s).unwrap());
}

// 9
#[test]
fn static_findings_and_cargos_word_are_kept_apart_and_no_compile_is_claimed() {
    let (page, r) = read("inherit", ".", "");
    let md = custodian_kit::reading::render::page(&page);
    assert!(r.unknowns[0].starts_with("whether the code compiles: this reading never compiles"), "{:#?}", r.unknowns);
    assert!(md.contains("whether the code compiles: this reading never compiles"));
    assert!(r.provenance.ran.iter().all(|x| !x.line.contains(" check") && !x.line.contains(" build") && !x.line.contains(" test")), "{:#?}", r.provenance.ran);
    assert!(r.provenance.ran.iter().any(|x| x.line.contains("metadata") && x.line.contains("--frozen")));
    assert_eq!(r.provenance.mutation, "none: no manifest, lock or source file was written");
    // the scanner's finding is the scanner's; cargo's refusal is cargo's
    assert_eq!(one(&r, Check::WorkspaceDependencyMissing).from.as_str(), "manifest");
    assert_eq!(one(&r, Check::MetadataRefused).from.as_str(), "metadata");
    // where cargo reads the workspace and the scanner found nothing, they agree
    let (_, ok) = read("add", ".", "");
    assert_eq!(ok.state.agreement, "agree: the scanner found no error in the workspace cargo read, and cargo read it");
    assert!(of(&ok, Check::ScannerAndCargoDisagree).is_empty());
}

#[test]
fn a_crate_that_promises_an_older_rust_than_its_lock_allows_is_named_with_the_package() {
    let (_, r) = read("rust", ".", "");
    let f = of(&r, Check::RustVersionBelowLock);
    assert_eq!(f.len(), 1, "{:#?}", r.findings);
    assert_eq!(f[0].krate.as_deref(), Some("app"));
    assert!(f[0].what.contains("promises Rust 1.60.0; lib 0.1.0 in its lock declares 1.80.0"), "{}", f[0].what);
    assert_eq!(f[0].at.as_ref().unwrap().line, Some(5));
    let row = r.rust.iter().find(|x| x.krate == "app").unwrap();
    assert_eq!((row.edition.as_str(), row.declared.as_deref(), row.edition_min.as_str()), ("2021", Some("1.60.0"), "1.56"));
}

#[test]
fn a_lock_newer_than_the_installed_toolchain_is_named() {
    let (_, r) = read("rust", ".", "");
    assert!(r.provenance.toolchain.release.is_some(), "rustc -vV answered");
    let f = one(&r, Check::ToolchainBelowLock);
    assert!(f.what.contains("up to 1.999"), "{}", f.what);
    assert!(f.evidence.contains("future 0.1.0 (1.999.0)"), "{}", f.evidence);
    assert!(f.action.as_deref().unwrap().contains("install Rust 1.999"));
}

#[test]
fn an_edition_its_declared_minimum_cannot_compile_is_named_without_cargo() {
    let (_, r) = read("edition", ".", "");
    let f = one(&r, Check::EditionNeedsNewerRust);
    assert_eq!((f.krate.as_deref(), f.severity), (Some("app"), Severity::Error));
    assert!(f.what.contains("rust-version 1.80 with edition 2024, which first compiles on Rust 1.85"), "{}", f.what);
    // with cargo not run at all, the scanner says it alone
    let (_, quiet) = read("edition", ".", "[requirements]\nfaces = [\"rust\", \"no_cargo\"]\n");
    assert!(quiet.provenance.ran.is_empty());
    assert_eq!(one(&quiet, Check::EditionNeedsNewerRust).from.as_str(), "manifest");
}

#[test]
fn a_package_in_two_versions_names_who_forces_each_and_the_one_change_that_leaves_one() {
    let (_, r) = read("dups", ".", "");
    assert_eq!(r.duplicates.len(), 1, "{:#?}", r.duplicates);
    let d = &r.duplicates[0];
    assert_eq!(d.name, "dup");
    assert_eq!(d.versions.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(), ["0.1.0", "0.2.0"]);
    assert_eq!(d.compiles, 2);
    let first = &d.versions[0].forced_by[0];
    assert_eq!((first.package.as_str(), first.requirement.as_deref(), first.local), ("app 0.1.0", Some("*"), true));
    assert_eq!(first.at.as_ref().map(|p| (p.file.as_str(), p.line)), Some(("app/Cargo.toml", Some(7))));
    assert_eq!(d.versions[1].forced_by[0].requirement.as_deref(), Some("^0.2"));
    assert_eq!(d.versions.iter().map(|v| (v.source.as_deref(), v.builds)).collect::<Vec<_>>(), [(Some("dup-0.1"), 1), (Some("dup-0.2"), 1)]);
    assert!(d.collapse.starts_with("one change: app 0.1.0's requirement `dup *` (app/Cargo.toml:7) admitting dup 0.2.0 (dup-0.2) would leave one version (inferred"), "{}", d.collapse);
    assert!(r.steps.iter().any(|s| !s.required && s.action.starts_with("`dup`: one change")), "{:#?}", r.steps);
    assert!(r.provenance.ran.iter().any(|x| x.line.contains("tree") && x.line.contains("--duplicates")));
}

#[test]
fn the_faces_are_one_word_and_the_lock_cannot_go_without_cargo() {
    use requirements::FacesWord;
    assert_eq!(FacesWord::from_names([]).unwrap().names(), ["prerequisites", "rust", "lock"]);
    assert_eq!(FacesWord::from_names(["no_cargo"]).unwrap().names(), ["prerequisites", "rust", "no_cargo"]);
    let refused = FacesWord::from_names(["lock", "no_cargo"]).unwrap_err().to_string();
    assert!(refused.contains("lock") && refused.contains("no_cargo"), "{refused}");
    assert!(FacesWord::from_names(["nope"]).is_err());
    // a requirements spec reads the same in TOML and JSON, and every other spec keeps its form (and its hash)
    let s = spec(".", "[requirements]\nintent = \"add util to app\"\nfaces = [\"prerequisites\"]\n");
    assert_eq!(ReadingSpec::from_json(&serde_json::to_string(&s).unwrap()).unwrap(), s);
    let plain = spec("crate:app", "");
    assert!(!serde_json::to_string(&plain).unwrap().contains("requirements\":{"), "an empty [requirements] is not written");
}

#[test]
fn every_finding_says_how_it_is_known_and_where_it_was_read() {
    for (name, extra) in [("inherit", ""), ("features", "[requirements]\nintent = \"app needs tool/x\"\n"), ("dups", ""), ("rust", "")] {
        let (_, r) = read(name, ".", extra);
        for f in &r.findings {
            assert!(!f.what.is_empty() && !f.evidence.is_empty(), "{name}: {f:#?}");
            if f.status == Status::Unsatisfied && f.from.as_str() == "manifest" {
                assert!(f.at.is_some(), "{name}: a manifest finding points at its manifest: {f:#?}");
            }
        }
    }
}
