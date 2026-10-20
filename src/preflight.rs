// Copyright © 2026 1o. All rights reserved.

//! The preflight: what a change spec entails over the code as it is, and
//! whether it is authorized, decided before anything is written.
//!
//! In order: the spec is read and checked; the target is resolved through the
//! scan and the place the operation lands is named ([`crate::change::landing`]);
//! the impact of the change is read (the `impact` reading, through the
//! reading path, so the same spec over the same code is a reuse); the
//! operation's preconditions are checked (the target is what the operation
//! needs, the name it makes is free, the trait or feature it names exists, a
//! new field has the default its struct literals need, ...); the files the
//! operation writes are set against the routes the spec is authorized to
//! write, and a file outside them is a scope expansion asked for, never taken;
//! the decisions the spec leaves open, and any answered before, are read; a
//! `raw` step is flagged. The findings make a big-bad-bool word, and the
//! word's own rules, checked by flaga_solver, decide the authorization: the
//! broken rules are the reasons. A bounded job's preflight needs the job's
//! target and outcome known.
//!
//! The record is kept (`data/preflight/<spec>/<stamp>.json` and `.md`) and a
//! receipt names it (`command` `preflight`, the spec's digest, the code's
//! digest, `reason` `authorized` or `not authorized`). A code-changing step
//! runs only on such a receipt of the same spec over the same code
//! ([`gate`]): a spec edited, or code moved, since its preflight is
//! preflighted again.

use crate::change::{landing, At, ChangeSpec, Compatibility, Landing, Operation};
use crate::jobs::JobMode;
use crate::reading::facts::Facts;
use crate::reading::impact::ImpactRecord;
use crate::reading::refusal::ReadingRefusal;
use crate::runs::{Ctx, InJob, Runner};
use crate::store::{named, write_whole, Receipt, Store, RECEIPT_SCHEMA};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const PREFLIGHT_SCHEMA: &str = "custodian.preflight.v1";

pub use word::PreflightWord;

/// The word, in a module of its own: the derive writes `Result` as big-bad-bool's.
mod word {
    use big_bad_bool::bitpack;

    /// What the preflight found, as one word; its rules decide `authorized`.
    #[bitpack(storage = u32, version = 1)]
    pub struct PreflightWord {
        #[bits(11)]
        #[describe("what a preflight found about a change spec, and whether the change is authorized")]
        #[flags(resolved = 0, preconditions = 1, in_scope = 2, free_name = 3, decisions_open = 4, raw = 5, bounded = 6, target_known = 7, outcome_known = 8, bounded_ready = 9, authorized = 10)]
        #[labels(resolved = 1, preconditions = 2, in_scope = 4, free_name = 8, decisions_open = 16, raw = 32, bounded = 64, target_known = 128, outcome_known = 256, bounded_ready = 512, authorized = 1024)]
        #[requires(authorized => facts.resolved)]
        #[requires(authorized => facts.preconditions)]
        #[requires(authorized => facts.in_scope)]
        #[requires(authorized => facts.free_name)]
        #[requires(authorized => facts.bounded_ready)]
        #[conflicts(authorized, decisions_open)]
        facts: u16,
    }
}

impl PreflightWord {
    /// The names of the flags set, in bit order.
    pub fn names(self) -> Vec<&'static str> {
        Self::FLAGS.iter().filter(|f| self.raw() as u128 & f.mask != 0).map(|f| f.name).collect()
    }

    fn set(self, name: &str, on: bool) -> Self {
        let mask = Self::FLAGS.iter().find(|f| f.name == name).map(|f| f.mask as u32).unwrap_or(0);
        Self::from_raw(if on { self.raw() | mask } else { self.raw() & !mask })
    }

    /// The facts with `authorized` asked for, checked by the word's rules: authorized when they hold, else
    /// the rules it breaks.
    pub fn decide(self) -> (PreflightWord, Vec<String>) {
        let asked = self.set("authorized", true);
        let verdict = flaga_solver::check(&flaga_solver::schema_of!(PreflightWord), asked.raw() as u128);
        if verdict.is_ok() {
            (asked, Vec::new())
        } else {
            (self, verdict.to_string().split("; ").map(str::to_string).collect())
        }
    }
}

/// One precondition and whether it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub what: String,
    pub holds: bool,
    pub detail: String,
}

/// The job a preflight runs in, as the job's file says it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobView {
    pub id: String,
    pub mode: JobMode,
    pub target_known: bool,
    pub outcome_known: bool,
}

/// A preflight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preflight {
    pub schema: String,
    pub spec_name: String,
    /// The spec file, relative to the root.
    pub spec: String,
    pub spec_digest: String,
    pub operation: String,
    pub objective: String,
    pub target: String,
    pub code_digest: String,
    pub index_key: Option<String>,
    /// What the target resolves to, and where the operation lands.
    pub landing: Option<Landing>,
    pub impact: Option<ImpactRecord>,
    /// The impact reading's receipt (its own, or the one it reused).
    pub impact_seq: Option<i64>,
    pub preconditions: Vec<Check>,
    /// Names the operation would make that are taken.
    pub collisions: Vec<String>,
    /// The files the operation writes, relative to the root.
    pub write_set: Vec<String>,
    /// The routes the spec may write (its scope, or the landing file when it names none).
    pub authorized_scope: Vec<String>,
    /// Files of the write set outside the authorized scope: a scope expansion to ask for.
    pub outside: Vec<String>,
    /// Sites the operation lists for a person and does not edit.
    pub listed: Vec<String>,
    /// `raw` steps, code blocks, and `value`/`let` steps that carry a closure or a macro call.
    pub raw: Vec<String>,
    /// The decisions still open, asked together.
    pub decisions: Vec<crate::questions::Question>,
    /// The ones answered before: not asked again.
    pub answered: Vec<crate::questions::Decision>,
    pub job: Option<JobView>,
    /// The word's raw value and the names of its flags.
    pub word: u32,
    pub flags: Vec<String>,
    pub authorized: bool,
    /// The rules the facts break, when not authorized.
    pub reasons: Vec<String>,
    /// Why it is not authorized, in words: each precondition that does not hold, each name taken, each file outside
    /// the scope, each decision open. The rules in `reasons` are what decide it; these are what a person fixes.
    #[serde(default)]
    pub why: Vec<String>,
    /// What the spec asks for is in the code already (the item gated by the feature, the dependency named, the
    /// feature declared as asked, the library there): the refusal is a change already made, not one waiting.
    #[serde(default)]
    pub in_place: Vec<String>,
    pub utc: String,
    pub millis: i64,
}

impl Preflight {
    /// Why it is not authorized, in one line: what is in place already, else each reason in words, else the rules
    /// it breaks; `None` when it is authorized.
    pub fn headline_why(&self) -> Option<String> {
        if self.authorized {
            return None;
        }
        Some(if !self.in_place.is_empty() {
            format!("already in place: {}", self.in_place.join("; "))
        } else if !self.why.is_empty() {
            self.why.join("; ")
        } else {
            self.reasons.join("; ")
        })
    }

    /// The receipt's reason: `authorized`, `not authorized`, or `not authorized: already in place`.
    pub fn reason(&self) -> &'static str {
        if self.authorized {
            AUTHORIZED
        } else if !self.in_place.is_empty() {
            ALREADY_IN_PLACE
        } else {
            NOT_AUTHORIZED
        }
    }

    /// The page a person reads: the decision first.
    pub fn markdown(&self) -> String {
        let mut s = format!("# preflight — {} ({})\n\n", self.spec_name, self.operation);
        s.push_str(&format!("- decision: **{}**{}\n", if self.authorized { "authorized" } else { "not authorized" }, self.headline_why().map(|w| format!(" — {w}")).unwrap_or_default()));
        if !self.reasons.is_empty() {
            s.push_str(&format!("- the rules it breaks: {}\n", self.reasons.join("; ")));
        }
        s.push_str(&format!("- objective: {}\n- target: `{}`\n", self.objective, self.target));
        if let Some(l) = &self.landing {
            s.push_str(&format!("- lands: {}\n", l.line()));
        }
        if let Some(j) = &self.job {
            s.push_str(&format!("- job: {} ({}; target known: {}, outcome known: {})\n", j.id, j.mode.as_str(), j.target_known, j.outcome_known));
        }
        s.push_str(&format!("- spec digest: `{}` · code digest: `{}`\n- flags: {}\n", self.spec_digest, self.code_digest, self.flags.join(", ")));
        if !self.decisions.is_empty() {
            s.push_str("\n## decisions to make first\n\n");
            for q in &self.decisions {
                s.push_str(&format!("- `{}` {}{}{}\n", q.id, q.question, if q.options.is_empty() { String::new() } else { format!(" — options: {}", q.options.join(" | ")) }, q.why.as_ref().map(|w| format!(" ({w})")).unwrap_or_default()));
            }
        }
        if !self.answered.is_empty() {
            s.push_str("\n## answered before (not asked again)\n\n");
            for d in &self.answered {
                s.push_str(&format!("- `{}` {} → {} ({})\n", d.id, d.question, d.answer, d.task));
            }
        }
        s.push_str("\n## preconditions\n\n");
        for c in &self.preconditions {
            s.push_str(&format!("- [{}] {} — {}\n", if c.holds { "x" } else { " " }, c.what, c.detail));
        }
        if !self.collisions.is_empty() {
            s.push_str(&format!("\n## names taken\n\n{}\n", self.collisions.iter().map(|c| format!("- {c}\n")).collect::<String>()));
        }
        s.push_str("\n## what it writes\n\n");
        for w in &self.write_set {
            s.push_str(&format!("- `{w}`{}\n", if self.outside.contains(w) { " — outside the authorized scope" } else { "" }));
        }
        s.push_str(&format!("\nauthorized to write: {}\n", self.authorized_scope.join(", ")));
        if !self.listed.is_empty() {
            s.push_str("\n## listed, not edited\n\n");
            for l in &self.listed {
                s.push_str(&format!("- {l}\n"));
            }
        }
        if !self.raw.is_empty() {
            s.push_str("\n## code written as given (flagged: raw steps, code blocks, values with a closure or a macro call)\n\n");
            for r in &self.raw {
                s.push_str(&format!("- `{}`\n", r.replace('`', "'")));
            }
        }
        if let Some(i) = &self.impact {
            s.push_str(&format!("\n## impact (reading #{})\n\n{}\n\n| dimension | value | basis | from |\n|---|---|---|---|\n", self.impact_seq.map(|n| n.to_string()).unwrap_or_else(|| "-".into()), i.line()));
            for d in &i.dimensions {
                s.push_str(&format!("| {} | {} | {} | {} |\n", d.name, d.value.map(|v| v.to_string()).unwrap_or_else(|| "unknown".into()), d.basis.as_str(), d.note));
            }
        }
        s
    }
}

/// The running job, as its file says it now.
pub fn job_view(store: &Store) -> Option<(JobView, PathBuf)> {
    let dir = store.jobs_dir(crate::store::JobState::Running).ok()?;
    let folder = std::fs::read_dir(&dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).min()?;
    let jf = crate::jobs::job_file(&folder);
    let mode = jf.mode.unwrap_or_else(|| store.settings().ok().and_then(|s| s.job_mode).unwrap_or(JobMode::Unbounded));
    let view = JobView { id: jf.id.clone().unwrap_or_default(), mode, target_known: jf.target_known.unwrap_or(false), outcome_known: !jf.verify.is_empty() };
    Some((view, folder))
}

/// The files under a route, relative to the root, as prefixes (`crates/x/`, `crates/x/src/y.rs`).
fn route_prefixes(f: &Facts, route: &str) -> Vec<String> {
    match f.paths.resolve(route, crate::paths::Resolving::Free) {
        Ok(r) => match (r.crate_name(), r.crate_file(&f.root)) {
            (Some(c), _) => f.packages.get(c).map(|p| vec![format!("{}/", p.dir).trim_start_matches('/').to_string()]).unwrap_or_default(),
            (None, Some(file)) => vec![file],
            _ => r.globs(),
        },
        Err(_) => Vec::new(),
    }
}

fn within(prefixes: &[String], file: &str) -> bool {
    prefixes.iter().any(|p| file == p || (p.ends_with('/') && file.starts_with(p.as_str())) || (p.is_empty()))
}

/// The traits the standard library gives, which the scan does not declare.
const STD_TRAITS: &[&str] = &["Clone", "Copy", "Debug", "Default", "Display", "Eq", "PartialEq", "Ord", "PartialOrd", "Hash", "From", "Into", "AsRef", "AsMut", "Deref", "DerefMut", "Drop", "Error", "Iterator", "IntoIterator", "FromStr", "TryFrom", "TryInto", "Send", "Sync", "Borrow", "ToString", "Add", "Sub", "Mul", "Div", "Neg", "Not", "Index", "IndexMut", "Fn", "FnMut", "FnOnce", "Serialize", "Deserialize"];

/// The findings for `spec` over `f`, with `impact` already read.
#[allow(clippy::too_many_arguments)]
pub fn assess(f: &Facts, store: Option<&Store>, spec_rel: &str, spec: &ChangeSpec, impact: Option<(ImpactRecord, Option<i64>, Vec<crate::CodeQueryResult>)>, job: Option<JobView>, code_digest: &str, index_key: Option<String>) -> Result<Preflight> {
    let started = std::time::Instant::now();
    let op = spec.op();
    let mut pre = Vec::new();
    let mut collisions = Vec::new();
    let mut listed = Vec::new();
    let mut write: BTreeSet<String> = BTreeSet::new();
    // the checks that, failing, say the change is in the code already
    let mut in_place_checks: Vec<String> = Vec::new();
    let mut questions: Vec<crate::questions::Question> = spec.decisions.iter().map(|q| crate::questions::question(&q.question, &q.options, q.why.as_deref(), &format!("change:{}", spec.name))).collect();
    let land = landing(&f.k, &f.root, spec);
    let resolved = land.is_ok();
    pre.push(Check { what: "the target resolves".into(), holds: resolved, detail: match &land { Ok(l) => l.line(), Err(e) => e.clone() } });
    let todo = crate::plan::has_todo(spec);
    pre.push(Check { what: "no TODO is left in the spec".into(), holds: !todo, detail: if todo { "a plan's approval left TODO where the operation's details go: fill them in".into() } else { "filled".into() } });
    let rows = impact.as_ref().map(|(_, _, r)| r.clone()).unwrap_or_default();
    let kind_rows = |k: &str| rows.iter().filter(move |r| r.kind == k).cloned().collect::<Vec<_>>();
    let preserve = spec.compatibility != Some(Compatibility::Break);
    let ty = land.as_ref().ok().and_then(|l| l.item.as_ref()).and_then(|id| f.k.type_decl(id));
    if let Ok(l) = &land {
        write.insert(l.file.clone());
        if let At::NewFile { parent } = &l.at {
            if !parent.is_empty() {
                write.insert(parent.clone());
            }
        }
        let module = f.k.modules.get(&l.module);
        let name_free = |name: &str| module.is_none_or(|m| !m.items.contains_key(name));
        let makes = op.makes(spec);
        match op {
            Operation::MakeStruct | Operation::MakeEnum | Operation::MakeFn | Operation::AddModule | Operation::Extract => {
                if let Some(n) = &makes {
                    let free = name_free(n);
                    if !free {
                        collisions.push(format!("{}::{n} is already declared", l.module.rsplit_once("::").map(|(m, _)| m).filter(|_| matches!(op, Operation::AddModule | Operation::Extract)).unwrap_or(&l.module)));
                    }
                    pre.push(Check { what: format!("{n} is free in {}", l.module), holds: free, detail: if free { "no item of that name there".into() } else { "taken".into() } });
                }
                if f.root.join(&l.file).exists() && matches!(l.at, At::NewFile { .. }) {
                    collisions.push(format!("{} already exists", l.file));
                }
                if op == Operation::AddModule {
                    let n = makes.clone().unwrap_or_default();
                    let root_name = ["lib", "main", "mod"].contains(&n.as_str());
                    pre.push(Check { what: format!("{n} is no crate root's file name"), holds: !root_name, detail: if root_name { format!("a module named {n} is no crate root: make_lib makes src/lib.rs, the library beside the binary") } else { "a module".into() } });
                }
            }
            Operation::MakeLib => {
                let lib = f.k.packages.get(&l.package.replace('-', "_")).and_then(|p| p.targets.iter().find(|t| t.kind == "lib" || t.kind == "proc-macro"));
                pre.push(Check { what: format!("{} has no library yet", l.package), holds: lib.is_none(), detail: match lib { Some(t) => format!("it has one: {}", t.src_path.display()), None => "its binary is its only root".into() } });
                in_place_checks.push(format!("{} has no library yet", l.package));
                if f.root.join(&l.file).exists() {
                    collisions.push(format!("{} already exists", l.file));
                }
            }
            Operation::AddField => {
                let t = ty.filter(|t| t.variants.is_empty());
                pre.push(Check { what: "the target is a struct".into(), holds: t.is_some(), detail: ty.map(|t| format!("{:?}", t.kind)).unwrap_or_else(|| "not a type".into()) });
                let name = spec.field.as_ref().map(|x| x.name.clone()).unwrap_or_default();
                if t.is_some_and(|t| t.fields.iter().any(|x| x.name.as_deref() == Some(name.as_str()))) {
                    collisions.push(format!("{} already has a field {name}", spec.target));
                }
                let literals: Vec<_> = kind_rows("construct").into_iter().filter(|r| r.attributes.get("how").is_some_and(|h| h == "struct literal")).collect();
                let default = spec.field.as_ref().and_then(|x| x.default.clone());
                for r in &literals {
                    let at = format!("{}:{}", r.path.clone().unwrap_or_default(), r.line.unwrap_or(0));
                    match &default {
                        Some(_) => {
                            write.insert(r.path.clone().unwrap_or_default());
                        }
                        None => listed.push(format!("struct literal {at}: needs the new field")),
                    }
                }
                pre.push(Check { what: "every struct literal gets the new field".into(), holds: !preserve || default.is_some() || literals.is_empty(), detail: format!("{} literals; {}", literals.len(), if default.is_some() { "the default fills them" } else { "no default given" }) });
                // a literal can name the new field only where the field is visible: a private one in the struct's module
                // and the modules inside it, a `pub(crate)` or `pub(super)` one in the struct's crate, a `pub` one anywhere
                let vis = spec.field.as_ref().and_then(|x| x.vis.clone()).unwrap_or_default();
                let module_of = |p: &str| crate::change::module_of_file(&f.k, &f.root.join(p)).map(|m| m.path.clone());
                let blind: Vec<String> = literals
                    .iter()
                    .filter_map(|r| {
                        let p = r.path.clone().unwrap_or_default();
                        let sees = match vis.trim() {
                            "pub" => true,
                            v if v.starts_with("pub(") => f.package_of_rel(&p).map(|k| k.name.as_str()) == Some(l.package.as_str()),
                            _ => module_of(&p).is_some_and(|m| m == l.module || m.starts_with(&format!("{}::", l.module))),
                        };
                        (!sees).then(|| format!("{p}:{}", r.line.unwrap_or(0)))
                    })
                    .collect();
                if default.is_some() {
                    pre.push(Check { what: "every struct literal can name the new field".into(), holds: !preserve || blind.is_empty(), detail: if blind.is_empty() { format!("the field is visible at every literal ({})", if vis.is_empty() { "private" } else { vis.as_str() }) } else { format!("{} cannot see a {} field: {}", blind.len(), if vis.is_empty() { "private" } else { vis.as_str() }, blind.join(", ")) } });
                }
            }
            Operation::AddVariant => {
                let t = ty.filter(|t| !t.variants.is_empty());
                pre.push(Check { what: "the target is an enum".into(), holds: t.is_some(), detail: ty.map(|t| format!("{:?}", t.kind)).unwrap_or_else(|| "not a type".into()) });
                let name = spec.variant.as_ref().map(|v| v.name.clone()).unwrap_or_default();
                if t.is_some_and(|t| t.variants.iter().any(|v| v.name == name)) {
                    collisions.push(format!("{} already has a variant {name}", spec.target));
                }
                let open: Vec<_> = kind_rows("match").into_iter().filter(|r| r.attributes.get("catch_all").is_some_and(|c| c == "false")).collect();
                for r in &open {
                    listed.push(format!("match without a catch-all {}:{}: needs an arm for {name}", r.path.clone().unwrap_or_default(), r.line.unwrap_or(0)));
                }
                pre.push(Check { what: "no match stops compiling".into(), holds: !preserve || open.is_empty(), detail: format!("{} matches name every variant with no catch-all arm", open.len()) });
            }
            Operation::AddMethod | Operation::AddConstructor | Operation::MakeImpl | Operation::AddTraitImpl | Operation::AddDerive => {
                pre.push(Check { what: "the target is a type".into(), holds: ty.is_some(), detail: ty.map(|t| t.id.clone()).unwrap_or_else(|| "not a type".into()) });
                if let (Some(t), Some(fname)) = (ty, spec.function.as_ref().map(|x| x.name.clone())) {
                    let taken = f.k.impls_of(&t.id).iter().flat_map(|i| i.methods.iter()).any(|m| m.rsplit("::").next() == Some(fname.as_str()));
                    if taken {
                        collisions.push(format!("{}::{fname} is already a method", t.name));
                    }
                }
                if op == Operation::AddTraitImpl {
                    let tr = spec.trait_name.clone().unwrap_or_else(|| spec.generator.as_ref().map(|g| g.piece.trim_start_matches("impl_").to_string()).unwrap_or_default());
                    let short = tr.rsplit("::").next().unwrap_or(&tr).to_string();
                    let local = f.k.types.iter().any(|x| x.name == short && matches!(x.kind, klepto::TypeKind::Trait));
                    let std = STD_TRAITS.contains(&short.as_str());
                    pre.push(Check { what: format!("the trait {tr} exists"), holds: local || std || spec.generator.is_some(), detail: if local { "declared in the workspace".into() } else if std { "the standard library's (the compiler checks it)".into() } else if spec.generator.is_some() { "a toolkit impl generator names it".into() } else { "not declared in the workspace".into() } });
                    if let Some(t) = ty {
                        if f.k.impls_of(&t.id).iter().any(|i| i.trait_.as_ref().is_some_and(|x| x.raw.rsplit("::").next() == Some(short.as_str()))) {
                            collisions.push(format!("{} already implements {short}", t.name));
                        }
                    }
                }
                if op == Operation::AddDerive {
                    if let Some(t) = ty {
                        for d in &spec.derive {
                            if t.derives.iter().any(|x| Facts::canonical(x).is_some_and(|p| p.rsplit("::").next() == Some(d.rsplit("::").next().unwrap_or(d)))) {
                                collisions.push(format!("{} already derives {d}", t.name));
                            }
                        }
                    }
                }
            }
            Operation::AddDependency => {
                let dep = spec.dependency.as_ref().map(|d| d.name.replace('-', "_")).unwrap_or_default();
                let has = f.k.features.get(&l.package).is_some_and(|m| m.dependencies.iter().any(|d| d.name.replace('-', "_") == dep));
                pre.push(Check { what: format!("{} does not depend on {dep} yet", l.package), holds: !has, detail: if has { "it does: nothing to add".into() } else { "a new line".into() } });
                in_place_checks.push(format!("{} does not depend on {dep} yet", l.package));
                if spec.dependency.as_ref().and_then(|d| d.optional) == Some(true) {
                    listed.push(format!("{dep} is optional: cargo gives it the feature {dep} until a feature names it dep:{dep} (declare_feature)"));
                }
            }
            Operation::DeclareFeature => match crate::change::feature_declaration(&f.k, &f.root, spec) {
                Ok(d) => {
                    let was = match &d.was {
                        Some(w) => format!("declared now as {w:?}, becomes {:?}", d.enables),
                        None => format!("a new feature: {:?}", d.enables),
                    };
                    let same = d.was.as_ref() == Some(&d.enables);
                    pre.push(Check { what: format!("{} can declare {} as asked", d.package, d.feature), holds: d.problems.is_empty(), detail: if d.problems.is_empty() { was } else { d.problems.join("; ") } });
                    if same && d.problems.len() == 1 {
                        in_place_checks.push(format!("{} can declare {} as asked", d.package, d.feature));
                    }
                    for n in &d.made_optional {
                        listed.push(format!("{n} becomes optional: dep:{n} turns it on, so code that uses it compiles only with {} on", d.feature));
                    }
                    for b in &d.brings {
                        listed.push(format!("{} brings {b}", d.feature));
                    }
                    for w in &d.weak {
                        listed.push(format!("{} weak: {w}", d.feature));
                    }
                    for u in &d.unchecked {
                        listed.push(format!("{} not checked: {u}", d.feature));
                    }
                }
                Err(e) => pre.push(Check { what: "the feature's crate".into(), holds: false, detail: e }),
            },
            Operation::Gate => {
                let feat = spec.feature.clone().unwrap_or_default();
                let declared = f.k.features.get(&l.package).is_some_and(|m| m.features.contains_key(&feat));
                pre.push(Check { what: format!("{} declares the feature {feat}", l.package), holds: declared, detail: if declared { "declared".into() } else { "declare it first (declare_feature)".into() } });
                // a use outside the item stops compiling without the feature, unless it is gated by the feature too, or
                // sits in a crate whose dependency on this one turns the feature on
                let item = l.item.clone().unwrap_or_default();
                let needs = |c: &klepto::CfgPredicate| -> bool {
                    fn needs(c: &klepto::CfgPredicate, feat: &str) -> bool {
                        match c {
                            klepto::CfgPredicate::Feature(x) => x == feat,
                            klepto::CfgPredicate::All(v) => v.iter().any(|c| needs(c, feat)),
                            _ => false,
                        }
                    }
                    needs(c, &feat)
                };
                let turned_on = |pkg: &str| f.k.features.get(pkg).is_some_and(|m| m.dependencies.iter().any(|d| d.package.replace('-', "_") == l.package && d.features.contains(&feat)));
                let ungated = |r: &crate::CodeQueryResult| {
                    let user = r.attributes.get("item").cloned().unwrap_or_default();
                    if user == item || user.starts_with(&format!("{item}::")) {
                        return false;
                    }
                    let gated = crate::change::item_named(&f.k, &user).is_some_and(|d| needs(&d.cfg)) || f.k.functions.iter().any(|x| x.fq_name == user && needs(&x.cfg));
                    let pkg = r.path.as_deref().and_then(|p| f.package_of_rel(p)).map(|p| p.name.clone()).unwrap_or_default();
                    !gated && !(pkg != l.package && turned_on(&pkg))
                };
                // an item the feature gates already: a second gate writes a second, identical #[cfg] line
                let own = crate::change::item_named(&f.k, &item).map(|d| needs(&d.cfg)).unwrap_or(false) || f.k.functions.iter().any(|x| x.fq_name == item && needs(&x.cfg));
                pre.push(Check { what: format!("{item} is not gated by {feat} yet"), holds: !own, detail: if own { format!("its cfg already needs {feat}: nothing to gate") } else { "ungated".into() } });
                in_place_checks.push(format!("{item} is not gated by {feat} yet"));
                let uses: Vec<String> = kind_rows("use-site").iter().filter(|r| ungated(r)).map(|r| format!("{}:{} ({})", r.path.clone().unwrap_or_default(), r.line.unwrap_or(0), r.attributes.get("item").cloned().unwrap_or_default())).collect();
                for u in &uses {
                    listed.push(format!("use {u}: stops compiling without {feat} unless it is gated too"));
                }
                pre.push(Check { what: format!("nothing outside {item} uses it ungated"), holds: !preserve || uses.is_empty(), detail: if uses.is_empty() { "no uses".into() } else { format!("{} uses outside it (compatibility break gates it anyway)", uses.len()) } });
            }
            Operation::Anchor => {
                let name = spec.linkage.clone().unwrap_or_default();
                let exists = store.and_then(|s| crate::linkage::load_all(s).ok()).is_some_and(|all| all.iter().any(|(_, r)| r.name == name));
                pre.push(Check { what: format!("the linkage record {name} exists"), holds: exists, detail: if exists { "recorded".into() } else { "record it first (custodian linkage add)".into() } });
            }
            Operation::Rename => {
                let n = spec.new_name.clone().unwrap_or_default();
                let free = name_free(&n);
                if !free {
                    collisions.push(format!("{}::{n} is already declared", l.module));
                }
                for r in kind_rows("use-site").iter().chain(kind_rows("definition").iter()) {
                    if let Some(p) = &r.path {
                        write.insert(p.clone());
                    }
                }
                let in_macros = rows.iter().filter(|r| r.kind == "macro-token").count();
                if in_macros > 0 {
                    listed.push(format!("{in_macros} uses inside macro tokens: klepto resolves no paths there; not renamed"));
                }
            }
            Operation::ChangeSignature => {
                if let Some(n) = spec.function.as_ref().map(|f| f.body.len()).filter(|n| *n > 0) {
                    listed.push(format!("its body is written with the signature: {n} pieces, proved together"));
                }
                let item = l.item.clone().unwrap_or_default();
                // the calls: klepto's resolved ones, and the ones inside a macro's arguments whose path resolves to it
                // (crate::change::macro_calls, the rule the writer rewrites by)
                let calls: Vec<_> = kind_rows("use-site");
                let in_macros = crate::change::macro_calls(&f.k, &item);
                let mut places: BTreeSet<(String, u32)> = calls.iter().filter_map(|r| Some((r.path.clone()?, r.line?))).collect();
                let resolved_n = places.len();
                places.extend(in_macros.iter().map(|m| (f.rel(&m.file), m.line)));
                let macro_n = places.len() - resolved_n;
                // what klepto could not resolve that could be a call of it, less the places a call is known at
                let mut unresolved: BTreeMap<(String, u32), String> = BTreeMap::new();
                for r in rows.iter().filter(|r| r.kind != "macro-token" && r.kind.ends_with("unresolved")) {
                    let at = (r.path.clone().unwrap_or_default(), r.line.unwrap_or(0));
                    if !places.contains(&at) {
                        unresolved.entry(at).or_insert_with(|| r.label.clone());
                    }
                }
                for m in crate::change::unresolved_macro_paths(&f.k, &item) {
                    let at = (f.rel(&m.file), m.line);
                    if !places.contains(&at) {
                        unresolved.entry(at).or_insert_with(|| format!("{} in {}!", m.paths.join(", "), m.macro_path));
                    }
                }
                let pass = spec.call_sites.as_ref().and_then(|c| c.pass.clone());
                let allow = spec.call_sites.as_ref().is_some_and(|c| c.allow_unresolved);
                let adds = spec.function.as_ref().is_some_and(|nf| f.k.functions.iter().find(|x| x.fq_name == item).is_some_and(|old| nf.params.len() > old.args.iter().filter(|a| !crate::change::is_receiver(a)).count()));
                if adds {
                    match &pass {
                        Some(_) => places.iter().for_each(|(p, _)| {
                            write.insert(p.clone());
                        }),
                        None => places.iter().for_each(|(p, line)| listed.push(format!("call {p}:{line}: needs the new argument"))),
                    }
                    pre.push(Check { what: "every call site gets the new argument".into(), holds: !preserve || pass.is_some() || places.is_empty(), detail: format!("{} call sites ({resolved_n} resolved by klepto, {macro_n} inside a macro's arguments)", places.len()) });
                }
                let shown: Vec<String> = unresolved.iter().take(8).map(|((p, line), what)| format!("{p}:{line} ({what})")).collect();
                for ((p, line), what) in &unresolved {
                    listed.push(format!("unresolved call site {p}:{line} ({what}): klepto could not resolve it{}", if allow { ", not changed (call_sites.allow_unresolved)" } else { "" }));
                }
                pre.push(Check {
                    what: "every call site resolves".into(),
                    holds: unresolved.is_empty() || allow || !preserve,
                    detail: if unresolved.is_empty() {
                        "unresolved: 0".into()
                    } else {
                        format!(
                            "unresolved: {} — {}{}",
                            unresolved.len(),
                            shown.join(", "),
                            if allow { " (allowed by call_sites.allow_unresolved: listed, not changed)" } else { "; a call no one resolved is not rewritten and stops compiling: name it so it resolves, or set call_sites.allow_unresolved = true to go ahead and list them" }
                        )
                    },
                });
            }
            Operation::Move => {
                let to = spec.to.clone().unwrap_or_default();
                let dest = crate::change::route_file(&f.k, &f.root, &to);
                pre.push(Check { what: format!("the module {to} exists"), holds: dest.is_some(), detail: dest.as_ref().map(|(_, p)| p.strip_prefix(&f.root).unwrap_or(p).display().to_string()).unwrap_or_else(|| "no such crate:<name>/<file>.rs route".into()) });
                if let Some((_, p)) = dest {
                    write.insert(p.strip_prefix(&f.root).unwrap_or(&p).to_string_lossy().into_owned());
                }
            }
            Operation::ChangeDependency => {
                match crate::change::dependency_changes(&f.k, &f.root, spec) {
                    Ok(changes) => {
                        for c in &changes {
                            write.insert(c.manifest.clone());
                        }
                        let named: Vec<&str> = spec.dependencies_named().iter().map(|d| d.name.as_str()).collect();
                        let unnamed: Vec<&str> = named.iter().copied().filter(|n| !changes.iter().any(|c| c.name == n.replace('-', "_"))).collect();
                        pre.push(Check { what: format!("a crate names each of {}", named.join(", ")), holds: unnamed.is_empty(), detail: if unnamed.is_empty() { format!("{} lines in {} manifests", changes.len(), changes.iter().map(|c| c.manifest.as_str()).collect::<BTreeSet<_>>().len()) } else { format!("no crate names {}", unnamed.join(", ")) } });
                    }
                    Err(e) => pre.push(Check { what: "each crate names the dependency it changes".into(), holds: false, detail: e }),
                }
            }
            Operation::ReplaceBody => {
                let item = l.item.clone().unwrap_or_default();
                let found = f.k.functions.iter().find(|x| x.fq_name == item);
                pre.push(Check { what: "the target is a function".into(), holds: found.is_some(), detail: found.map(|x| x.fq_name.clone()).unwrap_or_else(|| format!("{item}: not a function the scan captured")) });
                let name = spec.function.as_ref().map(|x| x.name.clone()).unwrap_or_default();
                let same = item.rsplit("::").next() == Some(name.as_str());
                pre.push(Check { what: "the body is the target's own".into(), holds: same, detail: if same { "its signature, docs and attributes are kept: no caller changes".into() } else { format!("the spec's function is {name}, the target {item}") } });
            }
            Operation::AddTest => {
                if let Some(m) = &spec.to {
                    let module = format!("{}::{m}", l.module);
                    match f.k.modules.get(&module) {
                        Some(_) => listed.push(format!("the test goes in the module {module}")),
                        None => listed.push(format!("the test module {module} is made: #[cfg(test)] mod {m} at the end of {}", l.file)),
                    }
                }
            }
            Operation::Inline => {
                let item = l.item.clone().unwrap_or_default();
                let blockers = crate::change::inline_blockers(&f.k, &item);
                pre.push(Check { what: "the function can be inlined".into(), holds: blockers.is_empty(), detail: if blockers.is_empty() { "each call becomes a block: its arguments bound to the parameters, then the body".into() } else { blockers.join("; ") } });
                let names = |r: &Option<klepto::Resolution>| matches!(r, Some(klepto::Resolution::Local(id) | klepto::Resolution::External(id)) if *id == item);
                for c in f.k.calls.iter().filter(|c| names(&c.resolution)) {
                    write.insert(c.location.path.strip_prefix(&f.root).unwrap_or(&c.location.path).to_string_lossy().replace('\\', "/"));
                }
                let public = f.k.functions.iter().find(|x| x.fq_name == item).is_some_and(|x| x.is_public);
                listed.push(if public { format!("{item} is pub: kept, where other crates may name it") } else { format!("{item} is removed: not pub, and every use of it is a call") });
            }
            Operation::Wrap => {
                let item = l.item.clone().unwrap_or_default();
                let found = f.k.functions.iter().find(|x| x.fq_name == item);
                pre.push(Check { what: "the target is a function".into(), holds: found.is_some(), detail: found.map(|x| x.fq_name.clone()).unwrap_or_else(|| format!("{item}: not a function the scan captured")) });
                let name = spec.function.as_ref().map(|x| x.name.clone()).unwrap_or_default();
                let same = item.rsplit("::").next() == Some(name.as_str());
                pre.push(Check { what: "the body wrapped is the target's own".into(), holds: same, detail: if same { "its signature, docs and attributes are kept: no caller changes".into() } else { format!("the spec's function is {name}, the target {item}") } });
                let pieces = spec.function.as_ref().map(|x| crate::change::render_wrapped(x, "", "{}"));
                pre.push(Check { what: "the pieces run first and the body gives the value".into(), holds: matches!(pieces, Some(Ok(_))), detail: match pieces { Some(Err(e)) => e, _ => "pieces first, then the body as it was".into() } });
            }
            Operation::AddAttribute => {}
        }
        // code that names an optional dependency builds only with a feature that turns it on (crate::change::optional_uses,
        // the rule the writer's refusal names too)
        for u in crate::change::optional_uses(&f.k, spec, l) {
            pre.push(Check {
                what: format!("the code names the optional dependency {} only under a feature that turns it on", u.dependency),
                holds: u.gated,
                detail: if u.gated { format!("gated by {}", u.features.join(" or ")) } else { u.fix() },
            });
        }
        // an attribute macro that rewrites the item: an operation that writes it as declared does not know what the
        // macro makes of it
        if let Some(why) = l.item.as_deref().and_then(|i| crate::change::rewritten_by(&f.k, op, i)) {
            pre.push(Check { what: "no attribute macro rewrites the item".into(), holds: false, detail: why });
        }
        if crate::change::uses_toolkit(spec) {
            let has = f.k.features.get(&l.package).is_some_and(|m| m.dependencies.iter().any(|d| d.name == crate::change::TOOLKIT.0));
            if !has {
                if let Some(p) = f.packages.get(&l.package) {
                    write.insert(format!("{}/Cargo.toml", p.dir).trim_start_matches('/').to_string());
                }
                listed.push(format!("{} gets the dependency {} = \"{}\" (its body is toolkit pieces)", l.package, crate::change::TOOLKIT.0, crate::change::TOOLKIT.1));
            }
        }
    }
    // the scope it may write
    let (scope_routes, prefixes): (Vec<String>, Vec<String>) = if spec.scope.write.is_empty() {
        let own: Vec<String> = land.as_ref().map(|l| match &l.at {
            At::NewFile { parent } if !parent.is_empty() => vec![l.file.clone(), parent.clone()],
            // a changed dependency's own files are the manifests that name it (every crate's, for `*`)
            At::Manifest if op == Operation::ChangeDependency => crate::change::dependency_changes(&f.k, &f.root, spec).map(|c| c.into_iter().map(|c| c.manifest).collect::<BTreeSet<_>>().into_iter().collect()).unwrap_or_else(|_| vec![l.file.clone()]),
            _ => vec![l.file.clone()],
        }).unwrap_or_default();
        (own.iter().map(|o| format!("the target's own file {o}")).collect(), own)
    } else {
        (spec.scope.write.clone(), spec.scope.write.iter().flat_map(|r| route_prefixes(f, r)).collect())
    };
    let outside: Vec<String> = write.iter().filter(|w| !within(&prefixes, w)).cloned().collect();
    if !outside.is_empty() {
        let crates: BTreeSet<String> = outside.iter().filter_map(|o| f.package_of_rel(o).map(|p| format!("crate:{}", p.name))).collect();
        questions.push(crate::questions::question(
            &format!("{} also writes {}: widen its scope.write to {}?", spec.name, outside.join(", "), crates.iter().cloned().collect::<Vec<_>>().join(", ")),
            &["widen the scope".into(), "change the spec so it stays inside".into()],
            Some("a bounded change never writes outside its declared scope"),
            &format!("preflight:{}", spec.name),
        ));
    }
    // decisions answered before are not asked again
    let decided = store.map(crate::questions::decided).transpose()?.unwrap_or_default();
    let (answered, open): (Vec<_>, Vec<_>) = questions.into_iter().partition(|q| decided.contains_key(&q.id));
    let answered: Vec<crate::questions::Decision> = answered.iter().filter_map(|q| decided.get(&q.id).cloned()).collect();
    let raw = spec.raw();
    let bounded = job.as_ref().is_some_and(|j| j.mode == JobMode::Bounded);
    let (tk, ok) = job.as_ref().map(|j| (j.target_known || resolved, j.outcome_known)).unwrap_or((resolved, true));
    let preconditions = pre.iter().all(|c| c.holds);
    // the configurations to check are the ones the change leaves: a feature declared, a dependency made optional
    let mut impact = impact;
    if let (Some((i, _, _)), Ok(l)) = (impact.as_mut(), land.as_ref()) {
        let packages: Vec<String> = if i.packages.is_empty() { vec![l.package.clone()] } else { i.packages.clone() };
        let after: usize = packages.iter().filter_map(|p| crate::change::model_after(&f.k, &f.root, spec, p)).map(|m| klepto::check_matrix(&m).len()).sum();
        if after != i.feature_configurations && after > 0 {
            let before = i.feature_configurations;
            i.feature_configurations = after;
            for d in i.dimensions.iter_mut() {
                match d.name.as_str() {
                    "feature configurations to check" => {
                        d.value = Some(after as u64);
                        d.note = format!("klepto's check matrix over each affected crate's features as the change leaves them ({before} before it)");
                    }
                    "cargo check runs" => d.value = Some(after.max(packages.len()) as u64),
                    _ => {}
                }
            }
        }
    }
    let word = PreflightWord::from_raw(0)
        .set("resolved", resolved)
        .set("preconditions", preconditions)
        .set("in_scope", outside.is_empty())
        .set("free_name", collisions.is_empty())
        .set("decisions_open", !open.is_empty())
        .set("raw", !raw.is_empty())
        .set("bounded", bounded)
        .set("target_known", tk)
        .set("outcome_known", ok)
        .set("bounded_ready", !bounded || (tk && ok));
    let (word, mut reasons) = word.decide();
    let authorized = word.names().contains(&"authorized");
    // the reasons in words, and whether the change is in the code already
    let mut why: Vec<String> = pre.iter().filter(|c| !c.holds).map(|c| format!("{}: {}", c.what, c.detail)).collect();
    why.extend(collisions.iter().map(|c| format!("a name is taken: {c}")));
    if !outside.is_empty() {
        why.push(format!("it writes outside its scope: {}", outside.join(", ")));
    }
    why.extend(open.iter().map(|q| format!("a decision is open: {}", q.question)));
    let in_place: Vec<String> = if authorized { Vec::new() } else { pre.iter().filter(|c| !c.holds && in_place_checks.contains(&c.what)).map(|c| c.detail.clone()).collect() };
    if bounded && !tk {
        reasons.push("bounded: the job's target is not known (target_known)".into());
        why.push("the job's target is not known".into());
    }
    if bounded && !ok {
        reasons.push("bounded: the job declares no verification (outcome_known): job start --verify, or answer what done looks like".into());
        why.push("the bounded job declares no verification: job start --verify, or answer what done looks like".into());
    }
    Ok(Preflight {
        schema: PREFLIGHT_SCHEMA.into(),
        spec_name: spec.name.clone(),
        spec: spec_rel.to_string(),
        spec_digest: spec.digest(),
        operation: op.as_str().into(),
        objective: spec.objective.clone(),
        target: spec.target.clone(),
        code_digest: code_digest.to_string(),
        index_key,
        landing: land.ok(),
        impact: impact.as_ref().map(|(i, _, _)| i.clone()),
        impact_seq: impact.as_ref().and_then(|(_, s, _)| *s),
        preconditions: pre,
        collisions,
        write_set: write.into_iter().collect(),
        authorized_scope: scope_routes,
        outside,
        listed,
        raw,
        decisions: open,
        answered,
        job,
        word: word.raw(),
        flags: word.names().into_iter().map(str::to_string).collect(),
        authorized,
        reasons,
        why: if authorized { Vec::new() } else { why },
        in_place,
        utc: crate::runs::now_utc(),
        millis: started.elapsed().as_millis() as i64,
    })
}

/// The impact reading's target for a spec: the item it works on, or the route it lands in.
pub fn impact_target(spec: &ChangeSpec) -> String {
    if spec.op().targets_an_item() {
        spec.target.clone()
    } else {
        spec.target.trim().to_string()
    }
}

/// What a preflight run left.
#[derive(Debug, Clone)]
pub struct Ran {
    pub preflight: Preflight,
    pub receipt: Option<Receipt>,
    pub receipt_file: Option<PathBuf>,
    /// The record and its page, relative to the root.
    pub outputs: Vec<String>,
    pub said: Vec<String>,
}

/// The preflight of the spec at `spec_path`: its impact read through the reading path (a receipt, or a reuse),
/// the findings assessed over the same facts, the record and its page kept and a `preflight` receipt naming them
/// (journaled); in a running job, its task history gets the impact as a diagnosis record and each open decision as
/// a problem record.
pub fn run_preflight(ctx: &Ctx, spec_path: &Path, job: Option<&InJob>, runner: Runner<'_>) -> Result<Ran> {
    let spec = ChangeSpec::from_path(spec_path)?;
    let spec_rel = ctx.rel(&spec_path.canonicalize().unwrap_or_else(|_| spec_path.to_path_buf()));
    let mut said = Vec::new();
    // the impact, as a reading: same spec over the same code is a reuse
    let target = impact_target(&spec);
    // the same search the command line's `run TARGET --kind impact` makes: one key, so either finds the other
    let ispec = crate::reading::spec::ReadingSpec::search(crate::reading::spec::ReadingKind::Impact, &target)?;
    let iname = ispec.name.clone();
    let dir = match (&ctx.store, job) {
        (_, Some(j)) => j.readings.parent().map(|p| p.join("steps")).unwrap_or_else(|| j.readings.clone()),
        (Some(s), None) => s.dir_of(&named::PREFLIGHT, &[("name", &spec.name), ("stamp", "x")])?,
        (None, None) => std::env::temp_dir().join(format!("custodian-preflight-{}", std::process::id())),
    };
    let ipath = dir.join(crate::oneo::Doc::Reading.file_name(&iname));
    write_whole(&ipath, ispec.to_1o().or_else(|_| toml::to_string(&ispec).map_err(|e| ReadingRefusal::Write.refuse(e)))?.as_bytes())?;
    let store = ctx.store.as_ref();
    let how = if job.is_some() { crate::paths::Resolving::Job } else { crate::paths::Resolving::Free };
    let facts_owned;
    let facts = match runner {
        Runner::InProcess(ws) if ws.scope == ispec.scope => ws.facts(),
        _ => {
            facts_owned = Facts::build(&ctx.root, &ispec.scope, how)?;
            &facts_owned
        }
    };
    let key = crate::api::SearchKey::of(&ctx.root, &ispec).map_err(crate::CustodianError::from)?;
    let impact = if store.is_some() {
        // journaled: through the reading path, with its receipt (or the reuse of an earlier one)
        match crate::runs::run_reading_by(ctx, &ipath, false, job, runner) {
            Ok(r) => {
                said.extend(r.said.iter().cloned());
                let receipt = r.receipt.clone();
                let json = receipt.as_ref().and_then(|x| x.outputs.iter().find(|o| o.ends_with(".json")).cloned());
                let report = json.and_then(|j| crate::reading::load(&ctx.root.join(j)).ok());
                let seq = receipt.as_ref().map(|x| x.reused_seq.unwrap_or(x.seq));
                report.map(|rep| (ImpactRecord::of(&rep), seq, rep.query.results.iter().chain(&rep.unresolved).cloned().collect()))
            }
            Err(e) => {
                said.push(format!("impact not read: {e}"));
                None
            }
        }
    } else {
        // free mode: answered here, nothing stored
        let rep = crate::reading::read_facts(facts, &ipath, &ispec, &crate::runs::stamp())?;
        Some((ImpactRecord::of(&rep), None, rep.query.results.iter().chain(&rep.unresolved).cloned().collect()))
    };
    let (code_digest, index_key) = (key.code_digest.clone(), impact.as_ref().and_then(|(i, _, _)| i.index_key.clone()));
    let view = store.and_then(job_view).map(|(v, _)| v);
    let p = assess(facts, store, &spec_rel, &spec, impact, view, &code_digest, index_key)?;
    said.push(format!("preflight {}: {} — {}", spec.name, if p.authorized { "authorized" } else { "not authorized" }, p.headline_why().unwrap_or_else(|| p.flags.join(", "))));
    if !p.reasons.is_empty() {
        said.push(format!("  the rules it breaks: {}", p.reasons.join("; ")));
    }
    let Some(store) = store else {
        return Ok(Ran { preflight: p, receipt: None, receipt_file: None, outputs: Vec::new(), said });
    };
    let stamp = crate::runs::stamp();
    let json_path = store.path(&named::PREFLIGHT, &[("name", &spec.name), ("stamp", &stamp)])?;
    let json = serde_json::to_string_pretty(&p)? + "\n";
    write_whole(&json_path, json.as_bytes())?;
    let md_path = json_path.with_extension("md");
    write_whole(&md_path, p.markdown().as_bytes())?;
    let outputs = vec![ctx.rel(&json_path), ctx.rel(&md_path)];
    // the job's task history: the impact found, the decisions asked
    if let (Some(j), Some((_, folder))) = (job, job_view(store)) {
        let jf = crate::jobs::job_file(&folder);
        if let Some(t) = crate::task_records::of_job(store, jf.task_id.as_deref(), jf.revision.as_deref())? {
            if let Some(i) = &p.impact {
                t.write(i.to_record(p.impact_seq))?;
            }
            for q in &p.decisions {
                crate::questions::ask(store, &t, q)?;
            }
            let _ = j;
        }
    }
    let digest = crate::digest::tagged(&json);
    let r = Receipt {
        schema: RECEIPT_SCHEMA.into(),
        seq: 0,
        utc: p.utc.clone(),
        command: "preflight".into(),
        name: spec.name.clone(),
        spec: Some(spec_rel.clone()),
        spec_hash: Some(p.spec_digest.clone()),
        code_digest: Some(p.code_digest.clone()),
        index_key: p.index_key.clone(),
        plan_id: format!("preflight:{}", spec.name),
        plan_digest: digest.clone(),
        operation_digest: digest.clone(),
        receipt_digest: digest,
        line: format!("custodian preflight {spec_rel}"),
        exit_code: Some(if p.authorized { 0 } else { 1 }),
        accepted: true,
        disposition: "Continue".into(),
        reason: p.reason().into(),
        outputs: outputs.clone(),
        job: job.map(|j| j.id.clone()),
        reused_seq: None,
        millis: p.millis,
        runner: crate::receipts::RUNNER_IN_PROCESS.into(),
    };
    let (r, f) = crate::receipts::keep(store, r)?;
    said.push(format!("preflight record: {} · receipt #{} {}", outputs.join(" + "), r.seq, ctx.rel(&f)));
    Ok(Ran { preflight: p, receipt: Some(r), receipt_file: Some(f), outputs, said })
}

/// A preflight receipt's reason when the change is authorized.
pub const AUTHORIZED: &str = "authorized";
pub const NOT_AUTHORIZED: &str = "not authorized";
/// A preflight receipt's reason when what the spec asks for is in the code already: nothing waits on a person.
pub const ALREADY_IN_PLACE: &str = "not authorized: already in place";

/// The step-input gate of a code-changing step: the newest preflight receipt of this spec (its digest now) over
/// this code (its digest now). Go only when it authorized the change; a spec edited or code moved since is
/// preflighted again.
pub fn gate(store: &Store, spec: &ChangeSpec, code_digest: &str) -> Result<(crate::decisions::Decided, Option<Receipt>)> {
    let journal = store.journal()?;
    let digest = spec.digest();
    let all: Vec<Receipt> = journal.runs()?.into_iter().filter(|r| r.command == "preflight" && r.name == spec.name).collect();
    let same = all.iter().rev().find(|r| r.spec_hash.as_deref() == Some(digest.as_str()) && r.code_digest.as_deref() == Some(code_digest)).cloned();
    let q = crate::decisions::journal_tool().required_question_spec("preflight", "transform", "did a preflight of this spec over this code authorize it?");
    let d = match (&same, all.last()) {
        (Some(r), _) if r.reason == AUTHORIZED => crate::decisions::Decided { question: q, facts: vec![format!("#{} {}", r.seq, r.utc)], go: true, reason: format!("preflight #{} authorized it", r.seq) },
        (Some(r), _) => crate::decisions::Decided { question: q, facts: vec![format!("#{} {}", r.seq, r.reason)], go: false, reason: format!("preflight #{} did not authorize it", r.seq) },
        (None, Some(last)) if last.spec_hash.as_deref() != Some(digest.as_str()) => crate::decisions::Decided { question: q, facts: vec![format!("#{}", last.seq)], go: false, reason: format!("the spec changed since preflight #{}: preflight it again", last.seq) },
        (None, Some(last)) => crate::decisions::Decided { question: q, facts: vec![format!("#{}", last.seq)], go: false, reason: format!("the code moved since preflight #{}: preflight it again", last.seq) },
        (None, None) => crate::decisions::Decided { question: q, facts: Vec::new(), go: false, reason: "no preflight of this spec: custodian preflight SPEC".into() },
    };
    Ok((d, same))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_authorizes_only_when_its_rules_hold() {
        let all = PreflightWord::from_raw(0).set("resolved", true).set("preconditions", true).set("in_scope", true).set("free_name", true).set("bounded_ready", true);
        let (w, reasons) = all.decide();
        assert!(w.names().contains(&"authorized") && reasons.is_empty());
        let (w, reasons) = all.set("in_scope", false).decide();
        assert!(!w.names().contains(&"authorized"));
        assert!(reasons.iter().any(|r| r.contains("in_scope")), "{reasons:?}");
        let (w, reasons) = all.set("decisions_open", true).decide();
        assert!(!w.names().contains(&"authorized") && reasons.iter().any(|r| r.contains("decisions_open")), "{reasons:?}");
        // raw is flagged, not refused
        assert!(all.set("raw", true).decide().0.names().contains(&"authorized"));
    }
}
