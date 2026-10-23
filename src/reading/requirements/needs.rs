//! The requirement side: what the intended change needs (from the intent's words, a change spec's dependency and
//! feature operations, or needs written in the spec), each checked against the workspace: declared in the crate it
//! lands in, at a version the lock holds, with the features it needs on and what turns them on (klepto's feature
//! model, the four entry forms), from a source that exists. Then its impact: the crate's dependencies and dependents
//! in cargo's resolve, and the readings that follow it item by item.

use super::manifest::{normalize, CrateRef, Manifest, Scan};
use super::{finding, Basis, Check, Finding, Impact, Need, Place, ReadFrom, Status};
use crate::change::{ChangeSpec, Operation};
use klepto::{CargoPackage, CargoWorkspace, DepKind, FeatureEnable, FeatureModel};
use std::collections::BTreeSet;
use std::path::Path;

pub struct NeedsFace {
    pub findings: Vec<Finding>,
    pub reuse: Vec<String>,
    pub unknowns: Vec<String>,
}

/// Words that say an intent brings a package into a crate: without one, a name the intent spells plainly is a crate
/// it works on, not a dependency it needs (`merge helper_a into helper_b` asks for no dependency).
const ASKING: &[&str] = &["add", "adds", "adding", "use", "uses", "using", "depend", "depends", "depending", "integrate", "integrates", "integrating", "bring", "brings", "need", "needs", "require", "requires", "import", "pull", "with"];

/// Words that join an intent and name nothing.
const JOINING: &[&str] = &["to", "in", "into", "for", "on", "of", "with", "from", "and", "the", "a", "an", "add", "use", "using", "crate", "integrate", "depend", "needs", "need"];

/// The needs an intent's words name: a crate of the workspace it lands in (`to app`, `crate:app`), and the
/// packages it brings (`hex`, `hex@0.4`, `hex/serde`), each a package the lock or a manifest already names; a name
/// spelled as code that nothing holds is a need of its own, unknown.
pub fn of_intent(intent: &str, scan: &Scan, ws: Option<&CargoWorkspace>, crates: &[CrateRef]) -> Vec<Need> {
    let norm = |s: &str| s.replace('-', "_");
    let members: Vec<String> = scan.packages().filter(|m| scan.is_member(&m.path)).filter_map(Manifest::name).collect();
    let is_member = |t: &str| members.iter().find(|m| norm(m) == norm(t)).cloned();
    let known = |t: &str| -> bool {
        ws.is_some_and(|w| !w.by_name(t).is_empty())
            || scan.manifests.values().any(|m| m.deps.iter().any(|d| norm(d.package_name()) == norm(t) || norm(&d.key) == norm(t)) || m.workspace.as_ref().is_some_and(|w| w.dependencies.iter().any(|d| norm(&d.key) == norm(t))))
    };
    let tokens: Vec<&str> = intent.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '"' | '\'' | '`')).map(|t| t.trim_end_matches(['.', ':', '!', '?'])).filter(|t| !t.is_empty()).collect();
    // the crate it lands in: the target's, else a member named after `to`/`in`/`into`/`for`, else the one member named
    let mut target: Option<String> = match &scan.target {
        super::manifest::Target::Crate(n) => Some(n.clone()),
        super::manifest::Target::Dir(_) => crates.first().map(|c| c.name.clone()),
        super::manifest::Target::All => None,
    };
    if target.is_none() {
        for w in tokens.windows(2) {
            if matches!(w[0], "to" | "in" | "into" | "for") {
                if let Some(m) = is_member(w[1].strip_prefix("crate:").unwrap_or(w[1])) {
                    target = Some(m);
                }
            }
        }
    }
    let named_members: Vec<String> = tokens.iter().filter_map(|t| is_member(t.strip_prefix("crate:").unwrap_or(t))).collect();
    if target.is_none() && named_members.len() == 1 {
        target = named_members.first().cloned();
    }
    let asking = tokens.iter().any(|t| ASKING.contains(&t.to_lowercase().as_str()));
    let mut needs: Vec<Need> = Vec::new();
    fn add(needs: &mut Vec<Need>, target: &Option<String>, dep: &str, version: Option<&str>, feature: Option<&str>, said: String) {
        let key = dep.replace('-', "_");
        let at = match needs.iter().position(|n| n.dependency.as_deref().map(|d| d.replace('-', "_")) == Some(key.clone())) {
            Some(i) => i,
            None => {
                needs.push(Need { member: target.clone(), dependency: Some(dep.to_string()), said, ..Default::default() });
                needs.len() - 1
            }
        };
        if let Some(v) = version {
            needs[at].version = Some(v.to_string());
        }
        if let Some(f) = feature {
            needs[at].features.push(f.to_string());
        }
    }
    for t in &tokens {
        let t = t.strip_prefix("crate:").unwrap_or(t);
        if JOINING.contains(&t.to_lowercase().as_str()) || target.as_deref().is_some_and(|x| norm(x) == norm(t)) {
            continue;
        }
        if let Some((d, v)) = t.split_once('@') {
            add(&mut needs, &target, d, Some(v), None, format!("intent: `{t}`"));
        } else if let Some((d, f)) = t.split_once('/').filter(|(d, f)| !d.is_empty() && !f.is_empty() && !f.contains('/')) {
            if known(d) || is_member(d).is_some() {
                add(&mut needs, &target, d, None, Some(f), format!("intent: `{t}`"));
            }
        } else if !asking {
            continue;
        } else if known(t) || is_member(t).is_some() {
            add(&mut needs, &target, t, None, None, format!("intent: `{t}`"));
        } else if t.contains('_') || t.contains("::") || (t.contains('-') && t.chars().any(char::is_alphabetic)) {
            needs.push(Need { member: target.clone(), dependency: Some(t.to_string()), said: format!("intent: `{t}` (named by nothing read)"), ..Default::default() });
        }
    }
    if needs.is_empty() {
        if let Some(t) = target {
            needs.push(Need { member: Some(t), said: "intent: the crate it lands in".into(), ..Default::default() });
        }
    }
    needs
}

/// The needs a change spec's operations name: `add_dependency`/`change_dependency` each dependency; `declare_feature`
/// each `dep:x` and `x/f` it turns on; any other operation the crate it lands in.
pub fn of_change(spec: &ChangeSpec) -> Vec<Need> {
    let member = spec.target.strip_prefix("crate:").map(|t| t.split('/').next().unwrap_or(t).to_string()).filter(|t| t != "*");
    let said = |op: &str| format!("spec {}: {op}", spec.name);
    match spec.operation {
        Some(op @ (Operation::AddDependency | Operation::ChangeDependency)) => spec
            .dependency
            .iter()
            .chain(&spec.dependencies)
            .map(|d| Need { member: member.clone(), dependency: Some(d.name.clone()), version: d.version.clone(), features: d.features.clone(), path: d.path.clone(), optional: d.optional, new_crate: None, said: said(op.as_str()) })
            .collect(),
        Some(Operation::DeclareFeature) => spec
            .enables
            .iter()
            .filter_map(|e| match FeatureEnable::parse(e) {
                FeatureEnable::Dep(d) => Some(Need { member: member.clone(), dependency: Some(d), optional: Some(true), said: said("declare_feature"), ..Default::default() }),
                FeatureEnable::DepFeature { dep, feature, .. } => Some(Need { member: member.clone(), dependency: Some(dep), features: vec![feature], said: said("declare_feature"), ..Default::default() }),
                FeatureEnable::Feature(_) => None,
            })
            .collect(),
        op => vec![Need { member, said: said(op.map(|o| o.as_str()).unwrap_or("no operation")), ..Default::default() }],
    }
}

fn norm(s: &str) -> String {
    s.replace('-', "_")
}

/// A member's manifest by its package name.
fn member<'a>(scan: &'a Scan, name: &str) -> Option<&'a Manifest> {
    let mut hits: Vec<&Manifest> = scan.packages().filter(|m| m.name().is_some_and(|n| norm(&n) == norm(name))).collect();
    hits.sort_by_key(|m| !scan.is_member(&m.path));
    hits.into_iter().next()
}

fn place(scan: &Scan, m: &Manifest, line: u32, section: &str, field: &str) -> Place {
    Place { file: scan.rel(&m.path), line: (line > 0).then_some(line), section: section.into(), field: field.into() }
}

/// The package cargo resolved a member's dependency `dep` to.
fn resolved_dep<'a>(ws: &'a CargoWorkspace, m: &Manifest, dep: &str) -> Option<&'a CargoPackage> {
    let me = ws.packages.values().find(|p| p.manifest_path == m.path)?;
    let node = ws.resolve.as_ref()?.get(&me.id)?;
    let e = node.deps.iter().find(|e| ws.packages.get(&e.package).is_some_and(|p| norm(&p.name) == norm(dep)) || norm(&e.name) == norm(dep))?;
    ws.packages.get(&e.package)
}

/// Every need checked.
pub fn check(needs: &[Need], scan: &Scan, ws: Option<&CargoWorkspace>, resolved: bool) -> NeedsFace {
    let mut out = Vec::new();
    let mut reuse: Vec<String> = Vec::new();
    let mut unknowns = Vec::new();
    let mut combos_for: BTreeSet<String> = BTreeSet::new();
    if let super::manifest::Target::Crate(n) = &scan.target {
        combos_for.insert(n.clone());
    }
    for need in needs {
        if let Some(dir) = &need.new_crate {
            new_crate(scan, need, dir, &mut out);
            continue;
        }
        let m = need.member.as_deref().and_then(|n| member(scan, n));
        let Some(dep) = need.dependency.as_deref() else {
            if let (Some(n), None) = (&need.member, m) {
                out.push(finding(Check::NeedDeclared, Status::Unsatisfied, Basis::Observed, ReadFrom::Intent, Some(n), None, format!("the change lands in `{n}`, and no manifest read names a package `{n}`"), format!("a crate `{n}` in the workspace"), format!("{} manifests read", scan.analyzed.len()), "the change has nowhere to land", Some(format!("make the crate `{n}` first, or name the crate it lands in"))));
            }
            if let Some(m) = m {
                combos_for.insert(m.name().unwrap_or_default());
            }
            continue;
        };
        let in_lock: Vec<&CargoPackage> = ws.map(|w| w.by_name(dep)).unwrap_or_default();
        // reuse: what the workspace already holds of it
        let mut users: Vec<String> = Vec::new();
        for x in scan.packages().filter(|x| scan.is_member(&x.path)) {
            for d in x.deps.iter().filter(|d| norm(scan.effective(&x.path, d).package_name()) == norm(dep)) {
                let e = scan.effective(&x.path, d);
                users.push(format!("{} ({}{})", x.name().unwrap_or_default(), if d.workspace { "workspace = true".to_string() } else { e.version.clone().or(e.path.clone()).unwrap_or_else(|| "*".into()) }, if d.kind == DepKind::Normal { String::new() } else { format!(", {}", d.section) }));
            }
        }
        users.sort();
        users.dedup();
        let root_entry = m.and_then(|m| scan.governing.get(&m.path).cloned().flatten()).and_then(|g| scan.manifests.get(&g)).and_then(|r| r.workspace.as_ref().and_then(|w| w.dependencies.iter().find(|d| norm(&d.key) == norm(dep))).map(|d| (r, d)));
        if let Some((r, d)) = &root_entry {
            reuse.push(format!("`{dep}` is declared once for the workspace: {}:{} (`{} = {{ workspace = true }}` inherits it)", scan.rel(&r.path), d.line, d.key));
        }
        if !in_lock.is_empty() {
            reuse.push(format!("the lock holds {dep} {}: a requirement one of them meets adds no new version{}", in_lock.iter().map(|p| p.version.as_str()).collect::<Vec<_>>().join(", "), if users.is_empty() { String::new() } else { format!(" (used by {})", super::shorten(&users, 8)) }));
        } else if !users.is_empty() {
            reuse.push(format!("`{dep}` is declared by {}", super::shorten(&users, 8)));
        }
        let Some(m) = m else {
            let n = need.member.clone().unwrap_or_default();
            let (what, action) = if n.is_empty() {
                (format!("`{dep}` is needed, and the intent names no crate it lands in"), "name the crate it lands in (crate:<name>)".to_string())
            } else {
                (format!("`{dep}` is needed in `{n}`, which no manifest read names"), format!("make `{n}` first"))
            };
            out.push(finding(Check::NeedDeclared, Status::Unsatisfied, Basis::Unknown, ReadFrom::Intent, None, None, what, format!("`{dep}` declared in the crate the change lands in"), need.said.clone(), "where it is declared decides which manifest changes", Some(action)));
            source(scan, need, dep, None, &in_lock, &mut out, &mut unknowns);
            continue;
        };
        let mname = m.name().unwrap_or_default();
        combos_for.insert(mname.clone());
        let decl = m.deps.iter().find(|d| norm(scan.effective(&m.path, d).package_name()) == norm(dep) || norm(&d.key) == norm(dep));
        match decl {
            Some(d) => out.push(finding(Check::NeedDeclared, Status::Satisfied, Basis::Observed, ReadFrom::Manifest, Some(&mname), Some(place(scan, m, d.line, &d.section, &d.key)), format!("`{mname}` declares `{dep}`"), format!("`{dep}` declared in `{mname}`"), format!("{} = {}", d.key, if d.workspace { "{ workspace = true }".to_string() } else { d.version.clone().or(d.path.clone()).unwrap_or_else(|| "*".into()) }), "the declaration is there; its version and features are below", None)),
            None => {
                let action = match (&root_entry, in_lock.last()) {
                    (Some((r, x)), _) => format!("add `{} = {{ workspace = true }}` to [dependencies] of {} (the root declares it: {}:{})", x.key, scan.rel(&m.path), scan.rel(&r.path), x.line),
                    (None, Some(p)) => format!("add `{dep}` to [dependencies] of {} with a requirement {} {} meets (the version the lock already holds; one more version otherwise)", scan.rel(&m.path), dep, p.version),
                    (None, None) => format!("add `{dep}` to [dependencies] of {}; its version is not established by this reading (not in the lock, and the registry is not read)", scan.rel(&m.path)),
                };
                out.push(finding(Check::NeedDeclared, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, Some(&mname), Some(place(scan, m, m.package.as_ref().map(|p| p.line).unwrap_or(0), "dependencies", dep)), format!("`{mname}` does not declare `{dep}`"), format!("`{dep}` in a dependencies table of `{mname}`"), format!("{} has no entry naming package `{dep}` ({})", scan.rel(&m.path), need.said), "the change cannot use it until it is declared", Some(action)));
            }
        }
        // the version asked for, against the lock
        if let Some(req) = &need.version {
            match (ws.filter(|_| resolved), resolved_dep_of(ws, m, dep)) {
                (Some(_), Some(p)) => {
                    let meets = klepto::version_meets(req, &p.version);
                    let status = if meets == Some(true) { Status::Satisfied } else { Status::Unsatisfied };
                    out.push(finding(Check::NeedVersion, status, if meets.is_some() { Basis::Observed } else { Basis::Unknown }, ReadFrom::Metadata, Some(&mname), None, format!("`{mname}` resolves `{dep}` to {}; the change asks `{req}`", p.version), format!("{dep} `{req}`"), format!("cargo's resolve: {mname} → {dep} {}; semver: `{req}` {} it", p.version, match meets { Some(true) => "admits", Some(false) => "does not admit", None => "does not read against" }), if meets == Some(false) { "the change needs another version than the one this crate builds with: a requirement change, and maybe a second version in the lock".to_string() } else { "the version is there".to_string() }, None));
                }
                (Some(_), None) => {
                    let meeting: Vec<&str> = in_lock.iter().filter(|p| klepto::version_meets(req, &p.version) == Some(true)).map(|p| p.version.as_str()).collect();
                    if meeting.is_empty() {
                        out.push(finding(Check::NeedVersion, Status::Unsatisfied, Basis::Inferred, ReadFrom::Metadata, Some(&mname), None, format!("no version of `{dep}` in the lock meets `{req}`"), format!("{dep} `{req}`"), format!("the lock holds {}", if in_lock.is_empty() { "none".to_string() } else { in_lock.iter().map(|p| p.version.as_str()).collect::<Vec<_>>().join(", ") }), "cargo would fetch one from the registry (not read here) and write the lock; with another requirement keeping the lock's version, the lock holds two", Some(format!("declare `{dep} = \"{req}\"` and let cargo resolve it (it writes Cargo.lock; this reading does not); which version it picks is cargo's"))));
                    } else {
                        out.push(finding(Check::NeedVersion, Status::Satisfied, Basis::Inferred, ReadFrom::Metadata, Some(&mname), None, format!("the lock holds {dep} {}, which `{req}` admits", meeting.join(", ")), format!("{dep} `{req}`"), "cargo metadata's packages; semver's answer for each", "declaring it should resolve to a version already locked (cargo confirms it when it resolves)", None));
                    }
                }
                (None, _) => unknowns.push(format!("whether {dep} `{req}` is satisfiable: cargo's resolve was not read")),
            }
        }
        // the features it needs
        let model = dep_model(scan, ws, m, dep, &in_lock);
        let my_model = scan.feature_model(m);
        for f in &need.features {
            let Some((dm, from_where)) = &model else {
                out.push(finding(Check::NeedFeature, Status::Unsatisfied, Basis::Unknown, ReadFrom::Intent, Some(&mname), None, format!("whether `{dep}` has feature `{f}` is unknown"), format!("{dep} feature `{f}`"), "the package is neither in the lock nor at a path read", "cargo says when it resolves it", None));
                continue;
            };
            if !dm.features.contains_key(f) {
                out.push(finding(Check::NeedFeature, Status::Unsatisfied, Basis::Observed, ReadFrom::Metadata, Some(&mname), None, format!("`{dep}` has no feature `{f}` ({from_where})"), format!("{dep} feature `{f}`"), format!("{dep}'s features: {}", dm.features.keys().cloned().collect::<Vec<_>>().join(", ")), "cargo refuses a dependency entry that asks for it (\"does not have that feature\")", Some(format!("pick a feature {dep} has, or a version of it that has `{f}` (which one: not read here)"))));
                continue;
            }
            let entry = decl.map(|d| scan.effective(&m.path, d));
            let asked: Vec<&str> = entry.as_ref().map(|e| e.features.iter().map(String::as_str).collect()).unwrap_or_default();
            let on = entry.as_ref().map(|e| dm.active_features(&asked, e.default_features.unwrap_or(true))).unwrap_or_default();
            let forwarders: Vec<String> = my_model
                .features
                .iter()
                .flat_map(|(g, info)| info.enables.iter().map(move |e| (g, e)))
                .filter_map(|(g, e)| match e {
                    FeatureEnable::DepFeature { dep: d, feature, weak } if norm(d) == norm(dep) && feature == f => Some(format!("`{g}` (`{}{}/{f}`{})", d, if *weak { "?" } else { "" }, if *weak { ": weak, only when the dependency is on by another feature" } else { "" })),
                    _ => None,
                })
                .collect();
            let unified = ws.and_then(|w| resolved_dep(w, m, dep).and_then(|p| w.resolve.as_ref()?.get(&p.id))).map(|n| n.features.contains(f));
            if on.contains(f) {
                out.push(finding(Check::NeedFeature, Status::Satisfied, Basis::Observed, ReadFrom::Manifest, Some(&mname), decl.map(|d| place(scan, m, d.line, &d.section, &d.key)), format!("`{mname}`'s declaration turns on `{dep}`'s `{f}` in every build"), format!("{dep} feature `{f}` on"), format!("features {:?}, default-features {} → {}'s active set holds `{f}` (klepto's feature model, {from_where})", asked, entry.as_ref().and_then(|e| e.default_features).unwrap_or(true), dep), "it is on", None));
            } else if !forwarders.is_empty() {
                out.push(finding(Check::NeedFeature, Status::Informational, Basis::Inferred, ReadFrom::Manifest, Some(&mname), decl.map(|d| place(scan, m, d.line, &d.section, &d.key)), format!("`{dep}`'s `{f}` is on only with `{mname}`'s feature {}", forwarders.join(", ")), format!("{dep} feature `{f}` on"), "klepto's feature model of the manifest as written", "a build without that feature does not have it; which combinations are built is unknown (below)", Some(format!("turn it on in every build: add \"{f}\" to the features of `{dep}` in {}", scan.rel(&m.path)))));
            } else {
                let note = match unified {
                    Some(true) => " (cargo's whole-workspace resolve has it on through another crate's request: `-p` alone may not)",
                    _ => "",
                };
                out.push(finding(Check::NeedFeature, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, Some(&mname), decl.map(|d| place(scan, m, d.line, &d.section, &d.key)), format!("nothing in `{mname}` turns on `{dep}`'s `{f}`{note}"), format!("{dep} feature `{f}` on"), format!("its entry asks {:?}; no feature of `{mname}` forwards `{dep}/{f}`", asked), "the change's code that needs it does not compile in the crate's own build", Some(format!("add \"{f}\" to the features of `{dep}` in {}", scan.rel(&m.path)))));
            }
        }
        source(scan, need, dep, Some(m), &in_lock, &mut out, &mut unknowns);
    }
    for c in combos_for {
        if let Some(m) = member(scan, &c) {
            combinations(scan, ws, m, &mut out);
        }
    }
    NeedsFace { findings: out, reuse, unknowns }
}

fn resolved_dep_of<'a>(ws: Option<&'a CargoWorkspace>, m: &Manifest, dep: &str) -> Option<&'a CargoPackage> {
    resolved_dep(ws?, m, dep)
}

/// The dependency's feature model: the package cargo resolved for the member, else the newest in the lock, else the
/// one at its path; and where it was read.
fn dep_model(scan: &Scan, ws: Option<&CargoWorkspace>, m: &Manifest, dep: &str, in_lock: &[&CargoPackage]) -> Option<(FeatureModel, String)> {
    if let Some(p) = ws.and_then(|w| resolved_dep(w, m, dep)).or(in_lock.last().copied()) {
        return Some((p.features.clone(), format!("{} {} in cargo metadata", p.name, p.version)));
    }
    let d = m.deps.iter().find(|d| norm(scan.effective(&m.path, d).package_name()) == norm(dep))?;
    let e = scan.effective(&m.path, d);
    let p = e.path.as_ref()?;
    let dir = if d.workspace { normalize(Path::new(p)) } else { normalize(&m.dir.join(p)) };
    let t = scan.manifests.get(&dir.join("Cargo.toml")).cloned().or_else(|| super::manifest::read_manifest(&dir.join("Cargo.toml")))?;
    Some((scan.feature_model(&t), format!("its manifest at {}", scan.rel(&dir))))
}

/// Where a needed dependency comes from.
fn source(scan: &Scan, need: &Need, dep: &str, m: Option<&Manifest>, in_lock: &[&CargoPackage], out: &mut Vec<Finding>, unknowns: &mut Vec<String>) {
    let krate = m.and_then(Manifest::name);
    if let Some(p) = &need.path {
        let base = m.map(|m| m.dir.clone()).unwrap_or_else(|| scan.root.clone());
        let dir = normalize(&base.join(p));
        let there = super::manifest::read_manifest(&dir.join("Cargo.toml"));
        let (status, what) = match there.as_ref().and_then(Manifest::name) {
            Some(n) if n == dep => (Status::Satisfied, format!("{} holds package `{dep}`", scan.rel(&dir))),
            Some(n) => (Status::Unsatisfied, format!("{} holds package `{n}`, not `{dep}`", scan.rel(&dir))),
            None => (Status::Unsatisfied, format!("{} holds no Cargo.toml", scan.rel(&dir))),
        };
        out.push(finding(Check::NeedSource, status, Basis::Observed, ReadFrom::Manifest, krate.as_deref(), None, what, format!("a package `{dep}` at path `{p}`"), format!("path = \"{p}\" from {}", scan.rel(&base)), if status == Status::Satisfied { "the source is there".into() } else { "cargo refuses the dependency".to_string() }, None));
        return;
    }
    if in_lock.is_empty() {
        let declared = m.is_some_and(|m| m.deps.iter().any(|d| norm(d.package_name()) == norm(dep) && (d.path.is_some() || d.workspace)));
        if !declared {
            out.push(finding(Check::NeedSource, Status::Informational, Basis::Unknown, ReadFrom::Intent, krate.as_deref(), None, format!("`{dep}` is not in the lock: it comes from a registry this reading does not read"), format!("a source for `{dep}`"), need.said.clone(), "its versions, features and rust-version are unknown until cargo resolves it (online)", None));
            unknowns.push(format!("`{dep}`: not in the lock; the registry is not read (offline), so its versions, features and rust-version are unknown"));
        }
    }
}

/// A crate's feature combinations: the default set cargo resolved, and how many others nothing resolved or built.
fn combinations(scan: &Scan, ws: Option<&CargoWorkspace>, m: &Manifest, out: &mut Vec<Finding>) {
    let model = scan.feature_model(m);
    let declared: Vec<(&String, String)> = model
        .features
        .iter()
        .filter(|(n, i)| !i.implicit && n.as_str() != "default")
        .map(|(n, i)| {
            (
                n,
                i.enables
                    .iter()
                    .map(|e| match e {
                        FeatureEnable::Feature(g) => g.clone(),
                        FeatureEnable::Dep(d) => format!("dep:{d}"),
                        FeatureEnable::DepFeature { dep, feature, weak } => format!("{dep}{}/{feature}", if *weak { "?" } else { "" }),
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        })
        .collect();
    let optional: Vec<String> = model.dependencies.iter().filter(|d| d.optional).map(|d| d.name.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    let n = declared.len() + model.features.values().filter(|i| i.implicit).count();
    if n == 0 {
        return;
    }
    let name = m.name().unwrap_or_default();
    let default: Vec<String> = model.active_features(&[], true).into_iter().collect();
    let resolved = ws.and_then(|w| {
        let me = w.packages.values().find(|p| p.manifest_path == m.path)?;
        w.resolve.as_ref()?.get(&me.id).map(|n| n.features.clone())
    });
    let others = if n >= 63 { "more than 2^62".to_string() } else { ((1u64 << n) - 1).to_string() };
    out.push(finding(
        Check::FeatureCombinations,
        Status::Informational,
        Basis::Unknown,
        ReadFrom::Manifest,
        Some(&name),
        None,
        format!("`{name}` has {n} features; {others} combinations besides the one cargo resolved are not resolved and not built"),
        "-",
        format!(
            "features: {}{}; default turns on [{}]; cargo's resolve: {}",
            declared.iter().map(|(n, e)| format!("{n} = [{e}]")).collect::<Vec<_>>().join("; "),
            if optional.is_empty() { String::new() } else { format!("; optional dependencies: {}", optional.join(", ")) },
            default.join(", "),
            resolved.map(|f| format!("[{}]", f.join(", "))).unwrap_or_else(|| "not read".into())
        ),
        "a combination nothing built is unknown: `cargo check -p <crate> --no-default-features`, `--all-features` and the combinations the change touches are the checks (not run here)",
        None,
    ));
}

/// A new crate's directory: the root that would govern it, whether its members take it, its name free.
fn new_crate(scan: &Scan, need: &Need, dir: &str, out: &mut Vec<Finding>) {
    let path = normalize(&scan.root.join(dir));
    let name = need.member.clone().unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let mut gov: Option<&Manifest> = None;
    for a in path.ancestors().skip(1) {
        if let Some(m) = scan.manifests.get(&a.join("Cargo.toml")).filter(|m| m.is_root()) {
            gov = Some(m);
            break;
        }
    }
    let taken = scan.packages().any(|m| m.name().as_deref() == Some(&name) && scan.is_member(&m.path));
    if taken {
        out.push(finding(Check::NeedNewCrate, Status::Unsatisfied, Basis::Observed, ReadFrom::Manifest, Some(&name), None, format!("a member is already named `{name}`"), "a package name no member has", "the members' [package] names", "cargo refuses two packages of one name in a workspace", Some("pick another name".into())));
    }
    match gov {
        Some(r) => {
            let ws = r.workspace.as_ref().expect("a root");
            let rel_dir = super::rel(&r.dir, &path);
            let covered = ws.members.iter().any(|(e, _)| {
                if e.contains(['*', '?', '[']) {
                    globset::GlobBuilder::new(e).literal_separator(true).build().map(|g| g.compile_matcher().is_match(&rel_dir)).unwrap_or(false)
                } else {
                    normalize(&r.dir.join(e)) == path
                }
            });
            let (status, what, action) = if covered {
                (Status::Satisfied, format!("{} would govern it, and its members take `{rel_dir}`", scan.rel(&r.path)), None)
            } else {
                (Status::Unsatisfied, format!("{} would govern it, and no members entry takes `{rel_dir}`", scan.rel(&r.path)), Some(format!("add \"{rel_dir}\" to [workspace].members of {} (or to its exclude, and the crate is its own)", scan.rel(&r.path))))
            };
            out.push(finding(Check::NeedNewCrate, status, Basis::Inferred, ReadFrom::Manifest, Some(&name), Some(Place { file: scan.rel(&r.path), line: Some(ws.line), section: "workspace".into(), field: "members".into() }), what, "the governing root lists the new crate", format!("nearest ancestor with [workspace] above {}", scan.rel(&path)), if covered { "cargo takes it as a member".to_string() } else { "cargo refuses it inside that directory (\"believes it's in a workspace when it's not\")".to_string() }, action));
        }
        None => out.push(finding(Check::NeedNewCrate, Status::Informational, Basis::Inferred, ReadFrom::Manifest, Some(&name), None, format!("no workspace above {}: the crate would be its own", scan.rel(&path)), "-", "no ancestor Cargo.toml with [workspace]", "it builds alone, with its own lock", None)),
    }
}

/// What the crates of the change touch, from cargo's resolve, and the readings that follow it item by item.
pub fn impact(scan: &Scan, ws: Option<&CargoWorkspace>, crates: &[CrateRef], needs: &[Need], asked: bool) -> Vec<Impact> {
    let mut names: Vec<String> = needs.iter().filter_map(|n| n.member.clone()).collect();
    if !matches!(scan.target, super::manifest::Target::All) {
        names.extend(crates.iter().map(|c| c.name.clone()));
    }
    if !asked && matches!(scan.target, super::manifest::Target::All) {
        return Vec::new();
    }
    names.sort();
    names.dedup();
    let label = |w: &CargoWorkspace, id: &str| w.packages.get(id).map(|p| format!("{} {}", p.name, p.version)).unwrap_or_else(|| id.to_string());
    names
        .into_iter()
        .map(|n| {
            let readings = vec![format!("custodian run crate:{n} --kind impact"), format!("custodian run crate:{n} --kind affected_tests")];
            let me = ws.and_then(|w| member(scan, &n).and_then(|m| w.packages.values().find(|p| p.manifest_path == m.path)));
            match (ws, me) {
                (Some(w), Some(p)) => {
                    let (direct, all) = w.dependents(&p.id);
                    let deps = w.resolve.as_ref().and_then(|r| r.get(&p.id)).map(|node| node.deps.iter().map(|e| label(w, &e.package)).collect()).unwrap_or_default();
                    Impact { krate: n, depends_on: deps, direct_dependents: direct.iter().map(|i| label(w, i)).collect(), all_dependents: all.iter().map(|i| label(w, i)).collect(), readings }
                }
                _ => Impact { krate: n, depends_on: Vec::new(), direct_dependents: Vec::new(), all_dependents: Vec::new(), readings },
            }
        })
        .collect()
}
