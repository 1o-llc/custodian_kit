// Copyright © 2026 1o. All rights reserved.

//! The `impact` reading: everything a change to one target entails, in one
//! reading over klepto's scan, and the structured record of it.
//!
//! The target is a crate (`crate:<name>`), a file (`crate:<name>/src/x.rs`)
//! or an item (`core_lib::Config`). The reading puts together what the other
//! kinds answer one at a time, on the same facts: the definitions and use
//! sites (`use_sites`), the places a type is built (`constructs`) and the
//! matches on an enum (`matches_on`), the impl blocks of a type, a crate's
//! public items and their uses from other crates (`public_api`), what depends
//! on it (`dependents`), what a change reaches (`reach`: items, files,
//! crates, binaries, scope depth, env keys, codes), the tests that cover it,
//! the features its code is built under and the feature configurations each
//! affected crate must still build in (klepto's check matrix), and the
//! linkage records that bind it to something else.
//!
//! Then the cost, by dimension (`impact-cost` rows), each saying how it is
//! known: `measured` (counted from the scan or the manifests), `heuristic`
//! (a rule over measured counts, the rule written in the row), or `unknown`
//! (not derivable from source facts: runtime, memory layout, allocation,
//! concurrency). No dimension is a guess written as a number.
//!
//! [`ImpactRecord`] is the same reading as a typed record: the numbers a
//! preflight and a plan carry, written into a job's task history as a
//! `diagnosis` record.

use super::facts::Facts;
use super::kinds::{constructs, dependents, item_id, item_row, matches_on, named, public_api, reach, sort_rows, test_rows, tests_reaching, types_named, use_sites, TestCase};
use super::spec::{ReadingKind, ReadingSpec};
use super::{Answer, ReadingReport};
use crate::{CodeQueryResult, Result};
use klepto::CfgPredicate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// How a dimension of the cost is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Basis {
    /// Counted from the scan or the manifests.
    Measured,
    /// A rule over measured counts; the rule is in the note.
    Heuristic,
    /// Not derivable from the source facts.
    Unknown,
}

impl Basis {
    pub fn as_str(self) -> &'static str {
        match self {
            Basis::Measured => "measured",
            Basis::Heuristic => "heuristic",
            Basis::Unknown => "unknown",
        }
    }

    fn parse(s: &str) -> Basis {
        match s {
            "measured" => Basis::Measured,
            "heuristic" => Basis::Heuristic,
            _ => Basis::Unknown,
        }
    }
}

/// One dimension of a change's cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dimension {
    pub name: String,
    /// `None` when the basis is unknown.
    pub value: Option<u64>,
    pub basis: Basis,
    /// Where the number comes from, or the rule.
    pub note: String,
}

/// What the target is.
fn target_kind(f: &Facts, target: &str) -> &'static str {
    if f.package_named(target).is_some() {
        "crate"
    } else if f.root.join(target.trim_start_matches("./")).is_file() {
        "file"
    } else {
        "item"
    }
}

/// The features a cfg names.
pub fn features_of(cfg: &CfgPredicate, out: &mut BTreeSet<String>) {
    match cfg {
        CfgPredicate::Feature(f) => {
            out.insert(f.clone());
        }
        CfgPredicate::All(v) | CfgPredicate::Any(v) => v.iter().for_each(|c| features_of(c, out)),
        CfgPredicate::Not(c) => features_of(c, out),
        _ => {}
    }
}

fn spec_of(spec: &ReadingSpec, kind: ReadingKind) -> ReadingSpec {
    ReadingSpec { kind, ..spec.clone() }
}

/// The `impact` reading.
pub fn answer(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let target = spec.target.trim();
    let what = target_kind(f, target);
    let mut ans = Answer::default();
    let mut rows: Vec<CodeQueryResult> = Vec::new();
    let mut unresolved: Vec<CodeQueryResult> = Vec::new();
    // the items the change is at
    let seeds: BTreeSet<String> = match what {
        "crate" => {
            let p = f.package_named(target).map(|p| p.name.clone()).unwrap_or_default();
            f.k.items.iter().filter(|d| d.package == p).map(|d| d.id.clone()).collect()
        }
        "file" => {
            let abs = f.root.join(target.trim_start_matches("./"));
            f.k.items.iter().filter(|d| d.location.path == abs).map(|d| d.id.clone()).collect()
        }
        _ => BTreeSet::from([item_id(f, target)?]),
    };
    let defs: usize;
    let mut uses = 0;
    let mut constructions = 0;
    let mut open_matches = 0;
    let mut impls = 0;
    let mut external_uses = 0;
    let mut serialized = false;
    let mut public = false;
    match what {
        "item" => {
            let a = use_sites(f, &spec_of(spec, ReadingKind::UseSites))?;
            defs = a.rows.iter().filter(|r| r.kind == "definition").count();
            uses = a.rows.len() - defs;
            rows.extend(a.rows);
            unresolved.extend(a.unresolved);
            let types = types_named(f, target);
            for t in &types {
                public |= t.is_public;
                serialized |= t.derives.iter().any(|d| super::facts::Facts::canonical(d).is_some_and(|p| p.ends_with("Serialize") || p.ends_with("Deserialize")));
                for i in f.k.impls_of(&t.id) {
                    impls += 1;
                    let label = i.trait_.as_ref().map(|tr| format!("impl {} for {}", tr.raw, t.name)).unwrap_or_else(|| format!("impl {}", t.name));
                    let mut r = named("impl", label, Some(i.package.clone()), "an impl block of the type", vec![("methods", i.methods.len().to_string()), ("trait", i.trait_.as_ref().map(|t| t.raw.clone()).unwrap_or_default())]);
                    r.path = Some(f.rel(&i.location.path));
                    r.line = i.location.line;
                    rows.push(r);
                }
            }
            if types.iter().any(|t| t.variants.is_empty()) {
                if let Ok(a) = constructs(f, &spec_of(spec, ReadingKind::Constructs)) {
                    constructions = a.rows.len();
                    rows.extend(a.rows);
                }
            }
            if types.iter().any(|t| !t.variants.is_empty()) {
                if let Ok(a) = matches_on(f, &spec_of(spec, ReadingKind::MatchesOn)) {
                    open_matches = a.rows.iter().filter(|r| r.attributes.get("catch_all").is_some_and(|c| c == "false")).count();
                    rows.extend(a.rows);
                }
            }
            // a type is used through its own items too (`Config::new`): its methods' and impls' referrers count
            for id in &seeds {
                if let Some(d) = f.item(id) {
                    public |= d.is_public;
                    let own: Vec<&str> = f.k.items.iter().filter(|x| x.id == *id || x.id.starts_with(&format!("{id}::")) || x.parent.as_deref() == Some(id.as_str())).map(|x| x.id.as_str()).collect();
                    let users: BTreeSet<&String> = own.iter().flat_map(|o| f.referrers_of(o)).filter(|r| f.item(r).is_some_and(|u| u.package != d.package)).collect();
                    external_uses += users.len();
                }
            }
        }
        "crate" => {
            let a = public_api(f, &spec_of(spec, ReadingKind::PublicApi))?;
            defs = a.rows.iter().filter(|r| r.kind == "public-item").count();
            external_uses = a.rows.iter().filter(|r| r.kind == "public-use").count();
            uses = external_uses;
            public = defs > 0;
            rows.extend(a.rows);
        }
        _ => {
            for id in &seeds {
                rows.push(item_row(f, "definition", id, "declared in the file", vec![]));
                if let Some(d) = f.item(id) {
                    public |= d.is_public;
                    external_uses += f.referrers_of(id).iter().filter(|r| f.item(r).is_some_and(|u| u.package != d.package)).count();
                }
            }
            defs = seeds.len();
        }
    }
    // what depends on it, and what a change reaches
    let dep_target = if what == "item" { seeds.iter().next().cloned().unwrap_or_default() } else { target.to_string() };
    let dependents_answer = dependents(f, &ReadingSpec { target: dep_target, ..spec_of(spec, ReadingKind::Dependents) }).ok();
    let fan_out = dependents_answer.as_ref().map(|a| a.rows.len()).unwrap_or(0);
    if let Some(a) = dependents_answer {
        rows.extend(a.rows);
    }
    let r = reach(f, &spec_of(spec, ReadingKind::Reach))?;
    let count_of = |a: &Answer, what: &str| a.counts.iter().find(|c| c.what == what).map(|c| c.n).unwrap_or(0);
    let (items_reached, files, crates, bins) = (count_of(&r, "items touched"), count_of(&r, "files"), count_of(&r, "crates"), count_of(&r, "binaries"));
    let packages: BTreeSet<String> = r.rows.iter().filter(|x| x.kind == "reach-crate").map(|x| x.label.clone()).collect();
    let files_of: BTreeSet<String> = seeds.iter().chain(r.rows.iter().filter_map(|x| x.attributes.get("item"))).filter_map(|id| f.item(id).map(|d| f.rel(&d.location.path))).collect();
    rows.extend(r.rows);
    // the tests that cover it
    let cases: Vec<TestCase> = tests_reaching(f, &seeds);
    rows.extend(test_rows(f, &cases, "a test whose calls reach the change"));
    // the features its code is built under, and the configurations each affected crate checks
    let mut gates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut gated_items: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut touched: BTreeSet<String> = seeds.clone();
    touched.extend(rows.iter().filter_map(|x| x.attributes.get("item").cloned()));
    for id in &touched {
        let Some(d) = f.item(id) else { continue };
        let mut fs = BTreeSet::new();
        features_of(&d.cfg, &mut fs);
        for feat in fs {
            *gated_items.entry((d.package.clone(), feat.clone())).or_default() += 1;
            gates.entry(d.package.clone()).or_default().insert(feat);
        }
    }
    for ((p, feat), n) in &gated_items {
        rows.push(named("impact-feature", format!("{p}: feature {feat}"), Some(p.clone()), "the code the change touches is built under this feature", vec![("feature", feat.clone()), ("items", n.to_string())]));
    }
    let mut configurations = 0usize;
    for p in &packages {
        if let Some(m) = f.k.features.get(p) {
            let jobs = klepto::check_matrix(m);
            configurations += jobs.len();
            rows.push(named("impact-configurations", p.clone(), Some(p.clone()), "the feature configurations the crate must still build in (klepto's check matrix)", vec![("configurations", jobs.len().to_string()), ("declared_features", m.features.len().to_string())]));
        }
    }
    // the linkage records naming it
    let linkage = crate::linkage::checked(f)?;
    let naming = crate::linkage::naming(&linkage, f, &seeds);
    for (p, rec, bs) in &naming {
        rows.extend(crate::linkage::rows(f, p, rec, bs, "binds the target to another declaration: read why before changing it"));
    }
    // the lines in the files it touches, from klepto's copy of each file
    let lines: usize = f.k.files.iter().filter(|x| files_of.contains(&f.rel(&x.path))).map(|x| x.source.lines().count()).sum();
    let functions = touched.iter().filter(|id| f.item(id).is_some_and(|d| matches!(d.kind, klepto::graph::NodeKind::Fn | klepto::graph::NodeKind::Method))).count();
    let risk = risk_class(public, external_uses, crates, bins);
    let dims = vec![
        dim("definitions", Some(defs), Basis::Measured, "klepto's items naming the target"),
        dim("use sites", Some(uses), Basis::Measured, "klepto's resolved occurrences"),
        dim("unresolved that could be it", Some(unresolved.len()), Basis::Measured, "klepto could not resolve these; the true use count may be higher"),
        dim("constructions (struct literals, constructors)", Some(constructions), Basis::Measured, "constructs: each must take a new field"),
        dim("matches without a catch-all", Some(open_matches), Basis::Measured, "matches_on: each must take a new variant"),
        dim("impl blocks", Some(impls), Basis::Measured, "klepto's impls of the type"),
        dim("uses from other crates", Some(external_uses), Basis::Measured, "referrers in another package: public API compatibility"),
        dim("items reached", Some(items_reached), Basis::Measured, "reach: items whose references reach the change"),
        dim("functions touched", Some(functions), Basis::Measured, "functions and methods among the definitions, uses and reached items"),
        dim("files", Some(files), Basis::Measured, "reach: files holding what is reached"),
        dim("lines in those files", Some(lines), Basis::Measured, "klepto's copy of each file"),
        dim("packages", Some(crates), Basis::Measured, "reach: crates holding or linking the change"),
        dim("dependency fan-out", Some(fan_out), Basis::Measured, "dependents: direct and transitive"),
        dim("binaries", Some(bins), Basis::Measured, "reach: binaries linking it"),
        dim("feature gates", Some(gated_items.len()), Basis::Measured, "features the touched code's cfg names, by crate"),
        dim("feature configurations to check", Some(configurations), Basis::Measured, "klepto's check matrix over each affected crate's features"),
        dim("tests covering it", Some(cases.len()), Basis::Measured, "tests whose calls reach the change"),
        dim("cargo check runs", Some(configurations.max(crates)), Basis::Heuristic, "one per feature configuration of each affected crate, at least one per crate"),
        dim("cargo test lines", Some(super::kinds::test_lines(&cases).len()), Basis::Measured, "one per package and test target"),
        dim("serialization", Some(serialized as usize), Basis::Measured, "1: the type derives Serialize or Deserialize, so its wire form changes with it"),
        dim("public API", Some(public as usize), Basis::Measured, "1: the target is public"),
        dim("linkage records", Some(naming.len()), Basis::Measured, "records binding it to other declarations"),
        dim("runtime cost", None, Basis::Unknown, "not derivable from the source facts"),
        dim("memory layout and allocation", None, Basis::Unknown, "depends on representation, alignment and use; not derivable from the source facts"),
        dim("concurrency", None, Basis::Unknown, "not derivable from the source facts"),
    ];
    for d in &dims {
        rows.push(named("impact-cost", d.name.clone(), None, d.note.as_str(), vec![("value", d.value.map(|v| v.to_string()).unwrap_or_else(|| "unknown".into())), ("basis", d.basis.as_str().into())]));
    }
    rows.push(named("impact-risk", risk.0.to_string(), None, risk.1, vec![("basis", "heuristic".to_string())]));
    sort_rows(&mut unresolved);
    ans.count("target", 1);
    ans.count("definitions", defs);
    ans.count("use sites", uses);
    ans.count("constructions", constructions);
    ans.count("matches without a catch-all", open_matches);
    ans.count("impl blocks", impls);
    ans.count("uses from other crates", external_uses);
    ans.count("dependents", fan_out);
    ans.count("items reached", items_reached);
    ans.count("files", files);
    ans.count("packages", crates);
    ans.count("binaries", bins);
    ans.count("tests covering it", cases.len());
    ans.count("feature gates", gated_items.len());
    ans.count("feature configurations to check", configurations);
    ans.count("linkage records", naming.len());
    ans.headline = vec![format!("{} definitions, {uses} use sites, {files} files in {crates} packages, {} tests, {} feature gates, risk {}", defs, cases.len(), gated_items.len(), risk.0)];
    ans.rows = rows;
    ans.unresolved = unresolved;
    Ok(ans)
}

fn dim(name: &str, value: Option<usize>, basis: Basis, note: &str) -> Dimension {
    Dimension { name: name.into(), value: value.map(|v| v as u64), basis, note: note.into() }
}

/// The risk class, by a rule over measured counts (the rule is the class's sentence).
fn risk_class(public: bool, external_uses: usize, crates: usize, bins: usize) -> (&'static str, &'static str) {
    if bins > 0 && external_uses > 0 {
        ("binary-facing", "used from other crates and linked into binaries: a change ships")
    } else if external_uses > 0 || crates > 1 {
        ("cross-crate", "reached from more than one crate")
    } else if public {
        ("crate-public", "public, used inside its own crate only")
    } else {
        ("contained", "private to its crate")
    }
}

/// An impact reading as a typed record: what a preflight, a plan and a job's
/// task history carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactRecord {
    pub target: String,
    /// `crate`, `file`, `item`.
    pub target_kind: String,
    pub definitions: Vec<String>,
    pub use_sites: usize,
    pub unresolved: usize,
    pub files: Vec<String>,
    pub packages: Vec<String>,
    pub functions: usize,
    pub tests: Vec<String>,
    pub test_lines: Vec<String>,
    /// `<crate>: feature <name>`.
    pub feature_gates: Vec<String>,
    pub feature_configurations: usize,
    pub lines: usize,
    pub risk_class: String,
    pub linkage: Vec<String>,
    pub dimensions: Vec<Dimension>,
    /// The reading's index key, so a record is never read over other code.
    pub index_key: Option<String>,
}

impl ImpactRecord {
    /// The record of an impact reading.
    pub fn of(r: &ReadingReport) -> ImpactRecord {
        let rows = &r.query.results;
        fn of<'r>(rows: &'r [CodeQueryResult], k: &'static str) -> impl Iterator<Item = &'r CodeQueryResult> {
            rows.iter().filter(move |x| x.kind == k)
        }
        let mut files: BTreeSet<String> = rows.iter().filter(|x| x.kind.starts_with("reach")).filter_map(|x| x.path.clone()).collect();
        files.extend(of(rows, "definition").filter_map(|x| x.path.clone()));
        let dimensions: Vec<Dimension> = of(rows, "impact-cost")
            .map(|x| Dimension {
                name: x.label.clone(),
                value: x.attributes.get("value").and_then(|v| v.parse().ok()),
                basis: Basis::parse(x.attributes.get("basis").map(String::as_str).unwrap_or("")),
                note: x.score_reason.first().cloned().unwrap_or_default(),
            })
            .collect();
        let n = |name: &str| dimensions.iter().find(|d| d.name == name).and_then(|d| d.value).unwrap_or(0) as usize;
        ImpactRecord {
            target: r.target.clone(),
            target_kind: if of(rows, "public-item").next().is_some() { "crate".into() } else if of(rows, "impl").next().is_some() || rows.iter().any(|x| x.kind == "use-site") { "item".into() } else { "file".into() },
            definitions: of(rows, "definition").chain(of(rows, "public-item")).map(|x| x.label.clone()).collect(),
            use_sites: n("use sites"),
            unresolved: n("unresolved that could be it"),
            files: files.into_iter().collect(),
            packages: of(rows, "reach-crate").map(|x| x.label.clone()).collect::<BTreeSet<_>>().into_iter().collect(),
            functions: n("functions touched"),
            tests: of(rows, "test").map(|x| x.label.clone()).collect(),
            test_lines: of(rows, "test-line").map(|x| x.label.clone()).collect(),
            feature_gates: of(rows, "impact-feature").map(|x| x.label.clone()).collect(),
            feature_configurations: n("feature configurations to check"),
            lines: n("lines in those files"),
            risk_class: of(rows, "impact-risk").next().map(|x| x.label.clone()).unwrap_or_default(),
            linkage: of(rows, "linkage").map(|x| format!("{} ({}, {})", x.label, x.attributes.get("kind").cloned().unwrap_or_default(), x.attributes.get("status").cloned().unwrap_or_default())).collect(),
            dimensions,
            index_key: r.index_key.clone(),
        }
    }

    /// One line a person reads.
    pub fn line(&self) -> String {
        format!(
            "{} ({}): {} definitions, {} use sites, {} files, {} packages, {} tests, {} feature gates, {} configurations to check, risk {}",
            self.target,
            self.target_kind,
            self.definitions.len(),
            self.use_sites,
            self.files.len(),
            self.packages.len(),
            self.tests.len(),
            self.feature_gates.len(),
            self.feature_configurations,
            self.risk_class
        )
    }

    /// The record as a task history `diagnosis` record (what the investigation found).
    pub fn to_record(&self, reading_seq: Option<i64>) -> task_history::DiffDocument {
        let mut d = task_history::DiffDocument::record(task_history::DiffRecordKind::Diagnosis).with("diagnosis", format!("impact: {}", self.line())).with("impact_target", self.target.clone()).with("risk_class", self.risk_class.clone());
        for p in &self.packages {
            d = d.with("package", p.clone());
        }
        for f in &self.files {
            d = d.with("file", f.clone());
        }
        for t in &self.test_lines {
            d = d.with("test_line", t.clone());
        }
        for g in &self.feature_gates {
            d = d.with("feature_gate", g.clone());
        }
        for x in &self.dimensions {
            d = d.with("dimension", format!("{}={} ({})", x.name, x.value.map(|v| v.to_string()).unwrap_or_else(|| "unknown".into()), x.basis.as_str()));
        }
        if let Some(s) = reading_seq {
            d = d.with("reading_seq", s.to_string());
        }
        d
    }
}

/// Every row kind the impact reading writes of its own (the rest are the composed kinds').
pub const ROW_KINDS: &[&str] = &["impl", "impact-feature", "impact-configurations", "impact-cost", "impact-risk"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_risk_rule_reads_in_order() {
        assert_eq!(risk_class(true, 2, 2, 1).0, "binary-facing");
        assert_eq!(risk_class(true, 1, 1, 0).0, "cross-crate");
        assert_eq!(risk_class(true, 0, 1, 0).0, "crate-public");
        assert_eq!(risk_class(false, 0, 1, 0).0, "contained");
        let mut fs = BTreeSet::new();
        features_of(&CfgPredicate::All(vec![CfgPredicate::Feature("log".into()), CfgPredicate::Not(Box::new(CfgPredicate::Feature("std".into())))]), &mut fs);
        assert_eq!(fs.into_iter().collect::<Vec<_>>(), ["log", "std"]);
    }
}
