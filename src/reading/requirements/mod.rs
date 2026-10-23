//! requirements: what an intended change needs from the workspace as it is, read before anything is changed.
//!
//! Three faces, one reading:
//!
//! - **prerequisites**: the manifests read as TOML with no cargo ([`manifest`]): the governing workspace root of
//!   every crate by cargo's own rule, `workspace = true` against that root's tables, path dependencies that point
//!   nowhere, renames, optional/dev/build/target tables, features in their four forms through klepto's feature
//!   model; then, when cargo metadata answers, the requirement side ([`needs`]): what the intent or the change spec
//!   needs and whether each is satisfied.
//! - **rust**: per crate its rust-version and edition, the highest rust-version any package of its lock demands, the
//!   installed toolchain, and the three disagreements between them ([`rust`]).
//! - **lock**: every package the lock holds in more than one version, from `cargo tree -d` read as cargo prints it:
//!   who forces each version, how many compiles it costs, and which one requirement change would leave one ([`lock`]).
//!
//! Where `cargo metadata` works it is the truth; the scanner is for the state cargo cannot speak in yet, and says
//! so. Nothing here resolves a version, writes a manifest, a lock or a source file, or compiles: the cargo lines it
//! runs are `metadata --frozen`, `tree --locked --offline` and the toolchain's `rustc -vV`, `rustup show`. Every
//! finding says whether it was observed, inferred or is unknown, and where it was read.

pub mod cargo;
pub mod lock;
pub mod manifest;
pub mod needs;
pub mod order;
pub mod render;
pub mod rust;

use super::refusal::ReadingRefusal;
use super::spec::{vocabulary, ReadingSpec};
use super::{Answer, ReadingReport};
use big_bad_bool::bitpack;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const REQUIREMENTS_SCHEMA: &str = "custodian.requirements.v1";

vocabulary! {
    /// How a finding is known.
    Basis, "basis" {
        Observed = "observed" : "read as it is: a file's text, or cargo's own output",
        Inferred = "inferred" : "follows from what was read by a stated rule (cargo's documented behaviour, a semver query, a text search); cargo has not confirmed it",
        Unknown = "unknown" : "not established by this reading; the finding names what would establish it",
    }
}

vocabulary! {
    /// How much a finding matters.
    Severity, "severity" {
        Error = "error" : "cargo refuses it, or the intended change cannot work without it",
        Warn = "warn" : "cargo accepts it, but a promise is false or something is ignored",
        Info = "info" : "worth knowing; not presumed a mistake",
    }
}

vocabulary! {
    /// Where a finding stands against the intended change.
    Status, "status" {
        Satisfied = "satisfied" : "present, with its evidence",
        Unsatisfied = "unsatisfied" : "missing or broken: a step in the order fixes it",
        Informational = "informational" : "neither: a fact the change should know",
    }
}

vocabulary! {
    /// The reading's three faces.
    Face, "face" {
        Prerequisites = "prerequisites" : "the manifests and workspaces a change needs, read without cargo, then what the intent or the spec requires against cargo metadata",
        Rust = "rust" : "each crate's rust-version and edition against the lock's demands and the installed toolchain",
        Lock = "lock" : "the packages the lock holds in more than one version, from cargo tree -d",
    }
}

vocabulary! {
    /// What a finding was read from.
    ReadFrom, "read from" {
        Manifest = "manifest" : "the manifest scanner: Cargo.toml read as TOML, no cargo",
        Metadata = "metadata" : "cargo metadata --frozen (or --no-deps), as cargo printed it",
        Tree = "tree" : "cargo tree --duplicates --locked --offline, as cargo printed it",
        Toolchain = "toolchain" : "rustc -vV and rustup show active-toolchain, run in the workspace root",
        Lockfile = "lockfile" : "Cargo.lock read as TOML",
        Intent = "intent" : "the intent's words, the change spec or the reading spec's needs",
        Sources = "sources" : "a text search of a crate's .rs files",
    }
}

/// Which faces a reading reads, as one packed word: its bits are what the spec keeps and what the same-search key
/// digests. `no_cargo` reads the manifests only (cargo is not run); the lock's shape is cargo's answer, so it cannot
/// go with `no_cargo` (flaga_solver checks the rule).
#[bitpack(storage = u8, version = 1)]
pub struct FacesWord {
    #[bits(4)]
    #[describe("the requirements reading's faces, and whether cargo is run")]
    #[flags(prerequisites = 0, rust = 1, lock = 2, no_cargo = 3)]
    #[labels(lock = 4, no_cargo = 8)]
    #[conflicts(lock, no_cargo)]
    faces: u8,
}

impl FacesWord {
    /// Every face, cargo run.
    pub const ALL: FacesWord = FacesWord::from_raw((FacesWord::FLAG_FACES_PREREQUISITES | FacesWord::FLAG_FACES_RUST | FacesWord::FLAG_FACES_LOCK) as u8);

    /// The word from flag names (`prerequisites`, `rust`, `lock`, `no_cargo`); none is every face. An unknown name,
    /// or a broken rule, is refused.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> crate::Result<Self> {
        let mut raw = 0u8;
        for name in names {
            let flag = Self::FLAGS.iter().find(|f| f.name == name.trim()).ok_or_else(|| ReadingRefusal::SpecInvalid.refuse(format!("`{name}` is not a requirements face ({})", Self::all_names().join(", "))))?;
            raw |= flag.mask as u8;
        }
        if raw & !(Self::FLAG_FACES_NO_CARGO as u8) == 0 {
            raw |= Self::ALL.raw();
            if raw & Self::FLAG_FACES_NO_CARGO as u8 != 0 {
                raw &= !(Self::FLAG_FACES_LOCK as u8);
            }
        }
        let w = Self::from_raw(raw);
        let verdict = flaga_solver::check(&flaga_solver::schema_of!(FacesWord), w.raw() as u128);
        if verdict.is_ok() {
            Ok(w)
        } else {
            Err(ReadingRefusal::SpecInvalid.refuse(format!("faces {}: {verdict}", w.names().join(","))))
        }
    }

    /// The names of the flags set, in bit order.
    pub fn names(self) -> Vec<&'static str> {
        Self::FLAGS.iter().filter(|f| self.raw() as u128 & f.mask != 0).map(|f| f.name).collect()
    }

    pub fn all_names() -> Vec<&'static str> {
        Self::FLAGS.iter().map(|f| f.name).collect()
    }

    pub fn reads(self, face: Face) -> bool {
        match face {
            Face::Prerequisites => self.has_faces_prerequisites(),
            Face::Rust => self.has_faces_rust(),
            Face::Lock => self.has_faces_lock(),
        }
    }

    /// Whether cargo is run.
    pub fn runs_cargo(self) -> bool {
        !self.has_faces_no_cargo()
    }
}

/// One thing an intended change needs: a dependency `dependency` in the crate `member`, with what it asks of it;
/// or a new crate at `new_crate`. Written in a reading spec's `[[requirements.needs]]`, or made from an intent's
/// words or a change spec's operations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Need {
    /// The crate the dependency lands in (a member's package name), or the new crate's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    /// The dependency, by its package name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency: Option<String>,
    /// The version requirement asked for, as cargo writes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Features of the dependency the change needs on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// A path source, relative to the member's directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    /// A new crate's directory, relative to the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_crate: Option<String>,
    /// Where the need came from: `intent: hex`, `spec add_dependency`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub said: String,
}

/// A requirements reading's own options (the spec's `[requirements]`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementsOptions {
    /// What the change is for, in the asker's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// What it needs, written out (or made from a change spec).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<Need>,
    /// The change spec the needs were made from, as it was named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_spec: Option<String>,
    /// The faces read (`prerequisites`, `rust`, `lock`, `no_cargo`); none is every face.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<String>,
}

impl RequirementsOptions {
    pub fn is_empty(&self) -> bool {
        self == &RequirementsOptions::default()
    }

    pub fn word(&self) -> crate::Result<FacesWord> {
        FacesWord::from_names(self.faces.iter().map(String::as_str))
    }
}

/// Where in a manifest a finding was read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    /// The manifest, relative to the root (absolute when outside it).
    pub file: String,
    pub line: Option<u32>,
    /// `dependencies`, `workspace.dependencies`, `target.'cfg(unix)'.dependencies`, `package`, `features`.
    pub section: String,
    /// The key in it: `hex`, `rust-version`, `members[2]`.
    pub field: String,
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable over readings of the same manifests: the check and the place.
    pub id: String,
    pub check: Check,
    pub face: Face,
    pub status: Status,
    pub severity: Severity,
    pub basis: Basis,
    pub from: ReadFrom,
    /// The crate it is about, when one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub krate: Option<String>,
    /// What was found, one sentence.
    pub what: String,
    /// What must hold.
    pub requirement: String,
    /// What was read that shows it.
    pub evidence: String,
    /// What follows from it.
    pub consequence: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Place>,
    /// What would satisfy it; never a version this reading did not read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// One line the reading ran, as it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ran {
    pub line: String,
    pub dir: String,
    pub code: Option<i32>,
    pub millis: i64,
    /// What it said on stderr, its first lines (a refusal's reason).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub said: String,
}

/// One workspace root the scanner found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    pub manifest: String,
    pub members: Vec<String>,
    /// Another root holds it in its directory, and it is its own workspace.
    pub nested: bool,
    /// The keys of its `[workspace.dependencies]`.
    pub dependencies: usize,
    /// The fields of its `[workspace.package]`.
    pub package_fields: Vec<String>,
    pub lints: bool,
}

/// What the scanner and cargo found the workspace to be.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub roots: Vec<Root>,
    /// Manifests read (members, roots, path dependencies, the target).
    pub manifests: usize,
    /// Manifests under the root that no workspace holds and nothing depends on: not read further.
    pub outside: Vec<String>,
    /// The crates the reading is about (the target's, or every member).
    pub crates: Vec<String>,
    /// The workspace cargo was run on.
    pub cargo_root: Option<String>,
    /// `read (N packages, M members)`, `refused: ...`, `not run`.
    pub metadata: String,
    pub lock: Option<String>,
    /// Satisfied checks counted, not listed (`workspace = true` entries found in their root: 214).
    pub satisfied: BTreeMap<String, usize>,
    /// The scanner's errors against cargo's word: agree, or where they part.
    pub agreement: String,
}

/// The toolchain that answers in the workspace root.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toolchain {
    /// `rustc -vV`'s `release:` (`1.90.0`).
    pub release: Option<String>,
    pub host: Option<String>,
    /// `rustup show active-toolchain`, as it printed it.
    pub active: Option<String>,
    /// A `rust-toolchain(.toml)` at or above the root, as written.
    pub file: Option<String>,
}

/// One crate's Rust versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustRow {
    pub krate: String,
    pub manifest: String,
    pub edition: String,
    /// The rust-version it declares (`None`: it promises nothing).
    pub declared: Option<String>,
    /// The highest rust-version a package of its lock (its closure in cargo's resolve) declares, and that package.
    pub lock_max: Option<(String, String)>,
    /// The first Rust its edition compiles on.
    pub edition_min: String,
}

/// One version of a package the lock holds more than once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateVersion {
    pub version: String,
    /// Where it comes from when it is not the registry: a path (from the root), a git source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// How many times cargo tree shows it as a unit of its own (another feature set, or the host beside the
    /// target): each is a compile.
    pub builds: usize,
    /// Its pullers were read from cargo metadata's resolve (cargo tree printed this unit once, elsewhere).
    #[serde(default)]
    pub pullers_from_metadata: bool,
    /// The packages whose own requirement pulls this version in (cargo tree's first level): `name version`, the
    /// requirement as written, and where it is written when it is a path package's manifest.
    pub forced_by: Vec<Forcing>,
    /// Packages that reach it only through those (cargo tree's deeper levels).
    pub through: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forcing {
    pub package: String,
    pub requirement: Option<String>,
    /// A workspace or path package: its manifest can change; a registry package's cannot here.
    pub local: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Place>,
}

/// A package the lock holds in more than one version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duplicate {
    pub name: String,
    pub versions: Vec<DuplicateVersion>,
    /// How many times it compiles on this host: every unit cargo tree shows of it (a version, a feature set, the
    /// host beside the target).
    pub compiles: usize,
    /// The one requirement change that would leave one version, when one would; why not, when none would.
    pub collapse: String,
    pub basis: Basis,
}

/// What a crate of the change touches, from cargo's resolve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Impact {
    pub krate: String,
    /// Its direct dependencies: `name version`.
    pub depends_on: Vec<String>,
    /// The packages that depend on it directly, then every one that reaches it.
    pub direct_dependents: Vec<String>,
    pub all_dependents: Vec<String>,
    /// The readings that read what it touches item by item, as lines to run.
    pub readings: Vec<String>,
}

/// One step of the suggested order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub n: usize,
    /// Required (the change cannot work without it) or recommended.
    pub required: bool,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Place>,
    /// How to see it done.
    pub verify: String,
    /// The findings it answers.
    pub answers: Vec<String>,
}

/// Where the reading came from: every file digested, every line run, what was written (nothing).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Each manifest read and its digest.
    pub manifests: BTreeMap<String, String>,
    pub lock: Option<(String, String)>,
    /// The digest of every input the reuse key covers: the manifests, the lock, the toolchain files and the spec's
    /// requirements.
    pub inputs: String,
    pub ran: Vec<Ran>,
    pub toolchain: Toolchain,
    /// What this reading wrote into the workspace: nothing.
    pub mutation: String,
}

/// The requirements reading's report: the brief's contract, section by section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementsReport {
    pub schema: String,
    /// The requested outcome and its scope.
    pub intent: Option<String>,
    pub target: String,
    pub faces: Vec<String>,
    pub needs: Vec<Need>,
    pub state: State,
    pub findings: Vec<Finding>,
    pub rust: Vec<RustRow>,
    pub duplicates: Vec<Duplicate>,
    pub impact: Vec<Impact>,
    pub reuse: Vec<String>,
    pub steps: Vec<Step>,
    pub unknowns: Vec<String>,
    pub provenance: Provenance,
}

impl RequirementsReport {
    /// The findings a change still has to answer: unsatisfied, errors first.
    pub fn unresolved(&self) -> Vec<&Finding> {
        let mut v: Vec<&Finding> = self.findings.iter().filter(|f| f.status == Status::Unsatisfied).collect();
        v.sort_by_key(|f| (f.severity, f.id.clone()));
        v
    }

    /// Whether every manifest and the lock the report rests on still hold the bytes it read (`root`: the reading's
    /// root). A manifest added since is not seen here; the reuse key sees it.
    pub fn still_holds(&self, root: &Path) -> bool {
        let same = |rel: &str, digest: &str| {
            let p = if Path::new(rel).is_absolute() { PathBuf::from(rel) } else { root.join(rel) };
            std::fs::read(p).is_ok_and(|b| crate::digest::of(b) == digest)
        };
        self.provenance.manifests.iter().all(|(f, d)| same(f, d)) && self.provenance.lock.as_ref().is_none_or(|(f, d)| same(f, d))
    }

    /// One line per unresolved finding, for a brief or a handoff: `error · hex: … (a/Cargo.toml:7)`.
    pub fn unresolved_lines(&self) -> Vec<String> {
        self.unresolved().iter().map(|f| f.line()).collect()
    }
}

impl Finding {
    /// `error · workspace_dependency_missing · one_db_kernel: … (crates/one_db_kernel/Cargo.toml:9)`.
    pub fn line(&self) -> String {
        let at = self.at.as_ref().map(|p| format!(" ({}{})", p.file, p.line.map(|l| format!(":{l}")).unwrap_or_default())).unwrap_or_default();
        format!("{} · {} · {}{}{}", self.severity, self.basis, self.check, self.krate.as_ref().map(|k| format!(" · {k}")).unwrap_or_default(), format_args!(": {}{at}", self.what))
    }
}

vocabulary! {
    /// Every check the reading makes, by name; [`Check::rule`] says its face, severity and the order step it lands in.
    Check, "check" {
        ManifestUnreadable = "manifest_unreadable" : "a Cargo.toml that is not TOML: cargo refuses everything that reads it",
        NotAMember = "not_a_member" : "the crate's governing root (the nearest ancestor with [workspace] that does not exclude it, as cargo finds it) does not list it: cargo refuses it (\"believes it's in a workspace when it's not\")",
        MemberMissing = "member_missing" : "a [workspace] members entry that names a directory with no Cargo.toml",
        WorkspaceNotARoot = "workspace_not_a_root" : "package.workspace points at a manifest with no [workspace]",
        DuplicatePackageName = "duplicate_package_name" : "two crates of one workspace with one package name",
        WorkspaceDependencyMissing = "workspace_dependency_missing" : "a `workspace = true` dependency whose governing root has no [workspace.dependencies] entry of that name (or no root at all)",
        WorkspaceFieldMissing = "workspace_field_missing" : "a [package] field inherited with `workspace = true` that the root's [workspace.package] does not set",
        WorkspaceLintsMissing = "workspace_lints_missing" : "[lints] workspace = true with no [workspace.lints] in the root",
        InheritedKeyIgnored = "inherited_key_ignored" : "a key beside `workspace = true` that cargo ignores (version, path, git, ...): the root's entry decides",
        PathMissing = "path_missing" : "a path dependency (or patch) whose directory has no Cargo.toml",
        PathNamesOtherPackage = "path_names_other_package" : "a path dependency whose directory holds a package of another name",
        PathVersionMismatch = "path_version_mismatch" : "a path dependency's version requirement that the package at the path does not meet: cargo refuses it, and which side moves is a choice",
        OptionalDevDependency = "optional_dev_dependency" : "a dev-dependency marked optional: cargo refuses it",
        RegistryUnconfigured = "registry_unconfigured" : "a dependency on a named registry no .cargo/config.toml at or above the root (or in CARGO_HOME) declares",
        FeatureEntryUnknown = "feature_entry_unknown" : "a [features] entry naming no feature and no dependency, `dep:x` on a dependency that is not optional, or `x/f` on no dependency",
        RenamedDependency = "renamed_dependency" : "a dependency whose key is not its package (`package = ...`): source names it by the key",
        RequirementSpread = "requirement_spread" : "one package required with different requirements across the workspace",
        PossiblyUnused = "possibly_unused" : "a dependency whose name no .rs file of the crate holds as a word (a text search: macros, build scripts and lib names that differ hide uses)",
        WorkspaceDependencyUnused = "workspace_dependency_unused" : "a [workspace.dependencies] entry no member inherits",
        OutsideAnyWorkspace = "outside_any_workspace" : "manifests under the root that no workspace holds and nothing depends on",
        MetadataRead = "metadata_read" : "cargo metadata read the workspace: cargo's resolve is the truth from here",
        MetadataRefused = "metadata_refused" : "cargo metadata refused the workspace: its words, as it printed them",
        ScannerAndCargoDisagree = "scanner_and_cargo_disagree" : "the scanner and cargo part: cargo's word stands, and the scanner's limit is named",
        NeedDeclared = "need_declared" : "a dependency the change needs, declared in the crate it lands in or not",
        NeedVersion = "need_version" : "the version the change asks for against the versions the lock holds",
        NeedFeature = "need_feature" : "a feature the change needs on a dependency: whether the dependency has it, and what turns it on",
        NeedSource = "need_source" : "where a needed dependency comes from: a path, the lock, or a registry this reading does not fetch",
        NeedNewCrate = "need_new_crate" : "a new crate's directory: the root that would govern it, whether its members take it, its name free",
        NeedUnnamed = "need_unnamed" : "a name in the intent that nothing in the manifests or the lock holds",
        FeatureCombinations = "feature_combinations" : "the feature combinations no resolve read and no build ran",
        EditionNeedsNewerRust = "edition_needs_newer_rust" : "a rust-version older than the first Rust that compiles the crate's edition",
        RustVersionBelowLock = "rust_version_below_lock" : "a crate that promises a rust-version older than a package of its lock demands",
        RustVersionUndeclared = "rust_version_undeclared" : "a crate that promises no rust-version, with what its lock demands",
        ToolchainBelowLock = "toolchain_below_lock" : "a lock that demands a newer Rust than the toolchain installed for the workspace",
        ToolchainUnknown = "toolchain_unknown" : "the installed toolchain could not be read",
        DuplicateVersions = "duplicate_versions" : "a package the lock holds in more than one version",
        TreeRefused = "tree_refused" : "cargo tree refused: the lock's shape is read from Cargo.lock's text, its forcing requirements unknown",
    }
}

impl Check {
    /// The check's face, its severity when unsatisfied, and the phase of the order its step lands in (roots and
    /// membership first, then the root's declarations, the crate's, the features, cargo's word, Rust, the lock).
    pub fn rule(self) -> (Face, Severity, u8) {
        use Check::*;
        use Face::*;
        use Severity::*;
        match self {
            ManifestUnreadable => (Prerequisites, Error, 0),
            NotAMember | MemberMissing | WorkspaceNotARoot | DuplicatePackageName | NeedNewCrate => (Prerequisites, Error, 1),
            WorkspaceDependencyMissing | WorkspaceFieldMissing | WorkspaceLintsMissing => (Prerequisites, Error, 2),
            PathMissing | PathNamesOtherPackage | PathVersionMismatch | OptionalDevDependency | RegistryUnconfigured | NeedDeclared | NeedSource | NeedVersion => (Prerequisites, Error, 3),
            InheritedKeyIgnored => (Prerequisites, Warn, 3),
            FeatureEntryUnknown | NeedFeature => (Prerequisites, Error, 4),
            MetadataRefused => (Prerequisites, Error, 5),
            ScannerAndCargoDisagree => (Prerequisites, Warn, 5),
            MetadataRead => (Prerequisites, Info, 5),
            RenamedDependency | RequirementSpread | PossiblyUnused | WorkspaceDependencyUnused | OutsideAnyWorkspace | NeedUnnamed | FeatureCombinations => (Prerequisites, Info, 9),
            EditionNeedsNewerRust | ToolchainBelowLock => (Rust, Error, 6),
            RustVersionBelowLock => (Rust, Warn, 6),
            RustVersionUndeclared | ToolchainUnknown => (Rust, Info, 6),
            DuplicateVersions => (Lock, Info, 7),
            TreeRefused => (Lock, Info, 7),
        }
    }
}

/// A finding made by a check: its face and severity the check's ([`Check::rule`]), its id the check and the place.
#[allow(clippy::too_many_arguments)]
pub fn finding(check: Check, status: Status, basis: Basis, from: ReadFrom, krate: Option<&str>, at: Option<Place>, what: impl Into<String>, requirement: impl Into<String>, evidence: impl Into<String>, consequence: impl Into<String>, action: Option<String>) -> Finding {
    let (face, severity, _) = check.rule();
    let severity = if status == Status::Informational { Severity::Info } else { severity };
    let place = at.as_ref().map(|p| format!("{}#{}.{}", p.file, p.section, p.field)).unwrap_or_default();
    let what = what.into();
    let id = crate::digest::short(&crate::digest::of_value(&(check.as_str(), krate, &place, &what)), 12);
    Finding {
        id: format!("{}-{id}", check.as_str()),
        check,
        face,
        status,
        severity,
        basis,
        from,
        krate: krate.map(str::to_string),
        what,
        requirement: requirement.into(),
        evidence: evidence.into(),
        consequence: consequence.into(),
        at,
        action,
    }
}

/// The digest of everything the reading's answer rests on that is a file: every manifest the scanner reads, the lock,
/// the toolchain files and `.cargo/config.toml`s at and above the root, and the spec's requirements. The reuse key's
/// code half for this kind: an edit to any of them is a new reading.
pub fn input_digest(root: &Path, spec: &ReadingSpec) -> crate::Result<String> {
    let root = root.canonicalize().map_err(|e| ReadingRefusal::NoWorkspace.refuse(format!("{}: {e}", root.display())))?;
    let scan = manifest::Scan::read(&root, &spec.scope, &target_of(spec))?;
    Ok(scan.digest(&spec.requirements))
}

/// The target as the reading takes it: `.`/`*` the whole root; `crate:<name>`; a directory.
pub fn target_of(spec: &ReadingSpec) -> String {
    spec.target.trim().to_string()
}

/// Answers a requirements spec over the workspace at `root`: no scan of the sources (a text search for unused
/// dependencies aside), cargo run only as metadata, tree and the toolchain's version lines; nothing written.
pub fn read(root: &Path, spec_path: &Path, spec: &ReadingSpec, stamp: &str, how: crate::paths::Resolving) -> crate::Result<ReadingReport> {
    let root = root.canonicalize().map_err(|e| ReadingRefusal::NoWorkspace.refuse(format!("{}: {e}", root.display())))?;
    let target = target_of(spec);
    if how == crate::paths::Resolving::Job && !(target == "." || target == "*" || target.starts_with("crate:")) {
        return Err(ReadingRefusal::WalkedPathInJob.refuse(format!("{}: in a job the requirements reading takes `.` or a crate: route, not `{target}`", spec.name)));
    }
    let word = spec.requirements.word()?;
    let report = answer(&root, spec, word)?;
    let mut ans = Answer::default();
    let unsat = |s: Severity| report.findings.iter().filter(|f| f.status == Status::Unsatisfied && f.severity == s).count();
    ans.count("unsatisfied (error)", unsat(Severity::Error));
    ans.count("unsatisfied (warn)", unsat(Severity::Warn));
    ans.count("satisfied", report.findings.iter().filter(|f| f.status == Status::Satisfied).count() + report.state.satisfied.values().sum::<usize>());
    ans.count("informational", report.findings.iter().filter(|f| f.status == Status::Informational).count());
    ans.count("unknown", report.findings.iter().filter(|f| f.basis == Basis::Unknown).count() + report.unknowns.len());
    ans.count("rust-version disagreements", report.findings.iter().filter(|f| f.face == Face::Rust && f.status == Status::Unsatisfied).count());
    ans.count("packages in more than one version", report.duplicates.iter().filter(|d| d.versions.len() > 1).count());
    ans.count("packages built more than once at one version", report.duplicates.iter().filter(|d| d.versions.len() == 1).count());
    ans.count("manifests read", report.state.manifests);
    ans.headline = report.unresolved_lines().into_iter().take(5).collect();
    if !report.duplicates.is_empty() {
        let extra: usize = report.duplicates.iter().map(|d| d.compiles.saturating_sub(1)).sum();
        let split = report.duplicates.iter().filter(|d| d.versions.len() > 1).count();
        ans.headline.push(format!("{split} packages in more than one version, {} more built twice at one version: {extra} compiles past one each", report.duplicates.len() - split));
    }
    ans.rows = rows(&report);
    let query_spec = crate::parse_code_query_spec(&spec.query_args());
    Ok(ReadingReport {
        schema: super::READING_SCHEMA.to_string(),
        name: spec.name.clone(),
        question: spec.question.clone(),
        kind: spec.kind,
        target: spec.target.clone(),
        spec: spec_path.display().to_string(),
        stamp: stamp.to_string(),
        root: root.display().to_string(),
        counts: ans.counts,
        query: crate::CodeQueryReport {
            schema: "code_passenger.query.v1".to_string(),
            graph_schema: REQUIREMENTS_SCHEMA.to_string(),
            root: root.display().to_string(),
            root_exists: true,
            max_depth: 0,
            query: query_spec,
            result_count: ans.rows.len(),
            truncated: false,
            results: ans.rows,
            summary: crate::CodeGraphSummary::default(),
            findings: Vec::new(),
        },
        unresolved: Vec::new(),
        binaries: BTreeMap::new(),
        not_read: Vec::new(),
        index_key: None,
        items: Vec::new(),
        artifact: None,
        count: None,
        headline: ans.headline,
        index: None,
        requirements: Some(report),
    })
}

/// The report: the scanner, then cargo, then the faces, the order, the provenance.
pub fn answer(root: &Path, spec: &ReadingSpec, word: FacesWord) -> crate::Result<RequirementsReport> {
    let target = target_of(spec);
    let scan = manifest::Scan::read(root, &spec.scope, &target)?;
    let mut findings = scan.findings();
    let mut state = scan.state();
    let mut ran = Vec::new();
    let mut unknowns = Vec::new();
    // cargo, where it can speak
    let cargo_root = scan.cargo_root();
    let toolchain = if word.runs_cargo() && (word.reads(Face::Rust) || word.reads(Face::Lock)) {
        cargo::toolchain(root, cargo_root.as_deref().unwrap_or(root), &mut ran)
    } else {
        cargo::toolchain_files(root)
    };
    let meta = match (&cargo_root, word.runs_cargo()) {
        (Some(m), true) => Some(cargo::metadata(root, m, &mut ran)),
        (None, true) => {
            state.metadata = "not run: no manifest at the target".into();
            None
        }
        (_, false) => {
            state.metadata = "not run: no_cargo".into();
            unknowns.push("cargo was not run (no_cargo): every finding is the scanner's, none is cargo's".into());
            None
        }
    };
    let resolved = meta.as_ref().and_then(|m| m.resolved.as_ref());
    if let Some(m) = &meta {
        state.cargo_root = cargo_root.as_ref().map(|p| scan.rel(p));
        state.metadata = m.line();
        findings.extend(m.findings(&scan));
        state.agreement = cargo::agreement(&scan, m, &mut findings);
    }
    let ws = meta.as_ref().and_then(|m| m.workspace());
    let crates = scan.crates_in_scope(ws);
    state.crates = crates.iter().map(|c| c.name.clone()).collect();
    // the requirement side
    let mut needs = spec.requirements.needs.clone();
    if let Some(intent) = &spec.requirements.intent {
        needs.extend(needs::of_intent(intent, &scan, ws, &crates));
    }
    let mut impact = Vec::new();
    let mut reuse = Vec::new();
    if word.reads(Face::Prerequisites) {
        let n = needs::check(&needs, &scan, ws, resolved.is_some());
        findings.extend(n.findings);
        reuse.extend(n.reuse);
        unknowns.extend(n.unknowns);
        impact = needs::impact(&scan, ws, &crates, &needs, spec.requirements.intent.is_some() || !needs.is_empty());
    } else {
        findings.retain(|f| f.face != Face::Prerequisites);
    }
    let mut rust_rows = Vec::new();
    if word.reads(Face::Rust) {
        let r = rust::read(&scan, ws, &crates, &toolchain);
        rust_rows = r.rows;
        findings.extend(r.findings);
    } else {
        findings.retain(|f| f.face != Face::Rust);
    }
    let mut duplicates = Vec::new();
    if word.reads(Face::Lock) {
        if let Some(m) = &cargo_root {
            let l = lock::read(root, m, &scan, ws, &mut ran);
            duplicates = l.duplicates;
            findings.extend(l.findings);
        }
    }
    findings.sort_by(|a, b| (a.check.rule().2, a.severity, &a.krate, &a.id).cmp(&(b.check.rule().2, b.severity, &b.krate, &b.id)));
    findings.dedup_by(|a, b| a.id == b.id);
    unknowns.extend(standing_unknowns(&scan, ws));
    let mut checked: Vec<String> = needs.iter().filter_map(|n| n.member.clone()).collect();
    if checked.is_empty() && !matches!(scan.target, manifest::Target::All) {
        checked = crates.iter().map(|c| c.name.clone()).collect();
    }
    checked.sort();
    checked.dedup();
    let steps = order::steps(&findings, &duplicates, &checked);
    let provenance = Provenance {
        manifests: scan.digests(),
        lock: scan.lock_digest(),
        inputs: scan.digest(&spec.requirements),
        ran,
        toolchain,
        mutation: "none: no manifest, lock or source file was written".into(),
    };
    state.manifests = provenance.manifests.len();
    Ok(RequirementsReport {
        schema: REQUIREMENTS_SCHEMA.into(),
        intent: spec.requirements.intent.clone(),
        target,
        faces: word.names().into_iter().map(str::to_string).collect(),
        needs,
        state,
        findings,
        rust: rust_rows,
        duplicates,
        impact,
        reuse,
        steps,
        unknowns,
        provenance,
    })
}

/// What no requirements reading establishes, said every time, with the crates it applies to.
fn standing_unknowns(scan: &manifest::Scan, ws: Option<&klepto::CargoWorkspace>) -> Vec<String> {
    let mut v = vec!["whether the code compiles: this reading never compiles (cargo check is the next verification, and it is not run here)".to_string()];
    let built: Vec<String> = scan.packages().filter(|m| m.has_build_rs || m.links.is_some()).filter_map(|m| m.name()).collect();
    if !built.is_empty() {
        v.push(format!("build scripts and native libraries are not inspected: {} crate(s) have a build.rs or `links` ({})", built.len(), shorten(&built, 12)));
    }
    v.push("public API, macro and proc-macro expectations between crates: a manifest graph cannot establish them; the compiler does".into());
    if ws.is_none() {
        v.push("cargo's resolve was not read: versions, unified features and the lock's demands are unknown".into());
    }
    v
}

/// The first `n` of a list, then how many more.
pub fn shorten(items: &[String], n: usize) -> String {
    if items.len() <= n {
        items.join(", ")
    } else {
        format!("{}, and {} more", items[..n].join(", "), items.len() - n)
    }
}

/// The report's findings and duplicates as code_passenger.query.v1 rows (what the reading machinery diffs).
fn rows(report: &RequirementsReport) -> Vec<crate::CodeQueryResult> {
    let mut out = Vec::new();
    for f in &report.findings {
        let mut a = BTreeMap::new();
        for (k, v) in [
            ("status", f.status.as_str().to_string()),
            ("severity", f.severity.as_str().to_string()),
            ("basis", f.basis.as_str().to_string()),
            ("face", f.face.as_str().to_string()),
            ("from", f.from.as_str().to_string()),
            ("requirement", f.requirement.clone()),
            ("evidence", f.evidence.clone()),
            ("consequence", f.consequence.clone()),
        ] {
            a.insert(k.to_string(), v);
        }
        if let Some(p) = &f.at {
            a.insert("section".into(), p.section.clone());
            a.insert("field".into(), p.field.clone());
        }
        if let Some(x) = &f.action {
            a.insert("action".into(), x.clone());
        }
        out.push(crate::CodeQueryResult {
            id: f.id.clone(),
            kind: "requirement".into(),
            label: format!("{}: {}", f.check, f.what),
            crate_name: f.krate.clone(),
            path: f.at.as_ref().map(|p| p.file.clone()),
            line: f.at.as_ref().and_then(|p| p.line),
            column: None,
            score_reason: vec![f.basis.as_str().to_string()],
            attributes: a,
        });
    }
    for d in &report.duplicates {
        let mut a = BTreeMap::new();
        a.insert("versions".to_string(), d.versions.iter().map(|v| v.version.clone()).collect::<Vec<_>>().join(", "));
        a.insert("compiles".to_string(), d.compiles.to_string());
        a.insert("collapse".to_string(), d.collapse.clone());
        out.push(crate::CodeQueryResult {
            id: crate::stable_id("duplicate-version", &d.name),
            kind: "duplicate-version".into(),
            label: d.name.clone(),
            crate_name: None,
            path: None,
            line: None,
            column: None,
            score_reason: vec![d.basis.as_str().to_string()],
            attributes: a,
        });
    }
    out
}

/// A path relative to `root`, `/`-separated; absolute when outside it.
pub fn rel(root: &Path, p: &Path) -> String {
    match p.strip_prefix(root) {
        Ok(r) if r.as_os_str().is_empty() => ".".into(),
        Ok(r) => r.to_string_lossy().replace('\\', "/"),
        Err(_) => p.display().to_string(),
    }
}

/// The manifest of a directory.
pub fn manifest_in(dir: &Path) -> PathBuf {
    dir.join("Cargo.toml")
}
