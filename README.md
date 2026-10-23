# custodian_kit

custodian_kit reads a Rust workspace as data and answers one question about it
at a time. The facts come from [klepto](../klepto): every file, path, call,
literal, match, attribute and item, with what each path names. A question is a
small declared file, a *reading spec*. Its answer is a *reading*: counts, rows
and the places nothing resolved, written as a dated page and a JSON record.
The `custodian` binary runs readings, keeps their receipts in a journal and
groups them into jobs; this crate is the reading half.

It never edits source or a manifest, and it never guesses. A path klepto
cannot resolve is a row of its own, with its reason; it is not dropped and it
is not counted as a use.

## Quick start

```rust,no_run
use custodian_kit::api::{self, ReadingSpec, Workspace};

let ws = Workspace::open("path/to/workspace".as_ref())?;     // one scan
let spec = ReadingSpec::from_toml(r#"
name = "serve-metrics-use-sites"
question = "Who calls serve_metrics?"
kind = "use_sites"
target = "serve_metrics"
"#)?;
let reading = api::run(&ws, &spec)?;                           // many readings
for c in reading.counts() {
    println!("{}: {}", c.what, c.n);
}
# Ok::<(), api::Refusal>(())
```

`Workspace::open` scans once; `api::run` answers any number of specs over it.
A spec whose scope (tests read, globs left out) differs from the open
workspace's is refused with `CUST0002`; open one for it with
`Workspace::open_for`.

## Documents in 1o

A reading spec, a job file and the paths table are 1o documents, each
declared once as a 1o schema shipped in this crate:

| document | file | schema |
|---|---|---|
| reading spec | `v1.<name>.1o.reading` | `schema/reading/v1.schema.json` |
| job file | `v1.job.1o.job` | `schema/job/v1.schema.json` |
| paths table | `v1.paths.1o.paths` | `schema/paths/v1.schema.json` |

```text
reading {
  name "serve-metrics-use-sites"
  question "Who calls serve_metrics?"
  kind "use_sites"
  target "serve_metrics"
  scope {
    exclude "attempts/**"
    exclude "crates/old/**"
  }
}
```

core-1o (0.2) parses the text and checks it against the schema; the result
reads into the same `ReadingSpec` a TOML spec reads into, so a spec and its
TOML twin are one spec with one digest (the digest is over that form, never the
text). The record is written at the top level; the older `{ reading { ... } }`
reads the same. A string is written with core-1o's escapes (`\\ \" \n \t \r`),
so any text is carried. The schema says which keys repeat (`type: array`: a key
written once per value) and that every other key is written once
(`x-1o-single-keys`), what each vocabulary word means (`enumDescriptions`, shown
by the language server), and, for a change spec, which keys each operation needs
(`x-1o-required-when`). Every spec, change spec, job file and linkage record is
written in 1o; TOML and JSON specs are still read, for one more version.

`custodian init` copies the schemas to `.custodian/config/schema/` and lists
that folder in the project's `oneo.project.json`, which is where core-1o's
workspace, and so language-server-1o, looks: a spec being edited is checked
against its schema as it is typed. The kinds below are the schema's `kind`
enum; a test holds the two equal.

## Reading kinds

| kind | what it means |
|---|---|
| `use_sites` | every place a path, fn, type, const or macro is used, as klepto resolves it, with its definitions |
| `readers_of_env` | every read of an env key (a literal or a constant holding it) by a reader fn or macro, the places that only name it, and the declarations of it |
| `duplicates` | function bodies repeated across crates (klepto's repeated code), grouped by the crate pairs that share them |
| `dependents` | what depends on a crate (the workspace's dependency graph) or on an item (klepto's item graph), direct then transitive |
| `declared_in` | which declaration files (TOML matching a glob) name the target at a key path |
| `sockets` | a binary's declared sockets, and who dials them: env reads of the socket keys, literals of the path, other declarations naming it |
| `reach` | what a change at a crate, a file or an item touches: the items that reach it, their files, crates and binaries, and the declarations naming those binaries |
| `constructs` | every place a type is built: a struct literal, a tuple-struct or variant constructor, or a call of its own associated fn returning Self or the type |
| `matches_on` | every match with an arm naming one of an enum's variants: the arms that name them, the variants never named there, whether a catch-all arm takes the rest |
| `public_api` | a crate's public items and, for each, the uses from other crates |
| `tests_covering` | the test functions whose calls reach an item (klepto's item graph), by crate, with the cargo test line that runs exactly them |
| `affected_tests` | from a change (files, a git ref's diff, or a job's touched items): the tests whose closure reaches what changed, and the cargo test lines that run exactly them |
| `sensitivity` | items by their #[sensitivity] level, with owners, and every use of a more sensitive item from a less sensitive one |
| `diagnostics` | every liaise code table (liaise_codes! and #[derive(LiaiseCodes)]): each code's meaning and declaration, and the sites that raise it; a code or a log's text as the target narrows it |
| `count` | how much there is in a route (the workspace, a crate, a declared set, a module): files, lines, code, comments, blanks per language and per crate by tokei, and the share that is tests by klepto |
| `bundle` | a crate, a declared set or a file as one annotated bundles file beside the reading, items at secret or above redacted |
| `sweep` | a read-only walk of an undeclared tree (the one walk custodian makes): files by extension, crates, TOML directories, the routes to declare; never in a job |
| `impact` | everything a change to a crate, a file or an item entails, in one reading: its definitions, use sites, constructions, matches, impls, dependents, reach, the tests that cover it, the features and feature configurations that build it, the linkage records naming it, and the cost of the change by dimension, each measured, heuristic or unknown |
| `linkage` | the architectural linkage records (why declarations are bound together) naming the target, `*` for all: each bound declaration resolved through klepto, stale when it no longer resolves, to review when its file changed since the record was made; comment anchors with no record behind them |
| `patterns` | every function body in scope (`*`, or crates by name) against the simple_impl toolkit: its shape, the toolkit pieces whose expansion it holds (exact: the call expands to the same tokens; near: the difference named) with the replace_body spec that swaps them, the shapes that repeat across crates and no piece writes (ranked by count times size, each with a body and a proposed macro), and the counts by crate |
| `requirements` | what an intended change needs from the workspace as it is, without compiling: the manifests read with no cargo (each crate's governing workspace root by cargo's rule, `workspace = true` against it, paths, renames, features in their four forms), then the intent's or the change spec's needs against cargo metadata; each crate's rust-version and edition against the lock's demands and the installed toolchain; the packages the lock holds in more than one version (cargo tree -d); every finding observed, inferred or unknown, with an order of steps, required or recommended |

### The toolkit's shapes (`patterns`)

The `patterns` kind matches every function body against the simple_impl 0.1.12 toolkit. Its catalog,
`schema/toolkit/simple_impl-0.1.12.shapes.json`, is one call of each piece on placeholder arguments (written by the
change vocabulary's own renderer), expanded by `cargo expand` as rustc does it, or by klepto where the expansion
holds a std macro (`vec!`, `format!`, `write!`, `matches!`) that hand-written code keeps as the macro. A statement
matches a piece when its tokens are the expansion's with the holes filled, and the filling put back into the
expansion reads the same in the normal form (`toolkit::normal`): exact. A near match names what differs (a
`return` without its `;`, a builder line's `#[inline]`, `a != b` for the macro's `!(a == b)`). The catalog is made
again by `cargo run --example toolkit_shapes -- <new dir>`, and `toolkit::tests` holds the shipped one equal to a
fresh expansion. Each function row carries the replace_body spec (`spec`, JSON) that code_passenger's `to-toolkit`
makes; the shapes no piece writes come with a proposed macro for the suite.

### What a change needs (`requirements`)

An agent that integrates a crate without reading the manifests first learns what is missing one cargo error at a
time: a `workspace = true` with no entry in the root, a path that points nowhere, a feature the dependency does not
have. Six of those in a row, each one a round trip through a person, is what this reading is for. It reads the
manifests before anything is changed and says in one report what the intended change needs and whether each is there.

It never compiles and never writes a manifest, a lock or a source file. The cargo lines it runs are
`cargo metadata --frozen` (no lock written, no network; `--no-deps` when that is refused) and
`cargo tree --duplicates --locked --offline`, and the toolchain's `rustc -vV` and `rustup show active-toolchain`, each
a declared line run in the workspace's directory and listed in the reading's provenance with its exit. Where cargo
metadata reads the workspace, cargo is the truth: nothing here resolves a version or recomputes cargo's resolve
(klepto 0.3.5's `CargoWorkspace` reads its JSON as it is). The manifest scanner is for the state cargo cannot speak in
yet, a broken manifest, and every finding says which of the two it came from and whether it is observed, inferred or
unknown.

- **prerequisites**: every Cargo.toml the target needs, read as TOML with toml_edit (each key's line from its span).
  A crate's governing root is found by cargo's own rule: the crate itself when it has `[workspace]`, else the root
  `package.workspace` names, else the nearest ancestor with `[workspace]` that does not exclude it. Never the top
  folder by default, so a nested independent workspace is its own root. Against that root: `workspace = true`
  dependencies, `field.workspace = true`, `[lints] workspace = true`, members that name no crate, a crate its root
  does not hold. Path dependencies are followed to the end of the chain (the package there must have the
  dependency's name, at a version its requirement admits). Dev-dependencies marked optional, renames, target tables,
  named registries no config declares. Every `[features]` entry in its four forms (`f`, `dep:x`, `x/f`, `x?/f`)
  through klepto's feature model. Requirements spread across members and dependencies no `.rs` file names are
  informational. Then the requirement side: each need checked against cargo's resolve (declared in the crate it
  lands in, the version the lock holds, the feature on and what turns it on, the source), its impact (the crate's
  dependencies and dependents in the resolve, and the `impact` and `affected_tests` lines that follow it item by
  item), and what can be reused (the root's entry, the version already locked and who uses it).
- **rust**: per crate its rust-version and edition, the highest rust-version a package of its lock demands (its
  closure in cargo's resolve), the toolchain installed for the workspace, and the three disagreements: a crate
  promising an older Rust than its lock allows, a lock newer than the installed toolchain, an edition its declared
  minimum cannot compile (the last also without cargo).
- **lock**: every package the lock holds in more than one version, from `cargo tree -d` as cargo prints it: the
  packages whose requirement pulls each version in (with the requirement and the line it is written on), how many
  times it compiles because of the split, and the one requirement change that would leave one version when there is
  one (inferred: cargo tree after the change is the proof).

```toml
name = "add-hex"
question = "what does adding hex to one_db_kernel need"
kind = "requirements"
target = "crate:one_db_kernel"      # `.` reads every workspace under the root; a directory reads the one there

[requirements]
intent = "add hex/serde to one_db_kernel"
faces = ["prerequisites", "rust", "lock"]   # none is all three; `no_cargo` reads the manifests only

[[requirements.needs]]                      # needs written out; the intent's words and a change spec make them too
member = "one_db_kernel"
dependency = "hex"
version = "0.4"
features = ["serde"]
```

The intent's words become needs: the crate it lands in (`to app`, `crate:app`), and the packages it names that a
manifest or the lock already holds (`hex`, `hex@0.4`, `hex/serde`); a name spelled as code that nothing holds is a
need of its own, unknown. A change spec gives its `add_dependency`, `change_dependency` and `declare_feature`
operations as needs (`reading::requirements::needs::of_change`). The faces are one big-bad-bool word, checked by
flaga_solver: the lock's shape is cargo's answer, so `lock` cannot go with `no_cargo`.

The report (`custodian.requirements.v1`, the reading record's `requirements`) has the brief's sections: the requested
outcome and scope; the current state (each root, its members and tables, cargo's word, where the scanner and cargo
agree or part); the satisfied prerequisites; the unsatisfied ones, each with its severity, manifest, line, section,
field, requirement, evidence, consequence and action; compatibility (the Rust table, the lock's shape); impact and
reuse; the suggested order, each step required or recommended with how to see it done, and never a version the
reading did not read; the unknowns; the provenance (every manifest's digest, the lock's, the lines run with their
exits, and the mutation: none).

Its reuse key's code half is the digest of the manifests, the locks, the toolchain files, the `.cargo/config.toml`s
and the spec's requirements (`requirements::input_digest`): an edit to a source file is the same reading, an edit to
a manifest or the lock is a new one. It is not a gate: a preflight of a dependency or feature change names the line
to run; `plan make` reads it for the plan's intent and folds it in (a dependency to declare is a step of its own,
`add_dependency`, first; the other required steps are listed before step 1; what can be reused is reuse); a handoff
carries the newest one's unresolved findings.

What it cannot claim: that the code compiles (`cargo check` is named as the next verification, and not run); what
proc macros, build scripts, native libraries and the API between crates expect; any feature combination besides the
one cargo resolved; a registry package that is not in the lock (it reads offline); a toolchain installed after a
kept reading (the key reads files; `--again` reads again).

| face | what it means |
|---|---|
| `prerequisites` | the manifests and workspaces a change needs, read without cargo, then what the intent or the spec requires against cargo metadata |
| `rust` | each crate's rust-version and edition against the lock's demands and the installed toolchain |
| `lock` | the packages the lock holds in more than one version, from cargo tree -d |

| basis | what it means |
|---|---|
| `observed` | read as it is: a file's text, or cargo's own output |
| `inferred` | follows from what was read by a stated rule (cargo's documented behaviour, a semver query, a text search); cargo has not confirmed it |
| `unknown` | not established by this reading; the finding names what would establish it |

| status | what it means |
|---|---|
| `satisfied` | present, with its evidence |
| `unsatisfied` | missing or broken: a step in the order fixes it |
| `informational` | neither: a fact the change should know |

| severity | what it means |
|---|---|
| `error` | cargo refuses it, or the intended change cannot work without it |
| `warn` | cargo accepts it, but a promise is false or something is ignored |
| `info` | worth knowing; not presumed a mistake |

| read from | what it means |
|---|---|
| `manifest` | the manifest scanner: Cargo.toml read as TOML, no cargo |
| `metadata` | cargo metadata --frozen (or --no-deps), as cargo printed it |
| `tree` | cargo tree --duplicates --locked --offline, as cargo printed it |
| `toolchain` | rustc -vV and rustup show active-toolchain, run in the workspace root |
| `lockfile` | Cargo.lock read as TOML |
| `intent` | the intent's words, the change spec or the reading spec's needs |
| `sources` | a text search of a crate's .rs files |

| check | what it means |
|---|---|
| `manifest_unreadable` | a Cargo.toml that is not TOML: cargo refuses everything that reads it |
| `not_a_member` | the crate's governing root (the nearest ancestor with [workspace] that does not exclude it, as cargo finds it) does not list it: cargo refuses it ("believes it's in a workspace when it's not") |
| `member_missing` | a [workspace] members entry that names a directory with no Cargo.toml |
| `workspace_not_a_root` | package.workspace points at a manifest with no [workspace] |
| `duplicate_package_name` | two crates of one workspace with one package name |
| `workspace_dependency_missing` | a `workspace = true` dependency whose governing root has no [workspace.dependencies] entry of that name (or no root at all) |
| `workspace_field_missing` | a [package] field inherited with `workspace = true` that the root's [workspace.package] does not set |
| `workspace_lints_missing` | [lints] workspace = true with no [workspace.lints] in the root |
| `inherited_key_ignored` | a key beside `workspace = true` that cargo ignores (version, path, git, ...): the root's entry decides |
| `path_missing` | a path dependency (or patch) whose directory has no Cargo.toml |
| `path_names_other_package` | a path dependency whose directory holds a package of another name |
| `path_version_mismatch` | a path dependency's version requirement that the package at the path does not meet: cargo refuses it, and which side moves is a choice |
| `optional_dev_dependency` | a dev-dependency marked optional: cargo refuses it |
| `registry_unconfigured` | a dependency on a named registry no .cargo/config.toml at or above the root (or in CARGO_HOME) declares |
| `feature_entry_unknown` | a [features] entry naming no feature and no dependency, `dep:x` on a dependency that is not optional, or `x/f` on no dependency |
| `renamed_dependency` | a dependency whose key is not its package (`package = ...`): source names it by the key |
| `requirement_spread` | one package required with different requirements across the workspace |
| `possibly_unused` | a dependency whose name no .rs file of the crate holds as a word (a text search: macros, build scripts and lib names that differ hide uses) |
| `workspace_dependency_unused` | a [workspace.dependencies] entry no member inherits |
| `outside_any_workspace` | manifests under the root that no workspace holds and nothing depends on |
| `metadata_read` | cargo metadata read the workspace: cargo's resolve is the truth from here |
| `metadata_refused` | cargo metadata refused the workspace: its words, as it printed them |
| `scanner_and_cargo_disagree` | the scanner and cargo part: cargo's word stands, and the scanner's limit is named |
| `need_declared` | a dependency the change needs, declared in the crate it lands in or not |
| `need_version` | the version the change asks for against the versions the lock holds |
| `need_feature` | a feature the change needs on a dependency: whether the dependency has it, and what turns it on |
| `need_source` | where a needed dependency comes from: a path, the lock, or a registry this reading does not fetch |
| `need_new_crate` | a new crate's directory: the root that would govern it, whether its members take it, its name free |
| `need_unnamed` | a name in the intent that nothing in the manifests or the lock holds |
| `feature_combinations` | the feature combinations no resolve read and no build ran |
| `edition_needs_newer_rust` | a rust-version older than the first Rust that compiles the crate's edition |
| `rust_version_below_lock` | a crate that promises a rust-version older than a package of its lock demands |
| `rust_version_undeclared` | a crate that promises no rust-version, with what its lock demands |
| `toolchain_below_lock` | a lock that demands a newer Rust than the toolchain installed for the workspace |
| `toolchain_unknown` | the installed toolchain could not be read |
| `duplicate_versions` | a package the lock holds in more than one version |
| `tree_refused` | cargo tree refused: the lock's shape is read from Cargo.lock's text, its forcing requirements unknown |

## Declaration roles

A `[[declared_in]]` table reads TOML files at a key path. Its `role` says what
the value is, so the kinds that join code to declarations know where to look.

| role | what it means |
|---|---|
| `name` | a name, matched against the target as it is |
| `binary` | a binary an entry runs; `reach` and `sockets` find entries by it |
| `env` | an env key an entry sets; `readers_of_env` joins reads to it |
| `socket` | a socket path an entry binds or names; `sockets` reads it |
| `socket_env` | the env key that carries a socket path; `sockets` follows its readers |

```toml
[[declared_in]]
files = ["decl:marketplace"]   # a declared route; a glob in free mode
key = "env.*"                  # a key path: dotted, `*` for every key at a level
role = "env"
entry = "id"                   # the field naming the entry each value sits in
```

Both tables are generated from the enums (`cargo run --example vocabulary`),
and a test fails when this README and the enums differ.

## Jobs are task histories

A job writes its history in `task_history`'s format (`nodeplan.1o.diff.v1`)
under `.custodian/data/history/<job name>/<revision>/`, one revision per run
(`base`, `fix1`, ...): the task and intent at `job start`, a solution per
step, a change record for every file that differs between the job's open
and close counts (path, operation, the digests before and after), a
verification per test or check, and the result at `job close` (status, and
the code's index key as the tree digest), then the history's index.
`job brief` reads them back through `load_task_history`. The receipts and
the journal stay beside them.

## Job templates

A template job (`kind` names a template) is a list of steps, each a reading.
`jobs::template(root, kind)` loads one as the project has it
(`config/jobs/<kind>.toml`, written by `agent init` for the user to edit),
else as shipped; `jobs::templates(root)` loads every kind the project knows.
A step reads an earlier step in one of three ways, and waits for it on
`scope` in each: `for_each` (one reading per value a row of that step's
readings holds), `change_items` (those values as the change), or `reads`
(each receipt file that step kept is this step's target, or fills
`{receipt}` in its `target`). A step whose earlier step gave it nothing is
skipped, and the step-input decision says so.

## Counts

`count` counts a route with tokei, as a library: files, lines, code,
comments and blanks per language, per crate. tokei walks inside the route's
own root (the workspace, a crate's directory) with the paths table's `ignore`
set and the spec's excludes; a declared set and a module are the files they
name. The share that is tests is klepto's, as the bundle filters read it:
`tests/`, `benches/`, test-only modules and `#[cfg(test)]` items.

A count's digest is over its counts sorted by crate then language, with no
time inside, so the same tree gives the same digest; its header carries the
index key and the git commit. `count::compare` says what moved between two
counts, file by file (lines added and removed per crate and language, files
added, removed and changed, the tests' share before and after), with its own
digest. `count::history` lists a route's counts from the journal.

A job whose template says `count = true` (triage and features do) is counted
at open, after each step that changes code, and at close; its closing
receipt lists the lines touched per step and for the job, and
`jobs::lines_within` sums them over a set of jobs. In a git repository the
job's own figure is git's, exact: `git diff --numstat` against the commit the
job opened at, new files counted whole, cargo's lock file aside
(`job: +55 −37 across 10 files in 9 crates (git diff --numstat 2d95b71c...)`);
a step's figure is the counts' and says `net`.

## Scope depth

`features` judges each dependency, and `reach` reads each change, with the
same scope: the crate's items that use it directly (klepto's dependency use
sites), every item that reaches those, and every item nested in one, to a
fixpoint. Each file and module is wholly behind it or partly; the units are
the widest wholes, and the depth is the widest unit. The *unavailable list*
says, with it off, which items go, which files and modules are wholly gated,
and which callers in other crates lose which items: the lines a gate's header
carries (`Unavailable::header`).

| depth | what it means |
|---|---|
| `item` | one item (a type, a const, an impl) uses it; the rest of its module does not |
| `function` | one function uses it, or calls one that does; the rest of its module does not |
| `module` | every item of an inline module is behind it; the file holding the module is not |
| `file` | every item of a whole file is behind it: the `mod` line itself can be gated |
| `crate` | every item of the crate is behind it: turning it off leaves nothing |

## Routes

Every path a spec names goes through the declared paths table,
`.custodian/config/paths.toml`, read through pathgen:

- `crate:<name>` and `crate:<name>/src/x.rs`: a workspace member, as cargo names it
- `decl:<name>`: a declared set of files (globs listed under `[decl]`)
- `custodian:<part>`: a part of the initialized folder (readings, jobs, journal)
- `path:<raw>`: a raw path, in free mode only

Inside a job only declared routes resolve; a raw or walked path is refused
(`CUST0023`). The one walk custodian makes is the `sweep` kind, which lists an
undeclared tree and offers the routes to declare; a job cannot run it.

## What a reading keeps

A reading is a `custodian.reading.v1` record holding the
`code_passenger.query.v1` receipt of its rows. The journal (SQLite, under
`.custodian/`) keeps one `custodian.receipt.v1` row per run:

| field | what it is |
|---|---|
| `seq`, `utc` | the run's number in the journal and when it ran |
| `command`, `name`, `spec` | what ran: `reading`, `test`, `check`, `git`, `gate`, `job-close`, and its spec |
| `spec_hash` | the digest of the spec's canonical form |
| `code_digest` | the digest of the canonical form of the files the scan read (klepto's fingerprint) |
| `index_key` | the registry's index key of the reading |
| `plan_id`, `plan_digest`, `operation_digest`, `receipt_digest` | the xccute plan, operation and receipt the line ran under |
| `line`, `exit_code`, `accepted`, `disposition`, `reason` | the line as it ran, and what came of it |
| `outputs`, `job`, `reused_seq`, `millis` | the files written, the job, the run reused when the same search had already answered |

Two runs with the same `spec_hash` and `code_digest` are the same search: the
second reuses the first's reading and says so. A reading is stored once: its
folder's `index.json` names the latest and every one, by stem; nothing is
copied to a `latest` file.

The parsed index is held once per code digest. A scan in a project with a
`.custodian` folder keeps klepto's model (its serde form, as CBOR) under
`.custodian/state/index/<digest>/model.cbor`, and the next scan of the same
code (the same klepto, the same scan options) reads it back instead of
parsing; the newest two are kept. The model holds no syntax trees, so `count`
and `bundle`, which walk them, parse. A reading's record and its receipt's
`reason` say where its index came from: `parsed (35901 ms)`,
`from cache (3314 ms)`, `from daemon` (custodian's resident). On the kit's
workspace: parsed in 35.9 s at 5.7 GB, read back in 3.3 s at 1.8 GB. Every digest is
`stable_digest`'s sha256 over a declared canonical form, written `sha256:<hex>`.

## Change specs and the preflight

A change spec (1o modifier `change`, `v1.<name>.1o.change`; a TOML spec is still read,
in `data/specs/changes/`) asks for one operation on one target, with its
objective, boundaries (the files it may write), the call sites a signature
change passes, the questions it leaves, and whether existing uses must keep
compiling (`compatibility`: `preserve` or `break`). `change::ChangeSpec`
reads both forms; `digest()` is the stable_digest the preflight and the
transform are bound to.

| operation | what it does |
|---|---|
| `make_struct` | a new struct in a module (target: crate:<name> or crate:<name>/src/<file>.rs): name, fields with types and visibility, derives, attributes, docs |
| `make_enum` | a new enum in a module: name, variants (unit, tuple or named fields, discriminants), derives, attributes, docs |
| `make_impl` | a new inherent impl block for a type (target: the type), with its functions |
| `add_method` | a function added to a type's impl (target: the type): its signature and body, or a toolkit method generator |
| `make_fn` | a free function in a module: its signature and body |
| `add_field` | a field added to a struct (target: the struct); with a default, every struct literal gets it too |
| `add_variant` | a variant added to an enum (target: the enum); the matches without a catch-all arm are listed, never edited |
| `add_derive` | derives added to a type (target: the type), merged into its #[derive(..)] |
| `add_attribute` | one attribute line placed on an item (target: the item) |
| `add_module` | a new module: its own file and its `mod` line in the parent (target: the parent route) |
| `add_dependency` | a dependency line in a crate's Cargo.toml (target: crate:<name>); `optional = true` makes it optional, and the lock file cargo writes is written with it |
| `add_test` | a test function (target: the function it proves): in the file's tests module (`to`: another inline test module of the file, by name; one the file does not have is made, `#[cfg(test)]`), its body from toolkit pieces |
| `add_constructor` | a constructor for a type (target: the type): `Self { field: param, .. }`, or a toolkit constructor generator |
| `add_trait_impl` | `impl Trait for Type` (target: the type): a toolkit impl generator, or the trait's functions |
| `change_signature` | a function's signature changed (target: the function): a new parameter with the expression its call sites pass (inside a macro's arguments too; refused while a call site is unresolved, unless `call_sites.allow_unresolved`), a new name, a new return type; with `function.body`, its body written in the same step (a body that compiles only against the new signature) |
| `rename` | an item renamed (target: the item) at its declaration and every use klepto resolves to it |
| `move` | an item moved to another module (target: the item, `to`: the module route), re-exported where it was so its paths still resolve |
| `extract` | items moved out of a file into a new module of their own (`items`, `module`), re-exported where they were |
| `gate` | an item put behind a crate feature the crate declares (target: the item, `feature`); an item already gated by it is refused. Code that compiles only with the feature on gets its gate as it is made, in the same step: `#[cfg(feature = "..")]` in the function's `attrs` (add_method, make_fn, add_test) |
| `anchor` | a linkage record's comment anchor placed above an item (target: the item, `linkage`: the record's name) |
| `replace_body` | a function's body replaced (target: the function, `function`: its name and the new body as toolkit pieces); its signature, docs and attributes kept, so its callers are untouched; with a toolkit method generator (`function.generator`) the whole function is written as the call, its signature, docs and attributes inside it |
| `change_dependency` | a dependency a crate already names, changed (target: crate:<name>, or `*`: every crate of the workspace naming it; `dependency`, or several in `dependencies` so a family moves together): its source becomes what the spec names (a version alone is the registry's, the path dropped; a path, with or without a version), features given replace its features; `optional` true or false, alone (its source kept) or with the rest |
| `inline` | a function's body put in place of every call to it (target: a free function, or an associated one without self): each call becomes a block that binds the parameters to the call's arguments with `let`, then the body; the function is removed when it is not `pub` and every use of it was a call |
| `wrap` | a function's body wrapped (target: the function, `function`: its name and the pieces that run first): the pieces, then the body as it was, as one block; its signature, docs and attributes kept, so its callers are untouched |
| `make_lib` | a crate's library root, src/lib.rs, made beside its binary (target: crate:<name>, a crate with no library; `module.doc`: the file's inner doc): cargo builds it as the crate's library, and the binary reaches it by the crate's name |
| `declare_feature` | a crate feature declared in its Cargo.toml (target: crate:<name>, `feature`: the name, `enables`: what it turns on, each `dep:<name>` (an optional dependency), `<dependency>/<feature>` or another feature of the crate); a feature the crate already declares gets the list given |

A function body is never free text the tool trusts. It is a list of pieces,
each one of simple_impl 0.1.12's toolkit macros (130 of them, the table is
`change::PIECES` and a test holds it equal to the published registry), or
`let`, `value`, or `raw`. A `raw` piece is allowed and the preflight flags it.
A whole function can come from a toolkit generator instead.

`preflight::run_preflight` decides a change spec over the code as it is,
before anything is written: where it lands (`change::landing`), its impact
(the `impact` reading), its preconditions (the target resolves, the name is
free, no TODO is left in the spec), the files it would write against the
files it may write (one outside is a scope expansion asked for, never
taken), and the decisions it leaves open. The answer is a big-bad-bool word
whose flaga rules authorize it or name what is missing. A bounded job needs
its target and its outcome known. `preflight::gate` lets a code-changing step
run only on a preflight of this spec's digest over this code's digest.

`preflight::run_preflight_batch` preflights many specs over one reading of the
code: one scan, one code digest, one code graph for the batch, each member's
impact answered from it and each member checked by the same rules as its own
preflight, so each gets the verdict its own preflight gives. One receipt
(`preflight-batch`) lists every member and its verdict; `preflight::gate_all`
takes it as each member's preflight.

## Time, the battery, the kept build

`step_time` reads what custodian's own steps cost from the receipts (a
preflight, a transform, a batch's compile, a test line, a reading, an index
parse): a job's planned cost before it runs, each figure measured here or
unknown; a job's `hours=<n>,battery=<percent>` budget; the step kinds a job
paid one member at a time where a batch form exists. `power` is James's
battery rule: under 20% and not charging, no cargo line starts (`CUST0042`).
`proof_place` is the one copy and kept build per workspace that every proof
and every test run uses.

## Decisions, plans, linkage

A question is asked once. `questions::ask` writes it as a task_history
`problem` record (its id the digest of the question), `questions::answer` as
a `solution` record with the same id, the answer and the reason, and
`questions::open` leaves out any question answered in any task history of
the project.

`plan::run_plan` reads an intent against the code: the candidates where the
work is, what can be reused, what is new, the top candidates' impact and the
cost, alternatives only where the facts give one, the decisions asked
together, and the steps. `plan::approve` writes a step as a change spec,
with TODO placeholders the preflight refuses until they are filled in.

A linkage record says why declarations are bound together. Each bind is
resolved when it is recorded and at every check: `holding`, `review` (a
bound file changed since) or `stale` (a bind no longer resolves). A
`// linkage: <name>` comment above an item is an anchor; one no record backs
is a finding.

| kind | what it records |
|---|---|
| `responsibility` | a module uses another crate or module because that one owns the responsibility |
| `invariant` | an implementation depends on an invariant another declaration keeps |
| `connected` | two types are connected for an architectural reason |
| `compatibility` | a feature or a shape exists to satisfy a compatibility requirement |
| `synchronized` | the declarations must stay synchronized with each other |
| `guarded` | a declaration must not be changed without considering the others |
| `semantics` | a dependency must not be replaced casually: its semantics are required |

## The exercise

`exercise` is a job kind that tries custodian's own change path on the
project's code, every result a receipt. For each operation of the change
vocabulary and each kind of unit it applies to (`exercise::units_of`: item,
function, impl, module, file, crate), N targets are picked from the index by a
seed, and each pick is a case run on a copy of the workspace in the job's data
folder (the project's tree is never written): the smallest change spec the
operation can make there, its preflight, code_passenger's `transform --write`,
`cargo check` over the copy (and the landing crate's tests with `tests`), and
the closing check that every touched file is covered by the transform record.
Every refusal, crash, compile error, failed test or uncovered file is a finding
(`exercise::FindingCode`) and a diagnosis record of the job's task naming its
receipt. With an intents file, each line goes to a cheap model's declared line
(`claude -p --model claude-haiku-4-5-20251001`) for a spec that runs the same
chain; no model on the machine is `not run: no model`. The options are one
big-bad-bool word (`exercise::ExerciseWord`, its rule `model => intents`
checked by flaga_solver); the cases run as tasks of one `scope`, numbered by a
uh-tom-mick counter; the copy's counts are tree_history's. The summary
(`data/exercise/summary.json` and `.md`) has the cases run and passed, the
findings by code, the operation × unit matrix, the slowest cases and what to fix
first; a `check` receipt named `exercise` is the job's verification.

| unit | what it means |
|---|---|
| `item` | a module-level declaration that is not a function: a struct, an enum, a trait, a const, a static, a type alias |
| `function` | a free function, or a method of an inherent impl |
| `impl` | a type with an inherent impl block (what a new method or constructor lands inside) |
| `module` | a module with a file of its own, as its route (crate:<name>/src/<file>.rs) |
| `file` | a crate's root file, as its route (crate:<name>/src/lib.rs) |
| `crate` | a crate of the workspace, as its route (crate:<name>) |

| finding | what it means |
|---|---|
| `spec-refused` | the change spec written for the case is refused by the spec's own checks |
| `preflight-refused` | the preflight did not authorize the change |
| `preflight-failed` | the preflight gave a refusal instead of a verdict |
| `gate-refused` | the transform's gate found no authorizing preflight of this spec over this code |
| `transform-refused` | code_passenger refused the spec (exit 2) |
| `transform-unproved` | code_passenger did not prove the change: a configuration did not compile, or a postcondition did not hold (exit 1) |
| `transform-crashed` | code_passenger panicked, was killed, or exited with a status it does not declare |
| `compile-error` | cargo check fails on the copy after the transform wrote it |
| `test-failure` | the landing crate's tests fail after the transform wrote it |
| `uncovered` | a file the case touched is not carried by its transform record from its digest before to its digest after |
| `model-miss` | the cheap model's answer is not a change spec, or names a target that does not resolve |

## Bounded and unbounded jobs

`jobs::JobMode`: a bounded job's touched code lines (Rust, and each crate's
Cargo.toml, which `add_dependency`, `change_dependency` and `declare_feature`
write) must each be covered by one of its transform records at close, else
the close is refused (CUST0031) and the result is `rejected`; an unbounded job
is help and nothing is refused. Any other file a bounded job touches by hand
(a features document, a test's expected output) is one no operation writes
(CUST0041, below). A close gives one refusal: CUST0031 when code was written by
hand, naming the other files too; CUST0041 when only other files were. Every job records at open whether its target is known (it
resolves) and whether its outcome is known (its `verify` lines:
`test:<name>`, or `test:<module>::*` for every test of a module,
`reading:<spec>`, `check:<which>`), and the closing receipt
says both as they were set at open and the receipt at which each became true.
`accepted` needs every verify line satisfied by the job's own runs; with none
the result is `pending`.

A job opens at a commit (`base_commit`, `git rev-parse HEAD` at `job start`).
A test the job ran that fails the same way at that base is not the job's
failure (`baseline`): at close each failing test is run again on the job's
tree for its first failing line (the panic's message, the trees' own paths
written `<root>`), and looked up at the base: in the baseline `job start`
kept when the job file says `baseline true` (the tests the target reaches,
run at open, `data/baseline.json`), else on a worktree of the repository at
the base commit, outside the project. The same first failing line there makes
it `pre-existing`: the closing receipt names it with that line and the job
can still be `accepted`. One that passes at the base, fails there another
way, or passes when run again, is the job's (`failing`, with why).

The closing receipt lists every file the job touched since its base (git's:
changed against the base commit and new files, a file that already differed
at open counted from its state then), each with what carries it: a
`transform #<seq>`, `allow_file <glob>`, `cargo` (the lock file), or `hand`.
In a bounded job a file no operation writes (a test's expected output, a
fixture) must be named by an `allow_file` glob in the job file
(`jobs::declare_allow`, `custodian job allow GLOB`), else the close is
refused (CUST0041) and `rejected`.
The task states (`tasks::TaskState`: proposed, ready, running, blocked,
paused, verifying, completed, failed) are derived from the records.

## The histories

What a project keeps over time is five crates of their own, and this crate calls them where it held the code
itself; each is re-exported where it was, so nothing moved out of this crate's API:

| crate | what | here |
|---|---|---|
| [receipt_history](https://crates.io/crates/receipt_history) | the receipts journal: the receipt, the journal's tables, its sequence, the rebuild from the receipt files, the digests | `store::Journal`, `receipts::Receipt`, `digest` |
| [revision_history](https://crates.io/crates/revision_history) | a job's revisions (`base`, `fix1`, ...): the records a run writes, read back, the chain compared | `task_records` |
| [tree_history](https://crates.io/crates/tree_history) | the count model, two counts compared, a route's counts over commits; and the tree itself, as a declared `tree` line | `reading::count` |
| [change_history](https://crates.io/crates/change_history) | the lines each job touched, summed across jobs; the change records two counts give | `jobs::Touched`, `jobs::lines_touched_of`, `task_records::changes` |
| [diff_history](https://crates.io/crates/diff_history) | two readings of one question compared | `diff` |

[differences_kit](https://crates.io/crates/differences_kit) composes them, one call per kind, and the
[differences](https://crates.io/crates/differences) command line shows any of them side by side.

## After a job: handoff, why, cost

Three modules read the records and write nothing but a cost record.

- `handoff`: `job_folder(store, name)` finds a job (the one closed last by default), `of_job(root, store, folder,
  receipts, since)` reads its job file, closing receipt, receipts, task history, transform records and count diff
  into a `Handoff`, and `render(&h, Reader::James | Reader::Agent)` writes it in a fixed order: what waits on a
  person (open decisions, change specs a preflight refused, verifications not satisfied, a refused close), what
  it was for, what changed (lines per crate, the change specs and their operations), what was decided and by
  whom, what was verified (tests passed and failed, which failures are pre-existing: the same test failed in a
  closed job's run before this job's first transform), and what runs next. `Reader::Agent` adds the exact lines
  and the routes. Lists stop at `SHOWN` (5), next steps at `NEXT_SHOWN` (8), the text at `MAX_LINES` (60).
- `why`: `of(root, store, route, readings)` for a `crate:` or `decl:` route: the task revisions whose change
  records touch its files, the linkage records binding it, the readings naming it. `exec::git_log_paths` is the
  declared `git log` line for its first and last commit; the caller runs it with a receipt and hands its text to
  `commits_of` and `ends`. `render` says "no record" for an empty section.
- `cost`: a reading's cost is the files its answer names (each row's file, else a crate row's `Cargo.toml`), their
  bytes when the reading is made, and tokens estimated as bytes / 4 (`BYTES_PER_TOKEN`; no tokenizer runs).
  `read_record` and `reuse_record` make the record of a reading and of a reuse (the reuse's `saved` is the reading's
  `cost`); `keep` writes it as `data/costs/<seq>.json` (`store::named::COST`) under the receipt's number, and the
  reading runner keeps one for every reading and reuse it runs. `sum` adds them per reading kind and per job;
  `brief_line` is `tokens: ~N read, ~M saved by reuse (estimate: bytes/4 of the files each reading touched)`.

`receipts::is_reading_run` is the one rule for which receipts are reading runs, whatever word an older custodian
wrote (`reading`; `run`; `count`, `diagnostics`, `bundle`, `sweep` with a spec's hash).

## Refusals

A refusal is a liaise code (`CUST0001`…`CUST0039`, `ReadingRefusal`) and one
sentence. `api::Refusal` carries both. A history crate's refusal arrives as the
kit's own code where the kit refused the same thing before the code moved (the
journal `CUST0012`, a write `CUST0009`, a reading that cannot be read back
`CUST0017`, a missing input `CUST0021`), else with the history crate's code.

## Releasing

Bump `version` in Cargo.toml and give CHANGELOG.md a dated `## <version> (YYYY-MM-DD)` entry, then push to master.
CI (`.github/workflows/ci.yml`) tests on stable and Rust 1.89, runs clippy, and publishes to crates.io
through trusted publishing; a version already on crates.io, or an entry still marked `(unreleased)`, publishes nothing.

## Ownership

Developed and published by James Hunt. The repository is held by the 1o organization
(https://github.com/1o-llc). Assignment of the copyright to 1o is pending documentation. The license texts
(LICENSE-MIT, LICENSE-APACHE) are left as they were found. Every file new in 0.2.0 carries the notice
Copyright © 2026 1o. All rights reserved.

## License

MIT OR Apache-2.0
