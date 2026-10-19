# Changelog

## 0.1.0 (unreleased)

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
- Reading specs, job files and the paths table are 1o documents with shipped
  schemas, read by core-1o into the same form TOML reads into.

Proof: `cargo test` (tests/reading.rs, one test per kind, on the fixture
workspace in tests/fixtures/ws), and the step42 suite.
