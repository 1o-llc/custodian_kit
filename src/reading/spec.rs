//! A reading spec: one question, declared as a small TOML file under
//! `<project>/readings/specs/<name>.toml`, that `reading run` answers from
//! klepto's facts into `<project>/readings/<name>/<UTC stamp>.md`.

use super::refusal::ReadingRefusal;
use crate::Result;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::Path;

/// Declares a closed vocabulary once: the enum, its spelling in spec files,
/// the one line that says what each word means, `FromStr`, serde both ways,
/// and the markdown table the README carries (a test holds the two equal).
macro_rules! vocabulary {
    ($(#[$meta:meta])* $name:ident, $what:literal { $($variant:ident = $word:literal : $doc:literal,)+ }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name { $($variant,)+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant,)+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $word,)+ }
            }

            pub fn doc(self) -> &'static str {
                match self { $($name::$variant => $doc,)+ }
            }

            /// The README's table of this vocabulary, generated from the enum.
            pub fn doc_table() -> String {
                let mut s = format!("| {} | what it reads |\n|---|---|\n", $what);
                for v in Self::ALL {
                    s.push_str(&format!("| `{}` | {} |\n", v.as_str(), v.doc()));
                }
                s
            }
        }

        impl std::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> std::result::Result<Self, String> {
                match s.trim() {
                    $($word => Ok($name::$variant),)+
                    other => Err(format!(
                        "unknown {} `{other}`; one of: {}",
                        $what,
                        Self::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>().join(", ")
                    )),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.as_str()) }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> { s.serialize_str(self.as_str()) }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

vocabulary! {
    /// What a reading reads.
    ReadingKind, "kind" {
        UseSites = "use_sites" : "every place a path, fn, type, const or macro is used, as klepto resolves it, with its definitions",
        ReadersOfEnv = "readers_of_env" : "every read of an env key (a literal or a constant holding it) by a reader fn or macro, the places that only name it, and the declarations of it",
        Duplicates = "duplicates" : "function bodies repeated across crates (klepto's repeated code), grouped by the crate pairs that share them",
        Dependents = "dependents" : "what depends on a crate (the workspace's dependency graph) or on an item (klepto's item graph), direct then transitive",
        DeclaredIn = "declared_in" : "which declaration files (TOML matching a glob) name the target at a key path",
        Sockets = "sockets" : "a binary's declared sockets, and who dials them: env reads of the socket keys, literals of the path, other declarations naming it",
        Reach = "reach" : "what a change at a crate, a file or an item touches: the items that reach it, their files, crates and binaries, and the declarations naming those binaries",
        Constructs = "constructs" : "every place a type is built: a struct literal, a tuple-struct or variant constructor, or a call of its own associated fn returning Self or the type",
        MatchesOn = "matches_on" : "every match with an arm naming one of an enum's variants: the arms that name them, the variants never named there, whether a catch-all arm takes the rest",
        PublicApi = "public_api" : "a crate's public items and, for each, the uses from other crates",
        TestsCovering = "tests_covering" : "the test functions whose calls reach an item (klepto's item graph), by crate, with the cargo test line that runs exactly them",
        AffectedTests = "affected_tests" : "from a change (files, a git ref's diff, or a job's touched items): the tests whose closure reaches what changed, and the cargo test lines that run exactly them",
        Sensitivity = "sensitivity" : "items by their #[sensitivity] level, with owners, and every use of a more sensitive item from a less sensitive one",
        Diagnostics = "diagnostics" : "every liaise code table (liaise_codes! and #[derive(LiaiseCodes)]): each code's meaning and declaration, and the sites that raise it; a code or a log's text as the target narrows it",
        Bundle = "bundle" : "a crate, a declared set or a file as one annotated bundles file beside the reading, items at secret or above redacted",
        Sweep = "sweep" : "a read-only walk of an undeclared tree (the one walk custodian makes): files by extension, crates, TOML directories, the routes to declare; never in a job",
    }
}

vocabulary! {
    /// What a declared_in key path holds, so kinds can join on it.
    DeclaredRole, "role" {
        Name = "name" : "a name, matched against the target as it is",
        Binary = "binary" : "a binary an entry runs; `reach` and `sockets` find entries by it",
        Env = "env" : "an env key an entry sets; `readers_of_env` joins reads to it",
        Socket = "socket" : "a socket path an entry binds or names; `sockets` reads it",
        SocketEnv = "socket_env" : "the env key that carries a socket path; `sockets` follows its readers",
    }
}

impl ReadingKind {
    /// The kind in code_passenger.query.v1 results this reading writes.
    pub fn query_kind(self) -> &'static str {
        match self {
            ReadingKind::UseSites => "use-site",
            ReadingKind::ReadersOfEnv => "env-read",
            ReadingKind::Duplicates => "duplicate",
            ReadingKind::Dependents => "dependent",
            ReadingKind::DeclaredIn => "declared",
            ReadingKind::Sockets => "socket",
            ReadingKind::Reach => "reach",
            ReadingKind::Constructs => "construct",
            ReadingKind::MatchesOn => "match",
            ReadingKind::PublicApi => "public-item",
            ReadingKind::TestsCovering => "test",
            ReadingKind::AffectedTests => "test",
            ReadingKind::Sensitivity => "sensitive-item",
            ReadingKind::Diagnostics => "code",
            ReadingKind::Bundle => "bundled-file",
            ReadingKind::Sweep => "swept",
        }
    }

    /// Kinds whose answer needs each member's `tests/` read too.
    pub fn reads_tests(self) -> bool {
        matches!(self, ReadingKind::TestsCovering | ReadingKind::AffectedTests)
    }
}

/// One family of declaration files: TOML matching `files`, read at `key`.
///
/// Key paths: `binary` is a field; `binds[]` each item of an array field;
/// `env.*` the keys of a table; `nodes[].answers.*` the keys of a table in
/// every element of an array of tables. A string value is one name, an array
/// of strings several.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredSource {
    /// Globs relative to the project root (`marketplace/*.toml`).
    pub files: Vec<String>,
    /// The key path whose values are the declared names.
    pub key: String,
    #[serde(default = "default_role")]
    pub role: DeclaredRole,
    /// The key path of the entry's own name (`id`); the file stem when unset.
    #[serde(default)]
    pub entry: Option<String>,
}

fn default_role() -> DeclaredRole {
    DeclaredRole::Name
}

/// Which part of the workspace is read. Defaults: every member, minus
/// `target/` and `.claude/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// Path globs (relative to the root) whose `.rs` files are not read,
    /// on top of `**/target/**` and `**/.claude/**`.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Read each member's `tests/` too.
    #[serde(default)]
    pub tests: bool,
    /// Expand the `#[macro_export]` macros of dependencies (klepto reads
    /// their sources when the lockfile resolves offline).
    #[serde(default = "yes")]
    pub dependency_macros: bool,
}

impl Default for Scope {
    fn default() -> Self {
        Self { exclude: Vec::new(), tests: false, dependency_macros: true }
    }
}

fn yes() -> bool {
    true
}

/// What counts as reading an env key (readers_of_env, sockets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Readers {
    /// Functions that read the environment.
    #[serde(default = "default_reader_fns")]
    pub fns: Vec<String>,
    /// Macros that read it at compile time.
    #[serde(default = "default_reader_macros")]
    pub macros: Vec<String>,
    /// Also every function klepto's item graph shows reaching one of `fns`
    /// (`env_or(KEY, ..)` around `std::env::var`).
    #[serde(default = "yes")]
    pub follow: bool,
}

fn default_reader_fns() -> Vec<String> {
    vec!["std::env::var".into(), "std::env::var_os".into()]
}

fn default_reader_macros() -> Vec<String> {
    vec!["env".into(), "option_env".into()]
}

impl Default for Readers {
    fn default() -> Self {
        Self { fns: default_reader_fns(), macros: default_reader_macros(), follow: true }
    }
}

/// duplicates: how alike two bodies must be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateOptions {
    /// klepto's shape level: exact, renamed, literals, types, callees.
    #[serde(default = "default_level")]
    pub level: String,
    /// Bodies with fewer significant tokens are not compared.
    #[serde(default = "default_min_tokens")]
    pub min_tokens: usize,
}

fn default_level() -> String {
    "exact".into()
}

fn default_min_tokens() -> usize {
    16
}

impl Default for DuplicateOptions {
    fn default() -> Self {
        Self { level: default_level(), min_tokens: default_min_tokens() }
    }
}

/// affected_tests: what changed. Files are relative to the root; a git ref
/// is diffed by `git diff --name-only <ref>` run as a declared line before the
/// reading; a job's touched items are the items its readings named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Change {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub git_ref: Option<String>,
    #[serde(default)]
    pub items: Vec<String>,
}

impl Change {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.git_ref.is_none() && self.items.is_empty()
    }
}

/// One reading spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadingSpec {
    /// The folder its readings go to: `readings/<name>/`.
    pub name: String,
    /// The question, one line, in the asker's words.
    pub question: String,
    pub kind: ReadingKind,
    /// A path or name (use_sites), an env key or `PREFIX*` (readers_of_env),
    /// a crate name or directory (dependents, reach), an item path
    /// (dependents), a binary (sockets), a value (declared_in), `*` for the
    /// whole scope (duplicates).
    pub target: String,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub declared_in: Vec<DeclaredSource>,
    #[serde(default)]
    pub readers: Readers,
    #[serde(default)]
    pub duplicates: DuplicateOptions,
    #[serde(default)]
    pub change: Change,
}

impl ReadingSpec {
    pub fn from_toml(text: &str) -> Result<Self> {
        let spec: ReadingSpec = toml::from_str(text).map_err(|e| ReadingRefusal::SpecInvalid.refuse(e.to_string().trim()))?;
        spec.checked()
    }

    /// The same spec written as JSON (the TOML file's equivalent).
    pub fn from_json(text: &str) -> Result<Self> {
        let spec: ReadingSpec = serde_json::from_str(text).map_err(|e| ReadingRefusal::SpecInvalid.refuse(e))?;
        spec.checked()
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| ReadingRefusal::SpecUnreadable.refuse(format!("{}: {e}", path.display())))?;
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            Self::from_json(&text)
        } else {
            Self::from_toml(&text)
        }
    }

    fn checked(mut self) -> Result<Self> {
        if self.kind.reads_tests() {
            self.scope.tests = true;
        }
        if self.target.trim().is_empty() {
            return Err(ReadingRefusal::TargetMissing.refuse(&self.name));
        }
        if self.name.trim().is_empty() || self.name.contains('/') || self.name.contains('\\') || self.name.starts_with('.') {
            return Err(ReadingRefusal::SpecInvalid.refuse(format!("name `{}` must be one folder name", self.name)));
        }
        Ok(self)
    }

    /// The flags of the equivalent code-query, recorded in the receipt.
    pub fn query_args(&self) -> Vec<String> {
        let mut v = vec!["--kind".to_string(), self.kind.query_kind().to_string()];
        match self.kind {
            ReadingKind::UseSites => v.extend(["--item".to_string(), self.target.clone()]),
            _ => v.extend(["--contains".to_string(), self.target.clone()]),
        }
        v
    }
}

/// `PREFIX*` matches by prefix; anything else matches exactly.
pub fn matches_target(target: &str, value: &str) -> bool {
    match target.strip_suffix('*') {
        Some(prefix) => value.starts_with(prefix),
        None => value == target,
    }
}
