# Changelog

## 0.3.5 (unreleased)

Three things an agent asked for as custodian's user, each read from the
records alone. After a job, the message a person reads was written by hand
each morning; now `handoff` makes it from the job's records. Before changing
a crate, nothing said who brought it in or why; now `why` does. And the
brief said "tokens: not measured here (no source in the records)"; now every
reading keeps what it stood for. Nothing is removed from the API. Every link
is pinned to a8f0f53.

- `handoff`: `of_job` reads a job's file, closing receipt, receipts, task
  history, transform records and count diff into one `Handoff`
  ([src/handoff.rs#L245](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/handoff.rs#L245)); `render` writes it in a fixed order, what
  waits on a person first, then what the job was for, what changed, what was
  decided and by whom, what was verified, and what runs next
  ([src/handoff.rs#L465](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/handoff.rs#L465)). A failure is pre-existing only when a closed
  job's run before this job's first transform failed the same test
  ([src/handoff.rs#L378](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/handoff.rs#L378)); otherwise it says it is not known. Lists stop
  at five, next steps at eight, the text at sixty lines. `Reader::Agent` adds
  the exact lines, a failed line's `cargo test` naming only its failures
  ([src/handoff.rs#L449](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/handoff.rs#L449)), and the routes. `job_folder` finds the job
  closed last by its closing time ([src/handoff.rs#L191](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/handoff.rs#L191)).
- `why`: `of` takes a `crate:` or `decl:` route ([src/why.rs#L188](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/why.rs#L188)): the
  task revisions whose change records touch its files
  ([src/why.rs#L142](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/why.rs#L142)), the linkage records that bind it
  ([src/why.rs#L169](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/why.rs#L169)), the readings that name it. git's first and last
  commit come from a declared line, `exec::git_log_paths`
  ([src/exec.rs#L144](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/exec.rs#L144)), which the caller runs with its receipt; `render`
  says "no record" for an empty section ([src/why.rs#L235](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/why.rs#L235)).
- `cost`: a reading's cost is the files its answer names, a row's file or a
  crate row's `Cargo.toml` ([src/cost.rs#L100](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/cost.rs#L100)), their bytes when it is
  made, and tokens estimated as bytes / 4 ([src/cost.rs#L118](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/cost.rs#L118)). A reuse
  saves the same number ([src/cost.rs#L143](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/cost.rs#L143)). The record is kept under
  the receipt's number, `data/costs/<seq>.json`, a new part of the folder
  ([src/store.rs#L84](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/store.rs#L84)); the receipt is receipt_history's and is left as it
  is, because a field added to it would break every crate that builds one.
  The reading runner keeps a record for each reading
  ([src/runs.rs#L219](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/runs.rs#L219)) and each reuse ([src/runs.rs#L145](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/runs.rs#L145)). `sum`
  adds them per reading kind and per job ([src/cost.rs#L247](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/cost.rs#L247)), and
  `brief_line` is the brief's line ([src/cost.rs#L274](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/cost.rs#L274)).
- `receipts::is_reading_run` is the one rule for which receipts are reading
  runs, whatever word an older custodian wrote ([src/receipts.rs#L63](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/receipts.rs#L63)).
  The kit's own job of 2026-10-19 wrote `run` and `count`; before, those
  readings were missed by anything that looked for `reading`.

Proof: `cargo test` has 227 tests, the 219 of 0.3.2 and eight new.
`a_handoff_is_made_from_the_records_in_its_order_and_pinned`
([tests/handoff.rs#L202](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/tests/handoff.rs#L202)) pins a bounded job's handoff whole, for a
person and for an agent: a refused preflight, a decision answered and one
open, two failures with one pre-existing, a test line that stopped.
`why_names_the_task_history_and_the_linkage_record_of_a_route`
([tests/handoff.rs#L220](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/tests/handoff.rs#L220)), `a_readings_cost_is_the_sum_of_its_files`
([src/cost.rs#L354](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/cost.rs#L354)), `a_reuse_saves_the_readings_cost_and_the_brief_says_both`
([src/cost.rs#L372](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/cost.rs#L372)), the layout golden with `custodian.cost`
([tests/layout.rs#L35](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/tests/layout.rs#L35)), the four unit tests of the rules: `a_route_names_its_crate_its_items_and_its_dependency`
([src/why.rs#L297](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/why.rs#L297)), `git_logs_lines_read_back_as_commits_and_their_ends` ([src/why.rs#L307](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/why.rs#L307)),
`a_failed_test_lines_cargo_line_names_only_its_failures` ([src/handoff.rs#L695](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/handoff.rs#L695)) and
`a_long_handoff_stops_at_its_cap_and_says_how_much_more` ([src/handoff.rs#L705](https://github.com/1o-llc/custodian_kit/blob/a8f0f538feaf1371762c72aadfdcf78af851996b/src/handoff.rs#L705)). The rustdoc JSON of 0.3.2 against this: 5,124
public items before, 5,451 after, 327 added, 0 removed or changed.

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
