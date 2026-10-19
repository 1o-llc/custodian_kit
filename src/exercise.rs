// Copyright © 2026 1o. All rights reserved.

//! The exercise: custodian trying its own change path on real code, every
//! result a receipt (job kind `exercise`).
//!
//! For every operation of the change vocabulary ([`Operation`]) and every kind
//! of unit it applies to ([`Unit`], [`units_of`]: one table), N targets are
//! picked from the project's index (seeded: the same seed over the same code
//! picks the same targets), and each pick is one case. A case never touches
//! the project's tree: the workspace is copied into the job's own data folder
//! (`data/exercise/cases/<n>-<operation>/tree`, without `target/`, `.git/`,
//! `.custodian/`, `.passenger/`; an empty `[workspace]` added to the copy's
//! manifest when it has none, so cargo stops there), a `.custodian` folder is
//! made in the copy, and the bounded chain runs on it:
//!
//! 1. the change spec, written from the target (the smallest change the
//!    operation can make there: a field `exercise_field_<n>: u8` with its
//!    default, a name with `_exercised_<n>` after it, a body of `todo!`, ...);
//! 2. the preflight (in this process, its receipt in the copy's journal);
//! 3. the transform: code_passenger's `transform --spec S --write` line on the
//!    copy, on that preflight, its receipt in the project's journal under the
//!    job;
//! 4. `cargo check` over the copy (every crate and target, offline), and with
//!    `tests` the landing crate's `cargo test`, each a declared line with its
//!    receipt;
//! 5. the closing check: the copy counted before and after (tree_history's
//!    counts), and every file the case touched must be carried by its
//!    transform record from its digest before to its digest after
//!    ([`crate::transform::coverage`]).
//!
//! Every refusal, crash, compile error, failed test or uncovered file is a
//! finding ([`FindingCode`]), written into the job's task history as a
//! diagnosis record naming the receipt that produced it. The cases run as
//! tasks in one `scope`, a stage each per poll, in a fixed order; their
//! numbers come from one uh-tom-mick counter.
//!
//! With intents (one sentence a line), each intent is also a case: the cheap
//! model's declared line (`claude -p --model` [`MODEL`]) turns it into a change
//! spec, which runs the same chain; a spec it cannot write, or one the
//! preflight refuses, is a finding tagged `model`. With no model on this
//! machine (not on PATH, or it does not answer) the case is `not run: no
//! model`, never a failure of the job.
//!
//! The job's closing receipt carries the summary (`data/exercise/summary.json`
//! and `.md`): cases run and passed, the findings by code, the operation ×
//! unit matrix, the slowest cases and a ranked list of what to fix. A `check`
//! receipt named `exercise` is the job's verification (`check:exercise`):
//! accepted when the exercise found nothing.

use crate::change::{CallSites, ChangeSpec, DependencySpec, FieldSpec, FnSpec, ItemSpec, ModuleSpec, Operation, Param, Piece, VariantSpec};
use crate::exec::{self, Plan, Walked, Watch};
use crate::reading::refusal::ReadingRefusal;
use crate::reading::spec::{vocabulary, Scope};
use crate::runs::{keep, now_utc, Ctx, InJob, Ran, Runner};
use crate::store::{write_whole, Receipt, Store, RECEIPT_SCHEMA};
use crate::task_records::TaskRef;
use crate::Result;
use klepto::{CfgPredicate, Klepto, NodeKind};
use scope::{run_scoped, Cx, ScopedTask, TaskPoll};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use task_history::{DiffDocument, DiffRecordKind};
use uh_tom_mick::AtomicCounter;

pub const EXERCISE_SCHEMA: &str = "custodian.exercise.v1";

/// The job kind.
pub const KIND: &str = "exercise";

/// The cheap model the intents go to.
pub const MODEL: &str = "claude-haiku-4-5-20251001";

/// The exercise's folder in its job's data folder, and the files it keeps there.
pub const DIR: &str = "exercise";
pub const OPTIONS_FILE: &str = "options.json";
pub const SUMMARY_FILE: &str = "summary.json";
pub const SUMMARY_PAGE: &str = "summary.md";

/// The name of the `check` receipt that is the job's verification.
pub const CHECK_NAME: &str = "exercise";

/// What a copy leaves out of the project's tree.
pub const NOT_COPIED: &[&str] = &["target", ".git", crate::store::FOLDER, ".passenger", ".claude"];

vocabulary! {
    /// The kinds of unit an exercise picks its targets from.
    Unit, "unit" {
        Item = "item" : "a module-level declaration that is not a function: a struct, an enum, a trait, a const, a static, a type alias",
        Function = "function" : "a free function, or a method of an inherent impl",
        Impl = "impl" : "a type with an inherent impl block (what a new method or constructor lands inside)",
        Module = "module" : "a module with a file of its own, as its route (crate:<name>/src/<file>.rs)",
        File = "file" : "a crate's root file, as its route (crate:<name>/src/lib.rs)",
        Crate = "crate" : "a crate of the workspace, as its route (crate:<name>)",
    }
}

vocabulary! {
    /// What a case can find.
    FindingCode, "finding" {
        SpecRefused = "spec-refused" : "the change spec written for the case is refused by the spec's own checks",
        PreflightRefused = "preflight-refused" : "the preflight did not authorize the change",
        PreflightFailed = "preflight-failed" : "the preflight gave a refusal instead of a verdict",
        GateRefused = "gate-refused" : "the transform's gate found no authorizing preflight of this spec over this code",
        TransformRefused = "transform-refused" : "code_passenger refused the spec (exit 2)",
        TransformUnproved = "transform-unproved" : "code_passenger did not prove the change: a configuration did not compile, or a postcondition did not hold (exit 1)",
        TransformCrashed = "transform-crashed" : "code_passenger panicked, was killed, or exited with a status it does not declare",
        CompileError = "compile-error" : "cargo check fails on the copy after the transform wrote it",
        TestFailure = "test-failure" : "the landing crate's tests fail after the transform wrote it",
        Uncovered = "uncovered" : "a file the case touched is not carried by its transform record from its digest before to its digest after",
        ModelMiss = "model-miss" : "the cheap model's answer is not a change spec, or names a target that does not resolve",
    }
}

/// Every unit kind an operation applies to: the one table the picks read.
pub fn units_of(op: Operation) -> &'static [Unit] {
    use Operation::*;
    match op {
        MakeStruct | MakeEnum | MakeFn | AddModule => &[Unit::File, Unit::Module, Unit::Crate],
        Extract => &[Unit::File, Unit::Module],
        AddDependency | ChangeDependency => &[Unit::Crate],
        MakeImpl | AddTraitImpl | AddField | AddVariant | AddDerive => &[Unit::Item],
        AddMethod | AddConstructor => &[Unit::Impl, Unit::Item],
        AddAttribute | Gate | Anchor | Rename | Move => &[Unit::Item, Unit::Function],
        ChangeSignature | ReplaceBody | AddTest => &[Unit::Function],
    }
}

pub use word::ExerciseWord;

/// The word, in a module of its own: the derive writes `Result` as big-bad-bool's.
mod word {
    use big_bad_bool::bitpack;

    /// An exercise's options as one word: the operations (bit `i` is `Operation::ALL[i]`), the unit kinds (bit `i`
    /// is `Unit::ALL[i]`), and how it runs; its rules are checked by flaga_solver.
    #[bitpack(storage = u64, version = 1)]
    pub struct ExerciseWord {
        #[bits(22)]
        #[describe("the operations an exercise runs, bit i the vocabulary's i-th")]
        ops: u32,
        #[bits(6)]
        #[describe("the unit kinds an exercise picks targets of, bit i the vocabulary's i-th")]
        units: u8,
        #[bits(4)]
        #[describe("how an exercise runs")]
        #[flags(tests = 0, intents = 1, model = 2, one_crate = 3)]
        #[labels(tests = 1, intents = 2, model = 4, one_crate = 8)]
        #[requires(model => options.intents)]
        options: u8,
    }
}

impl ExerciseWord {
    /// The names of the option flags set, in bit order (a flag's mask is within its field).
    pub fn names(self) -> Vec<&'static str> {
        Self::FLAGS.iter().filter(|f| f.field == "options" && self.options() as u128 & f.mask != 0).map(|f| f.name).collect()
    }

    /// The word's rules, checked by flaga_solver; every broken rule is named.
    pub fn check(self) -> Result<Self> {
        let verdict = flaga_solver::check(&flaga_solver::schema_of!(ExerciseWord), self.raw() as u128);
        if verdict.is_ok() {
            Ok(self)
        } else {
            Err(ReadingRefusal::ExerciseRefused.refuse(format!("{verdict} (word {:#x}: {})", self.raw(), self.names().join(", "))))
        }
    }

    /// The operations the word holds.
    pub fn operations(self) -> Vec<Operation> {
        Operation::ALL.iter().enumerate().filter(|(i, _)| self.ops() & (1 << i) != 0).map(|(_, o)| *o).collect()
    }

    /// The unit kinds the word holds.
    pub fn unit_kinds(self) -> Vec<Unit> {
        Unit::ALL.iter().enumerate().filter(|(i, _)| self.units() & (1 << i) != 0).map(|(_, u)| *u).collect()
    }
}

/// What an exercise was asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    /// The operations (every one when none is named).
    pub ops: Vec<Operation>,
    /// The unit kinds (every one when none is named).
    pub units: Vec<Unit>,
    /// Targets per operation and unit kind.
    pub n: usize,
    /// Run the landing crate's tests after each transform.
    pub tests: bool,
    /// Only this crate's units (`crate:<name>` or its name).
    pub krate: Option<String>,
    /// One intent a line, each a case through the cheap model.
    pub intents: Option<PathBuf>,
    /// The pick's seed.
    pub seed: u64,
    /// The options as one word (its raw value).
    pub word: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options { ops: Operation::ALL.to_vec(), units: Unit::ALL.to_vec(), n: 1, tests: false, krate: None, intents: None, seed: 0, word: 0 }
    }
}

fn list<T: std::str::FromStr<Err = String> + Copy>(words: Option<&str>, all: &[T]) -> Result<Vec<T>> {
    match words.map(str::trim).filter(|w| !w.is_empty()) {
        None => Ok(all.to_vec()),
        Some(w) => w.split(',').map(|x| x.trim().parse::<T>().map_err(|e| ReadingRefusal::ExerciseRefused.refuse(e))).collect(),
    }
}

impl Options {
    /// From the command line's words: `--ops a,b`, `--units item,function`, `--n N`, `--tests`, `--crate R`,
    /// `--intents FILE`, `--seed S`; the word made and its rules checked.
    pub fn from_words(ops: Option<&str>, units: Option<&str>, n: Option<&str>, tests: bool, krate: Option<&str>, intents: Option<&Path>, seed: Option<&str>) -> Result<Options> {
        let num = |w: Option<&str>, what: &str, dflt: u64| -> Result<u64> { w.map(|v| v.trim().parse::<u64>().map_err(|_| ReadingRefusal::ExerciseRefused.refuse(format!("{what} `{v}` is not a number")))).transpose().map(|v| v.unwrap_or(dflt)) };
        let n = num(n, "--n", 1)? as usize;
        if n == 0 {
            return Err(ReadingRefusal::ExerciseRefused.refuse("--n 0 picks nothing"));
        }
        let mut o = Options { ops: list(ops, Operation::ALL)?, units: list(units, Unit::ALL)?, n, tests, krate: krate.map(|k| k.trim_start_matches("crate:").to_string()), intents: intents.map(Path::to_path_buf), seed: num(seed, "--seed", 0)?, word: 0 };
        o.word = o.make_word().raw();
        o.checked()
    }

    fn make_word(&self) -> ExerciseWord {
        let ops = Operation::ALL.iter().enumerate().filter(|(_, x)| self.ops.contains(x)).fold(0u32, |m, (i, _)| m | 1 << i);
        let units = Unit::ALL.iter().enumerate().filter(|(_, x)| self.units.contains(x)).fold(0u8, |m, (i, _)| m | 1 << i);
        let mut options = 0u8;
        for (on, label) in [(self.tests, "tests"), (self.intents.is_some(), "intents"), (self.intents.is_some(), "model"), (self.krate.is_some(), "one_crate")] {
            if on {
                options |= ExerciseWord::LABELS.iter().find(|l| l.field == "options" && l.label == label).map(|l| l.value as u8).unwrap_or(0);
            }
        }
        ExerciseWord::new(ops, units, options)
    }

    /// The word's rules hold, and the word is the options' own.
    pub fn checked(self) -> Result<Options> {
        let w = ExerciseWord::try_from_raw(self.word).ok_or_else(|| ReadingRefusal::ExerciseRefused.refuse(format!("{:#x} sets bits outside the exercise word", self.word)))?;
        w.check()?;
        if w.raw() != self.make_word().raw() {
            return Err(ReadingRefusal::ExerciseRefused.refuse(format!("the word {:#x} is not these options' ({:#x})", w.raw(), self.make_word().raw())));
        }
        Ok(self)
    }

    /// One line a person reads.
    pub fn line(&self) -> String {
        let ops = if self.ops.len() == Operation::ALL.len() { "every operation".to_string() } else { self.ops.iter().map(|o| o.as_str()).collect::<Vec<_>>().join(",") };
        let units = if self.units.len() == Unit::ALL.len() { "every unit kind".to_string() } else { self.units.iter().map(|u| u.as_str()).collect::<Vec<_>>().join(",") };
        format!(
            "{ops} × {units}, {} target{} each, seed {}{}{}{}",
            self.n,
            if self.n == 1 { "" } else { "s" },
            self.seed,
            if self.tests { ", tests" } else { "" },
            self.krate.as_ref().map(|k| format!(", crate {k}")).unwrap_or_default(),
            self.intents.as_ref().map(|i| format!(", intents {}", i.display())).unwrap_or_default()
        )
    }
}

// ---------------------------------------------------------------------------
// the picks

/// One target of one unit kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub unit: Unit,
    /// What a spec's `target` says: an item's id, or a route.
    pub route: String,
    pub package: String,
}

/// The file route of a file in a package: `crate:<name>/<path in the package>`.
fn file_route(k: &Klepto, package: &str, file: &Path) -> Option<String> {
    let p = k.packages.get(package)?;
    let root = p.root.canonicalize().unwrap_or_else(|_| p.root.clone());
    let f = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    f.strip_prefix(&root).ok().map(|r| format!("crate:{}/{}", p.name, r.to_string_lossy().replace('\\', "/")))
}

fn plain_item(d: &klepto::ItemDecl) -> bool {
    d.cfg == CfgPredicate::True && d.generated.is_none() && d.parent.is_none()
}

fn has_inherent_impl(k: &Klepto, id: &str) -> bool {
    k.impls_of(id).iter().any(|i| i.trait_.is_none())
}

fn named_struct(k: &Klepto, id: &str) -> bool {
    k.type_decl(id).is_some_and(|t| matches!(t.kind, klepto::TypeKind::Struct) && t.generics.is_empty() && !t.fields.is_empty() && t.fields.iter().all(|f| f.name.is_some()))
}

fn plain_type(k: &Klepto, id: &str) -> bool {
    k.type_decl(id).is_some_and(|t| matches!(t.kind, klepto::TypeKind::Struct | klepto::TypeKind::Enum) && t.generics.is_empty())
}

/// The packages in the pick (one, with `krate`), in name order.
fn packages(k: &Klepto, krate: Option<&str>) -> Vec<String> {
    k.packages.keys().filter(|p| krate.is_none_or(|c| c.replace('-', "_") == **p)).cloned().collect()
}

/// A function klepto captured, not a test.
fn captured<'k>(k: &'k Klepto, id: &str) -> Option<&'k klepto::CapturedFn> {
    k.functions.iter().find(|f| f.fq_name == id && !f.attrs.iter().any(|a| a.contains("test")))
}

/// Every target of `unit` that `op` can be tried on in this index, in id order.
pub fn targets(k: &Klepto, root: &Path, op: Operation, unit: Unit, krate: Option<&str>) -> Vec<Target> {
    use Operation::*;
    let pkgs = packages(k, krate);
    let mut out: Vec<Target> = Vec::new();
    let item = |d: &klepto::ItemDecl| Target { unit, route: d.id.clone(), package: d.package.clone() };
    match unit {
        Unit::Crate => {
            for p in &pkgs {
                let deps = k.features.get(p).map(|m| m.dependencies.len()).unwrap_or(0);
                if op == ChangeDependency && deps == 0 {
                    continue;
                }
                if crate::change::route_file(k, root, &format!("crate:{p}")).is_some() {
                    out.push(Target { unit, route: format!("crate:{p}"), package: p.clone() });
                }
            }
        }
        Unit::File => {
            for p in &pkgs {
                if let Some((_, f)) = crate::change::route_file(k, root, &format!("crate:{p}")) {
                    if let Some(r) = file_route(k, p, &f) {
                        if op != Extract || extractable(k, &f).is_some() {
                            out.push(Target { unit, route: r, package: p.clone() });
                        }
                    }
                }
            }
        }
        Unit::Module => {
            for m in k.modules.modules.values().filter(|m| !m.inline && !m.segments.is_empty() && !m.orphan && pkgs.contains(&m.package)) {
                if m.segments.iter().any(|s| s == "tests") {
                    continue;
                }
                if let Some(r) = file_route(k, &m.package, &m.file) {
                    if op != Extract || extractable(k, &m.file).is_some() {
                        out.push(Target { unit, route: r, package: m.package.clone() });
                    }
                }
            }
        }
        Unit::Item => {
            for d in k.items.iter().filter(|d| plain_item(d) && pkgs.contains(&d.package)) {
                let ok = match op {
                    AddField => named_struct(k, &d.id),
                    AddVariant => matches!(d.kind, NodeKind::Enum) && plain_type(k, &d.id),
                    MakeImpl | AddDerive => plain_type(k, &d.id),
                    AddTraitImpl => plain_type(k, &d.id) && !k.impls_of(&d.id).iter().any(|i| i.trait_.as_ref().is_some_and(|t| t.raw.rsplit("::").next() == Some("Display"))),
                    AddMethod => plain_type(k, &d.id) && !has_inherent_impl(k, &d.id),
                    AddConstructor => named_struct(k, &d.id) && !has_inherent_impl(k, &d.id),
                    Gate => matches!(d.kind, NodeKind::Struct | NodeKind::Enum | NodeKind::Trait | NodeKind::Const | NodeKind::Static | NodeKind::TypeAlias) && first_feature(k, &d.package).is_some(),
                    Move => matches!(d.kind, NodeKind::Struct | NodeKind::Enum | NodeKind::Trait | NodeKind::Const | NodeKind::Static | NodeKind::TypeAlias) && other_module(k, root, &d.package, &d.module).is_some(),
                    _ => matches!(d.kind, NodeKind::Struct | NodeKind::Enum | NodeKind::Trait | NodeKind::Const | NodeKind::Static | NodeKind::TypeAlias),
                };
                if ok {
                    out.push(item(d));
                }
            }
        }
        Unit::Impl => {
            for d in k.items.iter().filter(|d| plain_item(d) && pkgs.contains(&d.package) && plain_type(k, &d.id) && has_inherent_impl(k, &d.id)) {
                if op == AddConstructor && !named_struct(k, &d.id) {
                    continue;
                }
                out.push(item(d));
            }
        }
        Unit::Function => {
            for d in k.items.iter().filter(|d| d.cfg == CfgPredicate::True && d.generated.is_none() && pkgs.contains(&d.package)) {
                let free = matches!(d.kind, NodeKind::Fn) && d.parent.is_none();
                // a method of an inherent impl (a trait's methods keep the trait's signature)
                let method = matches!(d.kind, NodeKind::Method) && d.parent.as_ref().is_some_and(|p| k.impls.iter().any(|i| &i.id == p && i.trait_.is_none()));
                if !(free || method) {
                    continue;
                }
                let Some(f) = captured(k, &d.id) else { continue };
                let ok = match op {
                    Move => free && other_module(k, root, &d.package, &d.module).is_some(),
                    Gate => first_feature(k, &d.package).is_some(),
                    ChangeSignature => !f.is_generic,
                    _ => true,
                };
                if ok {
                    out.push(item(d));
                }
            }
        }
    }
    out.sort_by(|a, b| a.route.cmp(&b.route));
    out.dedup();
    out
}

/// The `n` targets of a list the seed picks: each ranked by the digest of (seed, operation, target), lowest first.
pub fn pick(seed: u64, op: Operation, all: Vec<Target>, n: usize) -> Vec<Target> {
    let mut ranked: Vec<(String, Target)> = all.into_iter().map(|t| (crate::digest::of_value(&(seed, op.as_str(), &t.route)), t)).collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0));
    ranked.into_iter().take(n).map(|(_, t)| t).collect()
}

/// A crate's first declared feature (not `default`, not one cargo made for an optional dependency).
fn first_feature(k: &Klepto, package: &str) -> Option<String> {
    k.features.get(package)?.features.iter().find(|(n, f)| n.as_str() != "default" && !f.implicit).map(|(n, _)| n.clone())
}

/// Another module of the crate with a file of its own, as its route: where a `move` sends an item.
fn other_module(k: &Klepto, root: &Path, package: &str, module: &str) -> Option<String> {
    let _ = root;
    let mut v: Vec<&klepto::ModuleInfo> = k.modules.modules.values().filter(|m| m.package == package && !m.inline && !m.orphan && m.path != module && !m.segments.iter().any(|s| s == "tests")).collect();
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v.into_iter().find_map(|m| file_route(k, package, &m.file))
}

/// One item of a file `extract` can move out: a module-level struct, enum or function declared in it.
fn extractable(k: &Klepto, file: &Path) -> Option<String> {
    let want = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let mut v: Vec<&klepto::ItemDecl> = k
        .items
        .iter()
        .filter(|d| plain_item(d) && matches!(d.kind, NodeKind::Struct | NodeKind::Enum | NodeKind::Fn) && d.location.path.canonicalize().unwrap_or_else(|_| d.location.path.clone()) == want)
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v.first().map(|d| d.id.clone())
}

// ---------------------------------------------------------------------------
// the specs

fn last(id: &str) -> &str {
    id.rsplit("::").next().unwrap_or(id)
}

fn step_value(v: &str) -> Piece {
    Piece { piece: "value".into(), value: Some(v.into()), ..Piece::default() }
}

/// A dependency a crate can be given that resolves without the network: another crate of the workspace that does not
/// depend on it (by path), else a package of its lockfile it does not name (by its locked version).
fn new_dependency(k: &Klepto, root: &Path, package: &str) -> Option<DependencySpec> {
    let named: BTreeSet<String> = k.features.get(package).map(|m| m.dependencies.iter().map(|d| d.package.replace('-', "_")).collect()).unwrap_or_default();
    let p = k.packages.get(package)?;
    let reaches = |from: &str| -> bool {
        // does `from` depend on `package`, directly or through others?
        let mut seen = BTreeSet::new();
        let mut todo = vec![from.to_string()];
        while let Some(x) = todo.pop() {
            if x == package {
                return true;
            }
            if seen.insert(x.clone()) {
                todo.extend(k.package_deps.get(&x).into_iter().flatten().cloned());
            }
        }
        false
    };
    for (name, other) in &k.packages {
        if name == package || named.contains(name) || reaches(name) || !other.targets.iter().any(|t| t.kind == "lib") {
            continue;
        }
        let from = p.root.canonicalize().unwrap_or_else(|_| p.root.clone());
        let to = other.root.canonicalize().unwrap_or_else(|_| other.root.clone());
        return Some(DependencySpec { name: other.cargo_name.clone(), version: None, path: Some(relative_path(&from, &to)), features: Vec::new() });
    }
    let lock: toml::Value = std::fs::read_to_string(root.join("Cargo.lock")).ok()?.parse().ok()?;
    let mut locked: Vec<(String, String)> = lock
        .get("package")?
        .as_array()?
        .iter()
        .filter(|x| x.get("source").and_then(|s| s.as_str()).is_some_and(|s| s.starts_with("registry+")))
        .filter_map(|x| Some((x.get("name")?.as_str()?.to_string(), x.get("version")?.as_str()?.to_string())))
        .filter(|(n, _)| !named.contains(&n.replace('-', "_")))
        .collect();
    locked.sort();
    locked.into_iter().next().map(|(name, v)| DependencySpec { name, version: Some(format!("={v}")), path: None, features: Vec::new() })
}

/// A dependency a crate names, changed to a source that resolves the same: a path dependency given its crate's
/// version beside its path, a registry one its locked version exactly.
fn changed_dependency(k: &Klepto, root: &Path, package: &str) -> Option<DependencySpec> {
    let manifest = std::fs::read_to_string(&k.packages.get(package)?.manifest_path).ok()?;
    let doc: toml::Value = manifest.parse().ok()?;
    let deps = doc.get("dependencies")?.as_table()?;
    let lock: Option<toml::Value> = std::fs::read_to_string(root.join("Cargo.lock")).ok().and_then(|t| t.parse().ok());
    let locked = |name: &str| -> Option<String> {
        lock.as_ref()?.get("package")?.as_array()?.iter().find(|x| x.get("name").and_then(|n| n.as_str()) == Some(name) && x.get("source").is_some()).and_then(|x| x.get("version")?.as_str().map(str::to_string))
    };
    let mut names: Vec<&String> = deps.keys().collect();
    names.sort();
    for name in names {
        let v = &deps[name];
        if v.get("workspace").and_then(|w| w.as_bool()) == Some(true) {
            continue;
        }
        if let Some(path) = v.get("path").and_then(|p| p.as_str()) {
            let version = k.packages.get(&name.replace('-', "_")).map(|p| p.version.clone())?;
            return Some(DependencySpec { name: name.clone(), version: Some(version), path: Some(path.to_string()), features: Vec::new() });
        }
        if let Some(l) = locked(name) {
            return Some(DependencySpec { name: name.clone(), version: Some(format!("={l}")), path: None, features: Vec::new() });
        }
    }
    None
}

fn relative_path(from: &Path, to: &Path) -> String {
    let a: Vec<_> = from.components().collect();
    let b: Vec<_> = to.components().collect();
    let common = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<String> = std::iter::repeat_n("..".to_string(), a.len() - common).collect();
    parts.extend(b[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

/// A parameter from klepto's spelling of it (`x: u32`); a receiver is not one.
fn param_of(arg: &str) -> Option<Param> {
    let (name, ty) = arg.split_once(':')?;
    Some(Param { name: name.trim().to_string(), ty: ty.trim().to_string() })
}

fn receiver_of(f: &klepto::CapturedFn) -> Option<String> {
    f.args.iter().map(|a| a.trim()).find(|a| matches!(*a, "self" | "&self" | "&mut self" | "mut self") || a.starts_with("&'") && a.ends_with(" self")).map(str::to_string)
}

/// The change spec a case writes: the smallest change `op` can make at `t`, every crate of the copy in its scope
/// (the copy is the case's own; what the preflight decides is the operation, not a scope). `None` when the index
/// gives the operation nothing to write there.
pub fn spec_for(k: &Klepto, root: &Path, op: Operation, t: &Target, n: u64, crates: &[String]) -> Option<ChangeSpec> {
    use Operation::*;
    let name = format!("exercise-{n:03}-{}", op.as_str().replace('_', "-"));
    let mut s = ChangeSpec { name, objective: format!("exercise {n}: {} on {}", op.as_str(), t.route), operation: Some(op), target: t.route.clone(), ..ChangeSpec::default() };
    s.scope.write = crates.iter().map(|c| format!("crate:{c}")).collect();
    let u32fn = |name: String, receiver: Option<&str>| FnSpec { name, vis: Some("pub".into()), receiver: receiver.map(str::to_string), ret: Some("u32".into()), body: vec![step_value("0")], ..FnSpec::default() };
    match op {
        MakeStruct => s.item = Some(ItemSpec { name: format!("ExerciseStruct{n}"), vis: Some("pub".into()), derives: vec!["Debug".into()], fields: vec![FieldSpec { name: "value".into(), ty: "u32".into(), vis: Some("pub".into()), ..FieldSpec::default() }], ..ItemSpec::default() }),
        MakeEnum => s.item = Some(ItemSpec { name: format!("ExerciseEnum{n}"), vis: Some("pub".into()), derives: vec!["Debug".into()], variants: vec![VariantSpec { name: "One".into(), ..VariantSpec::default() }, VariantSpec { name: "Two".into(), ..VariantSpec::default() }], ..ItemSpec::default() }),
        MakeFn => s.function = Some(u32fn(format!("exercise_fn_{n}"), None)),
        AddModule => s.module = Some(ModuleSpec { name: format!("exercise_mod_{n}"), vis: Some("pub".into()), doc: Some("made by an exercise".into()) }),
        Extract => {
            let (_, file) = crate::change::route_file(k, root, &t.route)?;
            s.items = vec![extractable(k, &file)?];
            s.module = Some(ModuleSpec { name: format!("exercise_extract_{n}"), vis: Some("pub".into()), doc: None });
        }
        AddDependency => s.dependency = Some(new_dependency(k, root, &t.package)?),
        ChangeDependency => s.dependency = Some(changed_dependency(k, root, &t.package)?),
        MakeImpl => s.functions = vec![u32fn(format!("exercise_impl_fn_{n}"), Some("&self"))],
        AddTraitImpl => {
            s.trait_name = Some("std::fmt::Display".into());
            s.functions = vec![FnSpec {
                name: "fmt".into(),
                receiver: Some("&self".into()),
                params: vec![Param { name: "f".into(), ty: "&mut std::fmt::Formatter<'_>".into() }],
                ret: Some("std::fmt::Result".into()),
                body: vec![step_value("f.write_str(\"exercise\")")],
                ..FnSpec::default()
            }];
        }
        AddField => s.field = Some(FieldSpec { name: format!("exercise_field_{n}"), ty: "u8".into(), default: Some("0".into()), ..FieldSpec::default() }),
        AddVariant => s.variant = Some(VariantSpec { name: format!("ExerciseVariant{n}"), ..VariantSpec::default() }),
        AddDerive => {
            let ty = k.type_decl(&t.route)?;
            let has = |d: &str| ty.derives.iter().any(|r| crate::reading::facts::Facts::canonical(r).is_some_and(|p| last(p) == d));
            s.derive = vec![["Debug", "Clone", "PartialEq", "Eq", "Hash"].into_iter().find(|d| !has(d))?.to_string()];
        }
        AddAttribute => s.attribute = Some("#[allow(dead_code)]".into()),
        Gate => s.feature = Some(first_feature(k, &t.package)?),
        Anchor => s.linkage = Some(format!("exercise-link-{n}")),
        Rename => {
            let old = last(&t.route);
            s.new_name = Some(if old.chars().next().is_some_and(char::is_uppercase) {
                if old.chars().all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit()) {
                    format!("{old}_EXERCISED_{n}")
                } else {
                    format!("{old}Exercised{n}")
                }
            } else {
                format!("{old}_exercised_{n}")
            });
        }
        Move => {
            let d = crate::change::item_named(k, &t.route)?;
            s.to = Some(other_module(k, root, &d.package, &d.module)?);
        }
        ChangeSignature => {
            let f = captured(k, &t.route)?;
            let mut params: Vec<Param> = f.args.iter().filter(|a| receiver_of_arg(a).is_none()).map(|a| param_of(a)).collect::<Option<Vec<_>>>()?;
            params.push(Param { name: format!("exercise_{n}"), ty: "u8".into() });
            s.function = Some(FnSpec {
                name: f.name.clone(),
                vis: f.is_public.then(|| "pub".to_string()),
                receiver: receiver_of(f),
                params,
                ret: f.return_ty.clone(),
                is_async: f.is_async,
                is_const: f.is_const,
                is_unsafe: f.is_unsafe,
                ..FnSpec::default()
            });
            s.call_sites = Some(CallSites { pass: Some("0".into()) });
        }
        ReplaceBody => s.function = Some(FnSpec { name: last(&t.route).to_string(), doc: Some("replaced by an exercise".into()), ..FnSpec::default() }),
        AddTest => s.function = Some(FnSpec { name: format!("exercise_test_{n}"), body: vec![step_value("assert!(true)")], ..FnSpec::default() }),
        AddMethod => s.function = Some(u32fn(format!("exercise_method_{n}"), Some("&self"))),
        AddConstructor => {
            let ty = k.type_decl(&t.route)?;
            let params = ty.fields.iter().map(|f| Some(Param { name: f.name.clone()?, ty: f.ty.raw.clone() })).collect::<Option<Vec<_>>>()?;
            s.function = Some(FnSpec { name: format!("exercise_new_{n}"), vis: Some("pub".into()), params, ..FnSpec::default() });
        }
    }
    Some(s)
}

fn receiver_of_arg(a: &str) -> Option<&str> {
    let a = a.trim();
    (matches!(a, "self" | "&self" | "&mut self" | "mut self") || (a.starts_with("&'") && a.ends_with(" self"))).then_some(a)
}

// ---------------------------------------------------------------------------
// a case, its findings, the summary

/// One finding: what, where, and the receipt that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub code: FindingCode,
    /// `model` for a case an intent made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub case: u64,
    pub operation: String,
    pub unit: String,
    pub target: String,
    pub detail: String,
    /// The receipt: `#<seq>` in the project's journal; a preflight's is the copy's journal's, with its file relative to
    /// the exercise's folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<String>,
}

impl Finding {
    pub fn line(&self) -> String {
        format!(
            "{}{} — case {} {} on {} ({}): {}{}",
            self.code,
            self.tag.as_ref().map(|t| format!(" [{t}]")).unwrap_or_default(),
            self.case,
            self.operation,
            self.target,
            self.unit,
            self.detail,
            self.receipt.as_ref().map(|r| format!(" (receipt {r})")).unwrap_or_default()
        )
    }
}

/// A case as it ran.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    pub n: u64,
    pub operation: String,
    pub unit: String,
    pub target: String,
    /// `model` for an intent's case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// The spec file and the copy, relative to the exercise's folder (`data/exercise/` in the job's folder).
    #[serde(default)]
    pub spec: Option<String>,
    #[serde(default)]
    pub tree: Option<String>,
    /// `authorized`, `not authorized`, or the refusal.
    #[serde(default)]
    pub preflight: Option<String>,
    #[serde(default)]
    pub transform: Option<String>,
    #[serde(default)]
    pub compile: Option<String>,
    #[serde(default)]
    pub test: Option<String>,
    /// Lines added and removed, from the copy's counts before and after.
    #[serde(default)]
    pub lines_touched: usize,
    /// Lines the transform record says it wrote.
    #[serde(default)]
    pub lines_written: usize,
    #[serde(default)]
    pub uncovered: Vec<String>,
    pub millis: i64,
    /// `passed`, `finding`, `not run`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub findings: Vec<Finding>,
    /// The receipts in the project's journal, in order.
    #[serde(default)]
    pub receipts: Vec<i64>,
}

/// One row of the operation × unit matrix.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub operation: String,
    pub unit: String,
    pub cases: usize,
    pub passed: usize,
    pub findings: usize,
    /// No target of this kind in the index (or none the operation can write at).
    pub no_target: bool,
}

/// What an exercise found, all of it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub schema: String,
    pub job: String,
    pub options: Option<Options>,
    pub opened_utc: String,
    pub closed_utc: String,
    pub cases_run: usize,
    pub passed: usize,
    pub not_run: usize,
    pub findings_by_code: BTreeMap<String, usize>,
    pub matrix: Vec<Cell>,
    /// `(case, operation, target, millis)`, slowest first.
    pub slowest: Vec<(u64, String, String, i64)>,
    /// What to fix, most findings first: `<count>× <code> on <operation>: <a finding's detail>`.
    pub to_fix: Vec<String>,
    pub findings: Vec<Finding>,
    pub cases: Vec<Case>,
    /// The `check` receipt named `exercise`.
    pub receipt: Option<i64>,
}

impl Summary {
    fn of(job: &str, options: &Options, opened: String, cases: Vec<Case>, cells: &BTreeMap<(String, String), Cell>) -> Summary {
        let findings: Vec<Finding> = cases.iter().flat_map(|c| c.findings.iter().cloned()).collect();
        let mut by_code: BTreeMap<String, usize> = BTreeMap::new();
        for f in &findings {
            *by_code.entry(f.code.as_str().to_string()).or_default() += 1;
        }
        let mut matrix: BTreeMap<(String, String), Cell> = cells.clone();
        for c in cases.iter().filter(|c| c.tag.is_none()) {
            let cell = matrix.entry((c.operation.clone(), c.unit.clone())).or_insert_with(|| Cell { operation: c.operation.clone(), unit: c.unit.clone(), ..Cell::default() });
            cell.no_target = false;
            cell.cases += 1;
            if c.status == "passed" {
                cell.passed += 1;
            }
            cell.findings += c.findings.len();
        }
        let mut slow: Vec<&Case> = cases.iter().filter(|c| c.status != "not run").collect();
        slow.sort_by(|a, b| b.millis.cmp(&a.millis));
        let mut groups: BTreeMap<(String, String), (usize, String)> = BTreeMap::new();
        for f in &findings {
            let g = groups.entry((f.code.as_str().to_string(), f.operation.clone())).or_insert((0, f.detail.clone()));
            g.0 += 1;
        }
        let mut ranked: Vec<((String, String), (usize, String))> = groups.into_iter().collect();
        ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
        // the order a cell shows in: the vocabulary's, operation then unit
        let order = |c: &Cell| (Operation::ALL.iter().position(|o| o.as_str() == c.operation).unwrap_or(usize::MAX), Unit::ALL.iter().position(|u| u.as_str() == c.unit).unwrap_or(usize::MAX));
        let mut matrix: Vec<Cell> = matrix.into_values().collect();
        matrix.sort_by_key(order);
        Summary {
            schema: EXERCISE_SCHEMA.into(),
            job: job.to_string(),
            options: Some(options.clone()),
            opened_utc: opened,
            closed_utc: now_utc(),
            cases_run: cases.iter().filter(|c| c.status != "not run").count(),
            passed: cases.iter().filter(|c| c.status == "passed").count(),
            not_run: cases.iter().filter(|c| c.status == "not run").count(),
            findings_by_code: by_code,
            matrix,
            slowest: slow.iter().take(5).map(|c| (c.n, c.operation.clone(), c.target.clone(), c.millis)).collect(),
            to_fix: ranked.into_iter().map(|((code, op), (count, detail))| format!("{count}× {code} on {op}: {detail}")).collect(),
            findings,
            cases,
            receipt: None,
        }
    }

    /// The headline: cases, passed, findings.
    pub fn headline(&self) -> String {
        format!("exercise {}: {} cases run, {} passed, {} findings{}", self.job, self.cases_run, self.passed, self.findings.len(), if self.not_run > 0 { format!(", {} not run", self.not_run) } else { String::new() })
    }
}

/// The summary as a page.
pub fn markdown(s: &Summary) -> String {
    let mut m = format!("# {}\n\n", s.headline());
    if let Some(o) = &s.options {
        m.push_str(&format!("- options: {}\n", o.line()));
    }
    m.push_str(&format!("- opened {} · closed {}\n", s.opened_utc, s.closed_utc));
    if let Some(r) = s.receipt {
        m.push_str(&format!("- check receipt #{r}\n"));
    }
    m.push_str("\n## findings by code\n\n");
    if s.findings_by_code.is_empty() {
        m.push_str("none\n");
    }
    for (c, n) in &s.findings_by_code {
        m.push_str(&format!("- `{c}`: {n}\n"));
    }
    m.push_str("\n## what to fix, most findings first\n\n");
    if s.to_fix.is_empty() {
        m.push_str("nothing\n");
    }
    for (i, t) in s.to_fix.iter().enumerate() {
        m.push_str(&format!("{}. {t}\n", i + 1));
    }
    m.push_str("\n## operation × unit\n\n| operation | unit | cases | passed | findings |\n|---|---|---|---|---|\n");
    for c in &s.matrix {
        if c.no_target {
            m.push_str(&format!("| {} | {} | no target | | |\n", c.operation, c.unit));
        } else {
            m.push_str(&format!("| {} | {} | {} | {} | {} |\n", c.operation, c.unit, c.cases, c.passed, c.findings));
        }
    }
    m.push_str("\n## slowest\n\n");
    for (n, op, t, ms) in &s.slowest {
        m.push_str(&format!("- case {n} {op} on {t}: {:.1} s\n", *ms as f64 / 1000.0));
    }
    m.push_str("\n## findings\n\n");
    if s.findings.is_empty() {
        m.push_str("none\n");
    }
    for f in &s.findings {
        m.push_str(&format!("- {}\n", f.line()));
    }
    m.push_str("\n## cases\n\n| case | operation | unit | target | preflight | transform | compile | test | lines | s | status |\n|---|---|---|---|---|---|---|---|---|---|---|\n");
    for c in &s.cases {
        let cell = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into()).replace('|', "/").chars().take(60).collect::<String>();
        m.push_str(&format!(
            "| {} | {}{} | {} | `{}` | {} | {} | {} | {} | {} | {:.1} | {}{} |\n",
            c.n,
            c.operation,
            c.tag.as_ref().map(|t| format!(" [{t}]")).unwrap_or_default(),
            c.unit,
            c.target,
            cell(&c.preflight),
            cell(&c.transform),
            cell(&c.compile),
            cell(&c.test),
            c.lines_touched,
            c.millis as f64 / 1000.0,
            c.status,
            c.note.as_ref().map(|n| format!(": {}", n.chars().take(80).collect::<String>())).unwrap_or_default()
        ));
    }
    m
}

// ---------------------------------------------------------------------------
// the job

/// Queues an exercise job (`jobs/queued/exercise-<stamp>/`): its job file (kind `exercise`, unbounded, verified by
/// `check:exercise`) and its options in `data/exercise/options.json`. Returns the folder's name.
pub fn queue(ctx: &Ctx, options: &Options) -> Result<String> {
    let store = ctx.store()?;
    let name = format!("{KIND}-{}", crate::runs::stamp());
    let dir = store.job_dir(crate::store::JobState::Queued, &name)?;
    let jf = crate::jobs::JobFile {
        name: Some(name.clone()),
        kind: Some(KIND.into()),
        target: Some(options.krate.as_ref().map(|k| format!("crate:{k}")).unwrap_or_else(|| "*".into())),
        mode: Some(crate::jobs::JobMode::Unbounded),
        verify: vec![format!("check:{CHECK_NAME}")],
        ..crate::jobs::JobFile::default()
    };
    write_whole(&dir.join(crate::jobs::job_file_name()), crate::oneo::render(crate::oneo::Doc::Job, &jf)?.as_bytes())?;
    let mut o = options.clone();
    if let Some(i) = &o.intents {
        // the intents file as the job reads it: absolute
        o.intents = Some(if i.is_absolute() { i.clone() } else { std::env::current_dir().unwrap_or_default().join(i) });
    }
    write_whole(&dir.join(crate::jobs::JOB_DATA).join(DIR).join(OPTIONS_FILE), (serde_json::to_string_pretty(&o)? + "\n").as_bytes())?;
    Ok(name)
}

/// An exercise from the command line: queued, started (its cases run), closed.
pub fn run_job(ctx: &Ctx, options: &Options) -> Result<(crate::jobs::Started, crate::jobs::Closing, PathBuf, Vec<String>)> {
    let name = queue(ctx, options)?;
    let started = crate::jobs::start(ctx, Some(&name), false)?;
    let (closing, done, said) = crate::jobs::close(ctx, &started.id)?;
    Ok((started, closing, done, said))
}

/// The options a job folder holds.
pub fn options_of(folder: &Path) -> Result<Options> {
    let p = folder.join(crate::jobs::JOB_DATA).join(DIR).join(OPTIONS_FILE);
    let text = std::fs::read_to_string(&p).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", p.display())))?;
    let o: Options = serde_json::from_str(&text).map_err(|e| ReadingRefusal::ExerciseRefused.refuse(format!("{}: {e}", p.display())))?;
    o.checked()
}

/// The summary a job folder holds.
pub fn summary_of(folder: &Path) -> Option<Summary> {
    serde_json::from_str(&std::fs::read_to_string(folder.join(crate::jobs::JOB_DATA).join(DIR).join(SUMMARY_FILE)).ok()?).ok()
}

/// `exercise report JOB`: the summary of an exercise job (done or running), by its id or name; the newest when none
/// is named.
pub fn report(ctx: &Ctx, which: Option<&str>) -> Result<(Summary, PathBuf)> {
    let store = ctx.store()?;
    let mut found: Vec<PathBuf> = Vec::new();
    for state in [crate::store::JobState::Done, crate::store::JobState::Running] {
        let dir = store.jobs_dir(state)?;
        for e in std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect::<Vec<_>>()).unwrap_or_default() {
            let jf = crate::jobs::job_file(&e);
            if jf.kind.as_deref() != Some(KIND) {
                continue;
            }
            if which.is_none_or(|w| e.file_name().is_some_and(|n| n == w) || jf.id.as_deref() == Some(w) || jf.name.as_deref() == Some(w)) {
                found.push(e);
            }
        }
    }
    found.sort();
    let folder = found.pop().ok_or_else(|| ReadingRefusal::NoJob.refuse(format!("no exercise job {}", which.unwrap_or("(any)"))))?;
    let s = summary_of(&folder).ok_or_else(|| ReadingRefusal::MissingInput.refuse(format!("{}: no {DIR}/{SUMMARY_FILE} (did its cases run?)", folder.display())))?;
    Ok((s, folder))
}

/// One case to run: its number, operation, unit, target and spec (or, for an intent, the intent).
#[derive(Debug, Clone)]
struct Planned {
    n: u64,
    op: Option<Operation>,
    unit: Option<Unit>,
    target: Option<Target>,
    spec: Option<ChangeSpec>,
    intent: Option<String>,
}

/// What every case shares: the scope environment.
struct Env<'a> {
    ctx: &'a Ctx,
    job: &'a InJob,
    task: &'a TaskRef,
    dir: PathBuf,
    target_dir: PathBuf,
    tests: bool,
    crates: Vec<String>,
    /// The targets the index offers, for an intent's prompt.
    offered: Vec<String>,
    cases: Vec<Case>,
    runs: Vec<Ran>,
    said: Vec<String>,
    error: Option<crate::CustodianError>,
}

/// The stages of a case, one per poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Copy,
    Preflight,
    Transform,
    Compile,
    Test,
    Close,
}

struct CaseTask {
    planned: Planned,
    stage: Stage,
    case: Case,
    folder: PathBuf,
    tree: PathBuf,
    spec_path: PathBuf,
    spec: Option<ChangeSpec>,
    store: Option<Store>,
    open: BTreeMap<String, String>,
    open_count: Option<crate::reading::count::CountReport>,
    preflight_seq: Option<i64>,
    record: Option<crate::transform::TransformRecord>,
    package: Option<String>,
}

/// A path inside the exercise's folder, relative to it: what a case names (the job's folder moves to done at close).
fn in_dir(dir: &Path, p: &Path) -> String {
    p.strip_prefix(dir).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let walk = walkdir::WalkDir::new(from).into_iter().filter_entry(|e| !(e.depth() > 0 && e.file_type().is_dir() && NOT_COPIED.iter().any(|n| e.file_name() == *n)));
    for e in walk.filter_map(|e| e.ok()) {
        if e.depth() == 0 || e.file_type().is_symlink() {
            continue;
        }
        let r = e.path().strip_prefix(from).unwrap_or(e.path());
        let dst = to.join(r);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dst).map_err(|x| ReadingRefusal::Write.refuse(format!("{}: {x}", dst.display())))?;
        } else if e.file_type().is_file() {
            if let Some(p) = dst.parent() {
                std::fs::create_dir_all(p).map_err(|x| ReadingRefusal::Write.refuse(format!("{}: {x}", p.display())))?;
            }
            std::fs::copy(e.path(), &dst).map_err(|x| ReadingRefusal::Write.refuse(format!("{}: {x}", dst.display())))?;
        }
    }
    // cargo stops at the copy: a workspace of its own
    let m = to.join("Cargo.toml");
    let text = std::fs::read_to_string(&m).map_err(|x| ReadingRefusal::NoWorkspace.refuse(format!("{}: {x}", m.display())))?;
    if !text.lines().any(|l| l.trim() == "[workspace]") {
        write_whole(&m, format!("{text}\n[workspace]\n").as_bytes())?;
    }
    Ok(())
}

/// The digests of the code a count holds: Rust and TOML files, by path.
fn digests(c: &crate::reading::count::CountReport) -> BTreeMap<String, String> {
    c.crates.iter().flat_map(|k| k.file_counts.iter()).filter(|f| !f.path.contains('#') && crate::jobs::CODE_LANGUAGES.contains(&f.language.as_str())).map(|f| (f.path.clone(), f.digest.clone())).collect()
}

fn count_of(root: &Path) -> Result<crate::reading::count::CountReport> {
    let f = crate::reading::facts::Facts::build(root, &Scope { tests: true, ..Scope::default() }, crate::paths::Resolving::Free)?;
    let spec = crate::reading::spec::ReadingSpec::from_toml("name = \"exercise-count\"\nquestion = \"how much is in the copy\"\nkind = \"count\"\ntarget = \"*\"\n")?;
    crate::reading::count::count(&f, &spec)
}

impl CaseTask {
    fn finding(&mut self, code: FindingCode, detail: impl Into<String>, receipt: Option<String>) {
        self.case.findings.push(Finding {
            code,
            tag: self.case.tag.clone(),
            case: self.case.n,
            operation: self.case.operation.clone(),
            unit: self.case.unit.clone(),
            target: self.case.target.clone(),
            detail: detail.into().lines().take(6).collect::<Vec<_>>().join(" / "),
            receipt,
        });
    }

    /// A walked line's receipt, kept in the project's journal under the job.
    fn keep_line(&mut self, env: &mut Env<'_>, plan: &Plan, w: &Walked, command: &str, name: &str, outputs: Vec<String>) -> Result<Receipt> {
        let store = env.ctx.store()?;
        let receipt = crate::receipts::receipt_of(plan, w, command, name, self.case.spec.clone(), self.spec.as_ref().map(|s| s.digest()), None, None, outputs, Some(env.job));
        let (r, f) = keep(store, receipt)?;
        self.case.receipts.push(r.seq);
        env.runs.push(Ran { receipt: Some(r.clone()), receipt_file: Some(f), summary: None, reused: None, printed: None, said: Vec::new() });
        Ok(r)
    }

    fn step(&mut self, env: &mut Env<'_>) -> Result<bool> {
        match self.stage {
            Stage::Copy => {
                std::fs::create_dir_all(&self.folder).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", self.folder.display())))?;
                if let Some(intent) = self.planned.intent.clone() {
                    match self.model_spec(env, &intent)? {
                        Some(s) => self.spec = Some(s),
                        None => {
                            self.stage = Stage::Close;
                            return Ok(true);
                        }
                    }
                }
                let Some(spec) = self.spec.clone() else {
                    self.stage = Stage::Close;
                    return Ok(true);
                };
                copy_tree(&env.ctx.root, &self.tree)?;
                self.case.tree = Some(in_dir(&env.dir, &self.tree));
                let (store, _) = Store::init(&self.tree, crate::store::Mode::Plain)?;
                self.store = Some(store);
                self.spec_path = self.folder.join(format!("{}.toml", spec.name));
                write_whole(&self.spec_path, spec.to_toml()?.as_bytes())?;
                self.case.spec = Some(in_dir(&env.dir, &self.spec_path));
                // the spec as the kit reads it: its own checks first
                if let Err(e) = ChangeSpec::from_path(&self.spec_path) {
                    let tag = if self.case.tag.is_some() { FindingCode::ModelMiss } else { FindingCode::SpecRefused };
                    self.finding(tag, e.to_string(), None);
                    self.stage = Stage::Close;
                    return Ok(true);
                }
                self.stage = Stage::Preflight;
            }
            Stage::Preflight => {
                let spec = self.spec.clone().expect("a case past its copy has its spec");
                let store = self.store.clone().expect("a case past its copy has its folder");
                let cctx = Ctx { root: store.root.clone(), store: Some(store.clone()), exe: env.ctx.exe.clone() };
                let ws = crate::api::Workspace::open_with(&cctx.root, &Scope { tests: true, ..Scope::default() }, crate::paths::Resolving::Free).map_err(crate::CustodianError::from)?;
                let open = crate::reading::count::count(ws.facts(), &crate::reading::spec::ReadingSpec::from_toml("name = \"exercise-count\"\nquestion = \"how much is in the copy\"\nkind = \"count\"\ntarget = \"*\"\n")?)?;
                self.open = digests(&open);
                self.open_count = Some(open);
                if spec.op() == Operation::Anchor {
                    // the record the anchor names, binding the target and its crate
                    let pkg = crate::change::item_named(&ws.facts().k, &spec.target).map(|d| d.package.clone()).unwrap_or_default();
                    let name = spec.linkage.clone().unwrap_or_default();
                    match crate::linkage::record(ws.facts(), &name, crate::linkage::LinkKind::Connected, "an exercise's anchor", &[spec.target.clone(), format!("crate:{pkg}")], None) {
                        Ok(rec) => {
                            crate::linkage::write(&store, &rec)?;
                        }
                        Err(e) => self.case.note = Some(format!("the anchor's record was not made: {e}")),
                    }
                }
                match crate::preflight::run_preflight(&cctx, &self.spec_path, None, Runner::InProcess(&ws)) {
                    Ok(ran) => {
                        let p = &ran.preflight;
                        self.package = p.landing.as_ref().map(|l| l.package.clone());
                        let at = ran.receipt.as_ref().zip(ran.receipt_file.as_ref()).map(|(r, f)| format!("the copy's #{} ({})", r.seq, in_dir(&env.dir, f)));
                        self.preflight_seq = ran.receipt.as_ref().map(|r| r.seq);
                        if p.authorized {
                            self.case.preflight = Some("authorized".into());
                            self.stage = Stage::Transform;
                        } else {
                            let failing: Vec<String> = p.preconditions.iter().filter(|c| !c.holds).map(|c| format!("{} ({})", c.what, c.detail)).chain(p.collisions.iter().cloned()).chain(p.decisions.iter().map(|q| q.question.clone())).collect();
                            let why = if failing.is_empty() { p.reasons.join("; ") } else { failing.join("; ") };
                            self.case.preflight = Some(format!("not authorized: {why}"));
                            let code = if self.case.tag.is_some() { FindingCode::ModelMiss } else { FindingCode::PreflightRefused };
                            self.finding(code, why, at);
                            self.stage = Stage::Close;
                        }
                    }
                    Err(e) => {
                        self.case.preflight = Some(format!("refused: {e}"));
                        let code = if self.case.tag.is_some() { FindingCode::ModelMiss } else { FindingCode::PreflightFailed };
                        self.finding(code, e.to_string(), None);
                        self.stage = Stage::Close;
                    }
                }
            }
            Stage::Transform => {
                let spec = self.spec.clone().expect("spec");
                let store = self.store.clone().expect("store");
                let code = crate::code_digest(&self.tree, &Scope { tests: true, ..Scope::default() }.scan_options()).map_err(|e| ReadingRefusal::Scan.refuse(e))?;
                let (d, prev) = crate::preflight::gate(&store, &spec, &code)?;
                if !d.go {
                    self.case.transform = Some(format!("not run: {}", d.reason));
                    self.finding(FindingCode::GateRefused, d.reason.clone(), None);
                    self.stage = Stage::Close;
                    return Ok(true);
                }
                let out = self.folder.join("transform.json");
                let mut words: Vec<std::ffi::OsString> = vec!["transform".into(), "--spec".into(), self.spec_path.clone().into_os_string(), "--write".into(), "--out".into(), out.clone().into_os_string()];
                if let Some(p) = prev.or(self.preflight_seq.map(|s| Receipt { seq: s, ..empty_receipt() })) {
                    words.push("--preflight".into());
                    words.push(p.seq.to_string().into());
                }
                let line = exec::CodePassenger::at_root(self.tree.as_os_str()).words(words);
                let mut l = exec::line("code-passenger:transform", &line)?;
                l.env.push((crate::jobs::CODE_PASSENGER_JOB_ENV.to_string(), env.job.id.clone()));
                let plan = Plan::new(format!("exercise:{}:transform", self.case.n)).then(l);
                let w = plan.walk(Watch::Quiet, |_, _| {})?.into_iter().next().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse("code-passenger transform"))?;
                let outputs = if out.is_file() { vec![env.ctx.rel(&out)] } else { Vec::new() };
                let r = self.keep_line(env, &plan, &w, "transform", &format!("{} transform", spec.name), outputs)?;
                let at = Some(format!("#{}", r.seq));
                let text = format!("{}\n{}", w.stderr.trim(), w.stdout.trim());
                let said: Vec<&str> = text.lines().filter(|l| l.starts_with("does not compile") || l.starts_with("postcondition does not hold") || l.starts_with('[') || l.contains("panicked")).collect();
                let detail = if said.is_empty() { text.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" / ") } else { said.join(" / ") };
                self.record = std::fs::read_to_string(&out).ok().and_then(|t| serde_json::from_str(&t).ok());
                self.case.lines_written = self.record.as_ref().map(|r| r.files.iter().flat_map(|f| f.lines.iter()).map(|(a, b)| (b + 1).saturating_sub(*a) as usize).sum()).unwrap_or(0);
                match w.code {
                    Some(0) if self.record.as_ref().is_some_and(|r| r.written) => {
                        self.case.transform = Some(format!("written ({} files)", self.record.as_ref().map(|r| r.files.len()).unwrap_or(0)));
                        self.stage = Stage::Compile;
                    }
                    Some(0) => {
                        self.case.transform = Some("exit 0 with no written record".into());
                        self.finding(FindingCode::TransformUnproved, format!("exit 0 but no written record: {detail}"), at);
                        self.stage = Stage::Close;
                    }
                    Some(1) => {
                        self.case.transform = Some("not proved".into());
                        self.finding(FindingCode::TransformUnproved, detail, at);
                        self.stage = Stage::Close;
                    }
                    Some(2) => {
                        self.case.transform = Some("refused".into());
                        self.finding(FindingCode::TransformRefused, detail, at);
                        self.stage = Stage::Close;
                    }
                    other => {
                        self.case.transform = Some(format!("crashed ({})", other.map(|c| c.to_string()).unwrap_or_else(|| "a signal".into())));
                        self.finding(FindingCode::TransformCrashed, detail, at);
                        self.stage = Stage::Close;
                    }
                }
            }
            Stage::Compile => {
                let line = exec::cargo_check(&self.tree);
                let mut l = exec::line("cargo:check", &line)?;
                l.env.push(("CARGO_TARGET_DIR".into(), env.target_dir.display().to_string()));
                let plan = Plan::new(format!("exercise:{}:check", self.case.n)).then(l);
                let w = plan.walk(Watch::Quiet, |_, _| {})?.into_iter().next().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse("cargo check"))?;
                let name = format!("{} cargo check", self.spec.as_ref().map(|s| s.name.clone()).unwrap_or_default());
                let r = self.keep_line(env, &plan, &w, KIND, &name, Vec::new())?;
                if w.code == Some(0) {
                    self.case.compile = Some("compiles".into());
                    self.stage = if env.tests { Stage::Test } else { Stage::Close };
                } else {
                    let first = w.stderr.lines().find(|l| l.contains("error")).unwrap_or("does not compile").to_string();
                    self.case.compile = Some(format!("does not compile: {first}"));
                    self.finding(FindingCode::CompileError, first, Some(format!("#{}", r.seq)));
                    self.stage = Stage::Close;
                }
            }
            Stage::Test => {
                let package = self.package.clone().unwrap_or_default();
                let cargo_name = crate::reading::facts::Facts::build(&self.tree, &Scope::default(), crate::paths::Resolving::Free).ok().and_then(|f| f.k.packages.get(&package).map(|p| p.cargo_name.clone())).unwrap_or(package.clone());
                let line = exec::Cargo::test(self.tree.join("Cargo.toml").as_os_str(), cargo_name.as_str());
                let mut l = exec::line("cargo:test", &line)?;
                l.env.push(("CARGO_TARGET_DIR".into(), env.target_dir.display().to_string()));
                let plan = Plan::new(format!("exercise:{}:test", self.case.n)).then(l);
                let w = plan.walk(Watch::Quiet, |_, _| {})?.into_iter().next().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse("cargo test"))?;
                let name = format!("{} cargo test", self.spec.as_ref().map(|s| s.name.clone()).unwrap_or_default());
                let r = self.keep_line(env, &plan, &w, KIND, &name, Vec::new())?;
                if w.code == Some(0) {
                    self.case.test = Some("passed".into());
                } else {
                    let failed: Vec<&str> = w.stdout.lines().filter(|l| l.contains("FAILED") || l.contains("panicked")).take(3).collect();
                    let first = if failed.is_empty() { w.stderr.lines().find(|l| l.contains("error")).unwrap_or("tests failed").to_string() } else { failed.join(" / ") };
                    self.case.test = Some(format!("failed: {first}"));
                    self.finding(FindingCode::TestFailure, first, Some(format!("#{}", r.seq)));
                }
                self.stage = Stage::Close;
            }
            Stage::Close => {
                let closing = Instant::now();
                if let (Some(open), true) = (&self.open_count, self.tree.join("Cargo.toml").is_file()) {
                    match count_of(&self.tree) {
                        Ok(close) => {
                            let diff = crate::reading::count::compare_counts(open, &close);
                            self.case.lines_touched = diff.moved.added.lines + diff.moved.removed.lines;
                            let transforms: Vec<crate::transform::TransformRecord> = self.record.iter().cloned().collect();
                            let unc = crate::transform::coverage(&self.open, &digests(&close), &transforms);
                            for u in &unc {
                                self.case.uncovered.push(u.path.clone());
                            }
                            if !unc.is_empty() {
                                let receipt = self.case.receipts.first().map(|s| format!("#{s}"));
                                self.finding(FindingCode::Uncovered, unc.iter().map(|u| format!("{} ({} → {})", u.path, crate::digest::short(&u.from, 8), crate::digest::short(&u.now, 8))).collect::<Vec<_>>().join(", "), receipt);
                            }
                        }
                        Err(e) => self.case.note = Some(format!("no closing count: {e}")),
                    }
                }
                if self.case.status.is_empty() {
                    self.case.status = if self.case.findings.is_empty() { "passed".into() } else { "finding".into() };
                }
                self.case.millis += closing.elapsed().as_millis() as i64;
                write_whole(&self.folder.join("case.json"), (serde_json::to_string_pretty(&self.case)? + "\n").as_bytes())?;
                // the task history: each finding a diagnosis with its receipt, the case a verification
                for f in &self.case.findings {
                    let mut d = DiffDocument::record(DiffRecordKind::Diagnosis).with("diagnosis", f.line()).with("code", f.code.as_str()).with("case", f.case.to_string()).with("operation", f.operation.clone()).with("unit", f.unit.clone()).with("target", f.target.clone());
                    if let Some(t) = &f.tag {
                        d = d.with("tag", t.clone());
                    }
                    if let Some(r) = &f.receipt {
                        d = d.with("receipt", r.clone());
                    }
                    env.task.write(d)?;
                }
                let check = format!("exercise case {} {} on {}", self.case.n, self.case.operation, self.case.target);
                let passed = match self.case.status.as_str() {
                    "passed" => Some(true),
                    "finding" => Some(false),
                    _ => None,
                };
                crate::task_records::verification(env.task, &check, passed, self.case.note.as_deref().or(Some(self.case.status.as_str())))?;
                env.said.push(format!("case {} {} on {}: {} ({:.1} s)", self.case.n, self.case.operation, self.case.target, self.case.status, self.case.millis as f64 / 1000.0));
                env.cases.push(self.case.clone());
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// An intent's spec from the cheap model: its declared line run, its receipt kept; `None` when no model
    /// answered (the case is `not run`) or its answer is not a spec (a finding).
    fn model_spec(&mut self, env: &mut Env<'_>, intent: &str) -> Result<Option<ChangeSpec>> {
        let name = format!("exercise-{:03}-intent", self.case.n);
        if !exec::on_path("claude") {
            self.case.status = "not run".into();
            self.case.note = Some("not run: no model (claude is not on PATH)".into());
            return Ok(None);
        }
        let prompt = prompt(intent, &env.offered);
        let line = exec::ModelCli::print(MODEL, prompt.as_str());
        let l = exec::line("claude:print", &line)?;
        let plan = Plan::new(format!("exercise:{}:model", self.case.n)).then(l);
        let w = plan.walk(Watch::Quiet, |_, _| {})?.into_iter().next().ok_or_else(|| ReadingRefusal::LineNotStarted.refuse("claude -p"))?;
        let answer_file = self.folder.join("model-answer.txt");
        write_whole(&answer_file, w.stdout.as_bytes())?;
        let r = self.keep_line(env, &plan, &w, KIND, &format!("{name} model"), vec![env.ctx.rel(&answer_file)])?;
        if w.code != Some(0) {
            let why = format!("{} {}", w.stdout.trim(), w.stderr.trim());
            self.case.status = "not run".into();
            self.case.note = Some(format!("not run: no model ({}; receipt #{})", why.trim().lines().next().unwrap_or("no answer"), r.seq));
            return Ok(None);
        }
        let text = strip_fences(&w.stdout);
        match toml::from_str::<ChangeSpec>(&text) {
            Ok(mut s) => {
                s.name = name;
                if s.scope.write.is_empty() {
                    s.scope.write = env.crates.iter().map(|c| format!("crate:{c}")).collect();
                }
                self.case.operation = s.operation.map(|o| o.as_str().to_string()).unwrap_or_else(|| "none".into());
                self.case.target = s.target.clone();
                self.case.unit = unit_of_route(&s.target).as_str().into();
                Ok(Some(s))
            }
            Err(e) => {
                self.case.operation = "none".into();
                self.finding(FindingCode::ModelMiss, format!("the answer is not a change spec: {}", e.to_string().lines().next().unwrap_or_default()), Some(format!("#{}", r.seq)));
                Ok(None)
            }
        }
    }
}

fn empty_receipt() -> Receipt {
    Receipt {
        schema: RECEIPT_SCHEMA.into(),
        seq: 0,
        utc: String::new(),
        command: String::new(),
        name: String::new(),
        spec: None,
        spec_hash: None,
        code_digest: None,
        index_key: None,
        plan_id: String::new(),
        plan_digest: String::new(),
        operation_digest: String::new(),
        receipt_digest: String::new(),
        line: String::new(),
        exit_code: None,
        accepted: false,
        disposition: String::new(),
        reason: String::new(),
        outputs: Vec::new(),
        job: None,
        reused_seq: None,
        millis: 0,
        runner: String::new(),
    }
}

/// The unit kind a spec's target names, as near as its spelling says.
fn unit_of_route(t: &str) -> Unit {
    match t.trim().strip_prefix("crate:") {
        Some(r) if !r.contains('/') => Unit::Crate,
        Some(r) if r.ends_with("/lib.rs") || r.ends_with("/main.rs") => Unit::File,
        Some(_) => Unit::Module,
        None if last(t).chars().next().is_some_and(char::is_lowercase) => Unit::Function,
        None => Unit::Item,
    }
}

fn strip_fences(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix("```toml").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t);
    t.trim().to_string() + "\n"
}

/// The prompt an intent goes to the cheap model with: the vocabulary, the spec's shape, the targets, the intent.
pub fn prompt(intent: &str, offered: &[String]) -> String {
    let mut p = String::from("Write one change spec for custodian, in TOML. Answer with the TOML only: no prose, no code fences.\n\n");
    p.push_str("Fields: name (any), objective (the intent in your words), operation (one of the list), target (an item path or a route from the targets list), and what the operation needs.\n\nOperations (operation: what it does; needs):\n");
    for o in Operation::ALL {
        p.push_str(&format!("- {}: {}; needs: {}\n", o.as_str(), o.doc(), o.needs().join(", ")));
    }
    p.push_str(
        "\nShapes: [item] name, vis, derives = [..], fields = [{ name, ty, vis }], variants = [{ name }]; [function] name, vis, receiver (\"&self\"), params = [{ name, ty }], returns, body = [{ piece = \"value\", value = \"<expression>\" }]; functions = [ <function>, .. ]; [field] name, ty, default; [variant] name; derive = [\"Debug\"]; attribute = \"#[..]\"; trait = \"std::fmt::Display\"; [module] name, vis; [dependency] name, version or path; new_name = \"..\"; to = \"crate:<name>/src/<file>.rs\"; items = [\"<item path>\"]; feature = \"..\"; [call_sites] pass = \"<expression>\".\n",
    );
    p.push_str("\nExample:\nname = \"x\"\nobjective = \"carry a digest\"\noperation = \"add_field\"\ntarget = \"core_lib::Config\"\n\n[field]\nname = \"digest\"\nty = \"String\"\ndefault = \"String::new()\"\n");
    p.push_str("\nTargets in this project:\n");
    for t in offered {
        p.push_str(&format!("- {t}\n"));
    }
    p.push_str(&format!("\nIntent: {intent}\n"));
    p
}

impl<'s, 'a> ScopedTask<'s, Env<'a>> for CaseTask
where
    'a: 's,
{
    fn poll(&mut self, cx: &mut Cx<'_, 's, Env<'a>>) -> TaskPoll {
        let env = &mut *cx.resources;
        if env.error.is_some() {
            return TaskPoll::Ready;
        }
        // a case's time is its own polls' time: the cases interleave, one stage each per poll
        // (the closing stage adds its own before the case is written)
        let t = Instant::now();
        let stepped = self.step(env);
        if matches!(stepped, Ok(true)) {
            self.case.millis += t.elapsed().as_millis() as i64;
        }
        match stepped {
            Ok(true) => cx.yield_now(),
            Ok(false) => TaskPoll::Ready,
            Err(e) => {
                // a refusal of the harness itself (a write, the journal): every case stops
                env.error = Some(e);
                TaskPoll::Ready
            }
        }
    }
}

/// The exercise's cases, run in its job (`job start` of a job of kind `exercise`): planned from the project's index,
/// each run on a copy as a task of one scope, the summary written and its `check` receipt kept. Returns the runs
/// and what was said.
pub fn run_in_job(ctx: &Ctx, folder: &Path, job: &InJob, task: &TaskRef) -> Result<(Vec<Ran>, Vec<String>)> {
    let opened = now_utc();
    let options = options_of(folder)?;
    let dir = folder.join(crate::jobs::JOB_DATA).join(DIR);
    let ws = crate::api::Workspace::open(&ctx.root).map_err(crate::CustodianError::from)?;
    let k = ws.klepto();
    let crates: Vec<String> = k.packages.keys().cloned().collect();
    if let Some(c) = &options.krate {
        if !crates.contains(&c.replace('-', "_")) {
            return Err(ReadingRefusal::ExerciseRefused.refuse(format!("--crate {c}: no crate of that name ({})", crates.join(", "))));
        }
    }
    // the plan: every operation × unit kind it applies to, N targets each, numbered by one counter
    let counter = AtomicCounter::new(0);
    let mut planned: Vec<Planned> = Vec::new();
    let mut cells: BTreeMap<(String, String), Cell> = BTreeMap::new();
    for op in &options.ops {
        for unit in units_of(*op).iter().filter(|u| options.units.contains(u)) {
            let all = targets(k, &ctx.root, *op, *unit, options.krate.as_deref());
            let mut made = 0;
            for t in pick(options.seed, *op, all, options.n) {
                // the next number is taken only by a case that has a spec
                let n = counter.load() + 1;
                if let Some(spec) = spec_for(k, &ctx.root, *op, &t, n, &crates) {
                    counter.increment().map_err(|e| ReadingRefusal::ExerciseRefused.refuse(format!("{e:?}")))?;
                    planned.push(Planned { n, op: Some(*op), unit: Some(*unit), target: Some(t), spec: Some(spec), intent: None });
                    made += 1;
                }
            }
            if made == 0 {
                cells.insert((op.as_str().into(), unit.as_str().into()), Cell { operation: op.as_str().into(), unit: unit.as_str().into(), no_target: true, ..Cell::default() });
            }
        }
    }
    let mut offered: Vec<String> = Vec::new();
    if let Some(file) = &options.intents {
        let text = std::fs::read_to_string(file).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", file.display())))?;
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let n = counter.increment().map_err(|e| ReadingRefusal::ExerciseRefused.refuse(format!("{e:?}")))?;
            planned.push(Planned { n, op: None, unit: None, target: None, spec: None, intent: Some(line.to_string()) });
        }
        for unit in Unit::ALL {
            let mut seen = BTreeSet::new();
            for op in Operation::ALL.iter().filter(|o| units_of(**o).contains(unit)) {
                for t in targets(k, &ctx.root, *op, *unit, options.krate.as_deref()).into_iter().take(12) {
                    if seen.insert(t.route.clone()) && seen.len() <= 12 {
                        offered.push(format!("{} ({})", t.route, unit.as_str()));
                    }
                }
            }
        }
    }
    let mut env = Env {
        ctx,
        job,
        task,
        dir: dir.clone(),
        target_dir: dir.join("target"),
        tests: options.tests,
        crates: crates.clone(),
        offered,
        cases: Vec::new(),
        runs: Vec::new(),
        said: vec![format!("exercise: {} — {} cases planned", options.line(), planned.len())],
        error: None,
    };
    let tasks: Vec<CaseTask> = planned
        .into_iter()
        .map(|p| {
            let label = p.op.map(|o| o.as_str().to_string()).unwrap_or_else(|| "intent".into());
            let folder = env.dir.join("cases").join(format!("{:03}-{label}", p.n));
            let case = Case {
                n: p.n,
                operation: label.clone(),
                unit: p.unit.map(|u| u.as_str().to_string()).unwrap_or_default(),
                target: p.target.as_ref().map(|t| t.route.clone()).unwrap_or_default(),
                tag: p.intent.as_ref().map(|_| "model".to_string()),
                intent: p.intent.clone(),
                ..Case::default()
            };
            CaseTask { spec: p.spec.clone(), planned: p, stage: Stage::Copy, case, tree: folder.join("tree"), spec_path: PathBuf::new(), folder, store: None, open: BTreeMap::new(), open_count: None, preflight_seq: None, record: None, package: None }
        })
        .collect();
    let report = run_scoped(&mut env, |s| {
        for t in tasks {
            s.spawn_named(format!("case-{:03}", t.case.n), t);
        }
    });
    if let Some(e) = env.error {
        return Err(e);
    }
    if !report.all_completed() {
        return Err(ReadingRefusal::LineNotStarted.refuse(format!("the exercise's cases did not all finish: {report:?}")));
    }
    let mut cases = env.cases;
    cases.sort_by_key(|c| c.n);
    let mut summary = Summary::of(&job.id, &options, opened, cases, &cells);
    // the job's verification: a check receipt, accepted when nothing was found
    let store = ctx.store()?;
    let json = serde_json::to_string_pretty(&summary)? + "\n";
    let digest = crate::digest::tagged(&json);
    let r = Receipt {
        schema: RECEIPT_SCHEMA.into(),
        utc: now_utc(),
        command: "check".into(),
        name: CHECK_NAME.into(),
        plan_id: format!("exercise:{}", job.id),
        plan_digest: digest.clone(),
        operation_digest: digest.clone(),
        receipt_digest: digest,
        line: format!("custodian exercise run ({})", options.line()),
        exit_code: Some(if summary.findings.is_empty() { 0 } else { 1 }),
        accepted: summary.findings.is_empty(),
        disposition: "Continue".into(),
        reason: summary.headline(),
        outputs: vec![ctx.rel(&dir.join(SUMMARY_FILE)), ctx.rel(&dir.join(SUMMARY_PAGE))],
        job: Some(job.id.clone()),
        runner: crate::receipts::RUNNER_IN_PROCESS.into(),
        ..empty_receipt()
    };
    let (r, f) = keep(store, r)?;
    summary.receipt = Some(r.seq);
    write_whole(&dir.join(SUMMARY_FILE), (serde_json::to_string_pretty(&summary)? + "\n").as_bytes())?;
    write_whole(&dir.join(SUMMARY_PAGE), markdown(&summary).as_bytes())?;
    crate::task_records::verification(task, &format!("custodian exercise ({})", options.line()), Some(summary.findings.is_empty()), Some(&summary.headline()))?;
    let mut said = env.said;
    said.push(summary.headline());
    said.push(format!("summary: {} · check receipt #{}", ctx.rel(&dir.join(SUMMARY_PAGE)), r.seq));
    let mut runs = env.runs;
    runs.push(Ran { receipt: Some(r), receipt_file: Some(f), summary: None, reused: None, printed: None, said: Vec::new() });
    Ok((runs, said))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_applies_to_a_unit_and_the_word_holds_the_options() {
        for op in Operation::ALL {
            assert!(!units_of(*op).is_empty(), "{}", op.as_str());
        }
        let o = Options::from_words(Some("add_field,rename"), Some("item"), Some("2"), true, Some("crate:core_lib"), None, Some("7")).unwrap();
        let w = ExerciseWord::from_raw(o.word);
        assert_eq!(w.operations(), [Operation::AddField, Operation::Rename]);
        assert_eq!(w.unit_kinds(), [Unit::Item]);
        assert_eq!(w.names(), ["tests", "one_crate"]);
        assert_eq!((o.n, o.seed, o.krate.as_deref()), (2, 7, Some("core_lib")));
        // every operation and unit kind when none is named
        let all = Options::from_words(None, None, None, false, None, None, None).unwrap();
        assert_eq!(ExerciseWord::from_raw(all.word).operations().len(), Operation::ALL.len());
        assert_eq!(ExerciseWord::from_raw(all.word).unit_kinds().len(), Unit::ALL.len());
        // a word that is not a vocabulary's is refused, with what it may be
        let e = Options::from_words(Some("inline"), None, None, false, None, None, None).unwrap_err();
        assert!(e.to_string().contains("unknown operation `inline`"), "{e}");
        assert!(Options::from_words(None, Some("trait"), None, false, None, None, None).is_err());
        assert!(Options::from_words(None, None, Some("0"), false, None, None, None).is_err());
    }

    #[test]
    fn the_word_s_rule_is_checked_by_flaga() {
        // the model asked with no intents breaks `model => intents`
        let model = ExerciseWord::LABELS.iter().find(|l| l.label == "model").unwrap().value as u8;
        let w = ExerciseWord::new(1, 1, model);
        let e = w.check().unwrap_err();
        assert!(e.to_string().contains("CUST0120"), "{e}");
        let intents = ExerciseWord::LABELS.iter().find(|l| l.label == "intents").unwrap().value as u8;
        assert!(ExerciseWord::new(1, 1, model | intents).check().is_ok());
        // a word that is not the options' own is refused
        let mut o = Options::from_words(Some("add_field"), None, None, false, None, None, None).unwrap();
        o.tests = true;
        assert!(o.checked().is_err());
    }

    #[test]
    fn the_pick_is_the_seed_s() {
        let ts: Vec<Target> = (0..20).map(|i| Target { unit: Unit::Item, route: format!("c::T{i}"), package: "c".into() }).collect();
        let a = pick(1, Operation::Rename, ts.clone(), 3);
        assert_eq!(a, pick(1, Operation::Rename, ts.clone(), 3), "the same seed picks the same");
        assert_ne!(a, pick(2, Operation::Rename, ts.clone(), 3), "another seed picks others");
        assert_eq!(pick(1, Operation::Rename, ts, 30).len(), 20);
    }

    #[test]
    fn a_route_names_its_unit_and_an_answer_loses_its_fences() {
        assert_eq!(unit_of_route("crate:core_lib"), Unit::Crate);
        assert_eq!(unit_of_route("crate:core_lib/src/lib.rs"), Unit::File);
        assert_eq!(unit_of_route("crate:core_lib/src/store.rs"), Unit::Module);
        assert_eq!(unit_of_route("core_lib::Config"), Unit::Item);
        assert_eq!(unit_of_route("core_lib::load"), Unit::Function);
        assert_eq!(strip_fences("```toml\nname = \"x\"\n```"), "name = \"x\"\n");
        assert_eq!(relative_path(Path::new("/w/crates/a"), Path::new("/w/crates/b")), "../b");
        let p = prompt("add a field", &["core_lib::Config (item)".into()]);
        assert!(p.contains("- add_field:") && p.contains("core_lib::Config (item)") && p.ends_with("Intent: add a field\n"));
    }
}
