# Changelog

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
