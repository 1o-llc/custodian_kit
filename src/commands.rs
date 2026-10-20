//! The public commands: one function per CLI command, each returning its receipt as text.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub fn code_graph_json(root: impl AsRef<Path>, max_depth: usize) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    Ok(serde_json::to_string_pretty(&graph)?)
}

/// Render the Step 1 graph as stable, grep-friendly code navigation regions.
///
/// This is intentionally read-only. It does not use a pager, shell, fuzzy finder,
/// editor, or write path. The output is the first visual bridge between the
/// machine graph and the future selectable UI region model.
pub fn code_nav(root: impl AsRef<Path>, max_depth: usize) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    Ok(render_code_nav(&graph))
}

/// Probe one code graph target by id/name/path/dependency/feature/symbol text.
///
/// Step 3 stays read-only: it builds the same graph as Step 1/2, filters matching
/// graph regions, and renders an inspection card that explains related safe next
/// commands. It intentionally does not edit, write, page, shell out, or persist pins.
pub fn code_probe(root: impl AsRef<Path>, query: impl AsRef<str>, max_depth: usize) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    Ok(render_code_probe(&graph, query.as_ref()))
}

/// Render a feature/dependency usage map for manual inspection.
///
/// Step 4 is still read-only. It does not infer edits; it only connects Cargo
/// feature declarations, optional dependencies, forwarded dependency features,
/// and exact use-site regions already present in the graph.
pub fn feature_map(root: impl AsRef<Path>, max_depth: usize) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    Ok(render_feature_map(&graph))
}

/// Show exact usage sites for one dependency or feature.
///
/// Step 5 is still read-only. It narrows the feature map down to one target,
/// grouping exact use-sites by crate/file/scope so manual feature-gate review
/// can happen without editing or shell execution.
pub fn feature_sites(root: impl AsRef<Path>, target: impl AsRef<str>, max_depth: usize) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    Ok(render_feature_sites(&graph, target.as_ref()))
}

/// Render a decision-style feature/dependency inspection card.
///
/// Step 6 is still read-only. It synthesizes feature declarations, dependency
/// optionality, forwarded dependency features, exact use-sites, public import
/// signals, and scope shape into a manual review card. It does not write files,
/// generate patches, or claim a gate is correct.
pub fn feature_probe(root: impl AsRef<Path>, target: impl AsRef<str>, max_depth: usize) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    Ok(render_feature_probe(&graph, target.as_ref()))
}


/// Execute a buildable, machine-readable code query over the Step 1 graph.
///
/// Step 7 is still read-only. It turns the graph into a JSON query surface that
/// agents and humans can compose without scraping the terminal region renderer.
pub fn code_query_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let spec = parse_code_query_spec(args);
    let report = run_code_query(&graph, spec);
    Ok(serde_json::to_string_pretty(&report)?)
}

/// Execute a buildable code query and render it as stable terminal regions.
///
/// Step 8 is still read-only. It uses the same query engine and result model as
/// `code-query-json`, but renders the result set for humans as selectable visual
/// rows. It intentionally does not persist pins, write files, open editors, or
/// invoke Tantivy.
pub fn code_query(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let spec = parse_code_query_spec(args);
    let report = run_code_query(&graph, spec);
    Ok(render_code_query(&report))
}

/// Convert a buildable query result set into a machine-readable pin set.
///
/// Step 9 is still write-free. It does not persist a `.codepassenger` file yet;
/// it emits stable pin records that can be redirected to disk by the caller or
/// consumed by the next context-window pass.
pub fn pin_query_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let spec = parse_code_query_spec(args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    Ok(serde_json::to_string_pretty(&pin_set)?)
}

/// Render a query-derived pin set as stable terminal rows.
///
/// This is the human-facing companion to `pin-query-json`. It is intentionally
/// read-only and does not write pin state, open editors, or call shell tools.
pub fn pin_query(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let spec = parse_code_query_spec(args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    Ok(render_pin_query(&pin_set))
}


/// Build bounded source context windows from query-derived pins.
///
/// Step 10 is still read-only. It turns selected query results into pin records
/// and then resolves source snippets around each pin. The command emits JSON
/// only; it does not persist context files, edit code, open editors, or invoke
/// Tantivy. Context options are stripped before the query is parsed.
pub fn code_context_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (context_options, query_args) = parse_code_context_args(args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    Ok(serde_json::to_string_pretty(&context)?)
}


/// Build a Tantivy-ready, multi-tenant search indexing plan from context windows.
///
/// Step 11 intentionally does not add the Tantivy dependency or write an index.
/// It converts the green Step 10 context model into stable search documents,
/// schema fields, analyzer settings, tenant/extraction metadata, and bounded
/// writer-memory estimates. The shape is designed to be fed into a later
/// Tantivy indexer without making the core graph/query crate heavier yet.
pub fn code_search_plan_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (search_options, context_args) = parse_code_search_plan_args(args, graph.root.as_str());
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_search_index_plan_from_context(&context, search_options);
    Ok(serde_json::to_string_pretty(&plan)?)
}

/// Build a Klepto-style field assembly plan for future search indexing.
///
/// Phase 13 keeps the system read-only, but it makes the intended Klepto bridge
/// explicit: grab structured facts from pins, context windows, and use-site
/// attributes, check them against feature/dependency intent, then project them
/// into customizable search fields. This does not link Tantivy or write indexes.
pub fn klepto_field_plan_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (search_options, search_args) = parse_code_search_plan_args(args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    Ok(serde_json::to_string_pretty(&plan)?)
}


/// Bind a Klepto field schema to an index implementation schema and emit a stable key receipt.
///
/// Phase 22 keeps the same safety posture as the previous receipt phases: it
/// assembles Klepto field documents, builds a candidate RAM index to expose the
/// concrete implementation schema, and then hashes the Klepto/index binding into
/// a deterministic polymorphic index key. Disk write flags are recorded but
/// ignored. No repository file or disk index is created, overwritten, swapped, or
/// deleted.
pub fn klepto_index_key_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let receipt = index_key_of(&graph, args)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// The polymorphic index key of a query over a graph: the registry's key for
/// what those flags select (the same pipeline as `klepto-index-key-json`).
pub fn index_key_of(graph: &CodeGraph, args: &[String]) -> Result<CodeKleptoIndexKeyReceipt> {
    let (requested_index_options, field_plan_args) = parse_code_search_index_args(args);
    let mut candidate_index_options = requested_index_options.clone();
    candidate_index_options.write_index = false;
    candidate_index_options.overwrite_index = false;
    candidate_index_options.index_dir = None;

    let (search_options, search_args) = parse_code_search_plan_args(&field_plan_args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    let candidate = build_tantivy_index_receipt_from_klepto_plan(&plan, candidate_index_options)?;
    Ok(build_klepto_index_key_receipt(graph, plan, candidate, requested_index_options))
}


/// Plan Klepto index-key version registration without mutating a registry or index.
///
/// Phase 23 connects the polymorphic Klepto index key to the older versioning
/// idea. It compares the current key hashes to optional registered key/hash
/// values supplied on the command line, then recommends reuse, initial
/// registration, or a patch/minor/major version bump. It does not write a
/// registry, create an index, or mutate repository files.
pub fn klepto_index_version_plan_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (version_options, key_args) = parse_klepto_index_version_args(args);
    let (requested_index_options, field_plan_args) = parse_code_search_index_args(&key_args);
    let mut candidate_index_options = requested_index_options.clone();
    candidate_index_options.write_index = false;
    candidate_index_options.overwrite_index = false;
    candidate_index_options.index_dir = None;

    let (search_options, search_args) = parse_code_search_plan_args(&field_plan_args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    let candidate = build_tantivy_index_receipt_from_klepto_plan(&plan, candidate_index_options)?;
    let key_receipt = build_klepto_index_key_receipt(&graph, plan, candidate, requested_index_options.clone());
    let receipt = build_klepto_index_version_plan_receipt(&graph, key_receipt, version_options, requested_index_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Build a read-only registry-entry receipt from a Klepto index version plan.
///
/// Phase 24 takes the Phase 23 version decision and turns it into the concrete
/// registry row that a future registry writer could persist. It still does not
/// write a registry, create an index, or mutate repository files.
pub fn klepto_index_registry_entry_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (version_options, key_args) = parse_klepto_index_version_args(args);
    let (requested_index_options, field_plan_args) = parse_code_search_index_args(&key_args);
    let mut candidate_index_options = requested_index_options.clone();
    candidate_index_options.write_index = false;
    candidate_index_options.overwrite_index = false;
    candidate_index_options.index_dir = None;

    let (search_options, search_args) = parse_code_search_plan_args(&field_plan_args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    let candidate = build_tantivy_index_receipt_from_klepto_plan(&plan, candidate_index_options)?;
    let key_receipt = build_klepto_index_key_receipt(&graph, plan, candidate, requested_index_options.clone());
    let version_plan = build_klepto_index_version_plan_receipt(&graph, key_receipt, version_options, requested_index_options);
    let receipt = build_klepto_index_registry_entry_receipt(&graph, version_plan);
    Ok(serde_json::to_string_pretty(&receipt)?)
}


/// Optionally persist a Klepto index registry entry behind explicit write+ack flags.
///
/// Phase 25 is the first registry persistence boundary. By default it is a
/// dry-run receipt. A local JSON registry file is only written when both
/// `--write-registry` and `--ack-write-registry` are present. It never mutates
/// repository files or disk indexes.
pub fn klepto_index_registry_write_json(registry_path: impl AsRef<Path>, root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (write_options, entry_args) = parse_klepto_index_registry_write_args(args);
    let (version_options, key_args) = parse_klepto_index_version_args(&entry_args);
    let (requested_index_options, field_plan_args) = parse_code_search_index_args(&key_args);
    let mut candidate_index_options = requested_index_options.clone();
    candidate_index_options.write_index = false;
    candidate_index_options.overwrite_index = false;
    candidate_index_options.index_dir = None;

    let (search_options, search_args) = parse_code_search_plan_args(&field_plan_args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    let candidate = build_tantivy_index_receipt_from_klepto_plan(&plan, candidate_index_options)?;
    let key_receipt = build_klepto_index_key_receipt(&graph, plan, candidate, requested_index_options.clone());
    let version_plan = build_klepto_index_version_plan_receipt(&graph, key_receipt, version_options, requested_index_options);
    let entry_receipt = build_klepto_index_registry_entry_receipt(&graph, version_plan);
    let receipt = build_klepto_index_registry_write_receipt(registry_path.as_ref(), entry_receipt, write_options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Resolve the current Klepto index key against a persisted registry without writing.
///
/// Phase 26 closes the loop from the Phase 25 registry writer back into the
/// version planner. It reads a local registry JSON file, finds the exact or
/// closest registered baseline for the current Klepto key, then emits the same
/// reuse/patch/minor/major decision without requiring manual `--registered-*`
/// flags. It never mutates the registry or any Tantivy index.
pub fn klepto_index_registry_resolve_json(registry_path: impl AsRef<Path>, root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (write_options, entry_args) = parse_klepto_index_registry_write_args(args);
    let (_version_options, key_args) = parse_klepto_index_version_args(&entry_args);
    let (requested_index_options, field_plan_args) = parse_code_search_index_args(&key_args);
    let mut candidate_index_options = requested_index_options.clone();
    candidate_index_options.write_index = false;
    candidate_index_options.overwrite_index = false;
    candidate_index_options.index_dir = None;

    let (search_options, search_args) = parse_code_search_plan_args(&field_plan_args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    let candidate = build_tantivy_index_receipt_from_klepto_plan(&plan, candidate_index_options)?;
    let key_receipt = build_klepto_index_key_receipt(&graph, plan, candidate, requested_index_options.clone());
    let receipt = build_klepto_index_registry_resolve_receipt(registry_path.as_ref(), &graph, key_receipt, requested_index_options, write_options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Resolve the persisted Klepto registry and plan a disk refresh as one read-only preflight.
///
/// Phase 27 joins the persistent registry path with the persistent Tantivy index path: it
/// resolves the current Klepto/index key against the registry, opens the existing disk index
/// through the read-only manifest path, builds a fresh RAM candidate, and emits one combined
/// decision. It never writes the registry or mutates the disk index, even when write flags are
/// supplied.
pub fn klepto_index_registry_refresh_plan_json(registry_path: impl AsRef<Path>, index_dir: impl AsRef<Path>, root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (status_options, refresh_args) = parse_code_search_refresh_plan_args(args);
    let (write_options, entry_args) = parse_klepto_index_registry_write_args(&refresh_args);
    let (_version_options, key_args) = parse_klepto_index_version_args(&entry_args);
    let existing_manifest = build_tantivy_disk_manifest_receipt(index_dir.as_ref(), status_options)?;
    let (requested_index_options, field_plan_args) = parse_code_search_index_args(&key_args);
    let mut candidate_index_options = requested_index_options.clone();
    candidate_index_options.write_index = false;
    candidate_index_options.overwrite_index = false;
    candidate_index_options.index_dir = None;

    let (search_options, search_args) = parse_code_search_plan_args(&field_plan_args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    let candidate = build_tantivy_index_receipt_from_klepto_plan(&plan, candidate_index_options)?;
    let key_receipt = build_klepto_index_key_receipt(&graph, plan, candidate.clone(), requested_index_options.clone());
    let resolve_receipt = build_klepto_index_registry_resolve_receipt(registry_path.as_ref(), &graph, key_receipt, requested_index_options.clone(), write_options)?;
    let refresh_receipt = build_tantivy_disk_refresh_plan_receipt(&graph, index_dir.as_ref(), existing_manifest, candidate, requested_index_options);
    let receipt = build_klepto_index_registry_refresh_plan_receipt(registry_path.as_ref(), index_dir.as_ref(), resolve_receipt, refresh_receipt);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Convert the registry-aware refresh plan into a guarded execution ticket without mutating anything.
///
/// Phase 28 is the final pre-execution gate before a future writer/swapper: it
/// consumes the Phase 27 combined plan, classifies exactly which writes would be
/// needed, checks for explicit registry/index acknowledgement flags, and emits a
/// stable execution token. It does not write the registry or refresh the index.
pub fn klepto_index_registry_refresh_exec_plan_json(registry_path: impl AsRef<Path>, index_dir: impl AsRef<Path>, root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let (exec_options, plan_args) = parse_klepto_index_registry_refresh_exec_args(args);
    let plan_json = klepto_index_registry_refresh_plan_json(registry_path.as_ref(), index_dir.as_ref(), root.as_ref(), max_depth, &plan_args)?;
    let refresh_plan: CodeKleptoIndexRegistryRefreshPlanReceipt = serde_json::from_str(&plan_json)?;
    let receipt = build_klepto_index_registry_refresh_exec_plan_receipt(registry_path.as_ref(), index_dir.as_ref(), refresh_plan, exec_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Consume the registry-aware execution ticket and emit a dry-run execution receipt.
///
/// Phase 29 is intentionally still non-mutating: it verifies the Phase 28 ticket,
/// records whether an execution was requested and acknowledged, checks the optional
/// execution token, and proves which writes would be allowed for a future executor.
/// It never writes registry files and never refreshes Tantivy disk indexes.
pub fn klepto_index_registry_refresh_exec_json(registry_path: impl AsRef<Path>, index_dir: impl AsRef<Path>, root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let (execute_options, exec_plan_args) = parse_klepto_index_registry_refresh_execute_args(args);
    let exec_plan_json = klepto_index_registry_refresh_exec_plan_json(registry_path.as_ref(), index_dir.as_ref(), root.as_ref(), max_depth, &exec_plan_args)?;
    let exec_plan: CodeKleptoIndexRegistryRefreshExecPlanReceipt = serde_json::from_str(&exec_plan_json)?;
    let receipt = build_klepto_index_registry_refresh_exec_receipt(registry_path.as_ref(), index_dir.as_ref(), exec_plan, execute_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Stage a registry/index refresh execution receipt into an isolated staging directory.
///
/// Phase 30 is the first writer-shaped boundary after the dry-run execution receipt,
/// but it still never mutates the live registry path or the live Tantivy index. When
/// execution has already been permitted by Phase 29 and `--stage-refresh` plus
/// `--ack-stage-refresh` are supplied, it writes a staging manifest and receipt
/// files under the staging directory only.
pub fn klepto_index_registry_refresh_stage_json(staging_dir: impl AsRef<Path>, registry_path: impl AsRef<Path>, index_dir: impl AsRef<Path>, root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let (stage_options, exec_args) = parse_klepto_index_registry_refresh_stage_args(args);
    let exec_json = klepto_index_registry_refresh_exec_json(registry_path.as_ref(), index_dir.as_ref(), root.as_ref(), max_depth, &exec_args)?;
    let exec_receipt: CodeKleptoIndexRegistryRefreshExecReceipt = serde_json::from_str(&exec_json)?;
    let receipt = build_klepto_index_registry_refresh_stage_receipt(staging_dir.as_ref(), registry_path.as_ref(), index_dir.as_ref(), exec_receipt, stage_options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}


/// Verify isolated staging artifacts before any future swap/apply command exists.
///
/// Phase 31 is still read-only: it scans the staging directory produced by Phase 30,
/// verifies the staging manifest, execution receipt, optional registry candidate,
/// optional index marker, execution-token consistency, and staged-file counts. It
/// never writes staging files, never mutates the live registry, and never mutates
/// the live Tantivy index.
pub fn klepto_index_registry_refresh_stage_verify_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let options = parse_klepto_index_registry_refresh_stage_verify_args(args);
    let receipt = build_klepto_index_registry_refresh_stage_verify_receipt(staging_dir.as_ref(), options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Convert a verified isolated stage into a read-only swap plan.
///
/// Phase 32 does not apply staged artifacts. It consumes the Phase 31 verifier,
/// checks whether the selected stage is verified, classifies required registry and
/// index swaps, and emits a deterministic swap_plan_id that a later guarded apply
/// phase can require. No staging files, registry files, or Tantivy indexes are
/// modified by this command.
pub fn klepto_index_registry_refresh_swap_plan_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let verify_json = klepto_index_registry_refresh_stage_verify_json(staging_dir.as_ref(), args)?;
    let stage_verify: CodeKleptoIndexRegistryRefreshStageVerifyReceipt = serde_json::from_str(&verify_json)?;
    let receipt = build_klepto_index_registry_refresh_swap_plan_receipt(staging_dir.as_ref(), stage_verify);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Consume a verified-stage swap plan into a guarded dry-run swap execution receipt.
///
/// Phase 33 is intentionally still non-mutating. It checks the Phase 32 swap plan,
/// optional execution-token and swap-plan-id expectations, and explicit
/// `--execute-swap`/`--ack-execute-swap` intent. It records whether a future apply
/// would be permitted, but it never writes the live registry or Tantivy index.
pub fn klepto_index_registry_refresh_swap_exec_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let (swap_options, swap_plan_args) = parse_klepto_index_registry_refresh_swap_execute_args(args);
    let swap_plan_json = klepto_index_registry_refresh_swap_plan_json(staging_dir.as_ref(), &swap_plan_args)?;
    let swap_plan: CodeKleptoIndexRegistryRefreshSwapPlanReceipt = serde_json::from_str(&swap_plan_json)?;
    let receipt = build_klepto_index_registry_refresh_swap_exec_receipt(staging_dir.as_ref(), swap_plan, swap_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Convert a dry-run swap execution receipt into a read-only future apply plan.
///
/// Phase 34 is still non-mutating. It consumes the Phase 33 swap execution
/// receipt, classifies the exact live mutations that a future apply command
/// would need, and emits a deterministic apply_plan_id. It never writes the
/// live registry, never swaps Tantivy indexes, and never mutates staging files.
pub fn klepto_index_registry_refresh_apply_plan_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let swap_exec_json = klepto_index_registry_refresh_swap_exec_json(staging_dir.as_ref(), args)?;
    let swap_exec: CodeKleptoIndexRegistryRefreshSwapExecReceipt = serde_json::from_str(&swap_exec_json)?;
    let receipt = build_klepto_index_registry_refresh_apply_plan_receipt(staging_dir.as_ref(), swap_exec);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Consume an apply plan into a guarded dry-run apply execution receipt.
///
/// Phase 35 remains non-mutating. It consumes the Phase 34 apply plan, checks
/// explicit `--execute-apply`/`--ack-execute-apply` intent plus an optional
/// apply_plan_id expectation, and records whether a future live writer would be
/// allowed to apply staged registry/index artifacts. This command never mutates
/// staging, live registry files, or Tantivy indexes.
pub fn klepto_index_registry_refresh_apply_exec_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let (apply_options, apply_plan_args) = parse_klepto_index_registry_refresh_apply_execute_args(args);
    let apply_plan_json = klepto_index_registry_refresh_apply_plan_json(staging_dir.as_ref(), &apply_plan_args)?;
    let apply_plan: CodeKleptoIndexRegistryRefreshApplyPlanReceipt = serde_json::from_str(&apply_plan_json)?;
    let receipt = build_klepto_index_registry_refresh_apply_exec_receipt(staging_dir.as_ref(), apply_plan, apply_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Convert a dry-run apply execution receipt into a live-apply request receipt.
///
/// Phase 36 is still non-mutating. It consumes the Phase 35 apply execution
/// receipt, requires a permitted dry-run apply plus explicit
/// `--request-live-apply`/`--ack-request-live-apply` operator intent, and emits
/// an auditable request_id that a future real writer can require. This command
/// records intent only; it never writes staging, live registry files, or
/// Tantivy indexes.
pub fn klepto_index_registry_refresh_live_apply_request_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let (request_options, apply_exec_args) = parse_klepto_index_registry_refresh_live_apply_request_args(args);
    let apply_exec_json = klepto_index_registry_refresh_apply_exec_json(staging_dir.as_ref(), &apply_exec_args)?;
    let apply_exec: CodeKleptoIndexRegistryRefreshApplyExecReceipt = serde_json::from_str(&apply_exec_json)?;
    let receipt = build_klepto_index_registry_refresh_live_apply_request_receipt(staging_dir.as_ref(), apply_exec, request_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Preflight an acknowledged live-apply request before any real writer exists.
///
/// Phase 37 remains non-mutating. It consumes the Phase 36 live-apply request
/// receipt, checks optional request-id binding plus explicit
/// `--preflight-live-apply`/`--ack-preflight-live-apply` operator intent, and
/// emits the final preflight receipt a future real writer can require before
/// live registry or index paths may be touched.
pub fn klepto_index_registry_refresh_live_apply_preflight_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let (preflight_options, request_args) = parse_klepto_index_registry_refresh_live_apply_preflight_args(args);
    let request_json = klepto_index_registry_refresh_live_apply_request_json(staging_dir.as_ref(), &request_args)?;
    let request: CodeKleptoIndexRegistryRefreshLiveApplyRequestReceipt = serde_json::from_str(&request_json)?;
    let receipt = build_klepto_index_registry_refresh_live_apply_preflight_receipt(staging_dir.as_ref(), request, preflight_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Plan a future live commit after a successful live-apply preflight.
///
/// Phase 38 remains non-mutating. It consumes the Phase 37 preflight receipt,
/// requires `--plan-live-commit`/`--ack-plan-live-commit`, records an immutable
/// commit plan id, and enumerates the future lock, backup, and commit journal
/// artifacts a real writer would have to create before touching live paths.
pub fn klepto_index_registry_refresh_live_apply_commit_plan_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let (commit_options, preflight_args) = parse_klepto_index_registry_refresh_live_apply_commit_plan_args(args);
    let preflight_json = klepto_index_registry_refresh_live_apply_preflight_json(staging_dir.as_ref(), &preflight_args)?;
    let preflight: CodeKleptoIndexRegistryRefreshLiveApplyPreflightReceipt = serde_json::from_str(&preflight_json)?;
    let receipt = build_klepto_index_registry_refresh_live_apply_commit_plan_receipt(staging_dir.as_ref(), preflight, commit_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Execute the live commit path in dry-run receipt form.
///
/// Phase 39 is still non-mutating. It consumes the Phase 38 live commit plan,
/// requires explicit `--execute-live-commit` and `--ack-execute-live-commit`,
/// optionally binds `--expected-live-commit-plan-id`, and records whether a
/// future real writer would be allowed to create locks/backups/journals and
/// mutate live registry/index paths.
pub fn klepto_index_registry_refresh_live_apply_commit_exec_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let (exec_options, commit_plan_args) = parse_klepto_index_registry_refresh_live_apply_commit_exec_args(args);
    let commit_plan_json = klepto_index_registry_refresh_live_apply_commit_plan_json(staging_dir.as_ref(), &commit_plan_args)?;
    let commit_plan: CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanReceipt = serde_json::from_str(&commit_plan_json)?;
    let receipt = build_klepto_index_registry_refresh_live_apply_commit_exec_receipt(staging_dir.as_ref(), commit_plan, exec_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Stage a future live commit journal under the isolated staging directory.
///
/// Phase 40 still never touches live registry or index paths. It consumes the
/// Phase 39 live commit execution receipt, requires explicit
/// `--stage-live-commit` and `--ack-stage-live-commit`, optionally binds
/// `--expected-live-commit-execution-id`, and writes only commit-intent
/// artifacts under the already isolated stage directory.
pub fn klepto_index_registry_refresh_live_apply_commit_stage_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let (stage_options, commit_exec_args) = parse_klepto_index_registry_refresh_live_apply_commit_stage_args(args);
    let commit_exec_json = klepto_index_registry_refresh_live_apply_commit_exec_json(staging_dir.as_ref(), &commit_exec_args)?;
    let commit_exec: CodeKleptoIndexRegistryRefreshLiveApplyCommitExecReceipt = serde_json::from_str(&commit_exec_json)?;
    let receipt = build_klepto_index_registry_refresh_live_apply_commit_stage_receipt(staging_dir.as_ref(), commit_exec, stage_options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Verify staged live commit journal artifacts without mutation.
///
/// Phase 41 reads only the isolated stage written by Phase 40. It verifies the
/// staged manifest, commit execution receipt, journal schema, execution-id
/// consistency, optional expected execution id, and staged file counts. It
/// never writes staging, registry, or index paths.
pub fn klepto_index_registry_refresh_live_apply_commit_stage_verify_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let options = parse_klepto_index_registry_refresh_live_apply_commit_stage_verify_args(args);
    let receipt = build_klepto_index_registry_refresh_live_apply_commit_stage_verify_receipt(staging_dir.as_ref(), options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Open a final non-mutating live commit gate for future guarded writers.
///
/// Phase 42 consumes the Phase 41 staged commit verifier. It requires an
/// already verified staged live commit journal plus explicit gate+ack flags
/// before emitting a single gate receipt a future live writer can require. It
/// still never writes staging, registry, or index paths.
pub fn klepto_index_registry_refresh_live_apply_commit_gate_json(staging_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let options = parse_klepto_index_registry_refresh_live_apply_commit_gate_args(args);
    let verify_args = options.expected_live_commit_execution_id.as_ref()
        .map(|expected| vec!["--expected-live-commit-execution-id".to_string(), expected.clone()])
        .unwrap_or_default();
    let verify_json = klepto_index_registry_refresh_live_apply_commit_stage_verify_json(staging_dir.as_ref(), &verify_args)?;
    let verify_receipt: CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyReceipt = serde_json::from_str(&verify_json)?;
    let receipt = build_klepto_index_registry_refresh_live_apply_commit_gate_receipt(staging_dir.as_ref(), verify_receipt, options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Build a real Tantivy index receipt from Klepto-assembled field documents.
///
/// Phase 14 is the first pass that links Tantivy and executes the indexing
/// pipeline. By default it uses an in-memory Tantivy index, adds documents,
/// commits, and performs a query check. Disk writes require `--write-index` and
/// `--index-dir`; repository files are never modified.
pub fn code_search_index_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (index_options, field_plan_args) = parse_code_search_index_args(args);
    let (search_options, search_args) = parse_code_search_plan_args(&field_plan_args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    let receipt = build_tantivy_index_receipt_from_klepto_plan(&plan, index_options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Execute a real Tantivy query over a freshly assembled RAM index and emit
/// hydrated search results. This phase keeps repository files and disk indexes
/// untouched: even if `--write-index` is passed, query mode forces RAM indexing
/// and records that the disk write request was ignored. Use Phase 14's
/// `code-search-index-json --write-index --index-dir` when an index write receipt
/// is intentionally needed.
pub fn code_search_query_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (requested_index_options, field_plan_args) = parse_code_search_index_args(args);
    let mut query_index_options = requested_index_options.clone();
    query_index_options.write_index = false;
    query_index_options.overwrite_index = false;
    query_index_options.index_dir = None;

    let (search_options, search_args) = parse_code_search_plan_args(&field_plan_args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    let index_receipt = build_tantivy_index_receipt_from_klepto_plan(&plan, query_index_options)?;
    let receipt = build_tantivy_query_receipt_from_index_receipt(index_receipt, requested_index_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Open an existing Tantivy disk index and execute a read-only query receipt.
///
/// Phase 17 reuses the explicit disk index produced by `code-search-index-json
/// --write-index --index-dir`. It never writes to the repository and never creates
/// or mutates an index; it only opens an existing index, registers the code
/// analyzer, runs a query, hydrates hits from stored fields, and applies explicit
/// tenant filters before returning hits.
pub fn code_search_disk_query_json(index_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let options = parse_code_search_disk_query_args(args);
    let receipt = build_tantivy_disk_query_receipt(index_dir.as_ref(), options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

pub fn code_search_disk_status_json(index_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let options = parse_code_search_disk_status_args(args);
    let receipt = build_tantivy_disk_status_receipt(index_dir.as_ref(), options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Build a read-only manifest/hash receipt for a persistent Tantivy disk index.
///
/// Phase 19 does not create, commit, overwrite, or query an index. It opens the
/// existing index through the status path, inventories stored metadata, hashes
/// the index directory files in sorted order, and emits a stable manifest receipt
/// that can be compared before future refresh/merge/delete work.
pub fn code_search_disk_manifest_json(index_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let options = parse_code_search_disk_status_args(args);
    let receipt = build_tantivy_disk_manifest_receipt(index_dir.as_ref(), options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Compare two persistent disk-index manifest receipts without mutating either index.
///
/// Phase 20 opens both indexes through the read-only manifest/status path, compares
/// manifest digests, stored-field/schema counts, tenant scopes, file lists, and
/// byte totals. It is the checkpoint before any future index refresh/merge/delete
/// workflow because it can prove whether two disk indexes are identical, missing,
/// or materially different.
pub fn code_search_disk_compare_json(left_index_dir: impl AsRef<Path>, right_index_dir: impl AsRef<Path>, args: &[String]) -> Result<String> {
    let options = parse_code_search_disk_status_args(args);
    let receipt = build_tantivy_disk_compare_receipt(left_index_dir.as_ref(), right_index_dir.as_ref(), options)?;
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Plan a disk-index refresh without mutating the existing index.
///
/// Phase 21 opens the existing disk index through the read-only manifest path,
/// assembles a fresh candidate RAM index from the current root/query/Klepto
/// fields, and compares document counts, tenant scopes, schema counts, and
/// stored fields. It is intentionally a receipt-only preflight before any
/// future overwrite/swap/merge workflow.
pub fn code_search_disk_refresh_plan_json(root: impl AsRef<Path>, index_dir: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (status_options, candidate_args) = parse_code_search_refresh_plan_args(args);
    let existing_manifest = build_tantivy_disk_manifest_receipt(index_dir.as_ref(), status_options)?;
    let (requested_index_options, field_plan_args) = parse_code_search_index_args(&candidate_args);
    let mut candidate_index_options = requested_index_options.clone();
    candidate_index_options.write_index = false;
    candidate_index_options.overwrite_index = false;
    candidate_index_options.index_dir = None;

    let (search_options, search_args) = parse_code_search_plan_args(&field_plan_args, graph.root.as_str());
    let (klepto_options, context_args) = parse_klepto_field_plan_args(&search_args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_klepto_field_plan_from_context(&context, search_options, klepto_options);
    let candidate = build_tantivy_index_receipt_from_klepto_plan(&plan, candidate_index_options)?;
    let receipt = build_tantivy_disk_refresh_plan_receipt(&graph, index_dir.as_ref(), existing_manifest, candidate, requested_index_options);
    Ok(serde_json::to_string_pretty(&receipt)?)
}

/// Build a dry-run edit plan from query-derived context windows.
///
/// Step 12 intentionally stops at a receipt-like JSON plan. It proposes
/// candidate edits anchored to pins/context windows, but it never writes files,
/// applies patches, runs shell commands, or emits an edit-apply command. This is
/// the checkpoint before any future guarded write path.
pub fn edit_plan_json(root: impl AsRef<Path>, max_depth: usize, args: &[String]) -> Result<String> {
    let graph = build_code_graph(root, max_depth)?;
    let (edit_options, context_args) = parse_code_edit_plan_args(args);
    let (context_options, query_args) = parse_code_context_args(&context_args);
    let spec = parse_code_query_spec(&query_args);
    let report = run_code_query(&graph, spec);
    let pin_set = build_pin_set_from_query_report(&report);
    let context = build_context_report_from_pin_set(&pin_set, context_options);
    let plan = build_edit_plan_from_context(&context, edit_options);
    Ok(serde_json::to_string_pretty(&plan)?)
}
