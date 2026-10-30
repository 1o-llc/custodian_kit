// Copyright © 2026 1o. All rights reserved.

//! The laboratory-control repairs (2026-10-30), each on a workspace made here: a landing names its file relative to
//! the root however the root is spelled; a name without its generics is refused naming the scan's id; a raw body
//! adds no trailing whitespace; a governed preflight needs a lock and binds its digest; a write goes only on the
//! preflight that covered this exact spec over this exact code; a delta counts only what the candidate adds; an
//! experiment's provenance names its specimen, the lock put on the copy, and its measurements.

use custodian_kit::api::{Resolving, Scope, Workspace};
use custodian_kit::change::{self, ChangeSpec};
use custodian_kit::preflight;
use custodian_kit::runs::{Ctx, Runner};
use custodian_kit::store::{Mode, Store};
use std::path::{Path, PathBuf};

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn fresh(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("custodian-kit-lab-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

const LIB: &str = "pub struct Registry<const WORKS: usize, const SUBSCRIBERS: usize> {\n    slots: [Option<u64>; WORKS],\n}\n\nimpl<const WORKS: usize, const SUBSCRIBERS: usize> Registry<WORKS, SUBSCRIBERS> {\n    pub fn first_free(&self) -> Option<usize> {\n        self.slots.iter().position(|s| s.is_none())\n    }\n}\n";

/// One crate, `lab`, with a generic registry; its root as the temporary directory spells it (on macOS `/var/...`,
/// which klepto reads as `/private/var/...`).
fn crate_at(tag: &str) -> PathBuf {
    let root = fresh(tag);
    write(&root, "Cargo.toml", "[package]\nname = \"lab\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    write(&root, "src/lib.rs", LIB);
    root
}

fn spec_of(text: &str) -> ChangeSpec {
    toml::from_str(text).unwrap()
}

const BODY: &str = "name = \"first-free\"\nobjective = \"o\"\noperation = \"replace_body\"\ntarget = \"TARGET\"\n\n[function]\nname = \"first_free\"\n\n[[function.body]]\npiece = \"raw\"\ncode = '''\n\nlet mut found = None;\n\nfor (index, slot) in self.slots.iter().enumerate() {\n    if slot.is_none() && found.is_none() {\n        found = Some(index);\n    }\n}\nfound\n\n'''\n";

const ID: &str = "lab::Registry<WORKS,SUBSCRIBERS>::first_free";

/// CP-1's cause: the landing's file was the specimen's absolute path whenever the root was spelled otherwise than
/// klepto spells it, and the proof's text fact then read the specimen, not the proof's copy.
#[test]
fn a_landing_names_its_file_relative_to_the_root_however_the_root_is_spelled() {
    let root = crate_at("rel");
    let canonical = root.canonicalize().unwrap();
    let w = Workspace::open_with(&canonical, &Scope::default(), Resolving::Free).unwrap();
    let spec = spec_of(&BODY.replace("TARGET", ID));
    // a symlink to the root: another spelling of the same directory
    let link = fresh("rel-link").join("ws");
    std::os::unix::fs::symlink(&canonical, &link).unwrap();
    for spelled in [root.clone(), canonical.clone(), link.clone(), canonical.join(".")] {
        let land = change::landing(w.klepto(), &spelled, &spec).unwrap();
        assert_eq!(land.file, "src/lib.rs", "root spelled {}", spelled.display());
    }
}

/// CP-2: a target names one item as the scan writes its id; a name that differs only in generics or spacing is
/// refused, and the refusal names the id to copy.
#[test]
fn a_name_without_its_generics_is_refused_naming_the_scans_id() {
    let root = crate_at("ids");
    let w = Workspace::open_with(&root, &Scope::default(), Resolving::Free).unwrap();
    assert!(change::landing(w.klepto(), &root, &spec_of(&BODY.replace("TARGET", ID))).is_ok());
    for near in ["lab::Registry::first_free", "lab::Registry<WORKS, SUBSCRIBERS>::first_free"] {
        let e = change::landing(w.klepto(), &root, &spec_of(&BODY.replace("TARGET", near))).unwrap_err();
        assert!(e.contains(&format!("`{near}` names no item of the scan")) && e.contains(&format!("the scan's id for it is `{ID}`")), "{e}");
    }
    assert_eq!(change::ids_like(w.klepto(), "lab::Registry::first_free"), [ID]);
    assert!(change::ids_like(w.klepto(), "lab::Registry::other").is_empty());
}

/// A raw body's blank lines stay empty and the block opens and closes as rustfmt leaves one: the writer itself adds no
/// finding a format check would hold against the candidate (FIX7's diff had `+        ` lines).
#[test]
fn a_raw_body_leaves_blank_lines_empty() {
    let spec = spec_of(&BODY.replace("TARGET", ID));
    let body = change::render_body(spec.function.as_ref().unwrap(), "    ").unwrap();
    assert!(body.lines().all(|l| l == l.trim_end()), "trailing whitespace:\n{body:?}");
    assert!(body.starts_with("{\n        let mut found = None;\n\n        for "), "{body:?}");
    assert!(body.ends_with("        found\n    }"), "{body:?}");
}

fn governed(tag: &str) -> (Store, Ctx, PathBuf) {
    let root = crate_at(tag);
    let (store, _) = Store::init(&root, Mode::Agent).unwrap();
    let ctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: PathBuf::from("custodian") };
    (store, ctx, root)
}

fn spec_file(root: &Path, name: &str, text: &str) -> PathBuf {
    let p = root.join(".custodian/data/specs/changes").join(format!("{name}.toml"));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p
}

fn code_digest(root: &Path) -> String {
    custodian_kit::code_digest(root, &Scope { tests: true, ..Default::default() }.scan_options()).unwrap()
}

/// CUST-3: a project custodian keeps does not authorize a change over a workspace with no lock (the proof would build
/// on whatever cargo resolves that day), and says exactly what to do; with a lock on the copy, the preflight records
/// the lock's digest and binds it through the code digest.
#[test]
fn a_governed_preflight_needs_a_lock_and_binds_its_digest() {
    let (store, ctx, root) = governed("lock");
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let file = spec_file(&store.root, "first-free", &BODY.replace("TARGET", ID));
    let p = preflight::run_preflight(&ctx, &file, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized && p.lock.is_none(), "{:?}", p.preconditions);
    let c = p.preconditions.iter().find(|c| c.what == preflight::LOCK_CHECK).unwrap();
    assert!(!c.holds && c.detail.starts_with("[CUST0046] no Cargo.lock in the workspace at ") && c.detail.contains("never on the baseline"), "{}", c.detail);
    assert!(!root.join("Cargo.lock").exists(), "the preflight writes no lock");
    let before = code_digest(&store.root);
    custodian_kit::exec::lock_crate(&store.root).unwrap();
    let after = code_digest(&store.root);
    assert_ne!(before, after, "the lock is part of the code digest");
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let p = preflight::run_preflight(&ctx, &file, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(p.authorized, "{:?} {:?}", p.why, p.preconditions);
    let lock = p.lock.clone().unwrap();
    assert_eq!(lock.path, "Cargo.lock");
    assert_eq!(lock.digest, custodian_kit::digest::tagged(std::fs::read(root.join("Cargo.lock")).unwrap()));
    assert_eq!(p.code_digest, after);
    assert!(p.markdown().contains(&format!("- dependency state: `Cargo.lock` `{}`", lock.digest)), "{}", p.markdown());
}

/// INT-1: a write goes on the preflight that authorized this exact spec over this exact code, and on no other: the
/// receipt named, or the newest that covers both. Authorize A; alter the spec, or the code; A's receipt covers neither.
#[test]
fn a_write_goes_only_on_the_preflight_that_covered_this_spec_over_this_code() {
    let (store, ctx, root) = governed("auth");
    custodian_kit::exec::lock_crate(&store.root).unwrap();
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let text = BODY.replace("TARGET", ID);
    let file = spec_file(&store.root, "first-free", &text);
    let ran = preflight::run_preflight(&ctx, &file, None, Runner::InProcess(&w)).unwrap();
    assert!(ran.preflight.authorized, "{:?}", ran.preflight.why);
    let a = ran.receipt.unwrap().seq;
    let spec = ChangeSpec::from_path(&file).unwrap();
    let code = code_digest(&store.root);
    // the exact spec over the exact code: by its number, and as the newest
    assert_eq!(preflight::authorizing(&store, &spec, &code, Some(a)).unwrap().seq, a);
    assert_eq!(preflight::authorizing(&store, &spec, &code, None).unwrap().seq, a);
    // the spec altered: A covers the old one
    let altered: ChangeSpec = toml::from_str(&text.replace("found = Some(index);", "found = Some(index + 0);")).unwrap();
    let e = preflight::authorizing(&store, &altered, &code, Some(a)).unwrap_err().to_string();
    assert!(e.starts_with("[CUST0047]") && e.contains(&format!("preflight #{a} authorized another version of the spec")), "{e}");
    assert!(preflight::authorizing(&store, &altered, &code, None).unwrap_err().to_string().contains("the spec changed since preflight"));
    // the code altered: A read other code
    write(&root, "src/lib.rs", &LIB.replace("pub struct", "/// moved\npub struct"));
    let moved = code_digest(&store.root);
    let e = preflight::authorizing(&store, &spec, &moved, Some(a)).unwrap_err().to_string();
    assert!(e.starts_with("[CUST0047]") && e.contains("the code moved since"), "{e}");
    assert!(preflight::authorizing(&store, &spec, &moved, None).unwrap_err().to_string().contains("the code moved since preflight"));
    // a number that is no preflight of the spec, and a preflight that refused
    assert!(preflight::authorizing(&store, &spec, &code, Some(a + 100)).unwrap_err().to_string().contains("is no preflight of this spec"));
    let refused = spec_file(&store.root, "nothing", &BODY.replace("TARGET", "lab::Registry::nope").replace("first-free", "nothing"));
    let r = preflight::run_preflight(&ctx, &refused, None, Runner::InProcess(&w)).unwrap();
    let nothing = ChangeSpec::from_path(&refused).unwrap();
    let e = preflight::authorizing(&store, &nothing, &code, Some(r.receipt.unwrap().seq)).unwrap_err().to_string();
    assert!(e.contains("did not authorize it"), "{e}");
}

fn copy(from: &Path, to: &Path) {
    for e in walkdir::WalkDir::new(from) {
        let e = e.unwrap();
        let rel = e.path().strip_prefix(from).unwrap();
        if e.file_type().is_dir() {
            std::fs::create_dir_all(to.join(rel)).unwrap();
        } else {
            std::fs::copy(e.path(), to.join(rel)).unwrap();
        }
    }
}

/// CUST-2: the base's own format finding is inherited; the candidate is held only to the findings it adds. The base is
/// never written.
#[test]
fn a_delta_counts_only_what_the_candidate_adds() {
    let base = crate_at("delta-base");
    write(&base, "src/lib.rs", &format!("{LIB}\npub fn old( ) -> u8 {{ 1 }}\n"));
    let before = custodian_kit::provenance::tree_files(&base).unwrap();
    let same = fresh("delta-same");
    copy(&base, &same);
    let d = custodian_kit::delta::delta(custodian_kit::delta::Kind::Fmt, &base, &same).unwrap();
    assert!(d.clean() && d.inherited.len() == 1 && d.base_findings == 1, "{d:#?}");
    // a candidate that adds one finding and keeps the inherited one
    let more = fresh("delta-more");
    copy(&base, &more);
    write(&more, "src/extra.rs", "pub fn added( ) -> u8 { 2 }\n");
    write(&more, "src/lib.rs", &format!("pub mod extra;\n{LIB}\npub fn old( ) -> u8 {{ 1 }}\n"));
    let d = custodian_kit::delta::delta(custodian_kit::delta::Kind::Fmt, &base, &more).unwrap();
    assert_eq!((d.inherited.len(), d.introduced.len()), (1, 1), "{d:#?}");
    assert_eq!(d.introduced[0].file, "src/extra.rs");
    assert_eq!(custodian_kit::provenance::tree_files(&base).unwrap(), before, "the base is read, never written");
}

/// CUST-1: an experiment's record names its specimen, the files the copy differs by (the lock put on it), and each
/// measurement with its digest; a measurement is kept in the job's folder.
#[test]
fn an_experiment_names_its_specimen_the_lock_on_its_copy_and_its_measurements() {
    let specimen = crate_at("prov-specimen");
    let copy_root = fresh("prov-copy");
    copy(&specimen, &copy_root);
    let (store, _) = Store::init(&copy_root, Mode::Agent).unwrap();
    custodian_kit::exec::lock_crate(&store.root).unwrap();
    let folder = store.root.join(".custodian/state/jobs/running/exp");
    std::fs::create_dir_all(&folder).unwrap();
    let p = custodian_kit::provenance::open(&folder, &store.root, "job-x", &specimen).unwrap();
    let sp = p.specimen.clone().unwrap();
    assert_eq!((sp.form.as_str(), sp.state.files), ("folder", 2));
    assert!(sp.state.lock.is_none() && p.open.lock.is_some(), "{p:#?}");
    assert_eq!(p.open_vs_specimen, ["+Cargo.lock"], "the copy differs by the lock put on it, and nothing else (custodian's own files are left out)");
    let m = std::env::temp_dir().join(format!("lab-measure-{}.json", std::process::id()));
    std::fs::write(&m, "{\"ns\": 41}\n").unwrap();
    let kept = custodian_kit::provenance::measure(&folder, &store.root, &m, "bench run 1").unwrap();
    assert_eq!(kept.label, "bench-run-1");
    assert_eq!(kept.digest, custodian_kit::digest::tagged(b"{\"ns\": 41}\n"));
    assert_eq!(kept.file, "data/measurements/bench-run-1/".to_string() + m.file_name().unwrap().to_str().unwrap(), "relative to the job's folder");
    assert_eq!(std::fs::read(folder.join(&kept.file)).unwrap(), b"{\"ns\": 41}\n");
    let read = custodian_kit::provenance::read(&folder).unwrap();
    assert_eq!(read.measurements, [kept]);
    assert!(custodian_kit::provenance::measure(&store.root.join("nowhere"), &store.root, &m, "x").is_err(), "a job that is no experiment keeps none");
}
