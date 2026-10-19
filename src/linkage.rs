//! Architectural linkage records: why declarations are bound together, kept
//! as documents beside the readings and checked against the code each time
//! they are read.
//!
//! A record names a kind of linkage (the vocabulary below), the reason in the
//! author's words, and the declarations it binds: item paths as klepto names
//! them (`core_lib::Config`), routes (`crate:core_lib`, `crate:core_lib/src/x.rs`)
//! or a dependency (`dep:serde`). It lives in
//! `.custodian/data/linkage/v1.<name>.1o.linkage` (1o, schema declared once with
//! the other documents; `<name>.toml` is its twin).
//!
//! A record is never trusted as written. Read, every bound declaration is
//! resolved through klepto's scan: one that no longer resolves makes the
//! record `stale`; one whose file changed since the record was made (the
//! record keeps each bound file's digest as its evidence) makes it `review`.
//! A comment anchor in the source (`// linkage: <name>`, placed by
//! code_passenger's `anchor` operation) is only a pointer: an anchor whose
//! record does not exist is listed as unbacked.

use crate::reading::facts::Facts;
use crate::reading::kinds::{item_id, item_row, named, sort_rows};
use crate::reading::refusal::ReadingRefusal;
use crate::reading::spec::{vocabulary, ReadingSpec};
use crate::reading::Answer;
use crate::store::{named as layout, write_whole, Store};
use crate::{CodeQueryResult, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

vocabulary! {
    /// Why declarations are bound together.
    LinkKind, "linkage kind" {
        Responsibility = "responsibility" : "a module uses another crate or module because that one owns the responsibility",
        Invariant = "invariant" : "an implementation depends on an invariant another declaration keeps",
        Connected = "connected" : "two types are connected for an architectural reason",
        Compatibility = "compatibility" : "a feature or a shape exists to satisfy a compatibility requirement",
        Synchronized = "synchronized" : "the declarations must stay synchronized with each other",
        Guarded = "guarded" : "a declaration must not be changed without considering the others",
        Semantics = "semantics" : "a dependency must not be replaced casually: its semantics are required",
    }
}

/// One linkage record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkageRecord {
    /// The record's name: its file's name, the name an anchor carries.
    pub name: String,
    pub kind: LinkKind,
    /// The reason, in the author's words.
    pub why: String,
    /// The declarations bound: item paths, `crate:`/`decl:` routes, `dep:<crate>`.
    #[serde(default)]
    pub binds: Vec<String>,
    /// Who recorded it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// When, UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_utc: Option<String>,
    /// The bound files as they were when recorded: `<path>=<sha256 hex>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

/// The marker a comment anchor starts with.
pub const ANCHOR: &str = "// linkage: ";

/// The folder records live in.
pub fn dir(store: &Store) -> Result<PathBuf> {
    store.dir_of(&layout::LINKAGE, &[("name", "x")])
}

/// Every record of the project, by file name.
pub fn load_all(store: &Store) -> Result<Vec<(PathBuf, LinkageRecord)>> {
    let dir = dir(store)?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file()).collect()).unwrap_or_default();
    files.sort();
    let mut out = Vec::new();
    for p in files {
        let text = std::fs::read_to_string(&p).map_err(|e| ReadingRefusal::SpecUnreadable.refuse(format!("{}: {e}", p.display())))?;
        let rec: LinkageRecord = match crate::oneo::Doc::of_file(&p) {
            Some((crate::oneo::Doc::Linkage, _)) => crate::oneo::parse(crate::oneo::Doc::Linkage, &p.display().to_string(), &text)?,
            Some(_) => continue,
            None if p.extension().is_some_and(|e| e == "toml") => toml::from_str(&text).map_err(|e| ReadingRefusal::SpecInvalid.refuse(format!("{}: {e}", p.display())))?,
            None => continue,
        };
        out.push((p, rec));
    }
    Ok(out)
}

/// A record written: in 1o, or its TOML twin when 1o cannot carry a value.
pub fn write(store: &Store, rec: &LinkageRecord) -> Result<PathBuf> {
    let dir = dir(store)?;
    let p = match crate::oneo::render(crate::oneo::Doc::Linkage, rec) {
        Ok(text) => {
            let p = dir.join(crate::oneo::Doc::Linkage.file_name(&rec.name));
            write_whole(&p, text.as_bytes())?;
            p
        }
        Err(_) => {
            let p = dir.join(format!("{}.toml", rec.name));
            write_whole(&p, toml::to_string(rec).map_err(|e| ReadingRefusal::Write.refuse(e))?.as_bytes())?;
            p
        }
    };
    Ok(p)
}

/// One bound declaration, as the code has it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bound {
    pub bind: String,
    /// The item id, the crate, or the file it resolves to.
    pub resolved: Option<String>,
    /// The file holding it, relative to the root.
    pub file: Option<String>,
    pub line: Option<u32>,
    /// Its file changed since the record's evidence.
    pub moved: bool,
}

/// A record's state: every bind resolves and no file moved; one moved; one no longer resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Review,
    Stale,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Review => "review",
            Status::Stale => "stale",
        }
    }
}

/// A file's digest as a record keeps it.
pub fn file_digest(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(crate::digest::of)
}

/// Each bind resolved against the scan.
pub fn bounds(f: &Facts, rec: &LinkageRecord) -> Vec<Bound> {
    let evidence: std::collections::BTreeMap<&str, &str> = rec.evidence.iter().filter_map(|e| e.split_once('=')).collect();
    rec.binds
        .iter()
        .map(|b| {
            let (resolved, file, line) = resolve(f, b);
            let moved = file.as_deref().and_then(|p| evidence.get(p).map(|d| (p, *d))).is_some_and(|(p, d)| file_digest(&f.root.join(p)).as_deref() != Some(d));
            Bound { bind: b.clone(), resolved, file, line, moved }
        })
        .collect()
}

fn resolve(f: &Facts, b: &str) -> (Option<String>, Option<String>, Option<u32>) {
    let b = b.trim();
    if let Some(dep) = b.strip_prefix("dep:") {
        let d = dep.replace('-', "_");
        let user = f.k.features.iter().find(|(_, m)| m.dependencies.iter().any(|x| x.name.replace('-', "_") == d || x.package.replace('-', "_") == d));
        return (user.map(|(p, _)| format!("{p} → {dep}")), user.and_then(|(p, _)| f.packages.get(p).map(|x| format!("{}/Cargo.toml", x.dir).trim_start_matches('/').to_string())), None);
    }
    if crate::paths::names_a_place(b) {
        return match f.paths.resolve(b, crate::paths::Resolving::Free) {
            Ok(r) => match (r.crate_name(), r.crate_file(&f.root)) {
                (Some(c), _) => (Some(c.to_string()), f.packages.get(c).map(|p| format!("{}/Cargo.toml", p.dir).trim_start_matches('/').to_string()), None),
                (None, Some(file)) => (Some(file.clone()), Some(file), None),
                _ => (Some(r.name().to_string()), None, None),
            },
            Err(_) => (None, None, None),
        };
    }
    match item_id(f, b) {
        Ok(id) => {
            let d = f.item(&id);
            (Some(id.clone()), d.map(|d| f.rel(&d.location.path)), d.and_then(|d| d.location.line))
        }
        Err(_) => (None, None, None),
    }
}

/// The record's state from its bounds.
pub fn status(bounds: &[Bound]) -> Status {
    if bounds.iter().any(|b| b.resolved.is_none()) {
        Status::Stale
    } else if bounds.iter().any(|b| b.moved) {
        Status::Review
    } else {
        Status::Ok
    }
}

/// The evidence a record keeps of its binds: each bound file's digest now.
pub fn evidence_of(f: &Facts, binds: &[String]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for b in binds {
        if let (_, Some(file), _) = resolve(f, b) {
            if let Some(d) = file_digest(&f.root.join(&file)) {
                out.insert(format!("{file}={d}"));
            }
        }
    }
    out.into_iter().collect()
}

/// A new record checked against the code: every bind must resolve now.
pub fn record(f: &Facts, name: &str, kind: LinkKind, why: &str, binds: &[String], author: Option<&str>) -> Result<LinkageRecord> {
    if name.trim().is_empty() || name.contains('/') || name.starts_with('.') {
        return Err(ReadingRefusal::LinkageInvalid.refuse(format!("`{name}` is not a record name")));
    }
    if binds.len() < 2 {
        return Err(ReadingRefusal::LinkageInvalid.refuse(format!("{name}: a linkage binds two declarations or more")));
    }
    let rec = LinkageRecord { name: name.into(), kind, why: why.into(), binds: binds.to_vec(), author: author.map(str::to_string), recorded_utc: Some(crate::runs::now_utc()), evidence: evidence_of(f, binds) };
    let bs = bounds(f, &rec);
    let unresolved: Vec<&str> = bs.iter().filter(|b| b.resolved.is_none()).map(|b| b.bind.as_str()).collect();
    if !unresolved.is_empty() {
        return Err(ReadingRefusal::LinkageInvalid.refuse(format!("{name}: {} does not resolve in the scan", unresolved.join(", "))));
    }
    Ok(rec)
}

/// The comment anchors in the scanned sources: (file, line, record name).
pub fn anchors(f: &Facts) -> Vec<(String, u32, String)> {
    let mut out = Vec::new();
    for file in &f.k.files {
        for (i, l) in file.source.lines().enumerate() {
            if let Some(rest) = l.trim_start().strip_prefix(ANCHOR) {
                let name = rest.split_whitespace().next().unwrap_or("").to_string();
                if !name.is_empty() {
                    out.push((f.rel(&file.path), i as u32 + 1, name));
                }
            }
        }
    }
    out
}

/// The records of the project, each with its bounds, from the store at the facts' root.
pub fn checked(f: &Facts) -> Result<Vec<(PathBuf, LinkageRecord, Vec<Bound>)>> {
    let Some(store) = Store::find(&f.root) else { return Ok(Vec::new()) };
    Ok(load_all(&store)?.into_iter().map(|(p, r)| {
        let b = bounds(f, &r);
        (p, r, b)
    }).collect())
}

/// The records binding any of `ids` (an item, or an item inside a bound crate or file).
pub fn naming<'a>(all: &'a [(PathBuf, LinkageRecord, Vec<Bound>)], f: &Facts, ids: &BTreeSet<String>) -> Vec<&'a (PathBuf, LinkageRecord, Vec<Bound>)> {
    let files: BTreeSet<String> = ids.iter().filter_map(|id| f.item(id).map(|d| f.rel(&d.location.path))).collect();
    let packages: BTreeSet<String> = ids.iter().filter_map(|id| f.item(id).map(|d| d.package.clone())).chain(ids.iter().filter(|i| f.packages.contains_key(*i)).cloned()).collect();
    all.iter()
        .filter(|(_, _, bs)| {
            bs.iter().any(|b| match &b.resolved {
                Some(r) => ids.contains(r) || files.contains(r) || packages.contains(r) || ids.iter().any(|i| i.starts_with(&format!("{r}::"))),
                None => false,
            })
        })
        .collect()
}

/// A record's rows: the record, then each bind.
pub fn rows(f: &Facts, path: &Path, rec: &LinkageRecord, bs: &[Bound], why_listed: &str) -> Vec<CodeQueryResult> {
    let st = status(bs);
    let mut out = vec![named(
        "linkage",
        rec.name.clone(),
        None,
        why_listed,
        vec![("kind", rec.kind.as_str().into()), ("why", rec.why.clone()), ("status", st.as_str().into()), ("binds", rec.binds.join(", ")), ("record", f.rel(path)), ("recorded", rec.recorded_utc.clone().unwrap_or_default())],
    )];
    for b in bs {
        let state = if b.resolved.is_none() { "stale: no longer resolves" } else if b.moved { "review: its file changed since the record" } else { "resolves" };
        let row = match (&b.resolved, f.item(b.resolved.as_deref().unwrap_or(""))) {
            (Some(id), Some(_)) => item_row(f, "linkage-bind", id, state, vec![("linkage", rec.name.clone()), ("bind", b.bind.clone())]),
            _ => named("linkage-bind", b.bind.clone(), None, state, vec![("linkage", rec.name.clone()), ("resolved", b.resolved.clone().unwrap_or_default()), ("file", b.file.clone().unwrap_or_default())]),
        };
        out.push(row);
    }
    out
}

/// The `linkage` reading.
pub fn answer(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let all = checked(f)?;
    let target = spec.target.trim();
    let chosen: Vec<&(PathBuf, LinkageRecord, Vec<Bound>)> = if target == "*" {
        all.iter().collect()
    } else {
        let mut ids = BTreeSet::new();
        if let Ok(id) = item_id(f, target) {
            ids.insert(id);
        } else if let Some(p) = f.package_named(target) {
            ids.insert(p.name.clone());
        } else {
            ids.insert(target.to_string());
        }
        let by_name: Vec<&(PathBuf, LinkageRecord, Vec<Bound>)> = all.iter().filter(|(_, r, _)| r.name == target).collect();
        if by_name.is_empty() { naming(&all, f, &ids) } else { by_name }
    };
    let mut rows = Vec::new();
    for (p, r, b) in &chosen {
        rows.extend(rows_of(f, p, r, b));
    }
    let names: BTreeSet<&str> = all.iter().map(|(_, r, _)| r.name.as_str()).collect();
    let mut unbacked = 0;
    for (file, line, name) in anchors(f) {
        let backed = names.contains(name.as_str());
        if !backed {
            unbacked += 1;
        }
        if backed && target != "*" && !chosen.iter().any(|(_, r, _)| r.name == name) {
            continue;
        }
        let mut row = named(if backed { "linkage-anchor" } else { "linkage-anchor-unbacked" }, name.clone(), None, if backed { "a comment anchor of the record" } else { "a comment anchor naming no record: not trusted" }, vec![]);
        row.path = Some(file);
        row.line = Some(line);
        rows.push(row);
    }
    sort_rows(&mut rows);
    let mut ans = Answer::default();
    let st: Vec<Status> = chosen.iter().map(|(_, _, b)| status(b)).collect();
    ans.count("linkage records", chosen.len());
    ans.count("stale (a bound declaration no longer resolves)", st.iter().filter(|s| **s == Status::Stale).count());
    ans.count("to review (a bound file changed since recorded)", st.iter().filter(|s| **s == Status::Review).count());
    ans.count("comment anchors with no record", unbacked);
    ans.rows = rows;
    Ok(ans)
}

fn rows_of(f: &Facts, p: &Path, r: &LinkageRecord, b: &[Bound]) -> Vec<CodeQueryResult> {
    rows(f, p, r, b, "a linkage record")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_reads_back_from_1o_and_says_its_kind() {
        let rec = LinkageRecord { name: "config-settings".into(), kind: LinkKind::Synchronized, why: "the two carry the same keys".into(), binds: vec!["core_lib::Config".into(), "app::Settings".into()], author: None, recorded_utc: None, evidence: vec![] };
        let text = crate::oneo::render(crate::oneo::Doc::Linkage, &rec).unwrap();
        let back: LinkageRecord = crate::oneo::parse(crate::oneo::Doc::Linkage, "x", &text).unwrap();
        assert_eq!(back, rec);
        assert!("nonsense".parse::<LinkKind>().is_err());
        assert_eq!(status(&[Bound { bind: "a".into(), resolved: None, file: None, line: None, moved: false }]), Status::Stale);
    }
}
