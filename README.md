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
{
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
}
```

core-1o parses the text and checks it against the schema; the result reads
into the same `ReadingSpec` a TOML spec reads into, so a spec and its TOML
twin are one spec with one digest (the digest is over that form, never the
text). A key written several times is a list. TOML and JSON specs are still
read.

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
receipt lists the lines touched per step and for the job
(`job: +6 −0 across 2 files in 2 crates`), and `jobs::lines_within` sums them
over a set of jobs.

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
second reuses the first's reading and says so. Every digest is
`stable_digest`'s sha256 over a declared canonical form, written `sha256:<hex>`.

## Refusals

A refusal is a liaise code (`CUST0001`…`CUST0028`, `ReadingRefusal`) and one
sentence. `api::Refusal` carries both.

## Ownership

Developed and published by James Hunt. The repository is held by the 1o organization
(https://github.com/1o-llc). Assignment of the copyright to 1o is pending documentation. The license texts
(LICENSE-MIT, LICENSE-APACHE) are left as they were found.

## License

MIT OR Apache-2.0
