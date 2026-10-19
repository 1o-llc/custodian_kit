# Changelog

## 0.1.1 (unreleased)

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
- `jobs::template(root, kind)` and `jobs::templates(root)` load the job
  templates as the project at or above `root` has them, else as shipped, so
  a consumer reads the kit's templates through the kit instead of parsing
  the files itself. The loader the job code used is the same one
  (`template_in`). Proven by
  `templates_load_through_the_kit_shipped_or_as_the_project_has_them`.
- A job's close reads a test's outcome in cargo's words, as the journal's
  `tests` table keeps them (`ok`, `FAILED`, `ignored`; the kit's own test
  runner writes these): `FAILED` is a failed verification and makes the
  result `failed_test`. 0.1.0 matched only `failed`/`fail`, so a failed
  test was recorded `pending` and the job `accepted`. Proven by
  `a_failed_test_in_cargo_s_words_fails_the_jobs_result`.
- `rust-version` is 1.88 (tantivy 0.26.1 needs 1.86; time, globset, ignore
  and darling in the lock need 1.88), and CI tests stable and 1.88: 0.1.0's
  1.85 job could not build.

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
