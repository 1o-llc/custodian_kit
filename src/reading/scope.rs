//! Scope depth: how deep a dependency (or a set of items) sits in a crate,
//! and what turning it off leaves unavailable.
//!
//! The direct users are the crate's items that reference the dependency's
//! items (klepto's dependency use sites, on the item graph). The unavailable
//! set is those, every item of the crate that reaches them through
//! references, and every item nested in an unavailable one, to a fixpoint.
//! Each file and each module of the crate is then wholly behind it (every
//! item of it is unavailable) or partly. The units are the widest wholes: the
//! crate, then whole files, then whole inline modules, then each remaining
//! item (`function` for a fn or method, `item` otherwise). The depth is the
//! widest unit ([`Depth`]). Items of other crates that call an unavailable
//! item lose it: those are the callers elsewhere.
//!
//! `features` judges a dependency with it and `reach` reads it for a change;
//! the list is what a gate's header carries ([`Unavailable::header`]).

use super::facts::Facts;
use super::spec::Depth;
use klepto::graph::NodeKind;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A unit wholly behind the cause, labelled with its depth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub depth: Depth,
    /// The crate, the file (relative), the module key, or the item id.
    pub name: String,
    /// Items in it.
    pub items: usize,
}

/// A file or module of the crate: how much of it is behind the cause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub name: String,
    pub items: usize,
    pub unavailable: usize,
    pub wholly: bool,
}

/// An item of another crate that calls what goes away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lost {
    pub caller: String,
    pub package: String,
    pub loses: Vec<String>,
}

/// With the cause off: what is unavailable in the crate, how deep it sits,
/// and who elsewhere loses what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unavailable {
    pub package: String,
    /// The dependency, or the items, that go away.
    pub cause: String,
    pub depth: Option<Depth>,
    /// The crate's items that use the cause directly.
    pub direct: Vec<String>,
    /// Every item of the crate unavailable with the cause off.
    pub items: Vec<String>,
    pub units: Vec<Unit>,
    /// Files with an unavailable item, wholly or partly.
    pub files: Vec<Part>,
    /// Modules with an unavailable item, wholly or partly.
    pub modules: Vec<Part>,
    pub callers_elsewhere: Vec<Lost>,
}

/// The items scope counts: every declared item but the module items themselves.
fn counted(d: &klepto::ItemDecl) -> bool {
    d.kind != NodeKind::Module && d.kind != NodeKind::External
}

/// The crate's items that reference `dependency`'s items.
pub fn direct_users(f: &Facts, package: &str, dependency: &str) -> BTreeSet<String> {
    let g = &f.k.graph;
    let mut direct = BTreeSet::new();
    for (id, n) in &g.nodes {
        let of_dep = (n.kind == NodeKind::External && n.module == dependency) || n.package.as_deref() == Some(dependency) || f.item(id).is_some_and(|d| d.package == dependency);
        if !of_dep {
            continue;
        }
        for r in f.referrers_of(id) {
            if f.item(r).is_some_and(|i| i.package == package && counted(i)) {
                direct.insert(r.clone());
            }
        }
    }
    direct
}

/// With `direct` gone from `package` (the items using `cause`): the scope.
pub fn unavailable(f: &Facts, package: &str, cause: &str, direct: &BTreeSet<String>) -> Unavailable {
    let in_pkg: Vec<&klepto::ItemDecl> = f.k.items.iter().filter(|d| d.package == package && counted(d)).collect();
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for d in &in_pkg {
        if let Some(p) = &d.parent {
            children.entry(p.as_str()).or_default().push(d.id.as_str());
        }
    }
    // the unavailable set, to a fixpoint: referrers in the crate, and nested items
    let mut gone: BTreeSet<String> = direct.iter().filter(|id| f.item(id).is_some_and(|d| d.package == package)).cloned().collect();
    loop {
        let before = gone.len();
        let reached: Vec<String> = f.closure_of(gone.iter().map(String::as_str)).into_iter().filter(|id| f.item(id).is_some_and(|d| d.package == package && counted(d))).collect();
        gone.extend(reached);
        let nested: Vec<String> = gone.iter().flat_map(|id| children.get(id.as_str()).into_iter().flatten().map(|c| c.to_string())).collect();
        gone.extend(nested);
        if gone.len() == before {
            break;
        }
    }

    let mut by_file: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut by_module: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for d in &in_pkg {
        let g = gone.contains(&d.id) as usize;
        let e = by_file.entry(f.rel(&d.location.path)).or_default();
        e.0 += 1;
        e.1 += g;
        // a module counts its own items and its child modules'
        let mut m = d.module.as_str();
        loop {
            let e = by_module.entry(m.to_string()).or_default();
            e.0 += 1;
            e.1 += g;
            match m.rsplit_once("::") {
                Some((up, _)) => m = up,
                None => break,
            }
        }
    }
    let part = |(name, (n, u)): (&String, &(usize, usize))| Part { name: name.clone(), items: *n, unavailable: *u, wholly: *n > 0 && n == u };
    let files: Vec<Part> = by_file.iter().filter(|(_, (_, u))| *u > 0).map(part).collect();
    let modules: Vec<Part> = by_module.iter().filter(|(_, (_, u))| *u > 0).map(part).collect();

    let mut units = Vec::new();
    if !in_pkg.is_empty() && gone.len() >= in_pkg.len() && in_pkg.iter().all(|d| gone.contains(&d.id)) {
        units.push(Unit { depth: Depth::Crate, name: package.to_string(), items: in_pkg.len() });
    } else {
        let whole_files: BTreeSet<&str> = files.iter().filter(|p| p.wholly).map(|p| p.name.as_str()).collect();
        units.extend(files.iter().filter(|p| p.wholly).map(|p| Unit { depth: Depth::File, name: p.name.clone(), items: p.items }));
        let in_whole_file = |d: &klepto::ItemDecl| whole_files.contains(f.rel(&d.location.path).as_str());
        // the widest whole modules not already inside a whole file or a wider whole module
        let whole_modules: BTreeSet<&str> = modules.iter().filter(|p| p.wholly).map(|p| p.name.as_str()).collect();
        let mut taken: BTreeSet<&str> = BTreeSet::new();
        for m in &whole_modules {
            let wider = whole_modules.iter().any(|w| w != m && m.starts_with(&format!("{w}::")));
            let of_m: Vec<&&klepto::ItemDecl> = in_pkg.iter().filter(|d| d.module == *m || d.module.starts_with(&format!("{m}::"))).collect();
            if wider || of_m.iter().all(|d| in_whole_file(d)) {
                continue;
            }
            taken.insert(m);
            units.push(Unit { depth: Depth::Module, name: m.to_string(), items: of_m.len() });
        }
        for d in in_pkg.iter().filter(|d| gone.contains(&d.id)) {
            let in_module = taken.iter().any(|m| d.module == *m || d.module.starts_with(&format!("{m}::")));
            if in_whole_file(d) || in_module || d.parent.as_deref().is_some_and(|p| gone.contains(p)) {
                continue;
            }
            let depth = if matches!(d.kind, NodeKind::Fn | NodeKind::Method) { Depth::Function } else { Depth::Item };
            units.push(Unit { depth, name: d.id.clone(), items: 1 });
        }
    }
    let depth = units.iter().map(|u| u.depth).max();

    let mut lost: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    for g in &gone {
        for r in f.referrers_of(g) {
            if let Some(d) = f.item(r).filter(|d| d.package != package) {
                lost.entry(r.clone()).or_insert_with(|| (d.package.clone(), BTreeSet::new())).1.insert(g.clone());
            }
        }
    }
    Unavailable {
        package: package.to_string(),
        cause: cause.to_string(),
        depth,
        direct: direct.iter().cloned().collect(),
        items: gone.into_iter().collect(),
        units,
        files,
        modules,
        callers_elsewhere: lost.into_iter().map(|(caller, (package, loses))| Lost { caller, package, loses: loses.into_iter().collect() }).collect(),
    }
}

/// The scope of a dependency of `package`.
pub fn of_dependency(f: &Facts, package: &str, dependency: &str) -> Unavailable {
    let direct = direct_users(f, package, dependency);
    unavailable(f, package, dependency, &direct)
}

impl Unavailable {
    /// The list a gate's header carries and an eligibility decision shows:
    /// what is unavailable with the cause off, as comment lines.
    pub fn header(&self) -> Vec<String> {
        let mut out = vec![format!("with `{}` off in {} (depth: {}):", self.cause, self.package, self.depth.map(Depth::as_str).unwrap_or("none"))];
        let whole: Vec<&str> = self.units.iter().filter(|u| u.depth >= Depth::Module).map(|u| u.name.as_str()).collect();
        if !whole.is_empty() {
            out.push(format!("  wholly gated: {}", whole.join(", ")));
        }
        let items: Vec<&str> = self.units.iter().filter(|u| u.depth < Depth::Module).map(|u| u.name.as_str()).collect();
        if !items.is_empty() {
            out.push(format!("  unavailable: {}", items.join(", ")));
        }
        for l in &self.callers_elsewhere {
            out.push(format!("  {} ({}) loses {}", l.caller, l.package, l.loses.join(", ")));
        }
        out
    }
}
