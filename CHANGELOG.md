# Changelog

## 0.4.0 (2026-10-30)

The laboratory-control repair (2026-10-30). BFCB's Round 49 runs showed a proof that compiled under all twelve
configurations and passed its tests reported its own inserted line absent, and a change that could be written on a
receipt that did not cover it. This release makes the binding between a specimen, a change spec, the preflight that
authorized it and the tree that is proved and written exact, and adds what an experiment needs: a lock as the
dependency state, a delta for inherited format and lint findings, and a provenance record per experiment. Minor, not
patch: `TransformRecord` and `Preflight` gain public fields, and a governed preflight refuses what 0.3.15 authorized
(a workspace with no lock). Nothing is removed. Links are pinned to db4d314 (ecf0f02 for the code digest); the lines they replace to 4e5c57b (0.3.15).

- **A landing's file is relative to the root however the root is spelled.** `rel` ([change.rs L1575](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/change.rs#L1575))
  strips the canonical root when the given spelling does not match; it returned the absolute path for a root spelled
  `.` or `/var/...` against klepto's `/private/var/...` ([L1558](https://github.com/1o-llc/custodian_kit/blob/4e5c57b/src/change.rs#L1558)), and code_passenger's text
  fact then read the specimen instead of the proof's copy. Test: `a_landing_names_its_file_relative_to_the_root_however_the_root_is_spelled`.
- **A near name is refused naming the scan's id.** `landing` refuses a target that differs from an item's id only in
  generic arguments or spacing, and names the id to copy ([`ids_like`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/change.rs#L1590)); identity is
  unchanged. Test: `a_name_without_its_generics_is_refused_naming_the_scans_id`.
- **Blank lines stay empty.** `render_body` and `render_wrapped` indented blank lines and kept a blank line before the
  closing brace ([L1193](https://github.com/1o-llc/custodian_kit/blob/4e5c57b/src/change.rs#L1193), [L1325](https://github.com/1o-llc/custodian_kit/blob/4e5c57b/src/change.rs#L1325)); the writer itself added
  trailing whitespace a format check holds against the candidate. [`push_indented`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/change.rs#L1206). Test:
  `a_raw_body_leaves_blank_lines_empty`.
- **A governed preflight needs a lock.** With a store, a preflight adds the check `a Cargo.lock fixes the dependency
  state` ([preflight.rs L352](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/preflight.rs#L352)): without a lock it is not authorized, and the check says
  where to put one (the candidate's copy, never its baseline). The record keeps `lock` ([`lock_of`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/preflight.rs#L181)).
  CUST0046. Test: `a_governed_preflight_needs_a_lock_and_binds_its_digest`.
- **The preflight a write goes on.** [`authorizing`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/preflight.rs#L1004): the receipt that authorized this exact
  spec over this exact code, named or the newest; a named receipt is refused when a later preflight of the same spec
  over the same code did not authorize it ([L1013](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/preflight.rs#L1013)). The journal lookup `gate_all` used is
  one function now ([`preflights_by_name`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/preflight.rs#L984)). CUST0047, CUST0048. Tests:
  `a_write_goes_only_on_the_preflight_that_covered_this_spec_over_this_code`,
  `a_later_refusal_of_the_same_spec_over_the_same_code_outranks_a_named_older_receipt`.
- **A batch record is its receipt's only while it holds what the receipt digested.** The batch receipt's digest is
  [`members_digest`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/preflight.rs#L1161) (name, spec digest and verdict), checked by [`batch_record`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/preflight.rs#L1152)
  on every read: an edited record authorizes nothing. A batch receipt made by 0.3.x does not read as one: preflight
  again. Test: `an_edited_batch_record_authorizes_nothing`.
- **`TransformRecord`** gains `code_digest_after`, `lock`, `writer` ([transform.rs L84](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/transform.rs#L84)).
- **`delta`**, a module ([L90](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/delta.rs#L90)): rustfmt's check or clippy on a base and a candidate, each finding
  keyed by what it says, the base's inherited, the rest introduced; clippy on copies, neither tree written. Declared
  lines `CargoFmt`, `CargoClippy`; the verification form `delta:<kind>` ([jobs.rs L1286](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/jobs.rs#L1286));
  `data/deltas/{kind}/{stamp}.json`. Test: `a_delta_counts_only_what_the_candidate_adds`.
- **A format check that never ran is refused.** `cargo fmt --check` exits 1 both for a tree with diffs and when
  the toolchain has no rustfmt; the second read as a formatted tree, so a delta on a toolchain without rustfmt called
  any candidate clean (the release run on Rust 1.89 found it). An exit 1 with no diff is now refused with cargo's own
  words ([`read_fmt`](https://github.com/1o-llc/custodian_kit/blob/a363d9d/src/delta.rs#L123)), and CI's test job installs
  rustfmt on every Rust. Test: `a_fmt_that_never_ran_is_refused_never_a_formatted_tree`.
- **`provenance`**, a module: an experiment's record in its job's folder ([`open`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/provenance.rs#L207),
  [`measure`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/provenance.rs#L228), [`close`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/provenance.rs#L275), [`running_experiment`](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/provenance.rs#L290)).
  Test: `an_experiment_names_its_specimen_the_lock_on_its_copy_and_its_measurements`.
- **`jobs::close_as`** ([L935](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/jobs.rs#L935)): a close rejected with its reason. The close decides its result
  once; the task history kept a second computation of it that lost a rejection and a delta.
- `VERSION` ([lib.rs L73](https://github.com/1o-llc/custodian_kit/blob/db4d314/src/lib.rs#L73)): the writer names the kit it was built on.
- **The code digest is the tree's however the root is spelled.** `code_digest` resolves the root first
  ([graph.rs L40](https://github.com/1o-llc/custodian_kit/blob/ecf0f02/src/graph.rs#L40)); a root spelled `.` kept
  klepto's paths unstripped ([L38](https://github.com/1o-llc/custodian_kit/blob/4e5c57b/src/graph.rs#L38)), so the
  writer's digest from `--root .` never matched the preflight's. Test: `the_code_digest_is_the_same_however_the_root_is_spelled`.
- Test fixtures in `bounded`, `operations` and `exercise` get a lock (the new check).

`cargo test --all-features`: 344 passed, 0 failed. `tests/lab_control.rs`: 10.

## 0.3.15 (2026-10-25)

An archive: whole crates as one ZIP for delivery, beside the bundle (one annotated text file for reading, held
redacted). The engine was built and tested standalone first (a three-crate fixture, `fixture_app -> fixture_mid ->
fixture_leaf`, two builds the same bytes); this release carries it into the kit as the body of a declared line, so a
custodian line runs it through the executor with a receipt, and its crate discovery is the kit's own. Nothing is
removed from the API; the additions are a module, a declared line, a part of the folder and three refusal codes, so
this is the next patch, as 0.3.13's additions were. Links are pinned to f6e18a7. custodian 0.3.13 is its command line.

- **`archive`, a module.** [`Members::read`](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/archive.rs#L126) lists the workspace's members and
  their path dependencies by the declared `cargo metadata --no-deps` line (`exec::CargoMetadata`, run through
  `requirements::cargo::run`, so it is recorded as the requirements reading records a cargo line); the prototype's
  own TOML scan is gone. `plan_with_options` takes the closure when asked and records why each crate is there
  (explicit, or a dependency and of which), the gaps otherwise, every included crate's files under `crates/<name>/`
  (its `Cargo.toml` with them), attached files under `workspace/`, each archive path once, every path checked
  ([L428](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/archive.rs#L428): no `..`, no root, no symlink). [`build_zip`](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/archive.rs#L255) writes
  ZIP32, stored, no timestamps, the same bytes for the same inputs, to a `.zip.partial` renamed into place, and
  refuses an output that exists; the manifest `META/archive-manifest.tsv` (schema `custodian.archive.v1`) carries
  each crate's provenance, each attachment, each gap and every file's sha256 and size.
  [`verify_zip`](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/archive.rs#L311) reads an archive back: every CRC, the schema, every named file with the
  digest and size the manifest says, no file it does not name. Every digest is `crate::digest`'s (stable_digest);
  the prototype's own SHA-256 is gone.
- **`exec_archive`, the body of the declared line.** [`exec::ExecArchive`](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/exec.rs#L68) declares
  `custodian exec-archive --root R --include ROUTE... [--attach ROUTE]... [--workspace-deps] [--reveal] [--out ZIP]
  [--job ID]`, or `--verify ZIP`, with its statuses named (0 `archived`, 1 `gaps`, 2 `refused`) and its rule
  ([L100](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/exec.rs#L100): `--verify` alone, else at least one `--include`).
  [`archive::exec_archive`](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/archive.rs#L662) resolves every route through the declared table
  (`PathsTable::resolve`: a job archives declared routes only; an attached file is `crate:<name>/<file>` or a
  declared `path:`), reads the members, plans, and applies the bundle's rule through
  [`bundle::held_files`](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/bundle.rs#L424), one place: a file holding an item at `secret` or above is
  refused rather than copied open, unless `--reveal`. With `--out` the archive is written and read back; the summary
  (the crates and why, the gaps, the attachments, the file count and bytes, the revealed files, the archive and its
  digest, the read-back, the cargo line) is what the line prints last.
- **`data/archives/{name}/{stamp}.zip`**, a part of the folder ([`named::ARCHIVE`](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/store.rs#L89),
  append-only), pinned in `tests/layout.rs`.
- **Three refusal codes**: CUST0130 `ArchiveRefused` (a route that is not a crate, an attached file outside the
  root or missing, a duplicate path, an output that exists), CUST0131 `ArchiveHeld`, CUST0132 `ArchiveInvalid` (an
  archive that does not read back) ([refusal.rs L118](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/reading/refusal.rs#L118)).
- Tests (`archive::tests`): the members from the declared cargo line with their path dependencies; the gaps closing
  as crates are added by hand; [the closure's provenance and the attachments' place](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/archive.rs#L798)
  (13 files, the manifest's lines); an unsafe or missing attachment refused; [the build byte for byte repeatable,
  never written over, read back, a flipped byte caught](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/archive.rs#L832);
  [the line's body over routes](https://github.com/1o-llc/custodian_kit/blob/f6e18a7/src/archive.rs#L861) (a gap is status 1 and nothing written; a file route is not
  a crate; an undeclared path in a job is CUST0023). What differs from the prototype: the schema line of the
  manifest (`custodian.archive.v1`, was `custodian.archive.prototype.v3`); the twelve payload entries of the
  prototype's fixture archive are byte for byte the same, so only that line moves the whole-archive digest.

- **A member whose directory is a symlink out of the root is refused** ([archive.rs L154](https://github.com/1o-llc/custodian_kit/blob/68067d1/src/archive.rs#L154)), as an
  attached symlink and a symlink inside a crate already were: cargo lists such a member, and the first cut followed it
  into undeclared content. Four tests pin what the audit asked for: [every symlink refused](https://github.com/1o-llc/custodian_kit/blob/68067d1/src/archive.rs#L865),
  [two inputs never one entry](https://github.com/1o-llc/custodian_kit/blob/68067d1/src/archive.rs#L893) (the same attachment twice is a duplicate archive path; no
  input reaches `META/`), [a failed build leaves no archive and no partial, an existing one untouched](https://github.com/1o-llc/custodian_kit/blob/68067d1/src/archive.rs#L906),
  [the verifier refuses what it cannot trust](https://github.com/1o-llc/custodian_kit/blob/68067d1/src/archive.rs#L929) (`..` and absolute entry names, a duplicate
  entry, a payload the manifest does not name, a digest or size the payload does not meet, no manifest).

`cargo test --locked --all-features`: 331 passed, 0 failed (`archive::tests`: 13 of them). Links in the item above are pinned to 68067d1.

## 0.3.14 (2026-10-23)

Two fixes to 0.3.13, found by custodian's and code_passenger's CI on it. Nothing is added to or removed from the API.
Links are pinned to a664493; the lines they replace to d7b4eee (0.3.13).

- **A swap and its revert stay covered.** 0.3.13 followed a file's writes in whatever order carried the chain
  ([transform.rs L138](https://github.com/1o-llc/custodian_kit/blob/d7b4eee57c9d9ef50e999b297b8506b315aea02c/src/transform.rs#L138)), and could take a batch member's write before the revert that
  came first, ending short of the close: custodian's `a_bounded_job_that_swaps_then_reverts_closes_accepted_with_no_hand_lines`
  was rejected. The writes are walked in the order they ran, as before 0.3.13, and only when that walk does not
  reach the close is the file covered by its writes reaching the close from the open in any order, through one write
  or more ([transform.rs L161](https://github.com/1o-llc/custodian_kit/blob/a66449372ad535880cad88f201bb8deba41ff92e/src/transform.rs#L161), `reached_by_writes` [L120](https://github.com/1o-llc/custodian_kit/blob/a66449372ad535880cad88f201bb8deba41ff92e/src/transform.rs#L120)). A
  hand edit back to the open state is still a hand edit. Tests: that custodian test, and
  `a_batch_whose_members_stack_in_another_order_than_recorded_still_covers_its_file`.
- **A proof place is never pruned while it is being taken.** `take` wrote the place's `workspace` file before it
  held the lock, so another take's prune could read it empty, find no such workspace and take the place away
  (code_passenger's `the_proof_place_is_kept_and_a_sync_writes_only_what_differs`, CUST0009 on CI). The lock is held
  first ([proof_place.rs L105](https://github.com/1o-llc/custodian_kit/blob/a66449372ad535880cad88f201bb8deba41ff92e/src/proof_place.rs#L105)) and a prune passes an empty workspace file by
  ([L65](https://github.com/1o-llc/custodian_kit/blob/a66449372ad535880cad88f201bb8deba41ff92e/src/proof_place.rs#L65)).

`cargo test --all-features`: 318 passed, 0 failed.

## 0.3.13 (2026-10-23)

The whole-workspace swap job (toolkit-workspace) did its work, 5,215 functions in 282 crates through batches, and
its close refused it: it named all 1,531 touched files as written by hand, said no change spec was written and no
test line ran, and found its verification `test:chain::tests::*` not run. One cause under all four: the agent named
the job by its name (`--job toolkit-workspace`), every batch, transform and test receipt carried the name, and the
close read the job's receipts by its id (`job-db0288d94483`). The batch's member records were sound: each file's
digests chain from the base to the tree as it is. This release counts those receipts, closes a refused job again,
and fixes what the job found on the way. Nothing is removed from the API; the additions are listed at the end.
Links are pinned to baaac47 and fedee07; the lines they replace to 46dcc36 (0.3.12).

- **The close counts the receipts a line kept under the job's name.** `runs_of`
  ([jobs.rs L346](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/jobs.rs#L346)) takes the receipts carrying the job's id and those carrying its name after
  it opened, while it is the newest job of that name (one job runs at a time); the close and `verifications` read
  through it, where they read `runs_of_job(&id)` ([jobs.rs L912](https://github.com/1o-llc/custodian_kit/blob/46dcc36b37b7c35c8ab2299f172664b208ed0f26/src/jobs.rs#L912)). A batch's member records
  then cover their lines exactly as a single transform's do. The handoff counts the receipts the close counted
  ([handoff.rs L280](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/handoff.rs#L280)), so it names the batch's specs and the test lines. Test:
  `a_batch_over_three_files_closes_accepted_with_no_line_by_hand` (tests/close.rs): a bounded job, one batch over
  three files kept under the job's name, refused first, then closed accepted with no line by hand, its verification
  satisfied by the test run kept under the name, and the handoff naming the specs and the test line.
- **A batch's writes to one file are followed in the order they stack.** `coverage` took each record once, in the
  order recorded ([transform.rs L136](https://github.com/1o-llc/custodian_kit/blob/46dcc36b37b7c35c8ab2299f172664b208ed0f26/src/transform.rs#L136)); a batch records its members in the list's order,
  so seven swaps in one file (system_logging's config.rs) broke the chain at the first member recorded before the
  one it stacks on. The chain is now followed until no written record carries it further
  ([transform.rs L138](https://github.com/1o-llc/custodian_kit/blob/fedee07e48438e5994ead815f2268e5c5ddec31d/src/transform.rs#L138)). Test:
  `a_batch_whose_members_stack_in_another_order_than_recorded_still_covers_its_file`.
- **A file's base bytes are exact.** The base's digest came from git's text, which the line reader ends with a
  newline ([jobs.rs L1349](https://github.com/1o-llc/custodian_kit/blob/46dcc36b37b7c35c8ab2299f172664b208ed0f26/src/jobs.rs#L1349)), so a manifest without a final newline never matched its record.
  `base_bytes` ([jobs.rs L1384](https://github.com/1o-llc/custodian_kit/blob/fedee07e48438e5994ead815f2268e5c5ddec31d/src/jobs.rs#L1384)) takes that newline off when `git cat-file -s` says the
  blob is shorter. Test: `a_base_without_a_final_newline_is_covered_and_a_test_runs_timings_are_its_own` (tests/close.rs).
- **A test run's timing records are the test line's.** `.maintenance/timings/<run>.jsonl`, which `test run`
  writes, is carried as `test run` ([jobs.rs L1375](https://github.com/1o-llc/custodian_kit/blob/fedee07e48438e5994ead815f2268e5c5ddec31d/src/jobs.rs#L1375)); the job's 665 were named files no
  operation writes. Same test.
- **A failure that differs only in cargo's build hash is the same failure.** The first failing lines are compared
  with every `-<16 hex>` written `-#` (`same_first_line`, [baseline.rs L284](https://github.com/1o-llc/custodian_kit/blob/fedee07e48438e5994ead815f2268e5c5ddec31d/src/baseline.rs#L284); was
  [L321](https://github.com/1o-llc/custodian_kit/blob/46dcc36b37b7c35c8ab2299f172664b208ed0f26/src/baseline.rs#L321)): casino_daemon_leg names its own test binary, built under another hash at the
  base. Test: `a_first_line_that_differs_only_in_a_build_hash_is_the_same_failure`.
- **A refused close is closed again.** `job close` on a done job whose close was not accepted moves it back to
  running and closes it over the same receipts (`reopen_rejected`, [jobs.rs L358](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/jobs.rs#L358), called at
  [L934](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/jobs.rs#L934)); the handoff already gave that line for a refused close. Same test.
- **A test verification that names no test is refused when declared.** `job start` and `job verify` read the
  workspace's tests and refuse a `test:` line that matches none, as the close matches them ("no test matches";
  `check_tests_match`, [jobs.rs L1340](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/jobs.rs#L1340)), before the job opens. It was found `not run` at the
  close. Test: `a_test_verification_that_matches_no_test_is_refused_at_start` (tests/close.rs). The close tests that
  declared `test:x` over a fixture with no such test now declare `test:vault_key_is_42`.
- **The batch preflight runs over threads.** A batch of 64 members or more splits over up to 6 threads, each
  reading the code once itself (the parsed code holds proc_macro2 spans and is not shared between threads); the
  task's records are written after, in the list's order ([preflight.rs L1155](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/preflight.rs#L1155); was one
  loop, [L1105](https://github.com/1o-llc/custodian_kit/blob/46dcc36b37b7c35c8ab2299f172664b208ed0f26/src/preflight.rs#L1105)). 6,037 specs took 1 h 38 min on one core. Test:
  `a_large_batch_preflight_runs_over_threads_and_keeps_each_verdict_in_order` (tests/bounded.rs).
- **A patterns reading reads the code once per job.** klepto's shapes of the workspace are kept beside its parsed
  index, one file per mining option (`mined`, [patterns.rs L226](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/reading/patterns.rs#L226)); a patterns
  reading over code whose shapes are kept reads the index from its cache and parses nothing
  ([mod.rs L112](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/reading/mod.rs#L112)), and a parse keeps the index too
  ([facts.rs L221](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/reading/facts.rs#L221)). Each reading parsed the whole workspace and mined it again, about
  3 minutes whatever the crate. Test: `a_patterns_reading_over_the_same_code_reads_the_index_once` (tests/reading.rs).
- **Swap spec names stay under 200 characters.** A route too long keeps its first words and the same digest at its
  end (`spec_name_for`, [matcher.rs L874](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/toolkit/matcher.rs#L874)); the full route is the spec's target. A
  short route's name is as before. Test: `a_long_route_names_its_spec_under_200_characters_with_its_digest`.
- **A tuple argument keeps its parentheses.** A filling that is one parenthesised group was always unwrapped
  ([matcher.rs L402](https://github.com/1o-llc/custodian_kit/blob/46dcc36b37b7c35c8ab2299f172664b208ed0f26/src/toolkit/matcher.rs#L402)), so `if c { (a, b) } else { … }` became `if_else!(c => a, b, …)`,
  the composite_runtimed swap that did not compile. Only grouping parentheses go now
  ([matcher.rs L403](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/toolkit/matcher.rs#L403)). Test:
  `a_tuple_argument_keeps_its_parentheses_and_a_pub_forward_is_not_made_private`.
- **forward_trait_method never makes a `pub fn` private.** The macro writes a trait's method, with no visibility;
  the render dropped `pub` ([change.rs L1149](https://github.com/1o-llc/custodian_kit/blob/46dcc36b37b7c35c8ab2299f172664b208ed0f26/src/change.rs#L1149)). A function with a visibility is not matched to
  it ([matcher.rs L903](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/toolkit/matcher.rs#L903)) and the render refuses one
  ([change.rs L1149](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/change.rs#L1149)). Same test.
- **The proof's kept build is held under a cap.** `ProofPlace::take` measures the build before any compile and, over
  `CUSTODIAN_PROOF_CAP_GB` (40 GB by default), lets go of its incremental folders, then of the whole build if still
  over (`hold_to_cap`, [proof_place.rs L129](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/proof_place.rs#L129)); `prune_incremental`
  ([L116](https://github.com/1o-llc/custodian_kit/blob/baaac472e981798c9c16b80b8660670028576d51/src/proof_place.rs#L116)) is what code_passenger calls after each batch. The job's proof folder grew to
  168 GB. Test: `the_kept_build_is_held_under_its_cap_incremental_first`.

API, additive: `jobs::runs_of`, `jobs::reopen_rejected`, `jobs::TEST_RUN_WRITTEN`, `baseline::same_first_line`, `preflight::PARALLEL_FROM`, `preflight::PREFLIGHT_THREADS`,
`proof_place::{PROOF_CAP_ENV, PROOF_CAP_GB, cap_bytes, size_of}`, `ProofPlace::{prune_incremental, hold_to_cap}`,
`toolkit::matcher::{SPEC_NAME_MAX, spec_name_for}`. `cargo test --all-features`: 318 passed, 0 failed.

## 0.3.12 (2026-10-23)

core-1o 0.2 reads string escapes, comments, a record at the top level, and a schema that says which keys
repeat, which are written once, what each enum value means and which keys a kind needs. The kit had worked
around each of those: it refused a string holding a double quote (CUST0030) and fell back to a TOML twin, wrapped
every document in an outer `{ }`, and checked in Rust that a key taking one value was written once. This
release moves onto 0.2 and drops the workarounds. Nothing is removed from the API (the rustdoc JSON of 0.3.11 and
0.3.12 differ by one addition, `oneo::quote`, and no removal). Links are pinned to 308291c; the lines they replace
to 716e1b0 (0.3.11).

- **A double quote is an escape, not a refusal.** `render` ([oneo.rs L115](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/oneo.rs#L115)) writes every
  string through `quote` ([L128](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/oneo.rs#L128)), core-1o's escapes `\\ \" \n \t \r`, the set
  task_history's writer uses. It refused the string ([oneo.rs L147](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/oneo.rs#L147)). CUST0030 now says only
  a negative number, which core-1o still does not read ([refusal.rs L69](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/reading/refusal.rs#L69)). Test:
  `a_double_quote_is_written_as_an_escape_and_reads_back` (tests/oneo.rs).
- **No outer `{ }`.** A document is written as its record, `reading { ... }` ([oneo.rs L115](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/oneo.rs#L115));
  it was `{ reading { ... } }` ([oneo.rs L112](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/oneo.rs#L112)). The wrapped form still reads the same.
  Same test.
- **The schema says the single keys; the Rust check is gone.** Every block of the five schemas is
  `x-1o-single-keys`, and every key the Rust form takes as a list is `type: array` (e.g.
  [paths v1.schema.json L40](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/schema/paths/v1.schema.json#L40)), so core-1o refuses a second value of any
  other key, in the editor and on read. `Occurrences::one` ([oneo.rs L269](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/oneo.rs#L269)) takes the value;
  it counted them ([oneo.rs L252](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/oneo.rs#L252)). Test: `the_schema_refuses_what_the_spec_refuses`.
- **The schemas say the vocabularies and each operation's keys.** `enumDescriptions` for the reading kinds
  ([reading v1.schema.json L54](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/schema/reading/v1.schema.json#L54)), the declared roles, the job modes, the
  linkage kinds, the operations and the compatibilities, each the vocabulary's own `doc()`; the change spec's
  `x-1o-required-when` ([change v1.schema.json L3016](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/schema/change/v1.schema.json#L3016)) from
  `Operation::needs`, where one key says it (`functions`, `trait` and `dependency` each have another way to be
  given, so they stay the Rust check's). The change schema lacked two operations, `remove_dependency` and
  `rename_paths` ([L50](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/schema/change/v1.schema.json#L50)), and the key `same_expansion`
  ([L3003](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/schema/change/v1.schema.json#L3003)): a patterns reading's spec could not have been read back in 1o.
  Test: `the_schemas_carry_the_vocabularies_descriptions_and_each_operation_s_keys`, which checks every list
  against the Rust.
- **Written in 1o only; TOML still read for one more version.** The job's step specs
  ([jobs.rs L472](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/jobs.rs#L472), was [L472](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/jobs.rs#L472)), the preflight's reading specs
  ([preflight.rs L831](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/preflight.rs#L831), [L1123](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/preflight.rs#L1123); were
  [L831](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/preflight.rs#L831), [L1123](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/preflight.rs#L1123)), the plan's reading spec and its
  approved change spec ([plan.rs L141](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/plan.rs#L141), [L697](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/plan.rs#L697); were
  [L141](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/plan.rs#L141), [L697](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/plan.rs#L697)), a patterns reading's change specs
  ([patterns.rs L71](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/reading/patterns.rs#L71), was [L71](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/reading/patterns.rs#L71)), the exercise's
  specs ([exercise.rs L1229](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/exercise.rs#L1229), was [L1229](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/exercise.rs#L1229)) and linkage
  records ([linkage.rs L97](https://github.com/1o-llc/custodian_kit/blob/308291cc7b9f1a032f90ef515e7a1fbda1391d32/src/linkage.rs#L97), was [L97](https://github.com/1o-llc/custodian_kit/blob/716e1b0915b0dc03731aca7d240dc9430c171b97/src/linkage.rs#L97)). A TOML spec, job file,
  paths table or linkage record already on disk reads as before, for this one more version. The
  code-passenger that reads these specs must be built against 0.3.12: one built against 0.3.11 reads a 1o change
  spec only in the wrapped form. Tests: the patterns tests in tests/reading.rs and
  `the_picks_and_their_specs_come_from_the_index` (tests/exercise.rs) read the 1o files written.
- **Every existing document reads the same.** Over a copy of the kit's `.custodian` (912 reading specs, 12 job
  files, 1 paths table, 11,061 task-history records), core-1o 0.1.1 and 0.2 give the same nodes, spans included,
  each document valid under its 0.3.11 schema with 0.1.1 and under its 0.3.12 schema with 0.2; and 0.3.12 reads
  each, and each of the 3,697 TOML change specs, into its form, written in 1o and read back the same. Test:
  `every_document_of_a_corpus_reads_and_its_1o_reads_back`, with `CUSTODIAN_GOLDEN_CORPUS` set.
- core-1o 0.2; task_history 0.1.2 (its loader reads a record with core-1o).

Proof: `cargo test --all-features` (176 unit tests and every integration test, the bounded and exercise tests
with a code-passenger built against this tree).

## 0.3.11 (2026-10-23)

An agent swapped three of the kit's crates to simple_impl toolkit calls through custodian, and custodian's own
numbers were wrong in ways that cost the run. Its cost line predicted each job before it ran, and measured each
from job open to close:

| crate | the cost line before running | measured |
|---|---|---|
| nodeplan_package | 2,391 preflights × 0.2 s, 2 compiles × 3.2 min, ≈ 13.7 min | 42 min, less than half the work done |
| sidecar_core | 1,013 preflights × 0.4 s, 2 compiles × 3.2 min, ≈ 13.2 min | 40 min |
| xccute_observations | 272 preflights × 0.4 s, 2 compiles × 3.4 min, ≈ 8.9 min | 27.5 min, 118 of 271 swapped |

The line counted one compile per crate where James's rule pays two per shape (the proof and its batch), left out
the patterns reading, the dependency step and the index parse each shape pays, and took the preflight's rate from
single preflights. The budget beside it counted each command line and the code-passenger receipts inside it twice
(57.3 min said at 38 min real), and then refused the job's own tests, so a job past it could never close. Spec
names were cut at 48 characters, so 2,389 functions came out as 238 spec files; two `From` impls on one type had one
target; whole trait impls were counted as exact matches with no spec; and two checkouts numbered their receipts the
same. This release fixes each, with a test on a fixture for each. On a copy of the xccute_observations job's
folder, the line made from the receipts held when its patterns reading finished now says 33.2 min for the work
that job did, against the 27.5 min it took (8.9 min before); the rest is the test run, measured on larger crates. Nothing is removed from the API (the rustdoc JSON of 0.3.10 and 0.3.11 differ by 132 additions and no removal). Links are pinned to d8f9f7f; the lines they replace to a167869 (0.3.10).

- **Spec names never collide.** A patterns spec is named by its whole route and the first 8 hex of the route's
  digest (`route_name`, [spec.rs L421](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/reading/spec.rs#L421), used at [matcher.rs L873](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/toolkit/matcher.rs#L873));
  it was the route cut at 48 characters ([spec.rs L410](https://github.com/1o-llc/custodian_kit/blob/a16786941cf7dcfff8d85b005a1c2cb3b9d13fab/src/reading/spec.rs#L410), [matcher.rs L873](https://github.com/1o-llc/custodian_kit/blob/a16786941cf7dcfff8d85b005a1c2cb3b9d13fab/src/toolkit/matcher.rs#L873)).
  Test: `a_patterns_spec_is_named_by_its_whole_route_and_two_impls_on_one_path_get_two_targets` (tests/reading.rs,
  on tests/fixtures/routes_ws).
- **The cost line reads every spec, and only the job's.** `planned_from` ([step_time.rs L410](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/step_time.rs#L410))
  reads the change spec folder and each patterns reading's `<stamp>.specs` (`spec_folders`, [L454](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/step_time.rs#L454)),
  counting in a running job only the specs written since the job before it closed, so a spec an earlier job left
  and never made is not counted; it read only `data/specs/changes` ([step_time.rs L303](https://github.com/1o-llc/custodian_kit/blob/a16786941cf7dcfff8d85b005a1c2cb3b9d13fab/src/step_time.rs#L303)). Test:
  `the_plan_counts_the_patterns_readings_specs_and_never_an_old_unmade_one`.
- **The cost line's model is the rule's.** `estimate` ([step_time.rs L288](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/step_time.rs#L288)) counts per shape
  (`shape_of`, [L329](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/step_time.rs#L329)) an index parse, the proof's preflight and its compile, the members past
  the proofs at the batch preflight's own measured rate, a batch compile per batched shape (the batch line less its
  preflight), each dependency change, a test run per crate and the close; in a job it says the whole job with what
  it has spent, each run at its kind's median (`StepTime::typical`, [step_time.rs L78](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/step_time.rs#L78)) so a build
  that ran while others held the Mac does not set it. Before the reading it says "unknown until the reading". Was [step_time.rs L215](https://github.com/1o-llc/custodian_kit/blob/a16786941cf7dcfff8d85b005a1c2cb3b9d13fab/src/step_time.rs#L215).
  Test: `the_receipts_predict_a_batch_and_name_the_steps_paid_one_member_at_a_time`.
- **Each unit of work counted once.** `spent` ([step_time.rs L484](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/step_time.rs#L484)) is the length of the union
  of the receipts' spans (each from its time less its `millis` to its time); it summed them ([L309](https://github.com/1o-llc/custodian_kit/blob/a16786941cf7dcfff8d85b005a1c2cb3b9d13fab/src/step_time.rs#L309)).
  Test: `the_budget_counts_a_command_line_and_the_receipts_inside_it_once`, on receipts #4260–#4340 of the
  xccute_observations job: 518.5 s summed, 282.5 s spent.
- **The hours are information.** James's word: nothing is refused by hours; the battery rule is the one that
  stops a step. `over_by` ([step_time.rs L516](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/step_time.rs#L516)) says how far past its hours a job is, and the
  closing receipt carries it (`Closing::over_budget`); `is_change_step` ([L322](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/step_time.rs#L322)) names the steps that
  change code. Tests: `the_hours_are_information_never_a_refusal`, `the_budget_never_refuses_the_jobs_own_verification`.
- **Two impls on one path, two targets.** `route` ([route.rs L16](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/route.rs#L16)) names a function's impl from the
  source's header when two share a path (`path[impl From<i64> for Level]`: klepto's model drops the trait's generic
  arguments); `find_fn` ([L89](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/route.rs#L89)) finds one or refuses with CUST0044 and the routes that tell them
  apart; `change::landing` lands such a route at that function ([change.rs L2217](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/change.rs#L2217)). Test:
  `a_route_finds_one_function_or_is_refused_with_the_routes_that_tell_them_apart`.
- **Whole trait impls apart.** A function a piece writes as a whole trait impl, with no replace_body spec, is
  `impls (no swap)` ([patterns.rs L26](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/reading/patterns.rs#L26), [L88](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/reading/patterns.rs#L88)) and not in the
  exact count. Test: the patterns test above.
- **Receipts across checkouts.** `receipts::keep` ([receipts.rs L47](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/receipts.rs#L47)) numbers above every receipt
  file on disk (receipt_history's `Journal::keep`), so a file another checkout brought is never written over; it
  took the journal's next row alone ([receipts.rs L47](https://github.com/1o-llc/custodian_kit/blob/a16786941cf7dcfff8d85b005a1c2cb3b9d13fab/src/receipts.rs#L47)). `receipts::merge` ([L92](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/receipts.rs#L92))
  makes two checkouts one history by union, a receipt's identity what it says, its number aside; `collisions`
  names the numbers a plain copy would collide on (CUST0045). Test:
  `two_checkouts_numbering_receipts_the_same_merge_by_union_and_never_write_over_a_file` (tests/close.rs).
- **The close names what verified it.** `jobs::verifications` ([jobs.rs L1268](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/jobs.rs#L1268)) says each declared
  verification of a running job and the receipt that decides it (`verified_with`, [L1229](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/jobs.rs#L1229), was
  `verified`, [L1213](https://github.com/1o-llc/custodian_kit/blob/a16786941cf7dcfff8d85b005a1c2cb3b9d13fab/src/jobs.rs#L1213)); the closing receipt lists them (`Closing::verified_by`). Test:
  `a_verification_satisfied_by_a_test_receipt_names_it_and_the_close_is_accepted`.
- **Two operations.** `remove_dependency` and `rename_paths` ([change.rs L65-L66](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/change.rs#L65-L66)), with their
  preflight checks; the writer is code_passenger 0.4.8.
- **The governing root of a folder.** `manifest::governing_root_of` ([manifest.rs L406](https://github.com/1o-llc/custodian_kit/blob/d8f9f7fec8a101b0ddedd74868665714f8657b6a/src/reading/requirements/manifest.rs#L406)):
  the requirements reading's rule for one crate folder, so a proof's copy carries the root cargo looks for. Test:
  `a_crates_governing_root_is_the_workspace_above_it_by_cargos_rule`.

## 0.3.10 (2026-10-23)

James had an agent integrate a multi-crate package, and cargo failed on a dependency six times in a row: one_db_kernel
inherited `hex` through `workspace = true`, its governing root had no `[workspace.dependencies].hex`, cargo said so,
the agent fixed that line, and the next build failed on the next thing. The person was the build-and-error loop. Each
of those was in the manifests before the first build. The `requirements` reading reads them once, before anything is
changed, and says what the intended change needs and whether each is there. It never compiles and never writes:
the cargo lines it runs are `metadata --frozen` and `tree --duplicates --locked --offline`, and the toolchain's
`rustc -vV` and `rustup show active-toolchain`. Where cargo metadata reads the workspace, cargo is the truth (klepto
0.3.5 reads its JSON); the manifest scanner is for the state cargo cannot speak in yet. On a copy of the kit's
manifests, lock and sources it reads 449 manifests and cargo's 1,549 packages in 9.9 s (311 MB), no compile. Nothing
is removed from the API: the rustdoc JSON of 0.3.9 and 0.3.10 differ by 2,302 additions (512 without the derived
and blanket trait impls) and no removal. Every link is pinned to 01c0192; the lines it replaces to 393518b (0.3.9).

- **The reading kind.** `ReadingKind::Requirements`, answered with no scan of the sources
  (`ReadingKind::needs_scan`, [spec.rs L161](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/spec.rs#L161); `reading::read`, [mod.rs L115](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/mod.rs#L115)),
  so it answers over a manifest cargo refuses, where every other kind stops at klepto's scan. Its spec's
  `[requirements]` holds the intent, the needs and the faces; the faces are one big-bad-bool word checked by
  flaga_solver, `lock` conflicting with `no_cargo` (`FacesWord`, [mod.rs L91](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/mod.rs#L91)). The report
  is the brief's contract, section by section (`RequirementsReport`, [mod.rs L411](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/mod.rs#L411)), every
  finding observed, inferred or unknown, read from a manifest, cargo metadata, cargo tree, the toolchain, Cargo.lock,
  the intent or a text search, and pointing at its manifest, line, section and field. Every check is declared once
  with its face, severity and the step of the order it lands in (`Check::rule`, [mod.rs L507](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/mod.rs#L507));
  the README's tables are the enums.
- **The manifests, no cargo.** `manifest::Scan` ([manifest.rs L404-L411](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/manifest.rs#L404-L411)) reads every
  Cargo.toml the target needs with toml_edit (the kit's span reader, read side only). A crate's governing root is
  cargo's rule: itself with `[workspace]`, else `package.workspace`, else the nearest ancestor with `[workspace]`
  that does not exclude it ([manifest.rs L602-L610](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/manifest.rs#L602-L610)), so a nested independent workspace is
  its own root; members are the entries, globs expanded, and every path dependency under the root it does not
  exclude ([manifest.rs L553](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/manifest.rs#L553)). Checks: `workspace = true` against that root's tables,
  members that name no crate, a crate its root does not hold, path chains followed to their end (name and version at
  the path), optional dev-dependencies, named registries no config declares, every `[features]` entry in its four
  forms through klepto's `FeatureModel::from_declared` ([manifest.rs L863-L1128](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/manifest.rs#L863-L1128)).
  The walk is the sweep's own (`sweep::manifests`, `sweep::source_files`, [sweep.rs L35-L51](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/sweep.rs#L35-L51)).
- **Cargo's word, and where the scanner parts from it.** `cargo::metadata` ([cargo.rs L96](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/cargo.rs#L96))
  runs `--frozen`, then `--no-deps` when that is refused; `cargo::agreement` ([cargo.rs L119](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/cargo.rs#L119))
  says whether the scanner's errors in the workspace cargo read agree with cargo, and counts the ones in other
  workspaces under the root apart. The lines are declared once in `exec` (`CargoMetadata`, `CargoTree`, `Rustc`,
  `RustupShow`, [exec.rs L75](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/exec.rs#L75), [exec.rs L95](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/exec.rs#L95), [exec.rs L120](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/exec.rs#L120)), each run in the workspace's
  directory (`Line::dir`, [exec.rs L451](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/exec.rs#L451)) so rustup picks the toolchain the directory pins.
- **The requirement side.** `needs::of_intent` and `needs::of_change` ([needs.rs L30](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/needs.rs#L30),
  [needs.rs L105](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/needs.rs#L105)) make the needs from the intent's words (only with a word that brings
  something: `merge helper_a into helper_b` asks for no dependency) or a change spec's add_dependency,
  change_dependency and declare_feature; `needs::check` ([needs.rs L152](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/needs.rs#L152)) checks each
  against cargo's resolve: declared, the version the lock holds (`klepto::version_meets`), the feature there and what
  turns it on, the source; the combinations nothing resolved are unknown ([needs.rs L329](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/needs.rs#L329));
  impact is the crate's dependencies and dependents in the resolve with the `impact` and `affected_tests` lines
  ([needs.rs L421](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/needs.rs#L421)).
- **Rust versions.** `rust::read` ([rust.rs L48](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/rust.rs#L48)): per crate its rust-version and edition,
  the highest rust-version its closure in the resolve demands, and the three disagreements; the editions' first Rust
  written once (`EDITIONS`, [rust.rs L12](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/rust.rs#L12)).
- **The lock's shape.** `lock::read` ([lock.rs L145](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/lock.rs#L145)) reads `cargo tree -d` as cargo
  prints it: every root is a unit cargo builds (a version, a feature set, the host beside the target), its level-1
  lines the packages whose requirement pulls it in, each requirement cargo metadata's and its line the manifest's (an
  inherited one is the root's line). `collapse` ([lock.rs L101](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/lock.rs#L101)) names the one
  requirement change that would leave one version when the requirements forcing the others are written in one place,
  inferred. When cargo tree is refused, Cargo.lock's text gives the versions and who forces them is unknown.
- **The order.** `order::steps` ([order.rs L23](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/order.rs#L23)): the fewest steps, required or recommended,
  in cargo's order, each with how to see it done, never a version the reading did not read, then the compile and the
  tests this reading never runs.
- **Reuse and invalidation.** The same-search key's code half for this kind is `requirements::input_digest`
  ([mod.rs L560](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/reading/requirements/mod.rs#L560)): the manifests, the locks, the toolchain files, the `.cargo/config.toml`s
  and the spec's requirements; `api::SearchKey::of` takes it ([api.rs L151](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/api.rs#L151)), where 0.3.9 took klepto's
  fingerprint of the sources for every kind ([api.rs L149](https://github.com/1o-llc/custodian_kit/blob/393518b/src/api.rs#L149)). `api::run_at` ([api.rs L205](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/api.rs#L205)) answers
  a kind with no scan at a root, no workspace opened.
- **Not a gate; carried along.** A preflight of add_dependency, change_dependency or declare_feature names the line
  (`preflight::requirements_line`, [preflight.rs L165](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/preflight.rs#L165)) and decides nothing on it. `plan make` reads it for the
  plan's intent ([plan.rs L133](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/plan.rs#L133), [plan.rs L485](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/plan.rs#L485)): a dependency to declare is an `add_dependency` step,
  first; the other required steps are listed before step 1; what can be reused is reuse. A handoff carries the
  newest one's unresolved findings ([handoff.rs L448](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/handoff.rs#L448), [handoff.rs L582](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/src/handoff.rs#L582)).

Proof, tests/requirements.rs on tests/fixtures/requirements/ (the brief's acceptance cases that are the library's):
`a_workspace_dependency_with_no_root_entry_names_the_manifest_and_its_governing_root` ([L71](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L71)) (case 1, the `hex` case);
`a_nested_independent_workspace_is_its_own_root_and_no_declaration_goes_in_the_outer_one` ([L94](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L94)) (2);
`a_path_dependency_chain_is_followed_to_the_directory_that_is_not_there` ([L112](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L112)) (3);
`features_are_read_in_their_four_forms_and_untested_combinations_are_unknown` ([L124](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L124)) (4);
`a_real_version_conflict_is_reported_without_a_guessed_repair` ([L152](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L152)) (5);
`an_intent_to_add_a_crate_gives_its_prerequisites_impact_and_an_ordered_plan` ([L167](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L167)) (6);
`the_reading_writes_nothing_and_its_key_moves_with_the_manifests_only` ([L194](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L194)) (7);
`static_findings_and_cargos_word_are_kept_apart_and_no_compile_is_claimed` ([L222](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L222)) (9);
the three Rust-version disagreements, `a_crate_that_promises_an_older_rust_than_its_lock_allows_is_named_with_the_package` ([L240](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L240)),
`a_lock_newer_than_the_installed_toolchain_is_named` ([L252](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L252)), `an_edition_its_declared_minimum_cannot_compile_is_named_without_cargo` ([L262](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L262));
`a_package_in_two_versions_names_who_forces_each_and_the_one_change_that_leaves_one` ([L274](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L274));
`the_faces_are_one_word_and_the_lock_cannot_go_without_cargo` ([L292](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L292));
`every_finding_says_how_it_is_known_and_where_it_was_read` ([L307](https://github.com/1o-llc/custodian_kit/blob/01c0192e9908ec646f307ee581cd7d293031c964/tests/requirements.rs#L307)). Case 10: every existing test passes unchanged,
the 1o schema carries the new kind and `[requirements]` (tests/oneo.rs), and `cargo test --all-features` is 0 failed.

## 0.3.9 (2026-10-20)

What the fleet_ops swaps cost, read from their receipts, and the kit's half of making each cost once. The 295
members of the largest shape batch (receipt #1762) took 1,849 seconds; the batch's own compile, code_passenger's
line, took 253 of them (#1477). The other 1,595 seconds were the members' preflights, one after another, each
reading the code again: the code's digest (cargo metadata and a hash of every file) two or three times, a code graph
of the whole workspace, a copy of that graph for the index key, and an impact reading with a receipt of its own. On a
copy of the kit, 0.3.8 preflights those 295 members in 1,743 seconds; 0.3.9 in 55 seconds, after the one index parse
both pay (101 seconds for the whole line), and every member's record is the same as its own preflight's but for the
index key, which names the copy's path. Nothing is removed from the API: the rustdoc JSON of 0.3.8 and 0.3.9 differ
by 219 additions and no removal. Every link is pinned to 2899848; the lines it replaces to f824718 (0.3.8).

- **A batch preflight reads the code once.** `preflight::run_preflight_batch` ([preflight.rs L1048](https://github.com/1o-llc/custodian_kit/blob/2899848/src/preflight.rs#L1048)):
  one scan (the workspace the caller holds open), one code digest, one code graph for every member; each member's
  impact is answered from that reading by `reading::read_facts_over` ([mod.rs L126](https://github.com/1o-llc/custodian_kit/blob/2899848/src/reading/mod.rs#L126)), where
  0.3.8 ran a reading per member with its own code digest and receipt ([preflight.rs L815, L818 at 0.3.8](https://github.com/1o-llc/custodian_kit/blob/f824718/src/preflight.rs#L815))
  and a graph per reading ([mod.rs L128 at 0.3.8](https://github.com/1o-llc/custodian_kit/blob/f824718/src/reading/mod.rs#L128)). Each member is still checked by
  `preflight::assess`, the rules its own preflight uses, so the codes and refusals are the same. Its record is kept as
  its own preflight keeps it; one receipt, `preflight-batch`, names the batch's record, which lists each member and its
  verdict. Proof: tests/bounded.rs `a_batch_preflight_gives_each_member_the_verdict_its_own_preflight_gives`
  ([L142](https://github.com/1o-llc/custodian_kit/blob/2899848/tests/bounded.rs#L142)): eight specs, one of each verdict (authorized, a precondition broken, a write
  outside the scope, a name taken, a match that stops compiling, a target that does not resolve, a body on another
  function, a swap), each member's whole record equal to its own preflight's but for the time.
- **The gate takes the batch's receipt.** `preflight::gate_all` ([preflight.rs L915](https://github.com/1o-llc/custodian_kit/blob/2899848/src/preflight.rs#L915)) reads
  the journal once for a batch and finds each member's newest preflight, its own receipt or its line in a batch's;
  `gate` ([L907](https://github.com/1o-llc/custodian_kit/blob/2899848/src/preflight.rs#L907)) is the same rule for one spec. Proof: the same test, its gate half.
- **An index key reads the graph it is given.** `index_key_of` ([commands.rs L189](https://github.com/1o-llc/custodian_kit/blob/2899848/src/commands.rs#L189)) copied the
  whole graph first ([L190 at 0.3.8](https://github.com/1o-llc/custodian_kit/blob/f824718/src/commands.rs#L190)): 0.36 of the 0.54 seconds a member's impact took on
  the kit. It reads it in place now; the key is the same.
- **One kept build per workspace is the kit's.** `proof_place::ProofPlace` ([proof_place.rs L80](https://github.com/1o-llc/custodian_kit/blob/2899848/src/proof_place.rs#L80)):
  the copy and build folder code_passenger's proof kept (its place, its lock, its sync, unchanged), now where custodian's
  test runs reach it too, so a test run after a transform builds on what the proof built.
- **Affected tests are the proof's rule.** A test run after a swap in fleet_ops built every dependent's dependent,
  cold, in the worktree, for six hours on battery: affected_tests ran every test whose calls reach the change, in any
  crate ([kinds.rs L1279 at 0.3.8](https://github.com/1o-llc/custodian_kit/blob/f824718/src/reading/kinds.rs#L1279)). Now `reading::kinds::affected_crates`
  ([kinds.rs L1311](https://github.com/1o-llc/custodian_kit/blob/2899848/src/reading/kinds.rs#L1311)) keeps the tests of the crates the change is in, their direct
  dependents, and the readers of a public item whose signature it alters (`change.signatures`, found from the diff by
  `testrun::signature_changes`, [testrun.rs L446](https://github.com/1o-llc/custodian_kit/blob/2899848/src/testrun.rs#L446), comparing `testrun::public_signatures`,
  [L355](https://github.com/1o-llc/custodian_kit/blob/2899848/src/testrun.rs#L355)); the others are named as `test-further` rows, not run. Proof: testrun.rs
  `a_body_or_a_doc_changes_no_signature_and_a_parameter_or_a_field_does` ([L478](https://github.com/1o-llc/custodian_kit/blob/2899848/src/testrun.rs#L478)), and
  custodian's tests/kept_build.rs.
- **No cargo line starts on a dying battery.** `power` ([power.rs L93](https://github.com/1o-llc/custodian_kit/blob/2899848/src/power.rs#L93)): `pmset -g batt`, a
  declared line; under 20% and not charging, `[CUST0042]`, before the line starts ([exec.rs L473](https://github.com/1o-llc/custodian_kit/blob/2899848/src/exec.rs#L473),
  every cargo line the kit walks; the proof's place before it is taken). A job's budget can set the floor. Proof:
  power.rs `under_twenty_percent_and_not_charging_no_cargo_line_starts` ([L106](https://github.com/1o-llc/custodian_kit/blob/2899848/src/power.rs#L106)) and
  tests/power.rs `a_cargo_line_is_refused_before_it_starts_on_a_dying_battery_and_runs_on_a_charging_one`
  ([L10](https://github.com/1o-llc/custodian_kit/blob/2899848/tests/power.rs#L10)), the battery given by `CUSTODIAN_BATTERY`.
- **The kit reads its own cost.** `step_time` ([step_time.rs L89](https://github.com/1o-llc/custodian_kit/blob/2899848/src/step_time.rs#L89)): the time per step kind
  from the receipts (a preflight, a transform, a batch's compile, a test line, a reading, an index parse); `estimate`
  ([L215](https://github.com/1o-llc/custodian_kit/blob/2899848/src/step_time.rs#L215)), a job's planned cost, each figure measured here or unknown; `Budget` and
  `over_budget` ([L315](https://github.com/1o-llc/custodian_kit/blob/2899848/src/step_time.rs#L315)), `hours=<n>,battery=<percent>`, a step past the hours refused
  before it starts with `[CUST0043]`; `per_member` ([L118](https://github.com/1o-llc/custodian_kit/blob/2899848/src/step_time.rs#L118)), the step kinds a job paid one
  member at a time where a batch form exists. The closing receipt ([jobs.rs L1114](https://github.com/1o-llc/custodian_kit/blob/2899848/src/jobs.rs#L1114)), the
  exercise's summary ([exercise.rs L1674](https://github.com/1o-llc/custodian_kit/blob/2899848/src/exercise.rs#L1674)) and the handoff ([handoff.rs L287](https://github.com/1o-llc/custodian_kit/blob/2899848/src/handoff.rs#L287))
  carry them; a job file's `budget` and the workspace's `.custodian/config/budget.toml` hold the budget. Proof:
  step_time.rs `the_receipts_predict_a_batch_and_name_the_steps_paid_one_member_at_a_time` ([L335](https://github.com/1o-llc/custodian_kit/blob/2899848/src/step_time.rs#L335))
  and `a_step_past_the_hours_budget_is_refused_before_it_starts` ([L368](https://github.com/1o-llc/custodian_kit/blob/2899848/src/step_time.rs#L368)); tests/exercise.rs
  ([L118](https://github.com/1o-llc/custodian_kit/blob/2899848/tests/exercise.rs#L118)).

`cargo test --all-features` passes; `cargo clippy --all-targets --all-features` warns only where 0.3.8 did.

## 0.3.8 (2026-10-20)

The kit's half of the proof's new shape. A body swapped in fleet_ops on 0.3.7's rule was proved by compiling every
crate that depends on fleet_ops, near and far, 227 of them, from an empty build each time: one swap took 19.4
minutes and 37 GB. A body swap cannot break a crate that does not depend on the changed one directly, so the rule is
now the crate and its direct dependents. Also James's rule for a repeated shape, which these pieces carry: prove one
member of a shape fully, then batch the rest of that shape in the crate under one compile, one receipt per member, a
failing member excluded and named; it holds for every operation with repeated shapes, not only a swap. Nothing is
removed from the API: the rustdoc JSON of 0.3.7 and 0.3.8 differ by 32 additions and no removal. Every link is pinned to
9181d87; the lines it replaces to ee3c218 (0.3.7).

- **The packages a proof compiles are one rule.** `change::proof_packages` ([change.rs L1586](https://github.com/1o-llc/custodian_kit/blob/9181d87/src/change.rs#L1586)):
  the crate the change lands in, with its tests; each crate that depends on it directly; and, for an operation that
  changes what other code reads of its target (`change::reaches_readers`, [change.rs L1577](https://github.com/1o-llc/custodian_kit/blob/9181d87/src/change.rs#L1577):
  a signature, a name, a place, a field, a variant, a gate), each crate holding an occurrence klepto resolves to the
  target. Never the closure of dependents, which code_passenger 0.4.5 walked ([transform.rs L1527 at 0.4.5](https://github.com/jameshuntt/code_passenger/blob/70f0929/src/transform.rs#L1527)).
  On fleet_ops that is 8 packages, not 227. Proof: tests/operations.rs
  `a_proof_compiles_the_crate_and_its_direct_dependents_never_a_dependents_dependent` ([L366](https://github.com/1o-llc/custodian_kit/blob/9181d87/tests/operations.rs#L366)):
  p depends on a, a on c; a body swapped in c compiles c with its tests and a, never p.
- **A swap says it keeps its expansion.** A change spec's `same_expansion` ([change.rs L504](https://github.com/1o-llc/custodian_kit/blob/9181d87/src/change.rs#L504)): the
  transform proves `cargo expand` of the function before and after the same tokens as its own postcondition. The
  matcher sets it on every exact swap ([matcher.rs L878](https://github.com/1o-llc/custodian_kit/blob/9181d87/src/toolkit/matcher.rs#L878)). The expansion is kept
  beside the transform's record as `transform::Expansion` ([transform.rs L88](https://github.com/1o-llc/custodian_kit/blob/9181d87/src/transform.rs#L88)); the record itself
  is as it was, so a writer built on 0.3.7 still builds.
- **A patterns reading's swaps are spec files.** 0.3.7 kept each swap only as JSON inside the reading, with no scope
  ([patterns.rs L383 at 0.3.7](https://github.com/1o-llc/custodian_kit/blob/ee3c218/src/reading/patterns.rs#L383)). Now each exact match by a named piece (never the
  catch-all `simple_method`, never a near match) is written beside the reading as `<stamp>.specs/<name>.toml`
  ([mod.rs L352](https://github.com/1o-llc/custodian_kit/blob/9181d87/src/reading/mod.rs#L352), `patterns::spec_files`, [patterns.rs L57](https://github.com/1o-llc/custodian_kit/blob/9181d87/src/reading/patterns.rs#L57)),
  its scope the crate it lands in ([patterns.rs L78](https://github.com/1o-llc/custodian_kit/blob/9181d87/src/reading/patterns.rs#L78)), so a preflight, a transform
  or a batch is driven from the files as they are. Proof: tests/reading.rs
  `patterns_shape_every_function_and_find_the_shapes_repeated_across_crates` ([L108](https://github.com/1o-llc/custodian_kit/blob/9181d87/tests/reading.rs#L108)): one
  file per exact match, each a spec that keeps its expansion, scoped to its crate.

`cargo test --all-features` passes; `cargo clippy --all-targets --all-features` warns only where 0.3.7 did.

## 0.3.7 (2026-10-20)

custodian's exercise ran the new operations on the pantry and on `features_ws` with 0.3.6 and found ten things. The
first is a real fault: a preflight authorized a signature change while four of its call sites were unresolved, and
only the transform's own check stopped it. The other nine are a refusal told as waiting, two numbers for one count,
a check left to the transform, a count taken before the change, forms the exercise never tried, and words that hid
the reason. Each is fixed here, numbered as found. Nothing is removed from the API: the rustdoc JSON of 0.3.6 and
0.3.7 differ by 45 additions and no removal. Every link is pinned to 7c00110; the lines it replaces to a7d885b
(0.3.6).

1. **Calls inside a macro's arguments.** The pantry's `shopping_list` is called twice inside `simple_impl::ensure_eq!`
   and `ensure_empty!` in its test module, reached through `use super::*`. klepto names those paths in the macro's
   tokens and resolves none, so the impact reading counted them as unresolved, and the preflight counted only the
   one resolved call: [preflight.rs L477 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/preflight.rs#L477). Now one rule, `change::macro_calls`
   ([change.rs L1780](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/change.rs#L1780)), reads each macro's tokens ([change.rs L1852](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/change.rs#L1852)): a name written with its
   argument list is a call, `fn name` is a declaration, a name with no argument list is a value, `.name(..)` is a
   method call. Each call's path is resolved from the macro's module as the compiler would: the module's own items,
   its imports, a glob import's module, `crate::`, `self::`, `super::`, `Self::`, a crate by name
   ([change.rs L1927](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/change.rs#L1927), [change.rs L1953](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/change.rs#L1953)). A call that resolves to the function is a call site; one
   that resolves to nothing is unresolved (`change::unresolved_macro_paths`, [change.rs L1786](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/change.rs#L1786)). The
   preflight counts both kinds of call site and refuses while any is unresolved, `unresolved: N` and each place
   named, unless the spec sets `call_sites.allow_unresolved = true` ([change.rs L418](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/change.rs#L418)), which goes ahead and
   lists them: [preflight.rs L509](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/preflight.rs#L509), [preflight.rs L553](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/preflight.rs#L553). The impact reading's unresolved rows now
   reach the preflight too: [preflight.rs L825](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/preflight.rs#L825). code_passenger 0.4.5 rewrites by the same rule. Proof:
   tests/operations.rs `change_signature_counts_the_calls_inside_macro_arguments_and_refuses_while_one_is_unresolved`
   ([L207](https://github.com/1o-llc/custodian_kit/blob/7c00110/tests/operations.rs#L207)): two calls inside a macro klepto does not expand, through `use super::*` and
   `crate::`, are 2 call sites and 0 unresolved; a declaration and a value of the same name are neither; a path that
   resolves to nothing is `unresolved: 1` and refused, and allowed with `allow_unresolved`.
2. **A change already in place.** A preflight refused because what the spec asks is already in the code (the item
   already gated by the feature, the dependency already named, the feature already declared as asked, the library
   already there) now records that ([preflight.rs L481](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/preflight.rs#L481) and the three before it) and its receipt's reason is
   `not authorized: already in place` ([preflight.rs L902](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/preflight.rs#L902)). The handoff tells such a spec under what changed,
   as done, and never under waiting on a person or as a step to fix or drop: [handoff.rs L336](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/handoff.rs#L336),
   [handoff.rs L594](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/handoff.rs#L594); 0.3.6 listed it as waiting: [handoff.rs L506 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/handoff.rs#L506). `Handoff::waiting`
   counts what waits ([handoff.rs L444](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/handoff.rs#L444)), and custodian 0.3.5 exits by it. Proof: tests/handoff.rs
   `a_handoff_is_made_from_the_records_in_its_order_and_pinned` ([L214](https://github.com/1o-llc/custodian_kit/blob/7c00110/tests/handoff.rs#L214)), with one more
   spec already in place: three things wait, it does not, and its line is pinned.
3. **One number for the lines.** "What changed" gave git's 58 and then the count's +54. The count is now the one
   source, whole and per crate, and the sentence says so: "(the job's counts at open and close)"
   ([handoff.rs L362](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/handoff.rs#L362), [handoff.rs L576](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/handoff.rs#L576)); 0.3.6 took the total from the closing line:
   [handoff.rs L338 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/handoff.rs#L338). Proof: the same test, its closing line saying git's +8 and its counts +6;
   the handoff says 6.
4. The command line's refusal on a folder with no `.custodian` is custodian 0.3.5's.
5. A dry transform's wording is code_passenger 0.4.5's.
6. **Code that names an optional dependency.** A method using `chrono::` was authorized with no `#[cfg]`, and only
   the transform's check caught it. `change::optional_uses` ([change.rs L2084](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/change.rs#L2084)) reads the code a spec writes
   for paths whose head is an optional dependency of the crate, and whether a feature that turns it on gates the
   code: a `#[cfg(feature = ..)]` among its attributes, a `cfg_attr` around the path, or the item or module it lands
   in. The preflight refuses an ungated one and names the fix, `attrs = ['#[cfg(feature = "chrono")]']`
   ([preflight.rs L630](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/preflight.rs#L630)). Proof: tests/operations.rs
   `code_naming_an_optional_dependency_needs_a_feature_that_turns_it_on` ([L230](https://github.com/1o-llc/custodian_kit/blob/7c00110/tests/operations.rs#L230)).
7. **The configurations the change leaves.** The preflight's impact said 8 configurations for the pantry's
   `declare_feature` while the transform checked 4: the matrix was read before the change
   ([impact.rs L280 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/reading/impact.rs#L280)). `change::model_after` ([change.rs L1734](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/change.rs#L1734)) is now the one rule
   for the feature model as the change leaves the manifest (it was code_passenger's), and the preflight counts by it,
   saying how many there were before: [preflight.rs L682](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/preflight.rs#L682). Proof: tests/operations.rs
   `the_preflight_counts_the_configurations_the_change_leaves` ([L251](https://github.com/1o-llc/custodian_kit/blob/7c00110/tests/operations.rs#L251)): two optional
   dependencies made one feature, fewer configurations after than before, the transform's number.
8. **Every form, from the real feature graph.** The exercise's `declare_feature` always had an empty `enables`
   ([exercise.rs L585 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/exercise.rs#L585)). `exercise::feature_forms` ([exercise.rs L647](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/exercise.rs#L647)) reads the crate's
   dependencies and makes `dep:<x>` for an optional one, `<a>/<x>` and `<b>?/<y>` with a feature that resolves with
   no new package (one the manifest asks of it, else `default`, else a workspace crate's own); `specs_for`
   ([exercise.rs L691](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/exercise.rs#L691)) makes a case for each, and one more with `optional = true` for `add_dependency` and
   `change_dependency` ([exercise.rs L677](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/exercise.rs#L677)). The case table names the form. Proof: tests/operations.rs
   `the_exercise_makes_a_case_for_each_form_the_feature_graph_offers` ([L273](https://github.com/1o-llc/custodian_kit/blob/7c00110/tests/operations.rs#L273)): `dep:b`,
   `a/x` and `b?/y` on the fixture, each authorized, and `optional` on `add_dependency`.
9. **The exercise report.** Its `lines` column showed the count's net lines, 0 for a case that wrote 15
   ([exercise.rs L877 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/exercise.rs#L877)); it is now the lines the transform wrote, from its record, and none when it
   wrote nothing ([exercise.rs L977](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/exercise.rs#L977), [exercise.rs L1286](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/exercise.rs#L1286)). A group of findings showed the first
   one's text ([exercise.rs L786 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/exercise.rs#L786)); a group whose findings differ now lists each case's
   ([exercise.rs L903](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/exercise.rs#L903), [exercise.rs L939](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/exercise.rs#L939)). Proof: `the_summary_says_the_lines_written_and_each_finding_of_a_group`
   ([exercise.rs L1720](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/exercise.rs#L1720)).
10. **Words.** A refusal's top line was the word's rule, "`facts`=authorized requires `facts`=preconditions"
    ([preflight.rs L711 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/preflight.rs#L711), [preflight.rs L156 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/preflight.rs#L156)). It is now the reason in words, each
    precondition that does not hold with why, or what is already in place; the rule is on the line beneath
    (`Preflight::why`, `Preflight::headline_why`, [preflight.rs L163](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/preflight.rs#L163), [preflight.rs L716](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/preflight.rs#L716)). Proof:
    `a_refusals_top_line_is_the_reason_and_the_rule_is_beneath` ([L182](https://github.com/1o-llc/custodian_kit/blob/7c00110/tests/operations.rs#L182)) and
    `gate_refuses_an_item_its_feature_gates_already` ([L159](https://github.com/1o-llc/custodian_kit/blob/7c00110/tests/operations.rs#L159)). A count reading's cost
    said "0 files, 0 bytes"; a count names no file, and its line now says it is not a file reading
    ([cost.rs L194](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/cost.rs#L194), where 0.3.6 had [cost.rs L193 at 0.3.6](https://github.com/1o-llc/custodian_kit/blob/a7d885b/src/cost.rs#L193)). Proof:
    `a_count_says_it_is_not_a_file_reading` ([cost.rs L328](https://github.com/1o-llc/custodian_kit/blob/7c00110/src/cost.rs#L328)). The bare `job start` is
    custodian 0.3.5's.

The change spec's schema takes `call_sites.allow_unresolved`; the operations table in the README says what
`change_signature` now does. On the pantry and `features_ws`, with this release and code_passenger 0.4.5: 15 cases
and 12, 11 and 10 passed; 0.3.6 ran 10 and 10, 5 and 8 passed. The four and two findings left are `gate` targets the
exercise picks that other code uses ungated, refused correctly. `cargo test --all-features` passes; clippy reports
the 17 warnings 0.3.6 reports.

## 0.3.6 (2026-10-20)

A program built from an empty crate through change specs (the pantry: 42 specs, two bounded jobs) found where the
vocabulary stopped. Its first job closed accepted; its second, which made two crate features, was rejected because
no operation could write a feature or make a dependency optional, so the agent edited the manifest by hand. This
release adds what it needed and fixes the fourteen smaller things it found, numbered as the findings were. It also
brings in the `patterns` branch and the toolkit of simple_impl 0.1.12, and fixes how a job's close compares a test
that fails in a scratch folder. Nothing is removed from the API: the rustdoc JSON of 0.3.5 and 0.3.6 differ by 408
additions and no removal. Every link is pinned to b2a40dc.

### A failing test's scratch folder

- item-hold's close said `failed_test` for seven tests that fail the same way at the base: each panics on a path
  in a fresh temporary folder (`.tmpY3a7Dp` there, `.tmpbLm43K` here), and the first failing line kept the folder's
  name: [baseline.rs L124 at 0.3.5](https://github.com/1o-llc/custodian_kit/blob/797945c/src/baseline.rs#L124). The line now writes the system temp folder and the one name under it as
  `<temp>`: [baseline.rs L127-L159](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/baseline.rs#L127-L159). Proof: `the_first_failing_line_is_the_panics_message_with_the_root_written_root`
  ([baseline.rs L339](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/baseline.rs#L339)); item-hold's fix2 closed `accepted` with its 31 failing tests named pre-existing.

### patterns and the toolkit of simple_impl 0.1.12

- The `patterns` reading (merged from the branch of that name): every function body against the toolkit, its shape,
  the pieces whose expansion it holds (exact, or near with the difference named), the replace_body spec that swaps
  them, and the repeated shapes no piece writes. A kind that walks the syntax trees is now the count, the bundle,
  the duplicates and the patterns: [spec.rs L126-L128](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/reading/spec.rs#L126-L128).
- The toolkit is simple_impl 0.1.12 ([change.rs L859](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L859)): seven new pieces, `ensure_valid`, `require_ok_into`,
  `require_some_into`, `ok_or_else_return`, `return_if_err`, `if_else` and `table`
  ([change.rs L744](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L744) to [change.rs L763](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L763)), and eleven probes ([probe.rs L101-L125](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/toolkit/probe.rs#L101-L125)); the
  catalog is made again, 151 shapes (`schema/toolkit/simple_impl-0.1.12.shapes.json`; the 0.1.11 file is kept).
- A slot written around its placeholder (`require_some_into`'s `|| __si_error`) writes the body's code back inside
  it, so the `ok_or_else` call is never written as the `ok_or` one: [probe.rs L653](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/toolkit/probe.rs#L653),
  [matcher.rs L450-L455](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/toolkit/matcher.rs#L450-L455).
- Proof: `each_new_probe_expands_and_matches_its_own_shape` ([tests.rs L205](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/toolkit/tests.rs#L205)): each probe's call,
  expanded, is matched as its own shape exactly and the spec it proposes writes the call back. On the fixture `ws`
  the patterns reading finds 2 functions with an exact toolkit match (both `table`), 0 before; pieces matched
  somewhere 2, 1 before.

### The pantry's findings

1. **Features in the manifest.** `declare_feature` ([change.rs L64](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L64)): `feature` and `enables` write
   `[features] name = [..]`; `optional` on `add_dependency` and `change_dependency` ([change.rs L383](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L383)).
   Each entry is read through the feature graph ([change.rs L1622](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L1622), [change.rs L1655](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L1655)): `a/x` and
   `b?/y` name a feature the dependency has, else refused with its features named; `dep:b` an optional dependency,
   or a required one the declaration makes optional in the same step (cargo takes `dep:` only for an optional one);
   a plain name a feature the crate declares. The preflight says what the feature brings at each level and which
   entries are weak ([preflight.rs L384](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/preflight.rs#L384)). One refusal per close: code written by hand is CUST0031 and names
   the other files too; a file no operation writes, a features document among them, is CUST0041
   ([jobs.rs L1061](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/jobs.rs#L1061), [jobs.rs L1352](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/jobs.rs#L1352)); 0.3.5 counted every TOML file as code
   ([jobs.rs L973 at 0.3.5](https://github.com/1o-llc/custodian_kit/blob/797945c/src/jobs.rs#L973)). Proof: tests/operations.rs `declare_feature_takes_every_cargo_form_and_reads_it_through_the_feature_graph`
   (L190) and `declare_feature_refuses_a_feature_the_dependency_does_not_have_and_names_its_features` (L206) on a
   workspace with `a/x = ["c/z"]`; tests/close.rs `a_bounded_job_names_every_file_it_touched_and_a_file_no_operation_writes_needs_allow_file` (L129).
2. **A feature named like its dependency** (`chrono = ["dep:chrono"]`): klepto 0.3.4 writes it; through
   `declare_feature` too.
3. **A library beside a binary.** `make_lib` ([change.rs L63](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L63), [preflight.rs L287](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/preflight.rs#L287)); a module
   named `lib`, `main` or `mod` is refused and points to it ([preflight.rs L284](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/preflight.rs#L284)). Proof:
   `make_lib_is_for_a_crate_with_only_a_binary_and_a_module_named_lib_is_refused` (tests/operations.rs L112).
4. **A generator's function unseen.** The scan reads dependency macros through a frozen `cargo metadata`; while
   the tree's lock file lacked the dependency a transform had just added, it read none, and a function a toolkit
   generator writes was not there. code_passenger 0.4.4 writes the lock file with the manifest; the refusal now says
   why ([change.rs L1769](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L1769), was [change.rs L1585 at 0.3.5](https://github.com/1o-llc/custodian_kit/blob/797945c/src/change.rs#L1585)). Proof:
   `a_target_unseen_while_the_lock_file_is_behind_the_manifests_says_why` (tests/operations.rs L129).
5. **A signature and a body together.** `change_signature` with `function.body` writes both in one step, said in
   the preflight ([preflight.rs L465](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/preflight.rs#L465)). Proof: `change_signature_with_a_body_says_both_are_proved_together` (L143).
6. **A module's tests.** `test:<module>::*` verifies every test of a module ([jobs.rs L1227](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/jobs.rs#L1227), was
   [jobs.rs L1193 at 0.3.5](https://github.com/1o-llc/custodian_kit/blob/797945c/src/jobs.rs#L1193)); `add_test` into a module the file does not have makes it
   ([preflight.rs L514](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/preflight.rs#L514), was [preflight.rs L468 at 0.3.5](https://github.com/1o-llc/custodian_kit/blob/797945c/src/preflight.rs#L468)). Proof: `a_test_verification_names_a_test_or_every_test_of_a_module`
   (tests/close.rs L167), `add_test_into_a_module_the_file_does_not_have_says_it_is_made` (tests/operations.rs L151).
7. **A gate twice.** `gate` refuses an item its feature gates already ([preflight.rs L435](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/preflight.rs#L435)); code that
   compiles only with the feature on takes its `#[cfg]` in its `attrs` as it is made, said in the vocabulary.
   Proof: `gate_refuses_an_item_its_feature_gates_already` (L159).
8. **cfg_attr derives.** klepto 0.3.4: a use that exists only with the feature on is not lost when it is off; the
   pantry's "with json off" names `to_json` and its test, not all 26 items.
9. **The manifest's spacing.** code_passenger 0.4.4: `features = ["derive"], optional = true`.
10. **Features in a test run.** `testrun::Configuration` ([testrun.rs L44](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/testrun.rs#L44)): a line's feature
    configuration, carried in its target word (`lib [features json]`) so its outcomes are told apart and a close's
    rerun runs it the same way (baseline and handoff read it back); `Configuration::matrix` is the check matrix's
    ([testrun.rs L115](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/testrun.rs#L115)); `cargo test` takes the three flags ([exec.rs L159](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/exec.rs#L159)). custodian 0.3.4 has
    `test run --features`, `--all-features`, `--matrix`. Proof: `a_configuration_is_carried_in_the_target_word_and_read_back` ([testrun.rs L338](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/testrun.rs#L338)).
11. **One finding once.** A close's findings are each said once, and the lines that prove a fix only when a test of
    the job fails ([jobs.rs L1053-L1055](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/jobs.rs#L1053-L1055), was [jobs.rs L936 at 0.3.5](https://github.com/1o-llc/custodian_kit/blob/797945c/src/jobs.rs#L936)). Proof:
    `a_closes_findings_are_each_said_once_and_the_lines_that_prove_a_fix_only_with_a_failing_test` (tests/close.rs L176).
12. **init, then a job.** custodian 0.3.4 says the exact line: `custodian agent init`.
13. **A generator's docs.** A method generator's docs are written inside its call as `#[doc]` attributes; a `///`
    above a macro call documents nothing ([change.rs L1136-L1139](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L1136-L1139), was [change.rs L1059 at 0.3.5](https://github.com/1o-llc/custodian_kit/blob/797945c/src/change.rs#L1059)).
    Proof: `a_method_generators_docs_are_written_inside_its_call` (tests/operations.rs L216).
14. **Code in a value.** A `value` or `let` step that carries a closure or a macro call is reported with the raw
    code, flagged and never refused ([change.rs L617](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L617), [change.rs L630](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/change.rs#L630), was
    [change.rs L589 at 0.3.5](https://github.com/1o-llc/custodian_kit/blob/797945c/src/change.rs#L589)). Proof: `a_value_step_with_a_closure_or_a_macro_call_is_reported_not_refused` (L224).

### Proof, all of it

- `cargo test --all-features`: every test passes, the change schema's keys among them (`the_schemas_name_exactly_the_fields_of_the_rust_forms`, tests/oneo.rs L185).
- The exercise word holds 32 operations ([exercise.rs L140](https://github.com/1o-llc/custodian_kit/blob/b2a40dc/src/exercise.rs#L140)); it held 24, and the vocabulary has 26. No
  record decodes a word it kept.

## 0.3.5 (2026-10-20)

Three things an agent asked for as custodian's user, each read from the
records alone. After a job, the message a person reads was written by hand
each morning; now `handoff` makes it from the job's records. Before changing
a crate, nothing said who brought it in or why; now `why` does. And the
brief said "tokens: not measured here (no source in the records)"; now every
reading keeps what it stood for. Nothing is removed from the API. Every link
is pinned to a70c584.

- `handoff`: `of_job` reads a job's file, closing receipt, receipts, task
  history, transform records and count diff into one `Handoff`
  ([src/handoff.rs#L249](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/handoff.rs#L249)); `render` writes it in a fixed
  order, what waits on a person first, then what the job was for, what
  changed, what was decided and by whom, what was verified, and what runs
  next ([src/handoff.rs#L479](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/handoff.rs#L479)). Which failures are
  pre-existing is the close's own judgement from 0.3.3 (the baseline at open,
  else the base's worktree) when the closing receipt has one
  ([src/handoff.rs#L380](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/handoff.rs#L380)); for an older closing
  receipt, a failure is pre-existing only when a closed job's run before this
  job's first transform failed the same test
  ([src/handoff.rs#L390](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/handoff.rs#L390)), and
  otherwise it says it is not known. Lists stop at five, next steps at eight,
  the text at sixty lines. `Reader::Agent` adds the exact lines, a failed
  line's `cargo test` naming only its failures
  ([src/handoff.rs#L463](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/handoff.rs#L463)), and the routes. `job_folder`
  finds the job closed last by its closing time
  ([src/handoff.rs#L195](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/handoff.rs#L195)).
- `why`: `of` takes a `crate:` or `decl:` route ([src/why.rs#L188](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/why.rs#L188)):
  the task revisions whose change records touch its files
  ([src/why.rs#L142](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/why.rs#L142)), the linkage records that bind
  it ([src/why.rs#L169](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/why.rs#L169)), the readings that name it.
  git's first and last commit come from a declared line,
  `exec::git_log_paths` ([src/exec.rs#L177](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/exec.rs#L177)), which the
  caller runs with its receipt; `render` says "no record" for an empty
  section ([src/why.rs#L235](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/why.rs#L235)).
- `cost`: a reading's cost is the files its answer names, a row's file or a
  crate row's `Cargo.toml` ([src/cost.rs#L100](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/cost.rs#L100)), their bytes
  when it is made, and tokens estimated as bytes / 4
  ([src/cost.rs#L118](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/cost.rs#L118)). A reuse saves the same number
  ([src/cost.rs#L143](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/cost.rs#L143)). The record is kept under the
  receipt's number, `data/costs/<seq>.json`, a new part of the folder
  ([src/store.rs#L87](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/store.rs#L87)); the receipt is receipt_history's and
  is left as it is, because a field added to it would break every crate that
  builds one. The reading runner keeps a record for each reading
  ([src/runs.rs#L222](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/runs.rs#L222)) and each reuse
  ([src/runs.rs#L145](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/runs.rs#L145)). `sum` adds them per reading kind
  and per job ([src/cost.rs#L247](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/cost.rs#L247)), and `brief_line` is the
  brief's line ([src/cost.rs#L274](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/cost.rs#L274)).
- `receipts::is_reading_run` is the one rule for which receipts are reading
  runs, whatever word an older custodian wrote
  ([src/receipts.rs#L63](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/receipts.rs#L63)). The kit's own job of
  2026-10-19 wrote `run` and `count`; before, those readings were missed by
  anything that looked for `reading`.

Proof: `cargo test` has 253 tests, the 244 of 0.3.4 and nine new.
`a_handoff_is_made_from_the_records_in_its_order_and_pinned`
([tests/handoff.rs#L202](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/tests/handoff.rs#L202)) pins a bounded job's
handoff whole, for a person and for an agent: a refused preflight, a
decision answered and one open, two failures with one pre-existing, a test
line that stopped. `why_names_the_task_history_and_the_linkage_record_of_a_route`
([tests/handoff.rs#L220](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/tests/handoff.rs#L220)), `a_readings_cost_is_the_sum_of_its_files`
([src/cost.rs#L354](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/cost.rs#L354)),
`a_reuse_saves_the_readings_cost_and_the_brief_says_both`
([src/cost.rs#L372](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/cost.rs#L372)),
`the_closes_base_judgement_is_what_pre_existing_says`
([src/handoff.rs#L723](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/handoff.rs#L723)), the layout golden with
`custodian.cost` ([tests/layout.rs#L36](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/tests/layout.rs#L36)), and the rules'
unit tests `a_route_names_its_crate_its_items_and_its_dependency`
([src/why.rs#L297](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/why.rs#L297)),
`git_logs_lines_read_back_as_commits_and_their_ends`
([src/why.rs#L307](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/why.rs#L307)),
`a_failed_test_lines_cargo_line_names_only_its_failures`
([src/handoff.rs#L712](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/handoff.rs#L712)) and
`a_long_handoff_stops_at_its_cap_and_says_how_much_more`
([src/handoff.rs#L731](https://github.com/1o-llc/custodian_kit/blob/a70c584558f8e79ec75e211ccb78e21060eabd32/src/handoff.rs#L731)). The rustdoc JSON of 0.3.4
against this: 5,614 public items before, 5,944 after, 330 added, 0 removed
or changed.

## 0.3.4 (2026-10-20)

The parsed index 0.3.3 keeps was too big to keep: 975 MB to 1.1 GB for the kit's workspace, on a disk that ran down
to 126 MB free twice that night. Now the model is compressed and capped, and every reading that walks the code
parses. Nothing public removed: the rustdoc JSON of 0.3.3 and 0.3.4 differ only by added items. Pinned to 9f8e65c.

- The kept model is CBOR compressed with zstd (level 3, `INDEX_CACHE_ZSTD_LEVEL`), `index/<digest>/model.cbor.zst`:
  60 MB for the kit's workspace, read back in 3.4 s as before (was [src/reading/facts.rs#L139 at 0.3.3](https://github.com/1o-llc/custodian_kit/blob/75108fcb96af5dfcaad4cfd64954bb2bcf6bb56d/src/reading/facts.rs#L139),
  [src/reading/facts.rs#L179 at 0.3.3](https://github.com/1o-llc/custodian_kit/blob/75108fcb96af5dfcaad4cfd64954bb2bcf6bb56d/src/reading/facts.rs#L179); now [src/reading/facts.rs#L153](https://github.com/1o-llc/custodian_kit/blob/9f8e65cd8af4334b76f29c4e3736dcab70085ade/src/reading/facts.rs#L153), [src/reading/facts.rs#L195](https://github.com/1o-llc/custodian_kit/blob/9f8e65cd8af4334b76f29c4e3736dcab70085ade/src/reading/facts.rs#L195),
  [src/store.rs#L85](https://github.com/1o-llc/custodian_kit/blob/9f8e65cd8af4334b76f29c4e3736dcab70085ade/src/store.rs#L85)).
- A model over 256 MB compressed (`INDEX_CACHE_MAX_BYTES`) is not kept; the scan says so on the reading's record and
  its receipt, `parsed (35901 ms; not kept: 300 MB, over the 256 MB the cache holds)` (`IndexSource::ParsedNotKept`,
  [src/reading/facts.rs#L115](https://github.com/1o-llc/custodian_kit/blob/9f8e65cd8af4334b76f29c4e3736dcab70085ade/src/reading/facts.rs#L115), [src/reading/facts.rs#L142](https://github.com/1o-llc/custodian_kit/blob/9f8e65cd8af4334b76f29c4e3736dcab70085ade/src/reading/facts.rs#L142), [src/reading/facts.rs#L203](https://github.com/1o-llc/custodian_kit/blob/9f8e65cd8af4334b76f29c4e3736dcab70085ade/src/reading/facts.rs#L203)).
- 0.3.3's uncompressed `model.cbor` is let go by the next scan that keeps a model ([src/reading/facts.rs#L148](https://github.com/1o-llc/custodian_kit/blob/9f8e65cd8af4334b76f29c4e3736dcab70085ade/src/reading/facts.rs#L148)).
- `duplicates` parses, as `count` and `bundle` do: its shapes are the function bodies, which a model read back from
  the cache does not hold (was [src/reading/spec.rs#L125 at 0.3.3](https://github.com/1o-llc/custodian_kit/blob/75108fcb96af5dfcaad4cfd64954bb2bcf6bb56d/src/reading/spec.rs#L125); now [src/reading/spec.rs#L126](https://github.com/1o-llc/custodian_kit/blob/9f8e65cd8af4334b76f29c4e3736dcab70085ade/src/reading/spec.rs#L126)).

### Proof

- tests/index.rs, `the_second_scan_of_the_same_code_reads_the_index_back_and_a_change_parses_again`: the model on
  disk is `model.cbor.zst`, under the cap, 0.3.3's file beside it gone after the next scan;
  `a_reading_from_the_cache_says_so_and_a_count_parses_for_its_trees`: a duplicates reading says `index parsed`, and
  the line of a model not kept.
- On the kit's workspace (release build, a copy outside the kit): parsed in 34.8 s at 5.5 GB, kept as 60 MB (was
  1.1 GB); read back in 3.4 s, 64,133 items and 1,936 files the same; the 1.1 GB `model.cbor` was let go.
- Every suite passes: the lib tests and every integration file.

## 0.3.3 (2026-10-20)

Two real runs found these. The first bounded job in the kit (item-hold) did its work and closed `failed_test`: 26
tests failed, every one the same way at the commit it opened at (they need root, systemctl, getent), and the close
could not tell a test that was already failing from one the job broke. The exercise (custodian trying every
operation on copies of three projects, built on this branch and released here with it) found the rest. Nothing is
removed from the API: the rustdoc JSON of 0.3.2 and 0.3.3 differ only by added items. New code is pinned to 5a9fc99 (the inline rule's last two
checks to eb506f2); the exercise to f286e90.

### A test that fails the same way at the base is not the job's

- Was: any `FAILED` test the job ran made it `failed_test` ([src/jobs.rs#L918 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/jobs.rs#L918)).
- A job opens at a commit: `job start` records `base_commit` (`git rev-parse HEAD`, through the kit's declared git
  line) in the job file ([src/jobs.rs#L207](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L207), [src/jobs.rs#L610](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L610)).
- At close every failing test (its last outcome in the job) is run again on the job's tree for its first failing
  line, then looked up at the base: in the baseline when the job file says `baseline true`, else run on a worktree
  of the repository at the base commit, outside the project and kept for the next close at the same commit
  (`baseline::judge`, [src/baseline.rs#L252](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/baseline.rs#L252); `baseline::base_tree`, [src/baseline.rs#L217](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/baseline.rs#L217)). The same first
  failing line there makes it `pre-existing`; passing at the base, failing there another way, or passing when run
  again, makes it the job's, with why ([src/jobs.rs#L981](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L981)).
- The first failing line is the panic's message, not its place, the trees' own paths written `<root>`, so a job
  that edits the file a failing test sits in still matches ([src/baseline.rs#L94](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/baseline.rs#L94)).
- The baseline: with `baseline true`, `job start` runs the tests the target reaches (and every test of a crate it
  names) on the tree as it opens, and keeps them in `data/baseline.json` ([src/baseline.rs#L238](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/baseline.rs#L238),
  [src/jobs.rs#L674](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L674)). Its runs' receipts are `baseline`, never `test`, so they are never the job's tests.
- The closing receipt names each pre-existing test with its line and where it was judged, and the job's own
  failures with why (`pre_existing`, `failing`, `base_commit`: [src/jobs.rs#L847-L857](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L847-L857)). `failed_test` now means
  a test the job broke ([src/jobs.rs#L1031](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L1031)).
- `exec::CargoTest` takes `--bin`, so a binary's unit tests are run as cargo names them.

### Every file the job touched

- Was: the bounded check read only the counts' Rust and TOML digests, so a `.stderr` edit passed unseen
  ([src/jobs.rs#L1090 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/jobs.rs#L1090)).
- The closing receipt lists every file changed since the base (git's, new files too; a file that already differed
  at open counts from its state then, kept in `data/open_files.json`), each with what carries it: `transform
  #<seq>`, `allow_file <glob>`, `cargo` (the lock file cargo writes from the manifests), or `hand`
  ([src/jobs.rs#L1250-L1290](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L1250-L1290)).
- A bounded job's file that no operation writes must be named by an `allow_file` glob in its job file, else the
  close is refused, `[CUST0041]`, `rejected` ([src/reading/refusal.rs#L92](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/refusal.rs#L92)). `jobs::declare_allow` adds one to
  a running job with an `allow` receipt ([src/jobs.rs#L1409](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L1409)).

### Lines touched, exact

- Was: a count against a count, net per file: "+18 −0" for a 55-line diff ([src/jobs.rs#L806 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/jobs.rs#L806)).
- The job's figure is git's: `git diff --numstat` against the base, new files counted whole, cargo's lock file
  aside, said with the commit it is measured from ([src/jobs.rs#L1291](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L1291)). A step's figure is still the counts'
  and says `net`.

### The two facts, as set at open

- Was: the closing record said only when each became known, "open" for both ([src/jobs.rs#L1163 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/jobs.rs#L1163)).
- The closing record carries `target_known` and `outcome_known` as `job start` set them, and the page says
  `target_known: true at open · outcome_known: false at open, known at #128` ([src/jobs.rs#L819](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L819),
  [src/jobs.rs#L1394](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/jobs.rs#L1394)).

### A reading stored once

- Was: every reading was written twice, `latest.md` and `latest.json` beside the stamped files
  ([src/reading/mod.rs#L308-L310 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/reading/mod.rs#L308-L310)); the kit job's record was 33 MB.
- A spec's readings folder keeps `index.json`, naming the latest and every reading by stem; nothing is copied
  ([src/reading/mod.rs#L311-L317](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/mod.rs#L311-L317), [src/reading/mod.rs#L338](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/mod.rs#L338)).

### The parsed index, held once per code digest

- Was: every scan parsed the whole workspace and threw it away ([src/reading/facts.rs#L113 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/reading/facts.rs#L113)): 29 to 36 s and
  about 5.5 GB on the kit, for a reading of milliseconds.
- A scan in a project with a folder keeps klepto's model (klepto 0.3.3's serde form) as CBOR under
  `.custodian/state/index/<digest>/model.cbor`, keyed by klepto's model version, the scan options and the code
  digest, and the next scan of the same code reads it back; the newest two are kept
  ([src/reading/facts.rs#L129](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/facts.rs#L129), [src/reading/facts.rs#L139](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/facts.rs#L139), [src/reading/facts.rs#L177](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/facts.rs#L177);
  `store::named::INDEX_MODEL`, pathgen's `FileCbor`, [src/store.rs#L85](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/store.rs#L85)).
- The model holds no syntax trees, so `count` and `bundle`, which walk them, are answered over a parse
  (`ReadingKind::walks_trees`, [src/reading/spec.rs#L124](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/spec.rs#L124); `Facts::build_parsed`, [src/reading/facts.rs#L196](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/facts.rs#L196)).
- Where the index came from is on the reading's record (`index`) and its receipt's reason: `read · index from cache
  (3314 ms)` (`IndexSource`, [src/reading/facts.rs#L107](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/facts.rs#L107); [src/runs.rs#L201](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/runs.rs#L201)).
- On the kit's workspace (release build): parsed in 35.9 s at 5.7 GB; read back in 3.3 s at 1.8 GB, 64,133 items
  and 1,936 files the same; 1.1 GB on disk.

### Change operations

- `inline` and `wrap` ([src/change.rs#L61-L62](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/change.rs#L61-L62)). `inline`: each call becomes a block binding the parameters
  to its arguments, then the body; the function goes when it is not `pub`. What stops it is one rule the preflight
  and code_passenger read, `change::inline_blockers` (self, generics, async or unsafe, `return` or `?` in the body,
  calling itself, named as a value, `Self` in the body, a call from another file, where the body's names would not
  resolve: [src/change.rs#L1268](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/change.rs#L1268)). `wrap`: the spec's pieces first, then the body as it
  was as the block's value; none of the pieces may give the value (`render_wrapped`, [src/change.rs#L1223](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/change.rs#L1223);
  `render_inlined`, [src/change.rs#L1243](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/change.rs#L1243)). Their preflight checks: [src/preflight.rs#L471-L496](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/preflight.rs#L471-L496). The last two came from the exercise (`api::open` inlined into `cli`, `store::Store::new` into `api`) and are
  pinned to eb506f2: [src/change.rs#L1297](https://github.com/1o-llc/custodian_kit/blob/eb506f28675bdd7f4440e4c97b7bc86781d89053/src/change.rs#L1297),
  [src/change.rs#L1313](https://github.com/1o-llc/custodian_kit/blob/eb506f28675bdd7f4440e4c97b7bc86781d89053/src/change.rs#L1313).
- An attribute macro that rewrites a struct (`#[bitpack]` through `use big_bad_bool::bitpack`) refuses `add_field`
  and `add_constructor`, by the macro's name, `[CUST0040]`: an attribute whose path resolves to a crate's macro,
  the compiler's own attributes aside (`attribute_macros`, `rewritten_by`: [src/change.rs#L1364-L1400](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/change.rs#L1364-L1400);
  [src/preflight.rs#L497](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/preflight.rs#L497)).
- A binary's items land in its package, not its crate root: `stored::main` is `cli`'s (`change::package_of`,
  [src/change.rs#L1460](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/change.rs#L1460), [src/change.rs#L1535](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/change.rs#L1535); was [src/change.rs#L1349 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/change.rs#L1349)).
- A method's receiver is not a parameter when a signature change counts what it adds (`change::is_receiver`,
  [src/change.rs#L1259](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/change.rs#L1259); was [src/preflight.rs#L377 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/preflight.rs#L377)).
- `plan` offers `replace_body`, `wrap` and `inline` for a function and `change_dependency` for a crate.

### Readings

- `impact` answers for `*`, the whole workspace (was CUST0010: [src/reading/impact.rs#L79 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/reading/impact.rs#L79),
  [src/reading/impact.rs#L122 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/reading/impact.rs#L122); now [src/reading/impact.rs#L80](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/impact.rs#L80), [src/reading/impact.rs#L178](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/impact.rs#L178)).
- `affected_tests`: a changed file that holds no Rust (a manifest, a test's expected output) reaches every test of
  its crate ([src/reading/kinds.rs#L1276](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/kinds.rs#L1276); was [src/reading/kinds.rs#L1262 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/reading/kinds.rs#L1262)); a package with no library is
  never asked for `--lib`: its test's target is its binary ([src/reading/kinds.rs#L1197-L1202](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/src/reading/kinds.rs#L1197-L1202); was
  [src/reading/kinds.rs#L1198 at 0.3.2](https://github.com/1o-llc/custodian_kit/blob/a359c9f880b65a89de84b269c60dac6d873c7d63/src/reading/kinds.rs#L1198)).

### The exercise (job kind `exercise`, built on the branch `exercise`)

- Every operation tried on N targets of each unit kind it applies to, each on a copy of the project in the job's
  data folder: spec, preflight, transform, cargo check, the tests, the closing check; every result a receipt and
  every finding a diagnosis record of the job's task; the summary and the report ([src/exercise.rs](https://github.com/1o-llc/custodian_kit/blob/f286e904b89a4b2ba72561a33316cea625e0a93e/src/exercise.rs)).
- The preflight refuses a private field a struct literal elsewhere cannot name, and a `gate` while code outside the
  item uses it ungated; a job whose target is `*` resolves.

### Proof

- tests/close.rs: [tests/close.rs#L66](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/close.rs#L66) `a_test_failing_the_same_way_at_the_base_is_pre_existing_and_the_job_is_accepted`
  (a git repository, a test that always fails and one that passes; the job edits by hand; close: `accepted`,
  pre-existing `needs_the_gate` "the gate is not set" from `base <commit>`, the file listed, `job: +4 −0 across 1
  files ... (git diff --numstat ...)`); [tests/close.rs#L94](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/close.rs#L94) `a_test_the_job_breaks_is_the_jobs_and_the_baseline_answers_for_the_base`
  (the job breaks `vault_key`: `failed_test`, its test "ok at the base", the other pre-existing from the baseline);
  [tests/close.rs#L129](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/close.rs#L129) `a_bounded_job_names_every_file_it_touched_and_a_file_no_operation_writes_needs_allow_file`
  (`CUST0041`, then allowed).
- tests/index.rs: [tests/index.rs#L36](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/index.rs#L36) the second scan from the cache under 1 s, a changed file parsed again,
  two kept; [tests/index.rs#L65](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/index.rs#L65) the receipt says `index from cache`, a count parses; [tests/index.rs#L83](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/index.rs#L83)
  stored once; [tests/index.rs#L102](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/index.rs#L102) the impact of `*`; [tests/index.rs#L116](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/index.rs#L116) a manifest's and a
  `.stderr`'s change reach the crate's tests.
- tests/operations.rs: [tests/operations.rs#L44](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/operations.rs#L44) `#[keep] (marks::keep)` refused for both operations, a method
  authorized; [tests/operations.rs#L62](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/operations.rs#L62) inline authorized, refused on `return`, on a call from another crate and on `Self`; [tests/operations.rs#L75](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/operations.rs#L75) wrap.
- tests/bounded.rs: [tests/bounded.rs#L162](https://github.com/1o-llc/custodian_kit/blob/5a9fc991fe0bc53e3f4fc01406b2ad5e4abb5897/tests/bounded.rs#L162) the four combinations, the two facts as set at open.
- `baseline::tests::the_first_failing_line_is_the_panics_message_with_the_root_written_root`.
- Every suite passes: 151 lib tests and every integration file.

## 0.3.2 (2026-10-19)

Two operations the first real bounded job in the kit needed, and the change
vocabulary did not have. That job turns a hand-written lock-holding restart
into xccute's `Hold`, and moves the kit off its path copies of xccute onto
crates.io. Neither could be said as a change spec: no operation replaced a
function's body, and `add_dependency` refuses a dependency the crate already
names. Nothing is removed from the API. Every link is pinned to 2a99f02.

- `replace_body`: a function's body replaced (target: the function), the new
  body toolkit pieces like every other body. Its signature, docs and
  attributes stay as they are, so no caller changes
  ([src/change.rs#L59](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/change.rs#L59)). The preflight checks the target is a function the
  scan captured and that the spec's function is the target's own name
  ([src/preflight.rs#L409](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/preflight.rs#L409)). `change::render_body` renders a body from its
  opening brace to its closing one; `render_fn` writes it after the
  signature, so the two cannot differ ([src/change.rs#L1085](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/change.rs#L1085)).
- `change_dependency`: a dependency a crate already names, moved to the
  source the spec names ([src/change.rs#L60](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/change.rs#L60)). A version alone is the
  registry's and the path is dropped; a path goes with or without a version;
  features given replace the entry's. The target is `crate:<name>`, or `*`
  for every crate of the workspace naming it ([src/change.rs#L1301](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/change.rs#L1301)).
  Several dependencies in `dependencies` move together in one spec, so a
  family is never split across two specs ([src/change.rs#L450](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/change.rs#L450)).
  `change::dependency_changes` lists every line it changes, one rule for the
  preflight and the writer; a crate target that does not name the dependency
  is refused, so `add_dependency` stays the one way to add one
  ([src/change.rs#L1315](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/change.rs#L1315)). The preflight's write set is those manifests
  ([src/preflight.rs#L396](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/preflight.rs#L396)), and they are the change's own files, so a
  `*` change is in its scope without a widened `scope.write`
  ([src/preflight.rs#L441](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/preflight.rs#L441)).
- `add_test` takes `to`: the inline test module of the file it lands in, by
  name (`tests` when unset). The preflight checks the module is there
  ([src/preflight.rs#L417](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/src/preflight.rs#L417)).
- The change schema and the README's operations table carry both operations
  and the `dependencies` block.

Proof: [tests/bounded.rs#L343](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/tests/bounded.rs#L343) (a `*` change of two dependencies writes
the two manifests that name them, 3 lines; a crate that does not name one is
refused) and [tests/bounded.rs#L361](https://github.com/1o-llc/custodian_kit/blob/2a99f025864e23cbf78b7ade3d8029d81ea88931/tests/bounded.rs#L361) (a body replaced on its own
function is authorized, on another refused; the body renders from its
pieces). The schema test holds the schema equal to the Rust forms with
`dependencies` in them. All suites pass: 146 + 13 + 19 + 5 + 2 + 10 + 24.

## 0.3.1 (2026-10-19)

One rule, one place. The `.custodian` folder was declared three times: here,
in custodian's `src/layout.rs` and in differences_kit's `layout.rs`. Which
receipt made a reading was decided twice, and differently. A plan could not
name the crates its intent named, and a job that set out to make a reading
closed `pending` after making it. Each is now decided once, here. Nothing is
removed from the API. Every link is pinned to 0d4ac01; the earlier lines
are pinned to 35968aa (0.3.0).

- The folder is declared once, in `store::named`. The parts custodian kept
  for itself (a reading's record, a check's JSON, a log) and the tree
  snapshot differences_kit kept are declared here with the schema, route and
  place they had ([src/store.rs#L76](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/store.rs#L76) to
  [src/store.rs#L82](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/store.rs#L82)). `named::ALL` lists every part, each once
  ([src/store.rs#L85](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/store.rs#L85)). `named::policy` and `named::resolve` are the one
  way a part becomes a path ([src/store.rs#L92](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/store.rs#L92),
  [src/store.rs#L97](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/store.rs#L97)); the store resolves through them, and so do
  custodian and differences_kit. `store::job_folders` lists every job folder
  with its state ([src/store.rs#L306](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/store.rs#L306)), and `jobs::READINGS_DIR` names
  a job folder's readings ([src/jobs.rs#L212](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/jobs.rs#L212)).
- Which receipt made a reading, and where its record is now, is
  `receipts::reading_of` ([src/receipts.rs#L69](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/receipts.rs#L69)), with
  `receipts::readings_of` for one spec ([src/receipts.rs#L86](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/receipts.rs#L86)). A
  reading kept in a job's folder is found wherever that folder is now. The
  receipts are the one source: custodian's readings index, `differences
  readings` and a job's close read this rule. A job's close read the journal's
  own query before ([src/jobs.rs#L769](https://github.com/1o-llc/custodian_kit/blob/35968aa7b93a719854c64753c7a428db71eaa61e/src/jobs.rs#L769)).
- A plan resolves what its intent names before it matches words.
  `plan::resolve_named` turns each crate, `decl:` set, file path and
  declaration the intent names into a target through the index
  ([src/plan.rs#L235](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/plan.rs#L235)). Those targets are read and stepped first
  ([src/plan.rs#L317](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/plan.rs#L317)). The rest of the words are matched as before
  ([src/plan.rs#L259](https://github.com/1o-llc/custodian_kit/blob/35968aa7b93a719854c64753c7a428db71eaa61e/src/plan.rs#L259)). A plain word is asked about only when the intent
  named no target, and a name spelled as code that resolves to nothing is
  still asked ([src/plan.rs#L411](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/plan.rs#L411)). Before, every word was matched, so
  `merge lease_matrix_step3d_core into lease_matrix_core` asked where both
  crates go ([src/plan.rs#L339](https://github.com/1o-llc/custodian_kit/blob/35968aa7b93a719854c64753c7a428db71eaa61e/src/plan.rs#L339)).
- A `reading:<spec>` verification means the job changed what it set out to
  change. It is satisfied when the job's last reading of the spec differs
  from the latest reading before the job opened, or when the spec was never
  read before and this is its first reading ([src/jobs.rs#L1039](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/src/jobs.rs#L1039)). The
  same reading again is `unchanged`; no reading in the job is `not run`.
  Before, a first reading had nothing to compare with and stayed `not run`
  ([src/jobs.rs#L1040](https://github.com/1o-llc/custodian_kit/blob/35968aa7b93a719854c64753c7a428db71eaa61e/src/jobs.rs#L1040)).

Proof: the rustdoc JSON of 0.3.0 compared with this release: 5,084 public
items before, 5,106 after, 22 added, none removed or changed. `cargo test`
runs 217 tests, the 213 of 0.3.0 and four new:
`every_part_is_named_once_and_where_the_golden_says` pins every part's route
and path ([tests/layout.rs#L38](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/tests/layout.rs#L38)),
`a_part_resolves_under_the_projects_folder_and_never_leaves_it`
([tests/layout.rs#L48](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/tests/layout.rs#L48)),
`an_intent_naming_two_crates_targets_them_and_asks_nothing_about_them`
([tests/bounded.rs#L303](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/tests/bounded.rs#L303)) and
`a_reading_verification_is_the_job_changing_the_reading_it_named`
([tests/bounded.rs#L204](https://github.com/1o-llc/custodian_kit/blob/0d4ac01bb6274a7dbe669c0e05c96449ada1846d/tests/bounded.rs#L204)).

## 0.3.0 (2026-10-19)

The history machinery leaves this crate. Each kind of history is now a crate
of its own, and this crate calls them where it held the code itself. Every
item it exported before is still exported at the same path, so a program
built against 0.2.0 builds against 0.3.0: custodian 0.2.0's source, with only
its custodian_kit requirement raised, builds against it and its 55 tests
pass. One kind of signature change comes with the move, named below.
Every link is pinned to cc483ae; the
earlier lines are pinned to b17adf7 (0.2.0).

- The receipts journal is receipt_history's. `store::Journal`,
  `store::JobRow`, `store::TestRow`, `store::JOURNAL_SCHEMA` and
  `store::JOURNAL_BUSY` re-export it ([src/store.rs#L279](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/store.rs#L279),
  [src/store.rs#L21](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/store.rs#L21)). So do `receipts::Receipt`, `RECEIPT_SCHEMA`, the
  runners ([src/receipts.rs#L13](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/receipts.rs#L13)) and `digest` ([src/digest.rs#L9](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/digest.rs#L9)).
  These were [src/store.rs#L280](https://github.com/1o-llc/custodian_kit/blob/b17adf76375f72bebfb5ae1d1d93de3826369aac/src/store.rs#L280), [src/receipts.rs#L15](https://github.com/1o-llc/custodian_kit/blob/b17adf76375f72bebfb5ae1d1d93de3826369aac/src/receipts.rs#L15) and
  [src/digest.rs#L30](https://github.com/1o-llc/custodian_kit/blob/b17adf76375f72bebfb5ae1d1d93de3826369aac/src/digest.rs#L30).
  One behaviour changes. `Journal::record` now takes its number inside the
  writing transaction, above every row. Before, a second journal open on the
  same file could insert a number that was already taken.
  `store::write_whole` keeps its signature and calls receipt_history's
  ([src/store.rs#L123](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/store.rs#L123)).
- A job's revisions are revision_history's. `task_records` keeps `root`,
  `open`, `of_job` and `read`, which name the project's history root, and
  re-exports `TaskRef`, `task_id`, `solution`, `verification`, `close`,
  `close_noted` and `brief_lines` ([src/task_records.rs#L28](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/task_records.rs#L28)).
- The count model is tree_history's. `reading::count` re-exports
  `Lines`, `FileCount`, `CountReport`, `Moved`, `CountDiff`,
  `compare_counts`, `Within`, `within` and `CountSummary`
  ([src/reading/count.rs#L38](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/reading/count.rs#L38)). This crate still counts, with tokei and
  klepto. `count::compare` reads two of the kit's readings as counts
  ([src/reading/count.rs#L230](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/reading/count.rs#L230)). `count::history` reads the journal's
  receipts through tree_history ([src/reading/count.rs#L239](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/reading/count.rs#L239)). Before, these
  were [src/reading/count.rs#L45](https://github.com/1o-llc/custodian_kit/blob/b17adf76375f72bebfb5ae1d1d93de3826369aac/src/reading/count.rs#L45) to [src/reading/count.rs#L565](https://github.com/1o-llc/custodian_kit/blob/b17adf76375f72bebfb5ae1d1d93de3826369aac/src/reading/count.rs#L565).
  `count::share` is now public.
- Lines touched are change_history's. `jobs::Taken`, `jobs::Touched`,
  `jobs::JOB_DATA` and the new public `jobs::COUNTS_FILE`
  ([src/jobs.rs#L208](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/jobs.rs#L208)) and `jobs::lines_touched_of` ([src/jobs.rs#L265](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/jobs.rs#L265))
  re-export it. `jobs::lines_within` resolves the jobs through the journal
  and sums their closing receipts there ([src/jobs.rs#L269](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/jobs.rs#L269)).
  `task_records::changes` re-exports its change records
  ([src/task_records.rs#L27](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/task_records.rs#L27)). Before, these were [src/jobs.rs#L208](https://github.com/1o-llc/custodian_kit/blob/b17adf76375f72bebfb5ae1d1d93de3826369aac/src/jobs.rs#L208) to
  [src/jobs.rs#L340](https://github.com/1o-llc/custodian_kit/blob/b17adf76375f72bebfb5ae1d1d93de3826369aac/src/jobs.rs#L340) and [src/task_records.rs#L88](https://github.com/1o-llc/custodian_kit/blob/b17adf76375f72bebfb5ae1d1d93de3826369aac/src/task_records.rs#L88).
- The reading diff is diff_history's. `diff` re-exports `RowKey`,
  `CountChange`, `ReadingDiff` and `render` ([src/diff.rs#L13](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/diff.rs#L13)). New:
  `diff::reading_of` reads a `ReadingReport` as diff_history reads a
  reading ([src/diff.rs#L20](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/diff.rs#L20)). `diff::compare` keeps its signature
  ([src/diff.rs#L29](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/diff.rs#L29)). Before, it was [src/diff.rs#L56](https://github.com/1o-llc/custodian_kit/blob/b17adf76375f72bebfb5ae1d1d93de3826369aac/src/diff.rs#L56).
- The signature change: a re-exported function or method returns its history
  crate's error, not `CustodianError`. This crate converts each of them
  into `CustodianError` and into `api::Refusal` ([src/reading/refusal.rs#L120](https://github.com/1o-llc/custodian_kit/blob/cc483ae5e69b3874c90ea20503692aae833e599b/src/reading/refusal.rs#L120)),
  so `?` reads them as before. Where this crate refused the same thing
  before the move, the refusal keeps the kit's own code: the journal
  `CUST0012`, a write `CUST0009`, a reading that cannot be read back
  `CUST0017`, a missing input `CUST0021`. Any other refusal keeps the
  history crate's code.
- New dependencies: receipt_history, revision_history, tree_history,
  change_history and diff_history, each 0.1.0. tree_history brings xccute
  0.3.1 (`LineChain`).

Proof: the rustdoc JSON of 0.2.0 compared with this release. Of 5,493 public
items in 0.2.0, 5,120 are at the same path. The other 373 are re-exports whose
item, with its fields, methods and trait impls, is in its history crate.
None is removed. Three items are added: `diff::reading_of`,
`jobs::COUNTS_FILE` and `reading::count::share`. `cargo test` runs 213
tests, every test of 0.2.0 but one: `digest::tests::canonical_forms_do_not_depend_on_field_order`
moved with the digest to receipt_history, where it passes. custodian 0.2.0's
source builds against this release, its custodian_kit requirement raised to
0.3.0, and its 55 tests pass.

## 0.2.0 (2026-10-19)

The bounded and unbounded increment of the 2026-10-19 specification. An agent
can now ask, before it changes anything, what a change to one item entails and
what it would cost; write the change as a spec; have it decided against the
code as it is; and, in a bounded job, have the code written for it and every
touched line accounted for at close. Nothing is removed from the API. Three
behaviours change, each named below: a reading with no declaration source
refuses or says so instead of counting zeros; a job with no verification
closes `pending`, not `accepted`; and the same-search key is the kit's, so a
spec read under 0.1.1 is read once more under 0.2.0. Every link is pinned to
5fe79c7.

### Reading before changing

- The `impact` reading: one target's definitions, use sites, constructions,
  matches without a catch-all, impl blocks, uses from other crates,
  dependents, reach, the tests covering it, the features and klepto's check
  matrix, the linkage records naming it, and the cost of changing it by
  dimension, each marked measured, heuristic or unknown. Runtime cost, memory
  layout and concurrency are said unknown with the reason, never written as
  a number. Its record is a task_history `diagnosis` record.
  [src/reading/impact.rs#L106](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/impact.rs#L106), the basis [src/reading/impact.rs#L40](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/impact.rs#L40),
  the unknowns [src/reading/impact.rs#L267](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/impact.rs#L267), the record
  [src/reading/impact.rs#L318](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/impact.rs#L318). Proven by
  `a_struct_is_read_with_its_uses_constructions_impls_dependents_and_tests`
  ([tests/impact.rs#L33](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/impact.rs#L33)) and
  `a_change_under_several_feature_configurations_counts_them`
  ([tests/impact.rs#L77](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/impact.rs#L77)).
- One same-search key, the kit's: `api::SearchKey` (the spec's hash and
  klepto's fingerprint of the code its scope reads), [src/api.rs#L137](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/api.rs#L137).
  And one search spec per target, `ReadingSpec::search`
  ([src/reading/spec.rs#L349](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/spec.rs#L349)): the command line's `run TARGET --kind
  impact`, a preflight's impact and a plan's are the same spec, so each
  reuses the others' reading. On the kit a preflight reused the impact
  `custodian run` had read (receipts #77, #78). Proven by
  `a_preflight_finds_the_impact_the_command_line_already_read`
  ([tests/bounded.rs#L273](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/bounded.rs#L273)).
- `reach` lists the env keys its reached items read, from the registry's
  `reads_env` marks (rows `reach-env`, [src/reading/kinds.rs#L931](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/kinds.rs#L931)).
- A reading that reads declarations and has none no longer answers zeros.
  `sockets` refuses, CUST0039 ([src/reading/kinds.rs#L738](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/kinds.rs#L738)); `reach` and
  `readers_of_env` say they read no declarations. A workspace names one
  default source in `config/declared.toml` ([src/reading/kinds.rs#L488](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/kinds.rs#L488)).
  Before, [src/reading/kinds.rs#L695](https://github.com/1o-llc/custodian_kit/blob/528b2fd003a7cd95ce00c67137a291777a430319/src/reading/kinds.rs#L695) answered 0 entries and 0 sockets
  for `relayerd` on the kit when the spec named no source (receipt #62); with
  the job's source it is 2 and 1 (receipt #24). Proven by
  `a_reading_with_no_declaration_source_refuses_or_says_so_never_zeros`
  ([tests/reading.rs#L353](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/reading.rs#L353)).
- `duplicates` heads its answer with its top three crate pairs
  ([src/reading/kinds.rs#L661](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/kinds.rs#L661)); `the_duplicates_summary_names_the_top_crate_pairs`
  ([tests/reading.rs#L376](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/reading.rs#L376)).

### Change specs and the preflight

- The change spec (1o modifier `change`, and its TOML twin), one document for
  the preflight and the writer: `change::ChangeSpec`
  ([src/change.rs#L408](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/change.rs#L408)), its 20 operations declared once
  ([src/change.rs#L38](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/change.rs#L38)): creation first (make_struct, make_enum,
  make_impl, add_method, make_fn, add_field, add_variant, add_derive,
  add_attribute, add_module, add_dependency, add_test), then add_constructor,
  add_trait_impl, change_signature, rename, move, extract, gate, anchor.
  Where each lands is one rule, `change::landing` ([src/change.rs#L1282](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/change.rs#L1282)),
  read by the preflight and by code_passenger.
- A function body is simple_impl 0.1.11's toolkit pieces, not free text:
  `change::PIECES` ([src/change.rs#L638](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/change.rs#L638)) is its 123 macros plus `let`,
  `value` and `raw`, and `the_pieces_are_the_toolkit_exactly`
  ([src/change.rs#L1350](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/change.rs#L1350)) holds it equal to the published registry. A
  `raw` piece is allowed and flagged.
- `preflight::run_preflight` ([src/preflight.rs#L510](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/preflight.rs#L510)) decides a spec
  over the code as it is, before anything is written: where it lands, its
  impact, its preconditions, the files it would write against the files it
  may write, the decisions it leaves open (`preflight::assess`,
  [src/preflight.rs#L242](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/preflight.rs#L242)). Its answer is a big-bad-bool word whose flaga
  rules authorize it ([src/preflight.rs#L45](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/preflight.rs#L45)). A file outside the scope
  is a scope expansion asked for, never taken. On the kit, a field added to
  `nodeplan_sdk::uds_contract::UdsRouteTable` was not authorized: its default
  fills 19 struct literals, in 18 files of other crates (receipt #79).
  Proven by `a_field_with_a_default_is_authorized_and_lands_inside_the_struct`
  ([tests/bounded.rs#L53](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/bounded.rs#L53)),
  `a_write_outside_the_scope_is_asked_for_never_taken_and_a_taken_name_is_a_collision`
  ([tests/bounded.rs#L93](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/bounded.rs#L93)), `the_word_authorizes_only_when_its_rules_hold`
  ([src/preflight.rs#L644](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/preflight.rs#L644)).
- `preflight::gate` ([src/preflight.rs#L623](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/preflight.rs#L623)): a code-changing step runs
  only on a preflight of this spec's digest over this code's digest. A job
  step that changes code now runs code_passenger's `transform` line
  ([src/jobs.rs#L730](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/jobs.rs#L730)); CUST0024 is raised only for a step kind with no
  line ([src/jobs.rs#L753](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/jobs.rs#L753); before, every such step:
  [src/jobs.rs#L610](https://github.com/1o-llc/custodian_kit/blob/528b2fd003a7cd95ce00c67137a291777a430319/src/jobs.rs#L610)). `a_code_changing_step_needs_a_preflight_of_this_spec_over_this_code`
  ([tests/bounded.rs#L108](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/bounded.rs#L108)).

### Decisions, plans, linkage

- A question is asked once. `questions::ask` writes a task_history `problem`
  record whose id is the question's digest, `questions::answer` a `solution`
  with the answer and the reason, and `questions::open` leaves out any
  question answered in any task history of the project
  ([src/questions.rs#L130](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/questions.rs#L130), [src/questions.rs#L149](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/questions.rs#L149),
  [src/questions.rs#L94](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/questions.rs#L94)). `a_decision_answered_once_is_never_asked_again_in_any_task`
  ([tests/bounded.rs#L133](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/bounded.rs#L133)).
- `plan::run_plan` ([src/plan.rs#L244](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/plan.rs#L244)): an intent read against the code
  (candidates, reuse, what is new, the top candidates' impact, the cost,
  alternatives only where the facts give one, the decisions asked together,
  the steps). The thing the intent names ranks first (`rank`,
  [src/plan.rs#L148](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/plan.rs#L148)): on the kit, `UdsRouteTable` ranked fourth by word
  count alone, behind two functions of nodeplan_macros and a test. A plan keeps 25 candidates and
  counts the rest ([src/plan.rs#L227](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/plan.rs#L227)); on the kit the first record was
  1.3 MB. `plan::approve` writes a step as a change spec with TODO
  placeholders the preflight refuses ([src/plan.rs#L510](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/plan.rs#L510)).
  `an_intent_is_planned_from_the_code_and_a_step_becomes_a_change_spec`
  ([tests/bounded.rs#L248](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/bounded.rs#L248)), `the_named_thing_ranks_before_names_that_share_its_words`
  ([src/plan.rs#L558](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/plan.rs#L558)).
- Linkage records: why declarations are bound together, seven kinds, each
  bind resolved when recorded and at every check; holding, review or stale;
  a `// linkage:` anchor no record backs is a finding
  ([src/linkage.rs#L49](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/linkage.rs#L49), [src/linkage.rs#L191](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/linkage.rs#L191),
  [src/linkage.rs#L232](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/linkage.rs#L232)). `a_linkage_record_binds_two_declarations_and_goes_stale_when_one_is_gone`
  ([tests/impact.rs#L108](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/impact.rs#L108)).

### Bounded and unbounded jobs

- `jobs::JobMode` ([src/jobs.rs#L94](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/jobs.rs#L94)). A bounded job's touched Rust and
  TOML lines must each be covered by one of its transform records
  (`transform::coverage`, [src/transform.rs#L107](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/transform.rs#L107)); a line no transform
  covers refuses the close, CUST0031, and the job is closed `rejected`, its
  record terminal. Documentation is not code
  ([src/jobs.rs#L1133](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/jobs.rs#L1133)). A bounded job needs code-passenger installed,
  CUST0037 ([src/jobs.rs#L659](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/jobs.rs#L659)). Unbounded is help: nothing is refused.
- Every job records whether its target is known and whether its outcome is
  known (its `verify` lines, [src/jobs.rs#L104](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/jobs.rs#L104)) at open, and the closing
  receipt the receipt at which each became true. `accepted` needs every
  verify line satisfied by the job's own runs; with none the result is
  `pending` ([src/jobs.rs#L1085](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/jobs.rs#L1085)). Before, a job closed `accepted` unless
  a test failed: [src/jobs.rs#L785](https://github.com/1o-llc/custodian_kit/blob/528b2fd003a7cd95ce00c67137a291777a430319/src/jobs.rs#L785). `the_four_combinations_of_target_and_outcome_known`
  ([tests/bounded.rs#L162](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/bounded.rs#L162)),
  `a_bounded_job_refuses_hand_edits_at_close_and_needs_its_facts_before_a_preflight_authorizes`
  ([tests/bounded.rs#L216](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/tests/bounded.rs#L216)), `a_file_is_covered_only_when_its_transforms_carry_it_open_to_close`
  ([src/transform.rs#L160](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/transform.rs#L160)).
- The task states (proposed, ready, running, blocked, paused, verifying,
  completed, failed) are one word derived from the records
  ([src/tasks.rs#L19](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/tasks.rs#L19)); `the_states_are_the_specification_s_words`
  ([src/tasks.rs#L68](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/tasks.rs#L68)).

### The rest

- Refusals CUST0031–CUST0039 ([src/reading/refusal.rs#L72](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/reading/refusal.rs#L72) onward).
- simple_impl_derive 0.2.7, xccute_contract 0.1.3, xccute_decisions 0.2.1;
  `rust-version` 1.89, and CI tests on it. The journal's busy timeout is one
  named value, `store::JOURNAL_BUSY` ([src/store.rs#L21](https://github.com/1o-llc/custodian_kit/blob/5fe79c7dd7116094ced976fe44448e62e7dc91f3/src/store.rs#L21)).
- Public API: 1131 items added, none removed (rustdoc JSON of 528b2fd against
  5fe79c7). Every new file carries the notice
  Copyright © 2026 1o. All rights reserved.; the existing notices are as they
  were.
- 213 tests: 147 unit, 9 in tests/bounded.rs, 5 in tests/impact.rs, and the
  0.1.1 suites.

## 0.1.1 (2026-10-19)

Additive: nothing 0.1.0 does changes.

- A job template's step can read an earlier step's receipts: `Step::reads`
  names the step; the step waits for it on `scope` (`runner::dependencies`
  counts it), and each receipt file that step kept (relative to the root) is
  its target, or fills `{receipt}` in its `target`. A step that left no
  receipt gives nothing, and the step-input decision skips the reader. The
  receipts come from the executor (`RunStep::receipt`, defaulting to none;
  `job start`'s executor names the receipt of each reading it ran) and are
  kept by step in `StepEnv::receipts`. This is code_passenger's
  `features-apply` shape (plan → apply → annotate, each reading the one
  before) and the triage contract's "a step may read the prior step's
  receipt as its target". Proven by
  `a_step_that_reads_another_waits_for_it_and_takes_its_receipts_as_targets`
  in tests/count.rs. `Step` and `StepEnv` gain a field: a struct literal of
  either outside the kit must name it (`reads: None`).
  The field: [src/jobs.rs#L116-L121](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/src/jobs.rs#L116-L121). The wait:
  [src/jobs_runner.rs#L69-L71](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/src/jobs_runner.rs#L69-L71). The receipts as
  targets: [src/jobs_runner.rs#L95-L96](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/src/jobs_runner.rs#L95-L96). The test:
  [tests/count.rs#L439](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/tests/count.rs#L439).
- `jobs::template(root, kind)` and `jobs::templates(root)` load the job
  templates as the project at or above `root` has them, else as shipped, so
  a consumer reads the kit's templates through the kit instead of parsing
  the files itself. The loader the job code used is the same one
  (`template_in`). Proven by
  `templates_load_through_the_kit_shipped_or_as_the_project_has_them`.
  [src/jobs.rs#L415](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/src/jobs.rs#L415),
  [src/jobs.rs#L428](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/src/jobs.rs#L428); the test:
  [tests/count.rs#L499](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/tests/count.rs#L499).
- A job's close reads a test's outcome in cargo's words, as the journal's
  `tests` table keeps them (`ok`, `FAILED`, `ignored`; the kit's own test
  runner writes these): `FAILED` is a failed verification and makes the
  result `failed_test`. 0.1.0 matched only `failed`/`fail`, so a failed
  test was recorded `pending` and the job `accepted`. Proven by
  `a_failed_test_in_cargo_s_words_fails_the_jobs_result`.
  Was: [src/jobs.rs#L725](https://github.com/1o-llc/custodian_kit/blob/9a176fc41648dbbb8da97511321c2b0d89ccaaca/src/jobs.rs#L725) and
  [src/jobs.rs#L733](https://github.com/1o-llc/custodian_kit/blob/9a176fc41648dbbb8da97511321c2b0d89ccaaca/src/jobs.rs#L733). Now:
  [src/jobs.rs#L775](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/src/jobs.rs#L775) and
  [src/jobs.rs#L783-L785](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/src/jobs.rs#L783-L785). The test:
  [tests/count.rs#L303](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/tests/count.rs#L303).
- `rust-version` is 1.88 (tantivy 0.26.1 needs 1.86; time, globset, ignore
  and darling in the lock need 1.88), and CI tests stable and 1.88: 0.1.0's
  1.85 job could not build.
  [Cargo.toml#L5-L6](https://github.com/1o-llc/custodian_kit/blob/77e626f9e29e36ff363b2a2a8be4aa6be766c81d/Cargo.toml#L5-L6).
- CI publishes: on master, once the tests and clippy pass, a version not yet
  on crates.io with its dated entry here publishes through crates.io trusted
  publishing (.github/workflows/ci.yml).

## 0.1.0 (2026-10-19)

The first version. It is step42's code_userland_kit, lifted onto klepto: the
regex and walkdir scanning is gone, and every fact a reading uses comes from
one klepto scan.

- The graph, queries, pins, context windows, Tantivy search and registry are
  kept as they were, and their receipt schemas are unchanged. The 45-command
  step42 suite passes against them.
- Reading specs: sixteen kinds (`ReadingKind`), declaration roles
  (`DeclaredRole`), each declared once with the README table generated from it.
- Paths go through the declared routes table (pathgen); a job resolves
  declared routes only, and `sweep` is the one walk.
- Every digest is `stable_digest`'s sha256 over a declared canonical form. The
  fnv1a64 ids are gone, so five test expectations moved to sha256 values.
- Refusals are liaise codes `CUST0001`…`CUST0028`.
- `api.rs` is the stable surface the `custodian` binary binds to.
- Bundles go through bundles 0.2.3: an explicit file set, the filters word,
  and one redaction marker for a held item, in a bundle, a job bundle and a
  reading's row alike. Its refusals are CUST0101-0107 in the one table.
- Scope depth: how deep a dependency sits in a crate (item, function,
  module, file, crate) and what goes with it off, for `features` and `reach`.
- `count`: a route counted by tokei, its tests' share by klepto, a digest
  the same tree reproduces, `compare` and `history`, and lines touched in a
  job's closing receipt per step and for the job.
- Jobs are task histories: a job writes task, intent, solution, change,
  verification and result records (task_history) for each run, and its brief
  reads them back. Receipts have a module of their own (`receipts`).
- A template job's steps run on `scope`: steps that read nothing of each
  other run together in the scope's fixed order, a step that reads another's
  readings waits for it, and the open workspace is borrowed from the scope's
  environment. Receipts and task-history records are as before.
- A receipt for every run, whichever runner answered: a reading answered in
  this process over an open workspace (`runs::Runner::InProcess`,
  `jobs::start_with`/`close_with`) builds the same declared line, takes the
  same decision on its status and keeps the same receipt, its `runner` field
  saying `in_process` (or `line`). The journal gains the `runner` column; an
  older journal gets it with every run a line run.
- Reading specs, job files and the paths table are 1o documents with shipped
  schemas, read by core-1o into the same form TOML reads into.

Proof: `cargo test` (tests/reading.rs, one test per kind, on the fixture
workspace in tests/fixtures/ws), and the step42 suite.
