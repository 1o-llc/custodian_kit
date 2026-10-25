//! A route (`crate:<name>`, `crate:<name>/<file>`, `decl:<name>`, a declared
//! `path:`) as one annotated `bundles` file, its contents chosen by a filters
//! word and the items at `secret` or above held through `classified`.
//!
//! The file set is klepto's, not walked: a crate's files are the ones its
//! scan read, plus its `Cargo.toml`; a declared set is the table's. The
//! filters word drops `tests/`, `benches/`, `examples/` (and `#[cfg(test)]`
//! modules and items), or everything outside `src/`; `--no-feature f` drops
//! the files and items the feature model puts behind `f` (their cfg implies
//! `feature = "f"` in every closed assignment). The items the maintenance
//! registry marks `secret` or above are held: their lines go to bundles as a
//! redaction (one marker line carrying the level and the digest of what was
//! left out, `bundles::redaction`) unless the word reveals them. The bytes
//! are held in a `ClassifiedString` (zeroized on drop) while they are
//! written. The bundle's digest is stable_digest's, through `crate::digest`.

use crate::api::{Refusal, Workspace};
use crate::reading::facts::Facts;
use crate::reading::refusal::ReadingRefusal;
use big_bad_bool::bitpack;
use bundles::redaction::{LineRedaction, Redact};
use bundles::{BundleWriter, FileEntry};
use classified::ClassifiedString;
use klepto::{CfgPredicate, FeatureModel, Implication};
use std::path::{Path, PathBuf};

/// The bundle filters as one packed word. Its bits are what a receipt keeps
/// and what the same-search key digests. The rules are the word's own
/// (`#[requires]`, `#[conflicts]`), checked by flaga_solver: `reveal`
/// requires `explicit` (set only by [`FiltersWord::reveal_explicitly`], what
/// `--reveal` on a command line calls), and `reveal` conflicts with
/// `redact_secret`.
#[bitpack(storage = u32, version = 1)]
pub struct FiltersWord {
    #[bits(7)]
    #[describe("what a bundle leaves out, and whether its secret items travel redacted or revealed")]
    #[flags(no_tests = 0, no_benches = 1, no_examples = 2, only_src = 3, redact_secret = 4, reveal = 5, explicit = 6)]
    #[labels(redact_secret = 16, reveal = 32, explicit = 64)]
    #[requires(reveal => filters.explicit)]
    #[conflicts(reveal, redact_secret)]
    filters: u8,
}

impl FiltersWord {
    /// What a bundle does unless told otherwise: secret items redacted.
    pub const DEFAULT: FiltersWord = FiltersWord::from_raw(FiltersWord::FLAG_FILTERS_REDACT_SECRET as u32);

    /// The word with secret items revealed, as an explicit `--reveal` asks:
    /// `reveal` and `explicit` set, `redact_secret` cleared.
    pub fn reveal_explicitly(self) -> Self {
        self.without_filters_redact_secret().with_filters_reveal().with_filters_explicit()
    }

    /// The word from flag names (`no_tests`, `only_src`, ...), as a spec or a
    /// receipt spells it; an unknown name is refused.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<Self, Refusal> {
        let mut raw = 0u32;
        for name in names {
            let flag = Self::FLAGS.iter().find(|f| f.name == name).ok_or_else(|| ReadingRefusal::FiltersRefused.refuse(format!("`{name}` is not a filter ({})", Self::all_names().join(", "))))?;
            raw |= flag.mask as u32;
        }
        Ok(Self::from_raw(raw))
    }

    /// The names of the flags set, in bit order.
    pub fn names(self) -> Vec<&'static str> {
        Self::FLAGS.iter().filter(|f| self.raw() as u128 & f.mask != 0).map(|f| f.name).collect()
    }

    fn all_names() -> Vec<&'static str> {
        Self::FLAGS.iter().map(|f| f.name).collect()
    }

    /// The word's rules, checked by flaga_solver; every broken rule is named.
    pub fn check(self) -> Result<Self, Refusal> {
        let verdict = flaga_solver::check(&flaga_solver::schema_of!(FiltersWord), self.raw() as u128);
        if verdict.is_ok() {
            Ok(self)
        } else {
            Err(ReadingRefusal::FiltersRefused.refusal(format!("{verdict} (word {:#04x}: {})", self.raw(), self.names().join(", "))))
        }
    }

    /// Whether secret items travel as they are.
    pub fn reveals(self) -> bool {
        self.has_filters_reveal() && self.has_filters_explicit()
    }
}

impl serde::Serialize for FiltersWord {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u32(self.raw())
    }
}

impl<'de> serde::Deserialize<'de> for FiltersWord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = <u32 as serde::Deserialize>::deserialize(d)?;
        FiltersWord::try_from_raw(raw).ok_or_else(|| serde::de::Error::custom(format!("{raw:#x} sets bits outside the filters word")))
    }
}

/// A bundle written.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Bundle {
    /// The `.bundle` file.
    pub file: PathBuf,
    /// `sha256:<hex>` of its bytes.
    pub digest: String,
    /// Files in it.
    pub files: usize,
    /// Items held redacted (windows written as markers).
    pub held: usize,
    /// Files of the route the filters left out, relative to the root.
    pub dropped: Vec<String>,
    /// The word it was made with.
    pub filters: FiltersWord,
}

/// The marker prefix of every custodian bundle (`job_bundle` reads `#` too).
pub const PREFIX: &str = "#";
/// The lowest level whose items are held.
pub const HELD_FROM: &str = "secret";
/// The largest file held in one classified container.
const HOLD_LIMIT: usize = 1 << 26;

/// `route` as one bundle file under the workspace's readings, filtered by `filters`.
pub fn bundle(ws: &Workspace, route: &str, filters: FiltersWord) -> Result<Bundle, Refusal> {
    bundle_without_features(ws, route, filters, &[])
}

/// [`bundle`], with the files and items behind each of `no_features` left out.
pub fn bundle_without_features(ws: &Workspace, route: &str, filters: FiltersWord, no_features: &[&str]) -> Result<Bundle, Refusal> {
    let filters = filters.check()?;
    let (bytes, report) = bundle_bytes(ws.facts(), route, filters, no_features)?;
    let digest = crate::digest::tagged(&bytes);
    let name = format!("{}-{:02x}.bundle", route.replace([':', '/', '\\'], "_"), filters.raw());
    let file = match crate::store::Store::find(&ws.root) {
        Some(store) => store.reading_dir("bundle").map_err(Refusal::from)?.join(name),
        // free mode: nothing stored in the project; one file per workspace, route and word
        None => std::env::temp_dir().join(format!("custodian-bundle-{}-{name}", crate::digest::short(&crate::digest::of(ws.root.to_string_lossy().as_bytes()), 12))),
    };
    crate::store::write_whole(&file, &bytes).map_err(Refusal::from)?;
    Ok(Bundle { file, digest, files: report.included.len(), held: report.redacted.len(), dropped: report.dropped, filters })
}

/// What [`bundle_bytes`] wrote.
#[derive(Debug, Default)]
pub(crate) struct Written {
    pub(crate) included: Vec<String>,
    pub(crate) redacted: Vec<bundles::Redacted>,
    pub(crate) dropped: Vec<String>,
}

/// Where a file sits in its package, as cargo lays a package out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Src,
    Tests,
    Benches,
    Examples,
    Other,
}

/// One file planned: its text, the source lines the filters drop, the
/// windows held.
struct Planned {
    rel: String,
    text: String,
    /// `(start, end, why)`, 1-based, inclusive, source lines.
    drops: Vec<(usize, usize, String)>,
    /// `(start, end, level)` in source lines; `(0, 0, level)` holds the whole file.
    held: Vec<(usize, usize, String)>,
}

pub(crate) fn bundle_bytes(f: &Facts, route: &str, filters: FiltersWord, no_features: &[&str]) -> Result<(Vec<u8>, Written), Refusal> {
    let route = f.paths.resolve(route, f.how).map_err(Refusal::from)?;
    let mut files: Vec<PathBuf> = if let Some(name) = route.crate_name() {
        let p = f.k.packages.get(name).ok_or_else(|| Refusal::from(ReadingRefusal::TargetUnknown.refuse(route.name())))?;
        if let Some(model) = f.k.features.get(name) {
            if let Some(unknown) = no_features.iter().find(|g| !model.features.contains_key(**g)) {
                return Err(ReadingRefusal::UnknownFeature.refusal(format!("`{unknown}` in {}", route.name())));
            }
        }
        let mut v: Vec<PathBuf> = f.k.files.iter().filter(|x| f.package_of(&x.path) == Some(name)).map(|x| x.path.clone()).collect();
        v.push(p.manifest_path.clone());
        v
    } else if let Some(rel) = route.crate_file(&f.root) {
        vec![f.root.join(rel)]
    } else {
        route.files(&f.root).map_err(Refusal::from)?
    };
    files.sort_by_key(|p| f.rel(p));
    files.dedup();

    let held_items = held_items(f)?;
    let mut planned = Vec::new();
    let mut written = Written::default();
    for file in &files {
        let rel = f.rel(file);
        let parsed = f.k.files.iter().find(|x| &x.path == file);
        let pkg = f.package_of_rel(&rel);
        let model = pkg.and_then(|p| f.k.features.get(&p.name));
        let place = place_of(f, &rel, file);
        let module_cfg = parsed.and_then(|_| f.k.modules.for_file(file)).map(|m| m.cfg.clone()).unwrap_or_default();
        let keep = match place {
            _ if filters.has_filters_only_src() && place != Place::Src => false,
            Place::Tests => !filters.has_filters_no_tests(),
            Place::Benches => !filters.has_filters_no_benches(),
            Place::Examples => !filters.has_filters_no_examples(),
            Place::Src | Place::Other => true,
        } && !(filters.has_filters_no_tests() && requires_test(&module_cfg))
            && !no_features.iter().any(|g| behind_feature(model, &module_cfg, g));
        if !keep {
            written.dropped.push(rel);
            continue;
        }
        let text = match parsed {
            Some(p) => p.source.clone(),
            None => match std::fs::read(file) {
                Ok(b) => String::from_utf8(b).unwrap_or_default(),
                Err(e) => return Err(ReadingRefusal::MissingInput.refuse(format!("{rel}: {e}")).into()),
            },
        };
        let mut drops = Vec::new();
        let mut extents = Vec::new();
        if let Some(p) = parsed {
            walk(&p.ast.items, &mut |start, end, cfg| {
                extents.push((start, end));
                if filters.has_filters_no_tests() && requires_test(cfg) {
                    drops.push((start, end, "#[cfg(test)]".to_string()));
                    return false;
                }
                if let Some(g) = no_features.iter().find(|g| behind_feature(model, cfg, g)) {
                    drops.push((start, end, format!("feature `{g}`")));
                    return false;
                }
                true
            });
        }
        let lines = text.split_inclusive('\n').count();
        let mut held = Vec::new();
        for h in held_items.iter().filter(|h| h.covers(&rel)) {
            match h.line {
                None => held.push((0, 0, h.level.clone())),
                Some(line) => match extent_at(&extents, line) {
                    Some((s, e)) => held.push((s, e.min(lines), h.level.clone())),
                    // an item with no extent the scan can name: the whole file is held (more, never less)
                    None => held.push((0, 0, h.level.clone())),
                },
            }
        }
        planned.push(Planned { rel, text, drops, held });
    }
    if planned.is_empty() {
        return Err(ReadingRefusal::NothingToBundle.refusal(format!("{} ({} dropped by {})", route.name(), written.dropped.len(), filters.names().join(", "))));
    }

    let mut w = BundleWriter::new(Vec::new(), PREFIX).map_err(|e| ReadingRefusal::Entry.refuse(e))?;
    let rank = |level: &str| held_items.levels.iter().position(|l| l == level).unwrap_or(0);
    for (i, p) in planned.into_iter().enumerate() {
        let (text, redact, dropped_note) = p.filtered(filters.reveals(), &rank);
        let mut note = Vec::new();
        if i == 0 {
            note.push(format!("custodian bundle {}, filters {} (word {:#04x}){}", route.name(), filters.names().join(", "), filters.raw(), if no_features.is_empty() { String::new() } else { format!(", no feature {}", no_features.join(", ")) }));
        }
        if let Some(d) = dropped_note {
            note.push(d);
        }
        if p.held.is_empty() {
            w.entry(entry(&p.rel, text.as_bytes(), &redact, &note)).map_err(|e| ReadingRefusal::Entry.refuse(e))?;
        } else {
            // a file with held lines is kept in a classified container while it is written
            let kept = ClassifiedString::try_new(text, HOLD_LIMIT).map_err(|e| ReadingRefusal::Hold.refuse(format!("{}: {e:?}", p.rel)))?;
            kept.expose(|s| w.entry(entry(&p.rel, s.as_bytes(), &redact, &note))).map_err(|e| ReadingRefusal::Entry.refuse(e))?;
        }
    }
    let (bytes, report) = w.finish().map_err(|e| ReadingRefusal::Entry.refuse(e))?;
    written.included = report.included;
    written.redacted = report.redacted;
    Ok((bytes, written))
}

/// One bundles entry: borrowed bytes, the redaction, the note lines.
fn entry<'a>(rel: &str, bytes: &'a [u8], redact: &Redact, note: &[String]) -> FileEntry<'a> {
    let e = FileEntry::bytes(rel, bytes).with_redact(redact.clone());
    if note.is_empty() {
        e
    } else {
        e.with_note(note.join("\n"))
    }
}

impl Planned {
    /// The text without its dropped lines, the held windows in the dropped
    /// text's lines (merged, the higher level kept), and the note naming the
    /// source lines dropped.
    fn filtered(&self, reveal: bool, rank: &dyn Fn(&str) -> usize) -> (String, Redact, Option<String>) {
        let lines: Vec<&str> = self.text.split_inclusive('\n').collect();
        let dropped = |n: usize| self.drops.iter().any(|&(s, e, _)| n >= s && n <= e);
        let mut new_of = vec![0usize; lines.len() + 1];
        let mut text = String::new();
        let mut n_new = 0;
        for (i, l) in lines.iter().enumerate() {
            if !dropped(i + 1) {
                n_new += 1;
                new_of[i + 1] = n_new;
                text.push_str(l);
            }
        }
        let note = (!self.drops.is_empty()).then(|| {
            let mut d: Vec<String> = self.drops.iter().map(|(s, e, why)| if s == e { format!("{s} {why}") } else { format!("{s}-{e} {why}") }).collect();
            d.sort();
            format!("source lines dropped: {}; marker lines count the bundled text", d.join(", "))
        });
        if reveal || self.held.is_empty() {
            return (text, Redact::Keep, note);
        }
        if let Some((_, _, level)) = self.held.iter().filter(|h| h.0 == 0).max_by_key(|h| rank(&h.2)) {
            return (text, Redact::Whole { level: level.clone() }, note);
        }
        let mut windows: Vec<(usize, usize, String)> = self
            .held
            .iter()
            .filter_map(|(s, e, level)| {
                let kept: Vec<usize> = (*s..=*e).filter(|n| *n <= lines.len() && !dropped(*n)).map(|n| new_of[n]).collect();
                Some((*kept.first()?, *kept.last()?, level.clone()))
            })
            .collect();
        windows.sort();
        let mut merged: Vec<(usize, usize, String)> = Vec::new();
        for (s, e, level) in windows {
            match merged.last_mut() {
                Some(last) if s <= last.1 => {
                    last.1 = last.1.max(e);
                    if rank(&level) > rank(&last.2) {
                        last.2 = level;
                    }
                }
                _ => merged.push((s, e, level)),
            }
        }
        let redact = Redact::Lines(merged.into_iter().map(|(start, end, level)| LineRedaction { start, end, level }).collect());
        (text, redact, note)
    }
}

/// One item the registry holds: its file, the line it starts on (`None`:
/// the whole file, a secret module's own file), its level.
struct Held {
    file: String,
    line: Option<usize>,
    level: String,
}

impl Held {
    fn covers(&self, rel: &str) -> bool {
        self.file == rel
    }
}

struct HeldItems {
    items: Vec<Held>,
    /// The level table, lowest first.
    levels: Vec<String>,
}

impl std::ops::Deref for HeldItems {
    type Target = [Held];
    fn deref(&self) -> &[Held] {
        &self.items
    }
}

/// The items at `secret` or above: from `.maintenance/registry.json` when the
/// project has one (`maintenance::registry::read`), else from the marks
/// klepto recorded (the workspace's own reading of the attributes). A
/// secret module declared with `mod x;` holds its own file and every file
/// module under it.
fn held_items(f: &Facts) -> Result<HeldItems, Refusal> {
    // (id, kind, file, line, level) of every item at secret or above
    type Marked = (String, String, String, usize, String);
    let (marked, levels): (Vec<Marked>, Vec<String>) = match maintenance::registry::read(&f.root) {
        Ok(reg) => {
            if reg.rank(HELD_FROM).is_none() {
                return Err(ReadingRefusal::NoSecretLevel.refusal(format!("the registry's levels are {}", reg.levels.join(", "))));
            }
            let v = reg.at_or_above(HELD_FROM).map(|e| (e.id.clone(), e.kind.clone(), e.file.clone(), e.line as usize, e.sensitivity.clone().unwrap_or_default())).collect();
            (v, reg.levels.clone())
        }
        Err(e) if maintenance::registry::path(&f.root).exists() => return Err(ReadingRefusal::Registry.refusal(e)),
        Err(_) => {
            let floor = f.level_rank(HELD_FROM).ok_or_else(|| ReadingRefusal::NoSecretLevel.refuse(format!("the levels are {}", f.levels.join(", "))))?;
            let v = f
                .marks
                .iter()
                .filter_map(|(id, m)| {
                    let level = m.sensitivity.clone()?;
                    (f.level_rank(&level)? >= floor).then_some(())?;
                    let d = f.item(id)?;
                    let kind = if matches!(d.kind, klepto::graph::NodeKind::Module) { "mod" } else { "item" };
                    Some((id.clone(), kind.to_string(), f.rel(&d.location.path), d.location.line? as usize, level))
                })
                .collect();
            (v, f.levels.clone())
        }
    };
    let mut items = Vec::new();
    for (id, kind, file, line, level) in marked {
        if kind == "mod" {
            let own: Vec<String> = f.k.modules.modules.values().filter(|m| !m.inline && (m.path == id || m.path.starts_with(&format!("{id}::")))).map(|m| f.rel(&m.file)).collect();
            if !own.is_empty() {
                items.extend(own.into_iter().map(|file| Held { file, line: None, level: level.clone() }));
                continue;
            }
        }
        items.push(Held { file, line: Some(line), level });
    }
    Ok(HeldItems { items, levels })
}

/// The files (relative to the root) holding an item at `secret` or above, whole or in a window: what a bundle
/// holds redacted, and what an archive (which copies files whole) refuses unless revealed. One rule, [`held_items`].
pub(crate) fn held_files(f: &Facts) -> Result<std::collections::BTreeSet<String>, Refusal> {
    Ok(held_items(f)?.iter().map(|h| h.file.clone()).collect())
}

/// Where `rel` sits in its package: by cargo's layout (`src/`, `tests/`,
/// `benches/`, `examples/`) or as a declared target's own file.
pub(crate) fn place_of(f: &Facts, rel: &str, file: &Path) -> Place {
    let Some(pkg) = f.package_of_rel(rel) else { return Place::Other };
    if let Some(info) = f.k.packages.get(&pkg.name) {
        if let Some(t) = info.targets.iter().find(|t| t.src_path == file) {
            match t.kind.as_str() {
                "test" => return Place::Tests,
                "bench" => return Place::Benches,
                "example" => return Place::Examples,
                _ => {}
            }
        }
    }
    let inner = if pkg.dir.is_empty() { rel } else { rel.strip_prefix(&format!("{}/", pkg.dir)).unwrap_or(rel) };
    match inner.split('/').next() {
        Some("src") if inner.contains('/') => Place::Src,
        Some("tests") if inner.contains('/') => Place::Tests,
        Some("benches") if inner.contains('/') => Place::Benches,
        Some("examples") if inner.contains('/') => Place::Examples,
        _ => Place::Other,
    }
}

/// `cfg` holds only under `test`.
pub(crate) fn requires_test(cfg: &CfgPredicate) -> bool {
    matches!(cfg.implies(&CfgPredicate::Key("test".into()), None), Implication::Always)
}

/// The feature model puts `cfg` behind `feature`: it implies `feature = "<feature>"`
/// in every closed assignment. An unknown cfg is never behind anything.
fn behind_feature(model: Option<&FeatureModel>, cfg: &CfgPredicate, feature: &str) -> bool {
    model.is_some_and(|m| matches!(m.closed_implies(cfg, &CfgPredicate::Feature(feature.into())), Implication::Always))
}

/// The innermost extent starting at `line`, else the innermost containing it.
fn extent_at(extents: &[(usize, usize)], line: usize) -> Option<(usize, usize)> {
    let by_len = |&(s, e): &(usize, usize)| e - s;
    extents.iter().filter(|(s, _)| *s == line).min_by_key(|x| by_len(x)).or_else(|| extents.iter().filter(|(s, e)| *s <= line && line <= *e).min_by_key(|x| by_len(x))).copied()
}

/// Every item of klepto's parse of a file, with its lines (its first
/// attribute or doc line to its last token) and its own cfg; `visit`
/// returns whether to go inside it (inline modules, impl and trait items).
pub(crate) fn walk(items: &[syn::Item], visit: &mut dyn FnMut(usize, usize, &CfgPredicate) -> bool) {
    use syn::spanned::Spanned;
    fn lines(attrs: &[syn::Attribute], item: &dyn Spanned) -> (usize, usize) {
        let span = item.span();
        let start = attrs.iter().map(|a| a.pound_token.span.start().line).chain([span.start().line]).min().unwrap_or(0);
        (start, span.end().line)
    }
    for item in items {
        let attrs: &[syn::Attribute] = match item {
            syn::Item::Const(i) => &i.attrs,
            syn::Item::Enum(i) => &i.attrs,
            syn::Item::ExternCrate(i) => &i.attrs,
            syn::Item::Fn(i) => &i.attrs,
            syn::Item::ForeignMod(i) => &i.attrs,
            syn::Item::Impl(i) => &i.attrs,
            syn::Item::Macro(i) => &i.attrs,
            syn::Item::Mod(i) => &i.attrs,
            syn::Item::Static(i) => &i.attrs,
            syn::Item::Struct(i) => &i.attrs,
            syn::Item::Trait(i) => &i.attrs,
            syn::Item::TraitAlias(i) => &i.attrs,
            syn::Item::Type(i) => &i.attrs,
            syn::Item::Union(i) => &i.attrs,
            syn::Item::Use(i) => &i.attrs,
            _ => &[],
        };
        let (start, end) = lines(attrs, item);
        let cfg = klepto::resolve::cfg::cfg_of_attrs(attrs);
        if !visit(start, end, &cfg) {
            continue;
        }
        match item {
            syn::Item::Mod(syn::ItemMod { content: Some((_, inner)), .. }) => walk(inner, visit),
            syn::Item::Impl(i) => {
                for it in &i.items {
                    let attrs: &[syn::Attribute] = match it {
                        syn::ImplItem::Const(x) => &x.attrs,
                        syn::ImplItem::Fn(x) => &x.attrs,
                        syn::ImplItem::Type(x) => &x.attrs,
                        syn::ImplItem::Macro(x) => &x.attrs,
                        _ => &[],
                    };
                    let (s, e) = lines(attrs, it);
                    visit(s, e, &klepto::resolve::cfg::cfg_of_attrs(attrs));
                }
            }
            syn::Item::Trait(t) => {
                for it in &t.items {
                    let attrs: &[syn::Attribute] = match it {
                        syn::TraitItem::Const(x) => &x.attrs,
                        syn::TraitItem::Fn(x) => &x.attrs,
                        syn::TraitItem::Type(x) => &x.attrs,
                        syn::TraitItem::Macro(x) => &x.attrs,
                        _ => &[],
                    };
                    let (s, e) = lines(attrs, it);
                    visit(s, e, &klepto::resolve::cfg::cfg_of_attrs(attrs));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Resolving, Scope};
    use bundles::redaction::{markers_in, RedactionMarker, Span};

    const LIB: &str = "//! A crate with one secret.
pub fn open() -> u32 {
    1
}

/// The key.
#[sensitivity(secret)]
pub fn key() -> &'static str {
    \"hunter2-fixture\"
}

#[cfg(feature = \"extra\")]
pub mod extra;

mod inner;

#[cfg(feature = \"extra\")]
pub fn extra_fn() -> u32 {
    2
}

#[cfg(test)]
mod tests {
    #[test]
    fn opens() {
        assert_eq!(super::open(), 1);
    }
}
";

    /// Lines 6-10 of LIB: the secret item, its doc line to its brace.
    const KEY_WINDOW: &str = "/// The key.\n#[sensitivity(secret)]\npub fn key() -> &'static str {\n    \"hunter2-fixture\"\n}\n";

    const REGISTRY: &str = r#"{
  "format": 1,
  "levels": ["public", "internal", "confidential", "secret", "key_material"],
  "items": [
    { "id": "vault_demo::key", "package": "vault_demo", "kind": "fn", "file": "crates/vault_demo/src/lib.rs", "line": 6,
      "sensitivity": "secret", "sensitivity_from": "vault_demo::key", "owner": null, "owner_from": null, "review": null,
      "deprecated_after": null, "mechanism": null, "boundary": null, "supersedes": null, "reads_env": [],
      "golden_writer": null, "wire_writer": false }
  ]
}
"#;

    /// A one-crate workspace in a fresh directory: a secret fn, a feature-gated
    /// module and fn, an inline `#[cfg(test)]` module, an integration test.
    fn fixture(name: &str, registry: bool) -> PathBuf {
        let root = std::env::temp_dir().join(format!("custodian_bundle_{name}_{}_{}", std::process::id(), crate::runs::stamp().replace([':', '.'], "")));
        let put = |rel: &str, text: &str| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        put("Cargo.toml", "[workspace]\nresolver = \"2\"\nmembers = [\"crates/vault_demo\"]\n");
        put("crates/vault_demo/Cargo.toml", "[package]\nname = \"vault_demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[features]\nextra = []\n");
        put("crates/vault_demo/src/lib.rs", LIB);
        put("crates/vault_demo/src/extra.rs", "pub fn more() -> u32 {\n    3\n}\n");
        put("crates/vault_demo/src/inner.rs", "pub(crate) fn helper() {}\n");
        put("crates/vault_demo/tests/it.rs", "#[test]\nfn it() {\n    assert_eq!(vault_demo::open(), 1);\n}\n");
        if registry {
            put(".maintenance/registry.json", REGISTRY);
        }
        root
    }

    fn open(root: &Path) -> Workspace {
        Workspace::open_with(root, &Scope { tests: true, ..Scope::default() }, Resolving::Free).unwrap()
    }

    fn files_of(text: &str) -> Vec<String> {
        text.lines().filter_map(|l| l.strip_prefix("# start of ")).map(str::to_string).collect()
    }

    #[test]
    fn the_word_states_its_rules_and_flaga_solver_checks_them() {
        assert_eq!(FiltersWord::DEFAULT.names(), ["redact_secret"]);
        assert!(FiltersWord::DEFAULT.check().is_ok());
        let w = FiltersWord::from_names(["no_tests", "only_src", "redact_secret"]).unwrap();
        assert!(w.has_filters_no_tests() && w.has_filters_only_src() && !w.reveals());
        let e = FiltersWord::from_names(["no_docs"]).unwrap_err();
        assert_eq!(e.code, "CUST0101");
        // reveal without the explicit flag: the word's #[requires]
        let e = FiltersWord::from_names(["reveal"]).unwrap().check().unwrap_err();
        assert!(e.message.contains("`filters`=reveal requires `filters`=explicit"), "{e}");
        // reveal next to redact_secret: the word's #[conflicts]
        let e = FiltersWord::DEFAULT.with_filters_reveal().with_filters_explicit().check().unwrap_err();
        assert!(e.message.contains("`filters`=reveal conflicts with `filters`=redact_secret"), "{e}");
        let r = FiltersWord::DEFAULT.reveal_explicitly();
        assert!(r.check().is_ok() && r.reveals());
        assert_eq!(serde_json::to_string(&r).unwrap(), r.raw().to_string(), "a receipt keeps the word");
        assert_eq!(serde_json::from_str::<FiltersWord>(&r.raw().to_string()).unwrap(), r);
        assert!(serde_json::from_str::<FiltersWord>("4096").is_err());
    }

    #[test]
    fn a_secret_window_is_redacted_in_the_bundle_bytes() {
        let root = fixture("redact", true);
        let ws = open(&root);
        let b = bundle(&ws, "crate:vault_demo", FiltersWord::DEFAULT).unwrap();
        let bytes = std::fs::read(&b.file).unwrap();
        assert_eq!(b.digest, crate::digest::tagged(&bytes), "the digest is stable_digest's over the bytes");
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("hunter2-fixture"), "{text}");
        let marker = RedactionMarker::over("secret", Span::Lines { start: 6, end: 10 }, KEY_WINDOW.as_bytes());
        assert_eq!(marker.digest, crate::digest::tagged(KEY_WINDOW), "the marker's digest is the same digest");
        assert_eq!(markers_in(&text, PREFIX), [marker]);
        assert_eq!(b.held, 1);
        assert_eq!(
            files_of(&text),
            ["crates/vault_demo/Cargo.toml", "crates/vault_demo/src/extra.rs", "crates/vault_demo/src/inner.rs", "crates/vault_demo/src/lib.rs", "crates/vault_demo/tests/it.rs"]
        );
        assert!(text.contains("# note of crates/vault_demo/Cargo.toml: custodian bundle crate:vault_demo, filters redact_secret (word 0x10)"), "{text}");
    }

    #[test]
    fn the_filters_drop_the_right_files_and_items() {
        let root = fixture("filters", true);
        let ws = open(&root);
        let read = |b: &Bundle| std::fs::read_to_string(&b.file).unwrap();

        let no_tests = bundle(&ws, "crate:vault_demo", FiltersWord::DEFAULT.with_filters_no_tests()).unwrap();
        assert_eq!(no_tests.dropped, ["crates/vault_demo/tests/it.rs"]);
        let t = read(&no_tests);
        assert!(!t.contains("mod tests") && !t.contains("fn opens") && t.contains("pub fn extra_fn"), "{t}");
        assert!(t.contains("# note of crates/vault_demo/src/lib.rs: source lines dropped: 22-28 #[cfg(test)]; marker lines count the bundled text"), "{t}");

        let no_extra = bundle_without_features(&ws, "crate:vault_demo", FiltersWord::DEFAULT, &["extra"]).unwrap();
        assert_eq!(no_extra.dropped, ["crates/vault_demo/src/extra.rs"]);
        let t = read(&no_extra);
        assert!(!t.contains("extra_fn") && !t.contains("pub mod extra;") && t.contains("mod tests"), "{t}");
        assert!(t.contains("source lines dropped: 12-13 feature `extra`, 17-20 feature `extra`"), "{t}");

        let only_src = bundle(&ws, "crate:vault_demo", FiltersWord::DEFAULT.with_filters_only_src()).unwrap();
        assert_eq!(only_src.dropped, ["crates/vault_demo/Cargo.toml", "crates/vault_demo/tests/it.rs"]);
        assert_eq!(only_src.files, 3);

        let e = bundle_without_features(&ws, "crate:vault_demo", FiltersWord::DEFAULT, &["nope"]).unwrap_err();
        assert_eq!(e.code, "CUST0106");
        let e = bundle(&ws, "crate:not_here", FiltersWord::DEFAULT).unwrap_err();
        assert_eq!(e.code, "CUST0022", "an undeclared route is the table's refusal");
        let e = bundle(&ws, "crate:vault_demo", FiltersWord::from_names(["reveal"]).unwrap()).unwrap_err();
        assert_eq!(e.code, "CUST0101", "the word is checked before anything is read");
    }

    #[test]
    fn a_dropped_window_moves_the_marker_to_the_bundled_lines() {
        let root = fixture("moved", true);
        std::fs::write(root.join("crates/vault_demo/src/lib.rs"), format!("#[cfg(test)]\nfn only_in_tests() {{}}\n\n{KEY_WINDOW}")).unwrap();
        std::fs::write(root.join(".maintenance/registry.json"), REGISTRY.replace("\"line\": 6", "\"line\": 4")).unwrap();
        let ws = open(&root);
        // a crate:<file> route bundles the one file; the two dropped lines move the window from 4-8 to 2-6
        let b = bundle(&ws, "crate:vault_demo/src/lib.rs", FiltersWord::DEFAULT.with_filters_no_tests()).unwrap();
        let t = std::fs::read_to_string(&b.file).unwrap();
        assert_eq!(files_of(&t), ["crates/vault_demo/src/lib.rs"]);
        assert_eq!(markers_in(&t, PREFIX), [RedactionMarker::over("secret", Span::Lines { start: 2, end: 6 }, KEY_WINDOW.as_bytes())]);
        assert!(t.contains("source lines dropped: 1-2 #[cfg(test)]"), "{t}");
        assert!(!t.contains("hunter2-fixture") && !t.contains("only_in_tests"), "{t}");
    }

    #[test]
    fn the_round_trip_restores_markers_and_reveal_restores_the_lines() {
        let root = fixture("round_trip", true);
        let ws = open(&root);
        let b = bundle(&ws, "crate:vault_demo", FiltersWord::DEFAULT).unwrap();
        let out = root.join("restored");
        bundles::unbundle_files(&b.file, &out, PREFIX).unwrap();
        let lib = std::fs::read_to_string(out.join("crates/vault_demo/src/lib.rs")).unwrap();
        let marker = RedactionMarker::over("secret", Span::Lines { start: 6, end: 10 }, KEY_WINDOW.as_bytes()).render(PREFIX);
        assert_eq!(lib, LIB.replace(KEY_WINDOW, &format!("{marker}\n")), "the restored file holds the marker, never the bytes");
        assert_eq!(std::fs::read_to_string(out.join("crates/vault_demo/src/inner.rs")).unwrap(), "pub(crate) fn helper() {}\n");

        let shown = bundle(&ws, "crate:vault_demo", FiltersWord::DEFAULT.reveal_explicitly()).unwrap();
        assert_eq!(shown.held, 0);
        let again = root.join("revealed");
        bundles::unbundle_files(&shown.file, &again, PREFIX).unwrap();
        assert_eq!(std::fs::read_to_string(again.join("crates/vault_demo/src/lib.rs")).unwrap(), LIB);
        assert_ne!(shown.digest, b.digest);
    }

    #[test]
    fn without_a_registry_the_attributes_klepto_read_hold_the_item() {
        let root = fixture("attributes", false);
        let ws = open(&root);
        let b = bundle(&ws, "crate:vault_demo", FiltersWord::DEFAULT).unwrap();
        let t = std::fs::read_to_string(&b.file).unwrap();
        assert!(!t.contains("hunter2-fixture"), "{t}");
        assert_eq!(markers_in(&t, PREFIX).len(), 1);
    }
}
