//! The facts one reading is answered from, all klepto's: the scan of the
//! workspace (occurrences and their resolutions, string literals with the call
//! they are an argument of, matches, items, attributes, the item graph) and
//! its packages with their targets and dependencies. Nothing here reads
//! source itself; the one line of context a site shows is klepto's copy of
//! the file.

use super::refusal::ReadingRefusal;
use super::spec::Scope;
use crate::paths::{PathsTable, Resolving};
use crate::{klepto_scan, Result, ScanOptions};
use klepto::{DepKind, EdgeKind, Klepto, KleptoError, Resolution};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// How a workspace package depends on another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Link {
    Normal,
    Build,
    Dev,
}

impl Link {
    pub fn as_str(self) -> &'static str {
        match self {
            Link::Normal => "normal",
            Link::Build => "build",
            Link::Dev => "dev",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Package {
    /// `-` → `_`, as klepto names packages.
    pub name: String,
    /// The package directory relative to the root.
    pub dir: String,
    pub bins: Vec<String>,
    /// Workspace packages it depends on.
    pub deps: Vec<(String, Link)>,
}

/// An item's maintenance marks (the `maintenance` attributes), from the
/// registry file `.maintenance/registry.json` when the project has one, else
/// from the attributes as klepto recorded them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    pub sensitivity: Option<String>,
    pub owner: Option<String>,
    pub mechanism: Option<String>,
    pub boundary: Option<Vec<String>>,
    pub supersedes: Option<String>,
    pub reads_env: Vec<String>,
    pub review: Option<String>,
}

/// The first string literal in attribute tokens (`"platform"`, `"KEY", "OTHER"`).
fn strings_in(args: &str) -> Vec<String> {
    use syn::parse::Parser;
    syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
        .parse_str(args)
        .map(|p| {
            p.into_iter()
                .filter_map(|e| match e {
                    syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) => Some(s.value()),
                    syn::Expr::Path(p) => Some(quote::ToTokens::to_token_stream(&p).to_string().replace(' ', "")),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub struct Facts {
    pub root: PathBuf,
    pub k: Klepto,
    pub packages: BTreeMap<String, Package>,
    /// Package → the binaries that link it (its own and its dependents').
    pub links: BTreeMap<String, BTreeSet<String>>,
    /// Item id → its maintenance marks.
    pub marks: BTreeMap<String, Marks>,
    /// The level table, lowest first (the registry's, else the shipped word's).
    pub levels: Vec<String>,
    /// Where the marks came from: the registry file, or klepto's attributes.
    pub marks_from: &'static str,
    /// The declared routes a reading may read.
    pub paths: PathsTable,
    /// Free mode, or inside a job (declared routes only).
    pub how: Resolving,
    file_index: HashMap<PathBuf, usize>,
    file_package: HashMap<PathBuf, String>,
    items_by_id: HashMap<String, usize>,
    /// Item id → ids that reference or re-export it.
    referrers: HashMap<String, Vec<String>>,
    /// Item id → ids it references.
    references: HashMap<String, Vec<String>>,
    /// Where the parsed index came from, and how long it took.
    pub index: IndexSource,
    /// Whether klepto's syntax trees are held (an index read back from the cache holds none).
    pub trees: bool,
}

/// Where a scan's parsed index came from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IndexSource {
    /// klepto parsed the workspace (and the model was kept in the cache when the project has a folder).
    Parsed { millis: u64 },
    /// Read back from `.custodian/state/index/<digest>/model.cbor`: the same code was parsed before.
    FromCache { millis: u64 },
    /// Held open by `custodian serve`, from a run before this one.
    FromDaemon,
    /// klepto parsed the workspace, and the model was not kept: compressed it is `bytes`, over [`INDEX_CACHE_MAX_BYTES`].
    ParsedNotKept { millis: u64, bytes: u64 },
}

impl IndexSource {
    /// `parsed (31204 ms)`, `from cache (812 ms)`, `from daemon`.
    pub fn line(&self) -> String {
        match self {
            IndexSource::Parsed { millis } => format!("parsed ({millis} ms)"),
            IndexSource::FromCache { millis } => format!("from cache ({millis} ms)"),
            IndexSource::FromDaemon => "from daemon".to_string(),
            IndexSource::ParsedNotKept { millis, bytes } => format!("parsed ({millis} ms; not kept: {} MB, over the {} MB the cache holds)", bytes / 1_000_000, INDEX_CACHE_MAX_BYTES / 1_000_000),
        }
    }
}

/// The cache's key for a scan: the klepto that writes the model, the scan's options and the code it reads
/// (klepto's fingerprint, [`crate::code_digest`]).
pub fn index_cache_key(root: &Path, opts: &crate::ScanOptions) -> std::result::Result<String, KleptoError> {
    let code = crate::code_digest(root, opts)?;
    Ok(crate::digest::short(&crate::digest::of_value(&(Klepto::MODEL_VERSION, format!("{opts:?}"), code)), 24))
}

/// How many parsed indexes the cache keeps: the newest, and the one before it (a job's base and its tree).
pub const INDEX_CACHE_KEEP: usize = 2;

/// The most a kept model may take on disk, compressed: a larger one is not kept, the scan says so, and the next
/// scan parses again.
pub const INDEX_CACHE_MAX_BYTES: u64 = 256 * 1_000_000;

/// How hard the model is compressed (zstd's level): its CBOR repeats paths and names, and level 3 is the fast one.
pub const INDEX_CACHE_ZSTD_LEVEL: i32 = 3;

/// The file 0.3.3 kept, uncompressed: let go when a scan keeps the compressed one.
const UNCOMPRESSED_0_3_3: &str = "model.cbor";

/// The model written to its cache file whole, zstd-compressed CBOR (a temporary beside it, then renamed), and the
/// cache cut to the newest [`INDEX_CACHE_KEEP`]. A model that cannot be written, or is over
/// [`INDEX_CACHE_MAX_BYTES`] compressed, is not kept: its size, when it was over. The scan's answer stands either way.
fn keep_index(file: &Path, k: &Klepto) -> Option<u64> {
    let dir = file.parent()?;
    std::fs::create_dir_all(dir).ok()?;
    let tmp = file.with_extension("tmp");
    let written = std::fs::File::create(&tmp).ok().and_then(|f| zstd::stream::Encoder::new(std::io::BufWriter::new(f), INDEX_CACHE_ZSTD_LEVEL).ok()).is_some_and(|mut z| {
        ciborium::into_writer(k, &mut z).is_ok() && z.finish().is_ok_and(|mut w| std::io::Write::flush(&mut w).is_ok())
    });
    let bytes = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if !written || bytes > INDEX_CACHE_MAX_BYTES || std::fs::rename(&tmp, file).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return (written && bytes > INDEX_CACHE_MAX_BYTES).then_some(bytes);
    }
    // the cache's own folders, newest first; the older ones are parsed again if their code comes back
    let index = dir.parent()?;
    let mut held: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(index).map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path())).collect()).unwrap_or_default();
    held.sort_by_key(|h| std::cmp::Reverse(h.0));
    for (_, d) in &held {
        let _ = std::fs::remove_file(d.join(UNCOMPRESSED_0_3_3));
    }
    for (_, old) in held.into_iter().skip(INDEX_CACHE_KEEP) {
        let _ = std::fs::remove_dir_all(old);
    }
    None
}

impl Scope {
    pub fn scan_options(&self) -> ScanOptions {
        ScanOptions { exclude: self.exclude.clone(), tests: self.tests, dependency_macros: self.dependency_macros }
    }
}

impl Facts {
    pub fn build(root: &Path, scope: &Scope, how: Resolving) -> Result<Facts> {
        let root = root.canonicalize().map_err(|e| ReadingRefusal::NoWorkspace.refuse(format!("{}: {e}", root.display())))?;
        if !root.join("Cargo.toml").is_file() {
            return Err(ReadingRefusal::NoWorkspace.refuse(root.display()));
        }
        let opts = scope.scan_options();
        let started = std::time::Instant::now();
        // the parsed index, held once per code digest when the project has a folder
        let cached = crate::store::Store::find(&root).and_then(|s| index_cache_key(&root, &opts).ok().and_then(|key| s.path(&crate::store::named::INDEX_MODEL, &[("digest", &key)]).ok()));
        if let Some(file) = &cached {
            if let Some(k) = std::fs::File::open(file).ok().and_then(|f| zstd::stream::Decoder::new(f).ok()).and_then(|z| ciborium::from_reader::<Klepto, _>(std::io::BufReader::new(z)).ok()) {
                let mut f = Self::from_klepto(root, k, how)?;
                f.index = IndexSource::FromCache { millis: started.elapsed().as_millis() as u64 };
                f.trees = false;
                return Ok(f);
            }
        }
        let k = klepto_scan(&root, &opts).map_err(|e| ReadingRefusal::Scan.refuse(e))?;
        let over = cached.as_ref().and_then(|file| keep_index(file, &k));
        let mut f = Self::from_klepto(root, k, how)?;
        let millis = started.elapsed().as_millis() as u64;
        f.index = match over {
            Some(bytes) => IndexSource::ParsedNotKept { millis, bytes },
            None => IndexSource::Parsed { millis },
        };
        Ok(f)
    }

    /// The facts from a parse, never from the cache: for a reader that walks the syntax trees.
    pub fn build_parsed(root: &Path, scope: &Scope, how: Resolving) -> Result<Facts> {
        let root = root.canonicalize().map_err(|e| ReadingRefusal::NoWorkspace.refuse(format!("{}: {e}", root.display())))?;
        let started = std::time::Instant::now();
        let opts = scope.scan_options();
        let k = klepto_scan(&root, &opts).map_err(|e| ReadingRefusal::Scan.refuse(e))?;
        // kept for the readings after it over the same code (the cache holds the model, never the trees)
        let cached = crate::store::Store::find(&root).and_then(|s| index_cache_key(&root, &opts).ok().and_then(|key| s.path(&crate::store::named::INDEX_MODEL, &[("digest", &key)]).ok()));
        let over = cached.as_ref().filter(|f| !f.is_file()).and_then(|file| keep_index(file, &k));
        let mut f = Self::from_klepto(root, k, how)?;
        let millis = started.elapsed().as_millis() as u64;
        f.index = match over {
            Some(bytes) => IndexSource::ParsedNotKept { millis, bytes },
            None => IndexSource::Parsed { millis },
        };
        Ok(f)
    }

    pub fn from_klepto(root: PathBuf, mut k: Klepto, how: Resolving) -> Result<Facts> {
        // an item's package is its cargo package: a bin's crate root (`appd`) belongs to its package (`app`)
        let roots = k.crate_roots.clone();
        for d in &mut k.items {
            if let Some(p) = roots.get(&d.package) {
                d.package = p.clone();
            }
        }
        let paths = PathsTable::from_packages(&root, &k.packages, crate::store::Store::find(&root).as_ref())?;
        let members: BTreeSet<&String> = k.packages.keys().collect();
        let mut packages = BTreeMap::new();
        for p in k.packages.values() {
            let dir = p.root.strip_prefix(&root).unwrap_or(&p.root).to_string_lossy().replace('\\', "/");
            let mut deps: Vec<(String, Link)> = k
                .features
                .get(&p.name)
                .map(|m| {
                    m.dependencies
                        .iter()
                        .filter(|d| members.contains(&d.package.replace('-', "_")))
                        .map(|d| {
                            let l = match d.kind {
                                DepKind::Normal => Link::Normal,
                                DepKind::Build => Link::Build,
                                DepKind::Dev => Link::Dev,
                            };
                            (d.package.replace('-', "_"), l)
                        })
                        .collect()
                })
                .unwrap_or_default();
            deps.sort();
            deps.dedup();
            packages.insert(p.name.clone(), Package { name: p.name.clone(), dir, bins: p.bins(), deps });
        }
        let links = link_closure(&packages);
        let file_index = k.files.iter().enumerate().map(|(i, f)| (f.path.clone(), i)).collect();
        let file_package: HashMap<PathBuf, String> = crate::file_packages(&k).into_iter().map(|(f, p)| (f, roots.get(&p).cloned().unwrap_or(p))).collect();
        let items_by_id = k.items.iter().enumerate().map(|(i, d)| (d.id.clone(), i)).collect();
        let mut referrers: HashMap<String, Vec<String>> = HashMap::new();
        let mut references: HashMap<String, Vec<String>> = HashMap::new();
        for e in &k.graph.edges {
            if matches!(e.kind, EdgeKind::Reference | EdgeKind::ReExport) {
                referrers.entry(e.to.clone()).or_default().push(e.from.clone());
                references.entry(e.from.clone()).or_default().push(e.to.clone());
            }
        }
        let mut marks: BTreeMap<String, Marks> = BTreeMap::new();
        let (levels, marks_from) = match maintenance::registry::read(&root) {
            Ok(reg) => {
                for e in reg.items {
                    marks.insert(
                        e.id.clone(),
                        Marks {
                            sensitivity: e.sensitivity,
                            owner: e.owner,
                            mechanism: e.mechanism,
                            boundary: e.boundary,
                            supersedes: e.supersedes,
                            reads_env: e.reads_env,
                            review: e.review.map(|r| r.reason),
                        },
                    );
                }
                (reg.levels, "registry")
            }
            Err(_) => {
                for a in &k.attrs {
                    let Some(item) = &a.enclosing_item else { continue };
                    let m = marks.entry(item.clone()).or_default();
                    match a.path.rsplit("::").next().unwrap_or(&a.path) {
                        "sensitivity" => m.sensitivity = a.arg_paths.first().cloned(),
                        "owner" => m.owner = strings_in(&a.args).into_iter().next(),
                        "mechanism" => m.mechanism = strings_in(&a.args).into_iter().next(),
                        "supersedes" => m.supersedes = strings_in(&a.args).into_iter().next(),
                        "reads_env" => m.reads_env = strings_in(&a.args),
                        "requires_review" => m.review = Some(a.args.clone()),
                        "boundary" => m.boundary = Some(a.arg_paths.iter().filter(|p| *p != "allowed").cloned().collect()),
                        _ => {}
                    }
                }
                marks.retain(|_, m| *m != Marks::default());
                (maintenance::levels::LevelTable::shipped().names(), "attributes")
            }
        };
        Ok(Facts { root, k, packages, links, marks, levels, marks_from, paths, how, file_index, file_package, items_by_id, referrers, references, index: IndexSource::Parsed { millis: 0 }, trees: true })
    }

    /// `/`-separated, relative to the root when under it.
    pub fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).unwrap_or(p).to_string_lossy().replace('\\', "/")
    }

    pub fn package_of(&self, file: &Path) -> Option<&str> {
        self.file_package.get(file).map(String::as_str)
    }

    /// The source line, trimmed, at most 160 characters (klepto's copy of the file).
    pub fn context(&self, file: &Path, line: u32) -> String {
        let Some(&i) = self.file_index.get(file) else { return String::new() };
        let text = self.k.files[i].source.lines().nth(line.saturating_sub(1) as usize).unwrap_or("").trim();
        let mut out: String = text.chars().take(160).collect();
        if text.chars().count() > 160 {
            out.push('…');
        }
        out
    }

    pub fn bins_of(&self, package: &str) -> Vec<String> {
        self.links.get(package).map(|b| b.iter().cloned().collect()).unwrap_or_default()
    }

    pub fn item(&self, id: &str) -> Option<&klepto::ItemDecl> {
        self.items_by_id.get(id).map(|i| &self.k.items[*i])
    }

    /// A level's rank in the level table (lowest first).
    pub fn level_rank(&self, level: &str) -> Option<usize> {
        self.levels.iter().position(|l| l == level)
    }

    /// Whether an item at `level` is held (written redacted unless revealed):
    /// at `secret` (`crate::bundle::HELD_FROM`) or above. A table with no
    /// `secret` holds every marked item (more, never less).
    pub fn held(&self, level: &str) -> bool {
        match (self.level_rank(level), self.level_rank(crate::bundle::HELD_FROM)) {
            (Some(a), Some(floor)) => a >= floor,
            (None, Some(_)) => true,
            (_, None) => true,
        }
    }

    /// The marks of an item, or of the nearest item enclosing it that has some.
    pub fn marks_of(&self, id: &str) -> Option<&Marks> {
        let mut cur = Some(id.to_string());
        while let Some(i) = cur {
            if let Some(m) = self.marks.get(&i) {
                return Some(m);
            }
            cur = self.item(&i).and_then(|d| d.parent.clone()).or_else(|| i.rsplit_once("::").map(|(p, _)| p.to_string()).filter(|p| self.item(p).is_some()));
        }
        None
    }

    /// Ids that reference `id` directly.
    pub fn referrers_of(&self, id: &str) -> &[String] {
        self.referrers.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Ids `id` references directly.
    pub fn references_of(&self, id: &str) -> &[String] {
        self.references.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Everything that reaches any of `seeds` through references (seeds out).
    pub fn closure_of<'a>(&self, seeds: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
        let seeds: BTreeSet<String> = seeds.into_iter().map(str::to_string).collect();
        let mut seen = seeds.clone();
        let mut work: Vec<String> = seeds.iter().cloned().collect();
        while let Some(id) = work.pop() {
            for r in self.referrers_of(&id) {
                if seen.insert(r.clone()) {
                    work.push(r.clone());
                }
            }
        }
        seen.difference(&seeds).cloned().collect()
    }

    /// Everything `id` reaches through references (`id` out).
    pub fn reached_by(&self, id: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut work = vec![id.to_string()];
        while let Some(cur) = work.pop() {
            for r in self.references_of(&cur) {
                if seen.insert(r.clone()) {
                    work.push(r.clone());
                }
            }
        }
        seen.remove(id);
        seen
    }

    /// Workspace packages that depend on `package`, with how, direct only.
    pub fn direct_dependents(&self, package: &str) -> Vec<(String, Link)> {
        let mut out: Vec<(String, Link)> = self
            .packages
            .values()
            .flat_map(|p| p.deps.iter().filter(|(d, _)| d == package).map(move |(_, l)| (p.name.clone(), *l)))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Packages reaching `package` through normal and build dependencies.
    pub fn linked_dependents(&self, package: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut work = vec![package.to_string()];
        while let Some(p) = work.pop() {
            for (d, l) in self.direct_dependents(&p) {
                if l != Link::Dev && seen.insert(d.clone()) {
                    work.push(d);
                }
            }
        }
        seen.remove(package);
        seen
    }

    /// A package named by its name (either spelling) or its directory.
    pub fn package_named(&self, target: &str) -> Option<&Package> {
        let t = target.trim().trim_end_matches('/');
        self.packages.get(&t.replace('-', "_")).or_else(|| self.packages.values().find(|p| p.dir == t))
    }

    /// The package whose directory holds `rel` (the deepest one).
    pub fn package_of_rel(&self, rel: &str) -> Option<&Package> {
        self.packages
            .values()
            .filter(|p| p.dir.is_empty() || rel == p.dir || rel.starts_with(&format!("{}/", p.dir)))
            .max_by_key(|p| p.dir.len())
    }

    /// Klepto's resolution as a canonical path, if resolved.
    pub fn canonical(r: &Resolution) -> Option<&str> {
        match r {
            Resolution::Local(s) | Resolution::External(s) => Some(s),
            Resolution::Unresolved { .. } => None,
        }
    }

    /// Function ids klepto marks as tests (`#[test]`, `#[tokio::test]`, ... :
    /// an attribute whose last segment is `test`).
    pub fn tests(&self) -> BTreeSet<String> {
        self.k
            .attrs
            .iter()
            .filter(|a| a.path.rsplit("::").next() == Some("test"))
            .filter_map(|a| a.enclosing_item.clone())
            .filter(|id| self.item(id).is_some_and(|d| matches!(d.kind, klepto::graph::NodeKind::Fn | klepto::graph::NodeKind::Method)))
            .collect()
    }
}

/// Package → every binary built from it or from a package that depends on
/// it through normal or build dependencies.
fn link_closure(packages: &BTreeMap<String, Package>) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in packages.values().filter(|p| !p.bins.is_empty()) {
        let mut seen = BTreeSet::from([p.name.clone()]);
        let mut work = vec![p.name.clone()];
        while let Some(cur) = work.pop() {
            if let Some(cp) = packages.get(&cur) {
                for (d, l) in &cp.deps {
                    if *l != Link::Dev && seen.insert(d.clone()) {
                        work.push(d.clone());
                    }
                }
            }
        }
        for c in seen {
            out.entry(c).or_default().extend(p.bins.iter().cloned());
        }
    }
    out
}
