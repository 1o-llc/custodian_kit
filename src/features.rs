//! `custodian features [--apply]`: each crate's dependencies sorted into the
//! primary constituents (reached by most of the crate's public items, used by
//! most of its dependents) and the feature-eligible (reached by a bounded
//! subset), with the evidence per line. The rule is declared data
//! (`.custodian/config/features.toml`). For the eligible ones custodian writes
//! code_passenger's feature-gating spec (klepto's `klepto.toml` form, klepto's
//! own `PackageSpec`) as a plan beside the reading; `--apply` runs
//! code_passenger's gate loop on it through the executor. custodian never
//! edits source or a manifest itself.

use crate::exec::{self, Plan, Watch};
use crate::reading::facts::{Facts, Link};
use crate::reading::refusal::ReadingRefusal;
use crate::runs::{keep, receipt_of, stamp, Ctx};
use crate::store::{named, write_whole};
use crate::Result;
use klepto::graph::NodeKind;
use klepto::{DepKind, FeatureEnable, PackageSpec};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// The rule, as `agent init` / `init` write it the first time.
pub const SHIPPED_THRESHOLDS: &str = r#"# When a dependency is worth a feature: custodian features reads this.
[eligible]
# feature-eligible when at most this fraction of the crate's public items reach the dependency
max_public_fraction = 0.34
# ... and at most this fraction of the dependents that use the crate use those items (no dependents: not judged on it)
max_consumer_fraction = 0.5
# a crate with fewer public items is not judged
min_public_items = 4
"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Eligible {
    pub max_public_fraction: f64,
    pub max_consumer_fraction: f64,
    pub min_public_items: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    pub eligible: Eligible,
}

impl Thresholds {
    pub fn parse(text: &str) -> Result<Thresholds> {
        toml::from_str(text).map_err(|e| ReadingRefusal::Thresholds.refuse(e))
    }
}

/// One dependency of one crate, judged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Judged {
    pub package: String,
    pub dependency: String,
    pub eligible: bool,
    pub public_items: usize,
    pub public_reaching: usize,
    pub dependents_using_crate: usize,
    pub dependents_using_those: Vec<String>,
    /// The public items that reach the dependency (at most 12).
    pub examples: Vec<String>,
    pub why: String,
    /// With the dependency off: what goes, how deep it sits, who elsewhere loses what.
    pub unavailable: crate::reading::scope::Unavailable,
}

pub fn judge(f: &Facts, t: &Thresholds) -> Vec<Judged> {
    let mut out = Vec::new();
    for (pkg, model) in &f.k.features {
        let public: BTreeSet<String> = f.k.items.iter().filter(|d| &d.package == pkg && d.is_public && !matches!(d.kind, NodeKind::Module | NodeKind::Impl)).map(|d| d.id.clone()).collect();
        let users: Vec<String> = f.direct_dependents(pkg).into_iter().filter(|(_, l)| *l != Link::Dev).map(|(d, _)| d).collect();
        for d in model.dependencies.iter().filter(|d| d.kind == DepKind::Normal && !d.optional) {
            let scope = crate::reading::scope::of_dependency(f, pkg, &d.name);
            let reach: BTreeSet<String> = scope.items.iter().cloned().collect();
            let public_reaching: BTreeSet<&String> = public.intersection(&reach).collect();
            if public_reaching.is_empty() && reach.is_empty() {
                continue;
            }
            let using_those: Vec<String> = users
                .iter()
                .filter(|u| public_reaching.iter().any(|p| f.referrers_of(p).iter().any(|r| f.item(r).is_some_and(|i| &i.package == *u))))
                .cloned()
                .collect();
            let pf = if public.is_empty() { 1.0 } else { public_reaching.len() as f64 / public.len() as f64 };
            let cf = if users.is_empty() { 0.0 } else { using_those.len() as f64 / users.len() as f64 };
            let (eligible, why) = if public.len() < t.eligible.min_public_items {
                (false, format!("{} public items, fewer than {}: not judged", public.len(), t.eligible.min_public_items))
            } else if pf > t.eligible.max_public_fraction {
                (false, format!("{:.0}% of the public items reach it (more than {:.0}%): primary", pf * 100.0, t.eligible.max_public_fraction * 100.0))
            } else if !users.is_empty() && cf > t.eligible.max_consumer_fraction {
                (false, format!("{} of {} dependents use what reaches it (more than {:.0}%): primary", using_those.len(), users.len(), t.eligible.max_consumer_fraction * 100.0))
            } else {
                (true, format!("{} of {} public items reach it; {} of {} dependents use those: feature-eligible", public_reaching.len(), public.len(), using_those.len(), users.len()))
            };
            out.push(Judged {
                package: pkg.clone(),
                dependency: d.name.clone(),
                eligible,
                public_items: public.len(),
                public_reaching: public_reaching.len(),
                dependents_using_crate: users.len(),
                dependents_using_those: using_those,
                examples: public_reaching.iter().take(12).map(|s| (*s).clone()).collect(),
                why,
                unavailable: scope,
            });
        }
    }
    out
}

fn enable_text(e: &FeatureEnable) -> String {
    match e {
        FeatureEnable::Feature(f) => f.clone(),
        FeatureEnable::Dep(d) => format!("dep:{d}"),
        FeatureEnable::DepFeature { dep, feature, weak } => format!("{dep}{}/{feature}", if *weak { "?" } else { "" }),
    }
}

/// code_passenger's spec for the eligible: each dependency optional behind a
/// feature of its own name, every new feature in `default` beside the old
/// default ones, so nothing changes for a consumer until it turns one off.
pub fn spec_for(f: &Facts, judged: &[Judged]) -> BTreeMap<String, PackageSpec> {
    let mut out: BTreeMap<String, PackageSpec> = BTreeMap::new();
    for j in judged.iter().filter(|j| j.eligible) {
        let s = out.entry(j.package.clone()).or_default();
        s.optional.insert(j.dependency.clone());
        s.features.insert(j.dependency.clone(), vec![format!("dep:{}", j.dependency)]);
    }
    for (pkg, s) in out.iter_mut() {
        let mut default: Vec<String> = f.k.features.get(pkg).and_then(|m| m.features.get("default")).map(|i| i.enables.iter().map(enable_text).collect()).unwrap_or_default();
        default.extend(s.features.keys().cloned());
        default.sort();
        default.dedup();
        s.default = Some(default);
    }
    out
}

fn page(judged: &[Judged], spec_file: Option<&str>) -> String {
    let mut s = String::from("# features — primary constituents and feature-eligible dependencies\n\n");
    let eligible = judged.iter().filter(|j| j.eligible).count();
    s.push_str(&format!("- dependencies judged: {}\n- feature-eligible: {}\n- primary or not judged: {}\n", judged.len(), eligible, judged.len() - eligible));
    if let Some(p) = spec_file {
        s.push_str(&format!("- code_passenger spec: `{p}`\n"));
    }
    for (title, want) in [("feature-eligible", true), ("primary or not judged", false)] {
        s.push_str(&format!("\n## {title}\n\n"));
        for j in judged.iter().filter(|j| j.eligible == want) {
            s.push_str(&format!(
                "- **{}** in {} — {} · reaching: {} · dependents using those: {}\n",
                j.dependency,
                j.package,
                j.why,
                j.examples.join(", "),
                if j.dependents_using_those.is_empty() { "none".to_string() } else { j.dependents_using_those.join(", ") }
            ));
            for line in j.unavailable.header() {
                s.push_str(&format!("  - {line}\n"));
            }
        }
    }
    s
}

/// What `features` did.
#[derive(Debug)]
pub struct Featured {
    pub judged: Vec<Judged>,
    pub page: Option<PathBuf>,
    pub spec_file: Option<PathBuf>,
    pub said: Vec<String>,
}

pub fn run(ctx: &Ctx, apply: bool) -> Result<Featured> {
    let thresholds = match &ctx.store {
        Some(s) => {
            let p = s.path(&named::THRESHOLDS, &[("name", "features")])?;
            Thresholds::parse(&std::fs::read_to_string(&p).unwrap_or_else(|_| SHIPPED_THRESHOLDS.to_string()))?
        }
        None => Thresholds::parse(SHIPPED_THRESHOLDS)?,
    };
    let facts = Facts::build(&ctx.root, &Default::default(), crate::paths::Resolving::Free)?;
    let judged = judge(&facts, &thresholds);
    let spec = spec_for(&facts, &judged);
    let mut said = Vec::new();
    let (page_file, spec_file) = match &ctx.store {
        Some(store) => {
            let sf = store.path(&named::PLAN, &[("name", "features")])?;
            write_whole(&sf, toml::to_string(&spec).map_err(|e| ReadingRefusal::Write.refuse(e))?.as_bytes())?;
            let dir = store.reading_dir("features")?;
            let pf = dir.join(format!("{}.md", stamp()));
            let text = page(&judged, Some(&ctx.rel(&sf)));
            write_whole(&pf, text.as_bytes())?;
            write_whole(&dir.join("latest.md"), text.as_bytes())?;
            write_whole(&dir.join("latest.json"), (serde_json::to_string_pretty(&judged)? + "\n").as_bytes())?;
            said.push(format!("reading: {}", ctx.rel(&pf)));
            said.push(format!("code_passenger spec: {}", ctx.rel(&sf)));
            // the lines each eligible dependency's gate header carries
            let headers: BTreeMap<String, BTreeMap<String, Vec<String>>> = judged.iter().filter(|j| j.eligible).fold(BTreeMap::new(), |mut m, j| {
                m.entry(j.package.clone()).or_default().insert(j.dependency.clone(), j.unavailable.header());
                m
            });
            let hf = store.path(&named::PLAN, &[("name", "features-unavailable")])?;
            write_whole(&hf, toml::to_string(&headers).map_err(|e| ReadingRefusal::Write.refuse(e))?.as_bytes())?;
            said.push(format!("gate headers (unavailable with each off): {}", ctx.rel(&hf)));
            (Some(pf), Some(sf))
        }
        None => {
            print!("{}", page(&judged, None));
            println!("\n```toml\n{}```", toml::to_string(&spec).unwrap_or_default());
            said.push("free mode: nothing stored".into());
            (None, None)
        }
    };
    if apply {
        let sf = spec_file.clone().ok_or_else(|| ReadingRefusal::NotInitialized.refuse("features --apply keeps its spec in .custodian/"))?;
        let mut plan = Plan::new("features-apply");
        for pkg in spec.keys() {
            let mut line = exec::passenger_gate(&ctx.root, &sf, true);
            line = line.words(["--package", pkg.as_str()]);
            plan = plan.then(exec::line(format!("gate:{pkg}"), &line)?.going_on());
        }
        let walked = plan.walk(Watch::Quiet, |_, _| {}).map_err(|e| {
            if e.to_string().contains("CUST0014") {
                ReadingRefusal::ProgramMissing.refuse(format!("code-passenger (custodian self install code-passenger --from <the code_passenger repo>): {e}"))
            } else {
                e
            }
        })?;
        for w in &walked {
            let r = receipt_of(&plan, w, "gate", &w.operation.logical_id, None, None, None, None, vec![ctx.rel(&sf)], None);
            if let Some(store) = &ctx.store {
                let (r, f) = keep(store, r)?;
                said.push(format!("{}: {} (run #{}, receipt {})", r.line, r.reason, r.seq, ctx.rel(&f)));
            }
        }
    }
    Ok(Featured { judged, page: page_file, spec_file, said })
}
