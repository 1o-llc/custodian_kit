//! The rust face: per crate its `rust-version` and edition, the highest `rust-version` a package of its lock demands
//! (its closure in cargo's resolve), and the toolchain installed for the workspace; then the three disagreements:
//! a crate promising an older Rust than its dependencies allow, a lock newer than the installed toolchain, and an
//! edition its declared minimum cannot compile. Every version here is cargo's (metadata) or the manifest's as written.

use super::manifest::{CrateRef, Scan};
use super::{finding, Basis, Check, Finding, Place, ReadFrom, RustRow, Status, Toolchain};
use klepto::{CargoWorkspace, DepKind};
use std::path::Path;

/// The first Rust each edition compiles on: Rust's edition history, written once.
pub const EDITIONS: &[(&str, &str)] = &[("2015", "1.0"), ("2018", "1.31"), ("2021", "1.56"), ("2024", "1.85")];

/// The first Rust an edition compiles on.
pub fn edition_min(edition: &str) -> Option<&'static str> {
    EDITIONS.iter().find(|(e, _)| *e == edition).map(|(_, v)| *v)
}

/// `1.89`, `1.89.0`, `1.90.0-nightly` as (major, minor, patch).
pub fn version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().split(['-', '+', ' ']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

/// `a < b` as Rust versions; `None` when either does not read.
pub fn older(a: &str, b: &str) -> Option<bool> {
    Some(version(a)? < version(b)?)
}

pub struct RustFace {
    pub rows: Vec<RustRow>,
    pub findings: Vec<Finding>,
}

fn place(scan: &Scan, manifest: &Path, field: &str) -> Place {
    let line = scan.manifests.get(manifest).and_then(|m| m.package.as_ref()).and_then(|p| match field {
        "edition" => p.edition.line(),
        _ => p.rust_version.line(),
    });
    Place { file: scan.rel(manifest), line, section: "package".into(), field: field.into() }
}

/// The face over the crates in scope.
pub fn read(scan: &Scan, ws: Option<&CargoWorkspace>, crates: &[CrateRef], toolchain: &Toolchain) -> RustFace {
    let mut rows = Vec::new();
    let mut out = Vec::new();
    let resolved = ws.filter(|w| w.resolve.is_some());
    // the lock's highest demand over every crate in scope, and the packages above the toolchain
    let mut lock_packages: Vec<(String, String, String)> = Vec::new();
    let mut undeclared: Vec<(String, String, String)> = Vec::new();
    for c in crates {
        let pkg = ws.and_then(|w| w.packages.values().find(|p| p.manifest_path == c.manifest));
        let m = scan.manifests.get(&c.manifest);
        let edition = pkg.map(|p| p.edition.clone()).or_else(|| m.and_then(|m| scan.package_field(m, "edition"))).unwrap_or_else(|| "2015".into());
        let declared = match pkg {
            Some(p) => p.rust_version.clone(),
            None => m.and_then(|m| scan.package_field(m, "rust-version")),
        };
        let from = if pkg.is_some() { ReadFrom::Metadata } else { ReadFrom::Manifest };
        let min = edition_min(&edition).unwrap_or("?").to_string();
        let lock_max = match (resolved, pkg) {
            (Some(w), Some(p)) => {
                let closure = w.closure(&p.id, &[DepKind::Normal, DepKind::Build, DepKind::Dev]);
                let mut best: Option<(String, String)> = None;
                for id in &closure {
                    let Some(q) = w.packages.get(id) else { continue };
                    let Some(rv) = &q.rust_version else { continue };
                    lock_packages.push((q.name.clone(), q.version.clone(), rv.clone()));
                    if best.as_ref().is_none_or(|(b, _)| older(b, rv) == Some(true)) {
                        best = Some((rv.clone(), format!("{} {}", q.name, q.version)));
                    }
                }
                best
            }
            _ => None,
        };
        if let Some(rv) = &declared {
            lock_packages.push((c.name.clone(), pkg.map(|p| p.version.clone()).unwrap_or_default(), rv.clone()));
        }
        // an edition its declared minimum cannot compile
        if let (Some(rv), Some(e)) = (&declared, edition_min(&edition)) {
            if older(rv, e) == Some(true) {
                out.push(finding(Check::EditionNeedsNewerRust, Status::Unsatisfied, Basis::Observed, from, Some(&c.name), Some(place(scan, &c.manifest, "rust-version")), format!("rust-version {rv} with edition {edition}, which first compiles on Rust {e}"), format!("rust-version at least {e} for edition {edition}"), format!("rust-version = \"{rv}\", edition = \"{edition}\""), "cargo refuses the crate (\"rust-version is older than first version required by the specified edition\")", Some(format!("raise rust-version to {e} or later, or move the crate to an older edition (a choice)"))));
            }
        }
        match (&declared, &lock_max) {
            (Some(rv), Some((max, who))) if older(rv, max) == Some(true) => {
                out.push(finding(Check::RustVersionBelowLock, Status::Unsatisfied, Basis::Observed, ReadFrom::Metadata, Some(&c.name), Some(place(scan, &c.manifest, "rust-version")), format!("promises Rust {rv}; {who} in its lock declares {max}"), "a crate's rust-version is at least every rust-version its lock demands", format!("rust-version = \"{rv}\"; {who}: rust-version {max} (cargo metadata)"), format!("the promise is false: on Rust {rv} the lock as it is does not build"), Some(format!("raise rust-version to {max}, or hold {who} to an older release (a choice)"))));
            }
            (None, Some((max, who))) => undeclared.push((c.name.clone(), max.clone(), who.clone())),
            _ => {}
        }
        rows.push(RustRow { krate: c.name.clone(), manifest: scan.rel(&c.manifest), edition, declared, lock_max, edition_min: min });
    }
    // the crates that promise nothing, as one finding (the table has each)
    if !undeclared.is_empty() {
        let max = undeclared.iter().map(|(_, v, _)| v.clone()).max_by(|a, b| version(a).cmp(&version(b))).unwrap_or_default();
        let list: Vec<String> = undeclared.iter().map(|(c, v, who)| format!("{c} ({v}, {who})")).collect();
        let one = (undeclared.len() == 1).then(|| undeclared[0].0.clone());
        out.push(finding(Check::RustVersionUndeclared, Status::Informational, Basis::Observed, ReadFrom::Metadata, one.as_deref(), None, format!("{} crate(s) promise no Rust version; their locks demand up to {max}", undeclared.len()), "-", super::shorten(&list, 20), format!("a user on a Rust older than a crate's lock demands learns it from the compiler, not from the manifest (the highest here: {max})"), None));
    }
    // a lock newer than the toolchain installed for the workspace
    match &toolchain.release {
        Some(t) => {
            let mut above: Vec<(String, String, String)> = lock_packages.into_iter().filter(|(_, _, rv)| older(t, rv) == Some(true)).collect();
            above.sort();
            above.dedup();
            if !above.is_empty() {
                let max = above.iter().map(|(_, _, rv)| rv.clone()).max_by(|a, b| version(a).cmp(&version(b))).unwrap_or_default();
                let list: Vec<String> = above.iter().map(|(n, v, rv)| format!("{n} {v} ({rv})")).collect();
                out.push(finding(Check::ToolchainBelowLock, Status::Unsatisfied, Basis::Observed, ReadFrom::Toolchain, None, None, format!("the installed toolchain is Rust {t}; {} package(s) of the lock demand newer, up to {max}", above.len()), format!("a toolchain at least {max} in the workspace"), format!("rustc -vV: release {t}{}; {}", toolchain.active.as_ref().map(|a| format!(" (rustup: {a})")).unwrap_or_default(), super::shorten(&list, 12)), "cargo refuses to build those packages on this toolchain (\"package requires rustc\")", Some(format!("install Rust {max} or later for the workspace, or hold those packages to older releases (a choice)"))));
            }
        }
        None if !lock_packages.is_empty() || !rows.is_empty() => {
            out.push(finding(Check::ToolchainUnknown, Status::Informational, Basis::Unknown, ReadFrom::Toolchain, None, None, "the installed toolchain was not read", "-", "rustc -vV did not answer (or cargo was not run)", "whether this machine builds the lock is unknown", None));
        }
        None => {}
    }
    RustFace { rows, findings: out }
}
