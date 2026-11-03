// Copyright © 2026 1o. All rights reserved.

//! `replay`: an exact transition of whole files, checked whole before anything is written. The spec reads from TOML
//! and 1o and back; a path has one spelling and stays inside the tree; a mode is three octal digits; the rule
//! (`change::replay_read`) holds a transition whose every file is as declared before and whose every payload is the
//! bytes declared after, and refuses each way one is not; a governed preflight keeps each check, binds the spec, the
//! code and the lock as it does for every operation, and the writer's `authorizing` finds its receipt.

use custodian_kit::api::{Resolving, Scope, Workspace};
use custodian_kit::change::{self, ChangeSpec, Operation, ReplayAct, ReplayFile};
use custodian_kit::preflight;
use custodian_kit::runs::{Ctx, Runner};
use custodian_kit::store::{Mode, Store};
use std::path::{Path, PathBuf};

fn fresh(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("custodian-kit-replay-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn sha(bytes: &[u8]) -> String {
    change::sha256_hex(bytes)
}

/// A file written from `payload` (kept as `payloads/<name>` beside the spec), over a file whose bytes are `before`
/// (`None`: absent).
fn writes(path: &str, before: Option<&[u8]>, payload: &str, after: &[u8]) -> ReplayFile {
    ReplayFile { path: path.into(), before_sha256: before.map(sha), absent_before: before.is_none(), after_sha256: Some(sha(after)), payload: Some(format!("payloads/{payload}")), ..ReplayFile::default() }
}

fn deletes(path: &str, before: &[u8]) -> ReplayFile {
    ReplayFile { path: path.into(), before_sha256: Some(sha(before)), delete: true, ..ReplayFile::default() }
}

fn replay(files: Vec<ReplayFile>) -> ChangeSpec {
    ChangeSpec { name: "state-2".into(), objective: "the kept state 2".into(), operation: Some(Operation::Replay), target: "*".into(), replay: files, ..ChangeSpec::default() }
}

/// A tree and a spec folder beside it with its payloads.
fn tree(tag: &str) -> (PathBuf, PathBuf) {
    let root = fresh(tag);
    let specs = fresh(&format!("{tag}-spec"));
    (root, specs)
}

const SPEC: &str = r#"name = "state-2"
objective = "the kept state 2"
operation = "replay"
target = "*"

[[replay]]
path = "src/lib.rs"
before_sha256 = "BEFORE"
after_sha256 = "AFTER"
payload = "payloads/lib.rs"

[[replay]]
path = "scripts/run.sh"
absent_before = true
after_sha256 = "AFTER"
payload = "payloads/run.sh"
mode = "755"

[[replay]]
path = "notes.txt"
before_sha256 = "BEFORE"
delete = true
"#;

#[test]
fn a_replay_spec_reads_from_toml_and_1o_and_round_trips() {
    let text = SPEC.replace("BEFORE", &sha(b"a")).replace("AFTER", &sha(b"b"));
    let s = ChangeSpec::from_toml(&text).unwrap();
    assert_eq!((s.op(), s.replay.len()), (Operation::Replay, 3));
    assert_eq!(s.replay[1], ReplayFile { path: "scripts/run.sh".into(), absent_before: true, after_sha256: Some(sha(b"b")), payload: Some("payloads/run.sh".into()), mode: Some("755".into()), ..ReplayFile::default() });
    assert!(s.replay[2].delete && s.replay[2].payload.is_none());
    // serde, TOML and 1o: the same spec back, the same digest
    let json: ChangeSpec = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(json, s);
    assert_eq!(ChangeSpec::from_toml(&s.to_toml().unwrap()).unwrap(), s);
    let oneo = s.to_1o().unwrap();
    assert!(oneo.contains("replay {") && oneo.contains("absent_before true"), "{oneo}");
    assert_eq!(ChangeSpec::from_1o(&oneo).unwrap(), s);
    assert_eq!(ChangeSpec::from_1o(&oneo).unwrap().digest(), s.digest());
    // the operation needs its files
    let e = ChangeSpec::from_toml("name = \"x\"\nobjective = \"o\"\noperation = \"replay\"\ntarget = \"*\"\n").unwrap_err().to_string();
    assert!(e.contains("replay needs replay"), "{e}");
    let e = ChangeSpec::from_1o("change { name \"x\" objective \"o\" operation \"replay\" target \"*\" }").unwrap_err().to_string();
    assert!(e.contains("Missing required key 'replay' for operation 'replay'"), "{e}");
    // a key the file block does not have is refused
    assert!(ChangeSpec::from_toml(&text.replace("mode = \"755\"", "mode = \"755\"\nowner = \"root\"")).is_err());
    // a spec with no replay keeps the digest it had: the field is left out when empty
    let other = ChangeSpec { replay: Vec::new(), operation: Some(Operation::Rename), ..s.clone() };
    assert!(!serde_json::to_string(&other).unwrap().contains("replay"));
}

#[test]
fn a_replay_path_has_one_spelling_inside_the_root() {
    for (raw, one) in [("src/lib.rs", "src/lib.rs"), ("./src//lib.rs", "src/lib.rs"), ("a/./b/", "a/b"), ("run.sh", "run.sh")] {
        assert_eq!(change::replay_path(raw).as_deref(), Ok(one), "{raw}");
    }
    for raw in ["", ".", "/etc/passwd", "../x", "a/../../x", "a/..", "a\\b", "C:/x", "c:x", "a\u{0}b"] {
        assert!(change::replay_path(raw).is_err(), "{raw:?} is refused");
    }
    for (raw, bits) in [("644", 0o644), ("755", 0o755), ("0755", 0o755), ("600", 0o600), ("000", 0)] {
        assert_eq!(change::replay_mode(raw), Ok(bits), "{raw}");
    }
    for raw in ["", "64", "999", "rwx", "1755", "07777", "0o755", "+x", "7555"] {
        assert!(change::replay_mode(raw).is_err(), "{raw:?} is refused");
    }
}

fn mode(root: &Path, rel: &str) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(root.join(rel)).unwrap().permissions().mode() & 0o777
}

fn set_mode(root: &Path, rel: &str, bits: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root.join(rel), std::fs::Permissions::from_mode(bits)).unwrap();
}

#[test]
fn a_whole_transition_holds_and_its_steps_say_each_file_after() {
    let (root, specs) = tree("holds");
    write(&root, "src/lib.rs", b"pub fn a() {}\n");
    write(&root, "notes.txt", b"old notes\n");
    write(&root, "bin/tool", b"#!/bin/sh\necho 1\n");
    set_mode(&root, "bin/tool", 0o755);
    write(&specs, "payloads/lib.rs", b"pub fn b() {}\n");
    write(&specs, "payloads/run.sh", b"#!/bin/sh\nexit 0\n");
    write(&specs, "payloads/data.bin", &[0, 159, 146, 150, 255]);
    write(&specs, "payloads/tool", b"#!/bin/sh\necho 2\n");
    let spec = replay(vec![
        writes("src/lib.rs", Some(b"pub fn a() {}\n"), "lib.rs", b"pub fn b() {}\n"),
        ReplayFile { mode: Some("755".into()), ..writes("scripts/run.sh", None, "run.sh", b"#!/bin/sh\nexit 0\n") },
        writes("assets/data.bin", None, "data.bin", &[0, 159, 146, 150, 255]),
        writes("bin/tool", Some(b"#!/bin/sh\necho 1\n"), "tool", b"#!/bin/sh\necho 2\n"),
        deletes("notes.txt", b"old notes\n"),
    ]);
    let r = change::replay_read(&root, &specs, &spec);
    assert!(r.holds(), "{:?}", r.failures());
    let got: Vec<(&str, Option<&str>, Option<u32>)> = r.steps.iter().map(|s| (s.path.as_str(), s.after(), s.mode())).collect();
    assert_eq!(
        got,
        [
            ("assets/data.bin", Some(sha(&[0, 159, 146, 150, 255]).as_str()), Some(0o644)),
            ("bin/tool", Some(sha(b"#!/bin/sh\necho 2\n").as_str()), Some(0o755)),
            ("notes.txt", None, None),
            ("scripts/run.sh", Some(sha(b"#!/bin/sh\nexit 0\n").as_str()), Some(0o755)),
            ("src/lib.rs", Some(sha(b"pub fn b() {}\n").as_str()), Some(0o644)),
        ],
        "path order; a declared mode, a replaced file's own, a new file's 644"
    );
    assert_eq!(r.steps[0].before, None);
    assert_eq!(r.steps[2].act, ReplayAct::Delete);
    assert!(matches!(&r.steps[0].act, ReplayAct::Write { bytes, .. } if bytes == &[0, 159, 146, 150, 255]), "the payload's bytes, exactly");
    // reading writes nothing
    assert_eq!(std::fs::read(root.join("src/lib.rs")).unwrap(), b"pub fn a() {}\n");
    assert!(!root.join("scripts").exists() && root.join("notes.txt").exists());
    assert_eq!(mode(&root, "bin/tool"), 0o755);
}

/// A file replaced with no `mode` keeps every bit of its own: set-user-id, set-group-id and sticky with the nine.
#[test]
fn a_replaced_file_with_no_mode_keeps_its_set_id_and_sticky_bits() {
    let (root, specs) = tree("setid");
    write(&root, "bin/tool", b"old");
    write(&root, "bin/flag", b"old");
    set_mode(&root, "bin/tool", 0o4755);
    set_mode(&root, "bin/flag", 0o1640);
    write(&specs, "payloads/new", b"new");
    let r = change::replay_read(&root, &specs, &replay(vec![writes("bin/tool", Some(b"old"), "new", b"new"), writes("bin/flag", Some(b"old"), "new", b"new")]));
    assert!(r.holds(), "{:?}", r.failures());
    assert_eq!(r.steps.iter().map(|s| (s.path.as_str(), s.mode())).collect::<Vec<_>>(), [("bin/flag", Some(0o1640)), ("bin/tool", Some(0o4755))]);
}

/// The one check that fails (its `what`) for a replay of `files` over a tree with `src/lib.rs` = "a" and payload
/// `payloads/b` = "b"; the rest hold, and no step is given.
fn fails(tag: &str, files: Vec<ReplayFile>, setup: impl FnOnce(&Path, &Path)) -> Vec<String> {
    let (root, specs) = tree(tag);
    write(&root, "src/lib.rs", b"a");
    write(&specs, "payloads/b", b"b");
    setup(&root, &specs);
    let r = change::replay_read(&root, &specs, &replay(files));
    assert!(!r.holds() && r.steps.is_empty(), "{tag}: {:?}", r.checks);
    assert_eq!(std::fs::read(root.join("src/lib.rs")).unwrap(), b"a", "{tag}: nothing written");
    r.failures()
}

fn one(failures: Vec<String>, what: &str) {
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].contains(what), "{what}: {failures:?}");
}

#[test]
fn each_way_a_transition_does_not_hold_is_refused() {
    let w = |path: &str, before: Option<&[u8]>| writes(path, before, "b", b"b");
    // the right before hash holds: the rule reads exactly what is there
    let (root, specs) = tree("right");
    write(&root, "src/lib.rs", b"a");
    write(&specs, "payloads/b", b"b");
    assert!(change::replay_read(&root, &specs, &replay(vec![w("src/lib.rs", Some(b"a"))])).holds());
    one(fails("wrong-before", vec![w("src/lib.rs", Some(b"x"))], |_, _| {}), &format!("src/lib.rs: as declared before: sha256 {}, declared {}", sha(b"a"), sha(b"x")));
    one(fails("absent-but-there", vec![w("src/lib.rs", None)], |_, _| {}), &format!("src/lib.rs: as declared before: there (sha256 {}), declared absent", sha(b"a")));
    one(fails("there-but-absent", vec![w("src/new.rs", Some(b"a"))], |_, _| {}), "src/new.rs: as declared before: absent, declared sha256");
    one(fails("no-payload", vec![ReplayFile { payload: Some("payloads/missing".into()), ..w("src/lib.rs", Some(b"a")) }], |_, _| {}), "src/lib.rs: its payload holds the bytes after");
    one(fails("payload-hash", vec![ReplayFile { after_sha256: Some(sha(b"c")), ..w("src/lib.rs", Some(b"a")) }], |_, _| {}), &format!("payloads/b: sha256 {}, declared {}", sha(b"b"), sha(b"c")));
    one(fails("absolute", vec![w("/etc/hosts", Some(b"a"))], |_, _| {}), "is absolute");
    one(fails("escape", vec![w("src/../../x", None)], |_, _| {}), "climbs out of the root");
    one(fails("payload-escape", vec![ReplayFile { payload: Some("../b".into()), ..w("src/lib.rs", Some(b"a")) }], |_, _| {}), "climbs out of the root");
    one(fails("duplicate", vec![w("src/lib.rs", Some(b"a")), w("./src/lib.rs", Some(b"a"))], |_, _| {}), "src/lib.rs: named once: named twice (written, then written)");
    one(fails("write-delete", vec![w("src/lib.rs", Some(b"a")), deletes("src/lib.rs", b"a")], |_, _| {}), "src/lib.rs: named once: named twice (written, then deleted)");
    one(fails("case", vec![w("src/new.rs", None), w("SRC/NEW.rs", None)], |_, _| {}), "src/new.rs and SRC/NEW.rs are one file where case is ignored");
    one(fails("mode", vec![ReplayFile { mode: Some("8755".into()), ..w("src/lib.rs", Some(b"a")) }], |_, _| {}), "src/lib.rs: its mode: mode `8755` is not three octal digits");
    one(fails("delete-mode", vec![ReplayFile { mode: Some("644".into()), ..deletes("src/lib.rs", b"a") }], |_, _| {}), "delete with a mode");
    one(fails("delete-payload", vec![ReplayFile { payload: Some("payloads/b".into()), ..deletes("src/lib.rs", b"a") }], |_, _| {}), "delete with after_sha256 or a payload");
    one(fails("delete-absent", vec![ReplayFile { before_sha256: None, absent_before: true, ..deletes("src/gone.rs", b"a") }], |_, _| {}), "delete of a file absent before");
    one(fails("two-befores", vec![ReplayFile { absent_before: true, ..w("src/lib.rs", Some(b"a")) }], |_, _| {}), "before_sha256 and absent_before both");
    one(fails("no-before", vec![ReplayFile { absent_before: false, ..w("src/new.rs", None) }], |_, _| {}), "neither before_sha256 nor absent_before");
    one(fails("no-after", vec![ReplayFile { after_sha256: None, payload: None, ..w("src/lib.rs", Some(b"a")) }], |_, _| {}), "neither after_sha256 nor delete");
    one(fails("upper-hex", vec![ReplayFile { before_sha256: Some(sha(b"a").to_uppercase()), ..w("src/lib.rs", Some(b"a")) }], |_, _| {}), "is not 64 lowercase hex digits");
    one(fails("governance", vec![w(".custodian/data/receipts/1.json", None)], |_, _| {}), "where the tree digest does not read");
    one(fails("folder-of-another", vec![w("gen/out", None), w("gen/out/inner", None)], |_, _| {}), "gen/out: not a folder of another file named: gen/out/inner is under it");
    one(fails("under-a-file", vec![w("src/lib.rs/inner", None)], |_, _| {}), "src/lib.rs is a file, so nothing can be under it");
    one(fails("is-a-folder", vec![w("src", Some(b"a"))], |_, _| {}), "src is not a file");
    one(
        fails("through-a-link", vec![w("out/escaped.txt", None)], |root, _| {
            let outside = fresh("outside");
            std::os::unix::fs::symlink(&outside, root.join("out")).unwrap();
        }),
        "reaches",
    );
    // a link inside the root to a folder the tree digest leaves out: where the path reaches on disk is refused
    one(
        fails("link-to-git", vec![w("g/hooks/post-checkout", None)], |root, _| {
            std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
            std::os::unix::fs::symlink(root.join(".git"), root.join("g")).unwrap();
        }),
        "(on disk .git",
    );
    // the left-out folders and root files in any case (on a disk that ignores case, `.GIT` is `.git`)
    for (tag, path) in [("git-case", ".GIT/hooks/post-checkout"), ("custodian-case", ".Custodian/state/j.txt"), ("target-case", "Target/x"), ("oneo-case", "OneO.Project.JSON"), ("deep-case", "crates/a/TARGET/debug/x")] {
        one(fails(tag, vec![w(path, None)], |_, _| {}), "where the tree digest does not read");
    }
    // a file named is never a folder of another, whatever the case
    one(fails("folder-case", vec![w("Gen/out/x", None), w("gen/OUT", None)], |_, _| {}), "gen/OUT: not a folder of another file named: Gen/out/x is under it");
    one(fails("no-files", Vec::new(), |_, _| {}), "the replay names its files");
}

// ---------------------------------------------------------------------------
// the preflight

fn governed(tag: &str) -> (Store, Ctx) {
    let root = fresh(tag);
    write(&root, "Cargo.toml", b"[package]\nname = \"lab\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    write(&root, "src/lib.rs", b"pub fn a() {}\n");
    write(&root, "notes.txt", b"old notes\n");
    let (store, _) = Store::init(&root, Mode::Agent).unwrap();
    custodian_kit::exec::lock_crate(&store.root).unwrap();
    let ctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: PathBuf::from("custodian") };
    (store, ctx)
}

/// The spec in 1o in the project's change specs, its payloads beside it.
fn spec_file(root: &Path, spec: &ChangeSpec, payloads: &[(&str, &[u8])]) -> PathBuf {
    let dir = root.join(".custodian/data/specs/changes");
    for (name, bytes) in payloads {
        write(&dir, &format!("payloads/{name}"), bytes);
    }
    let p = dir.join(custodian_kit::oneo::Doc::Change.file_name(&spec.name));
    write(&dir, p.file_name().unwrap().to_str().unwrap(), spec.to_1o().unwrap().as_bytes());
    p
}

fn code_digest(root: &Path) -> String {
    custodian_kit::code_digest(root, &Scope { tests: true, ..Default::default() }.scan_options()).unwrap()
}

#[test]
fn a_governed_preflight_of_a_replay_keeps_each_check_and_binds_the_spec_the_code_and_the_lock() {
    let (store, ctx) = governed("preflight");
    let spec = replay(vec![
        writes("src/lib.rs", Some(b"pub fn a() {}\n"), "lib.rs", b"pub fn b() {}\n"),
        ReplayFile { mode: Some("755".into()), ..writes("scripts/run.sh", None, "run.sh", b"#!/bin/sh\n") },
        deletes("notes.txt", b"old notes\n"),
    ]);
    let file = spec_file(&store.root, &spec, &[("lib.rs", b"pub fn b() {}\n"), ("run.sh", b"#!/bin/sh\n")]);
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    let ran = preflight::run_preflight(&ctx, &file, None, Runner::InProcess(&w)).unwrap();
    let p = &ran.preflight;
    assert!(p.authorized, "{:?} {:?}", p.why, p.preconditions);
    assert_eq!(p.operation, "replay");
    assert_eq!(p.write_set, ["notes.txt", "scripts/run.sh", "src/lib.rs"], "the files the replay names");
    assert_eq!(p.authorized_scope, ["the target's own file notes.txt", "the target's own file scripts/run.sh", "the target's own file src/lib.rs"]);
    assert!(p.landing.as_ref().is_some_and(|l| l.at == change::At::Files { count: 3 } && l.file == "notes.txt"), "{:?}", p.landing);
    for what in ["src/lib.rs: as declared before", "src/lib.rs: its payload holds the bytes after", "scripts/run.sh: its mode", "notes.txt: one state after", preflight::LOCK_CHECK] {
        assert!(p.preconditions.iter().any(|c| c.what == what && c.holds), "{what}: {:?}", p.preconditions);
    }
    // the binding every operation has: the spec's digest, the code's, the lock's
    assert_eq!((p.spec_digest.as_str(), p.code_digest.clone()), (spec.digest().as_str(), code_digest(&store.root)));
    assert_eq!(p.lock.as_ref().map(|l| l.path.as_str()), Some("Cargo.lock"));
    let r = ran.receipt.clone().unwrap();
    assert_eq!((r.command.as_str(), r.reason.as_str(), r.spec_hash.as_deref()), ("preflight", preflight::AUTHORIZED, Some(spec.digest().as_str())));
    // the writer's question: the receipt that authorized this spec over this code
    assert_eq!(preflight::authorizing(&store, &spec, &code_digest(&store.root), None).unwrap().seq, r.seq);
    assert_eq!(preflight::authorizing(&store, &spec, &code_digest(&store.root), Some(r.seq)).unwrap().seq, r.seq);
    // the spec altered (another digest after): the receipt covers it no more
    let mut altered = spec.clone();
    altered.replay[0].after_sha256 = Some(sha(b"other"));
    assert!(preflight::authorizing(&store, &altered, &code_digest(&store.root), Some(r.seq)).is_err());
    // the preflight wrote nothing
    assert_eq!(std::fs::read(store.root.join("src/lib.rs")).unwrap(), b"pub fn a() {}\n");
    assert!(store.root.join("notes.txt").exists() && !store.root.join("scripts").exists());
}

#[test]
fn a_replay_that_does_not_hold_or_writes_outside_its_scope_is_not_authorized() {
    let (store, ctx) = governed("refused");
    let w = Workspace::open_with(&store.root, &Scope::default(), Resolving::Free).unwrap();
    // entry 2 of 2 is not as declared before: not authorized, and the check says so
    let spec = replay(vec![writes("src/lib.rs", Some(b"pub fn a() {}\n"), "lib.rs", b"pub fn b() {}\n"), deletes("notes.txt", b"other notes\n")]);
    let file = spec_file(&store.root, &spec, &[("lib.rs", b"pub fn b() {}\n")]);
    let p = preflight::run_preflight(&ctx, &file, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized && p.why.iter().any(|y| y.starts_with("notes.txt: as declared before: sha256 ")), "{:?}", p.why);
    assert_eq!(p.reason(), preflight::NOT_AUTHORIZED);
    // a scope that does not hold every file: the file outside is a scope expansion asked for
    let mut scoped = replay(vec![writes("src/lib.rs", Some(b"pub fn a() {}\n"), "lib.rs", b"pub fn b() {}\n"), writes("tools/x.sh", None, "lib.rs", b"pub fn b() {}\n")]);
    scoped.name = "scoped".into();
    scoped.scope.write = vec!["crate:lab/src/lib.rs".into()];
    let file = spec_file(&store.root, &scoped, &[("lib.rs", b"pub fn b() {}\n")]);
    let p = preflight::run_preflight(&ctx, &file, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized, "{:?}", p.preconditions);
    assert_eq!(p.outside, ["tools/x.sh"]);
    // a target other than the workspace does not resolve
    let mut crate_target = replay(vec![writes("src/lib.rs", Some(b"pub fn a() {}\n"), "lib.rs", b"pub fn b() {}\n")]);
    crate_target.name = "crate-target".into();
    crate_target.target = "crate:lab".into();
    let file = spec_file(&store.root, &crate_target, &[("lib.rs", b"pub fn b() {}\n")]);
    let p = preflight::run_preflight(&ctx, &file, None, Runner::InProcess(&w)).unwrap().preflight;
    assert!(!p.authorized && p.why.iter().any(|y| y.contains("a replay's target is `*`")), "{:?}", p.why);
    assert_eq!(std::fs::read(store.root.join("src/lib.rs")).unwrap(), b"pub fn a() {}\n", "nothing written");
}
