//! The versioned index registry: version plan, entry, write, resolve, refresh plan, exec, stage, verify, swap, apply, live apply.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn build_klepto_index_version_plan_receipt(
    graph: &CodeGraph,
    key_receipt: CodeKleptoIndexKeyReceipt,
    options: CodeKleptoIndexVersionOptions,
    requested_options: CodeTantivyIndexOptions,
) -> CodeKleptoIndexVersionPlanReceipt {
    let current = key_receipt.key.clone();
    let registered = options.registered_key;
    let mut deltas = Vec::<CodeKleptoIndexVersionHashDelta>::new();
    collect_version_hash_delta(&mut deltas, "schema_hash", &registered.schema_hash, &current.schema_hash, "major");
    collect_version_hash_delta(&mut deltas, "field_contract_hash", &registered.field_contract_hash, &current.field_contract_hash, "major");
    collect_version_hash_delta(&mut deltas, "analyzer_hash", &registered.analyzer_hash, &current.analyzer_hash, "minor");
    collect_version_hash_delta(&mut deltas, "tenant_scope_hash", &registered.tenant_scope_hash, &current.tenant_scope_hash, "patch");
    collect_version_hash_delta(&mut deltas, "feature_binding_hash", &registered.feature_binding_hash, &current.feature_binding_hash, "patch");
    collect_version_hash_delta(&mut deltas, "document_shape_hash", &registered.document_shape_hash, &current.document_shape_hash, "minor");

    let registered_hash_count = [
        &registered.schema_hash,
        &registered.field_contract_hash,
        &registered.analyzer_hash,
        &registered.tenant_scope_hash,
        &registered.feature_binding_hash,
        &registered.document_shape_hash,
    ]
    .iter()
    .filter(|value| value.is_some())
    .count();

    let same_registered_key = registered
        .key_id
        .as_ref()
        .map(|key_id| key_id == &current.key_id)
        .unwrap_or(false)
        && deltas.is_empty();

    let has_registered_state = registered.key_id.is_some() || registered_hash_count > 0;
    let strongest_bump = strongest_version_bump(&deltas);
    let (recommended_action, recommended_bump, version_reason) = if !graph.root_exists {
        (
            "defer".to_string(),
            "none".to_string(),
            "missing root has no stable Klepto/index key to register".to_string(),
        )
    } else if !has_registered_state {
        (
            "register_initial".to_string(),
            "initial".to_string(),
            "no registered key or hash baseline was supplied".to_string(),
        )
    } else if same_registered_key || deltas.is_empty() {
        (
            "reuse_existing_version".to_string(),
            "none".to_string(),
            "registered key/hash baseline matches the current Klepto index key".to_string(),
        )
    } else {
        (
            "register_new_version".to_string(),
            strongest_bump.clone(),
            format!("registered baseline differs from current Klepto index key; strongest required bump is {strongest_bump}"),
        )
    };

    let mut findings = key_receipt.findings.clone();
    if requested_options.write_index || requested_options.overwrite_index || requested_options.index_dir.is_some() {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPV001".to_string(),
            message: "klepto-index-version-plan mode records but ignores write/index flags; it never mutates a registry or disk index".to_string(),
            path: requested_options.index_dir.clone(),
            line: None,
            column: None,
        });
    }
    if graph.root_exists && !has_registered_state {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPV002".to_string(),
            message: "no registered index key baseline supplied; initial registration is recommended".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }
    if graph.root_exists && has_registered_state && !deltas.is_empty() {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPV003".to_string(),
            message: "registered Klepto index key baseline differs from the current key".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }

    CodeKleptoIndexVersionPlanReceipt {
        schema: "code_passenger.klepto_index_version_plan_receipt.v1".to_string(),
        graph_schema: graph.schema.clone(),
        root: graph.root.clone(),
        root_exists: graph.root_exists,
        max_depth: graph.max_depth,
        source_key_schema: key_receipt.schema.clone(),
        source_klepto_schema: key_receipt.source_klepto_schema.clone(),
        implementation_schema: key_receipt.candidate_index_schema.clone(),
        version: CodeKleptoIndexVersionPlanStatus {
            model: "klepto_index_key_version_plan".to_string(),
            registry_model: "read_only_flag_supplied_registered_baseline".to_string(),
            source_key_model: "polymorphic_klepto_index_key".to_string(),
            mutates_registry: false,
            mutates_index: false,
            notes: vec![
                "version plan compares the current Klepto index key to an optional registered baseline".to_string(),
                "schema/field-contract changes require a major bump recommendation".to_string(),
                "analyzer/document-shape changes require a minor bump recommendation".to_string(),
                "tenant/feature binding changes require a patch bump recommendation".to_string(),
                "future registries can persist this receipt, but this command is read-only".to_string(),
            ],
        },
        current_key: current,
        registered_key: registered,
        registered_hash_count,
        same_registered_key,
        hash_delta_count: deltas.len(),
        hash_deltas: deltas,
        recommended_action,
        recommended_bump,
        version_reason,
        registry_mutated: false,
        write_requested: requested_options.write_index,
        overwrite_requested: requested_options.overwrite_index,
        write_allowed: false,
        index_mutated: false,
        candidate_committed: key_receipt.candidate_committed,
        candidate_search_hits: key_receipt.candidate_search_hits,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_entry_receipt(
    graph: &CodeGraph,
    version_plan: CodeKleptoIndexVersionPlanReceipt,
) -> CodeKleptoIndexRegistryEntryReceipt {
    let current = version_plan.current_key.clone();
    let registered = version_plan.registered_key.clone();
    let target_version = target_registry_version(&version_plan.recommended_action, &version_plan.recommended_bump, &registered, &current);
    let entry_material = vec![
        format!("key_id={}", current.key_id),
        format!("version={}", target_version),
        format!("action={}", version_plan.recommended_action),
        format!("schema_hash={}", current.schema_hash),
        format!("field_contract_hash={}", current.field_contract_hash),
        format!("analyzer_hash={}", current.analyzer_hash),
        format!("tenant_scope_hash={}", current.tenant_scope_hash),
        format!("feature_binding_hash={}", current.feature_binding_hash),
        format!("document_shape_hash={}", current.document_shape_hash),
    ];
    let entry_hash = serialized_digest(&entry_material);
    let entry_id = format!("kver_{}", crate::digest::short(&entry_hash, 16));
    let entry_state = match version_plan.recommended_action.as_str() {
        "reuse_existing_version" => "existing",
        "defer" => "deferred",
        _ => "candidate",
    }
    .to_string();

    let mut findings = version_plan.findings.clone();
    if graph.root_exists && version_plan.recommended_action == "reuse_existing_version" {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPR100".to_string(),
            message: "registered Klepto index key can be reused; registry entry is read-only and unchanged".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }
    if graph.root_exists && version_plan.recommended_action == "register_initial" {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPR101".to_string(),
            message: "initial Klepto index registry entry is recommended but not written".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }
    if graph.root_exists && version_plan.recommended_action == "register_new_version" {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPR102".to_string(),
            message: format!("new Klepto index registry entry is recommended with a {} bump but not written", version_plan.recommended_bump),
            path: None,
            line: None,
            column: None,
        });
    }

    CodeKleptoIndexRegistryEntryReceipt {
        schema: "code_passenger.klepto_index_registry_entry_receipt.v1".to_string(),
        graph_schema: version_plan.graph_schema.clone(),
        root: version_plan.root.clone(),
        root_exists: version_plan.root_exists,
        max_depth: version_plan.max_depth,
        source_version_plan_schema: version_plan.schema.clone(),
        source_key_schema: version_plan.source_key_schema.clone(),
        source_klepto_schema: version_plan.source_klepto_schema.clone(),
        implementation_schema: version_plan.implementation_schema.clone(),
        registry: CodeKleptoIndexRegistryEntryStatus {
            model: "klepto_index_registry_entry_plan".to_string(),
            source: "phase23_version_plan_receipt".to_string(),
            entry_model: "single_polymorphic_index_key_version_row".to_string(),
            mutates_registry: false,
            mutates_index: false,
            notes: vec![
                "registry-entry receipts convert version decisions into concrete rows without writing them".to_string(),
                "entry_id is derived from key_id, target version, action, and key component hashes".to_string(),
                "future registry writers can persist this receipt behind an explicit write/swap command".to_string(),
                "reuse entries are represented as existing/read-only rather than candidate writes".to_string(),
            ],
        },
        current_key: current.clone(),
        registered_key: registered.clone(),
        entry: CodeKleptoIndexRegistryEntry {
            entry_id,
            key_id: current.key_id.clone(),
            key_version: target_version,
            previous_key_id: registered.key_id.clone(),
            previous_key_version: registered.key_version.clone(),
            source_key_schema: version_plan.source_key_schema.clone(),
            source_klepto_schema: version_plan.source_klepto_schema.clone(),
            implementation_schema: version_plan.implementation_schema.clone(),
            schema_hash: current.schema_hash.clone(),
            field_contract_hash: current.field_contract_hash.clone(),
            analyzer_hash: current.analyzer_hash.clone(),
            tenant_scope_hash: current.tenant_scope_hash.clone(),
            feature_binding_hash: current.feature_binding_hash.clone(),
            document_shape_hash: current.document_shape_hash.clone(),
            recommended_action: version_plan.recommended_action.clone(),
            recommended_bump: version_plan.recommended_bump.clone(),
            state: entry_state,
            storage_mode: "receipt_only".to_string(),
            read_only: true,
        },
        version_plan_action: version_plan.recommended_action.clone(),
        version_plan_bump: version_plan.recommended_bump.clone(),
        version_plan_reason: version_plan.version_reason.clone(),
        same_registered_key: version_plan.same_registered_key,
        hash_delta_count: version_plan.hash_delta_count,
        hash_deltas: version_plan.hash_deltas.clone(),
        registry_mutated: false,
        write_requested: version_plan.write_requested,
        overwrite_requested: version_plan.overwrite_requested,
        write_allowed: false,
        index_mutated: false,
        candidate_committed: version_plan.candidate_committed,
        candidate_search_hits: version_plan.candidate_search_hits,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_write_receipt(
    registry_path: &Path,
    entry_receipt: CodeKleptoIndexRegistryEntryReceipt,
    options: CodeKleptoIndexRegistryWriteOptions,
) -> Result<CodeKleptoIndexRegistryWriteReceipt> {
    let path_exists_before = registry_path.exists();
    let mut entries = read_klepto_registry_entries(registry_path)?;
    let entry_count_before = entries.len();
    let mut findings = entry_receipt.findings.clone();

    if options.write_registry && !options.ack_write_registry {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPW001".to_string(),
            message: "registry write was requested but --ack-write-registry was not supplied".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if options.ack_write_registry && !options.write_registry {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPW002".to_string(),
            message: "registry write ack was supplied without --write-registry; staying dry-run".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if entry_receipt.entry.state != "candidate" {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPW003".to_string(),
            message: "registry entry is not a candidate write; no registry mutation is needed".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }

    let write_allowed = entry_receipt.root_exists
        && entry_receipt.entry.state == "candidate"
        && options.write_registry
        && options.ack_write_registry;

    let mut registry_mutated = false;
    let mut written_entry_count = 0usize;
    if write_allowed {
        let new_entry_id = entry_receipt.entry.entry_id.clone();
        let new_key_id = entry_receipt.entry.key_id.clone();
        let new_key_version = entry_receipt.entry.key_version.clone();
        entries.retain(|entry| {
            entry.entry_id.as_str() != new_entry_id.as_str()
                && !(entry.key_id.as_str() == new_key_id.as_str() && entry.key_version.as_str() == new_key_version.as_str())
        });
        entries.push(entry_receipt.entry.clone());
        let registry_file = CodeKleptoIndexRegistryFile {
            schema: "code_passenger.klepto_index_registry_file.v1".to_string(),
            registry_version: "0.1.0".to_string(),
            storage_model: if options.overwrite_registry { "local_json_overwrite" } else { "local_json_upsert" }.to_string(),
            entry_count: entries.len(),
            latest_entry_id: Some(entry_receipt.entry.entry_id.clone()),
            entries: entries.clone(),
        };
        if let Some(parent) = registry_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(registry_path, format!("{}\n", serde_json::to_string_pretty(&registry_file)?))?;
        registry_mutated = true;
        written_entry_count = 1;
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPW004".to_string(),
            message: "Klepto index registry entry was written after explicit write and ack flags".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }

    let path_exists_after = registry_path.exists();
    let entry_count_after = if registry_mutated { entries.len() } else { entry_count_before };
    let registry_file_digest = if path_exists_after {
        let bytes = fs::read(registry_path)?;
        crate::digest::tagged(&bytes)
    } else {
        "none".to_string()
    };

    Ok(CodeKleptoIndexRegistryWriteReceipt {
        schema: "code_passenger.klepto_index_registry_write_receipt.v1".to_string(),
        graph_schema: entry_receipt.graph_schema.clone(),
        root: entry_receipt.root.clone(),
        root_exists: entry_receipt.root_exists,
        max_depth: entry_receipt.max_depth,
        registry_path: registry_path.to_string_lossy().to_string(),
        registry_path_exists_before: path_exists_before,
        registry_path_exists_after: path_exists_after,
        source_registry_entry_schema: entry_receipt.schema.clone(),
        source_version_plan_schema: entry_receipt.source_version_plan_schema.clone(),
        source_key_schema: entry_receipt.source_key_schema.clone(),
        registry: CodeKleptoIndexRegistryWriteStatus {
            model: "klepto_index_registry_writer".to_string(),
            storage_model: "local_json_registry_file".to_string(),
            mutates_registry: registry_mutated,
            mutates_index: false,
            requires_write_flag: true,
            requires_ack_flag: true,
            notes: vec![
                "default mode is dry-run and produces a write receipt only".to_string(),
                "registry mutation requires both --write-registry and --ack-write-registry".to_string(),
                "writer upserts candidate registry entries by entry_id and key_id/key_version".to_string(),
                "writer never mutates repository files or Tantivy disk indexes".to_string(),
            ],
        },
        entry: entry_receipt.entry.clone(),
        write_requested: options.write_registry,
        ack_write_registry: options.ack_write_registry,
        overwrite_requested: options.overwrite_registry,
        write_allowed,
        registry_mutated,
        index_mutated: false,
        entry_count_before,
        entry_count_after,
        written_entry_count,
        registry_file_schema: "code_passenger.klepto_index_registry_file.v1".to_string(),
        registry_file_digest,
        findings,
    })
}

pub(crate) fn build_klepto_index_registry_resolve_receipt(
    registry_path: &Path,
    graph: &CodeGraph,
    key_receipt: CodeKleptoIndexKeyReceipt,
    requested_options: CodeTantivyIndexOptions,
    write_options: CodeKleptoIndexRegistryWriteOptions,
) -> Result<CodeKleptoIndexRegistryResolveReceipt> {
    let registry_path_exists = registry_path.exists();
    let entries = read_klepto_registry_entries(registry_path)?;
    let registry_file_digest = if registry_path_exists {
        let bytes = fs::read(registry_path)?;
        crate::digest::tagged(&bytes)
    } else {
        "none".to_string()
    };

    let current = key_receipt.key.clone();
    let (matched_entry, match_model) = select_klepto_registry_entry_for_key(&entries, &current);
    let matched_registered_key = matched_entry
        .as_ref()
        .map(klepto_registered_key_from_entry)
        .unwrap_or_else(|| CodeKleptoRegisteredIndexKey {
            key_id: None,
            key_version: None,
            schema_hash: None,
            field_contract_hash: None,
            analyzer_hash: None,
            tenant_scope_hash: None,
            feature_binding_hash: None,
            document_shape_hash: None,
            source: if registry_path_exists { "registry_no_match".to_string() } else { "registry_missing".to_string() },
        });

    let version_options = CodeKleptoIndexVersionOptions {
        registered_key: matched_registered_key.clone(),
    };
    let version_plan = build_klepto_index_version_plan_receipt(graph, key_receipt, version_options, requested_options);

    let mut findings = version_plan.findings.clone();
    if !registry_path_exists {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPR201".to_string(),
            message: "Klepto index registry file does not exist; current key should be registered before automatic reuse".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    } else if entries.is_empty() {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPR202".to_string(),
            message: "Klepto index registry file exists but has no entries".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    } else if matched_entry.is_some() && version_plan.same_registered_key {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPR203".to_string(),
            message: "current Klepto index key resolved to an existing registry entry".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    } else if matched_entry.is_some() {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPR204".to_string(),
            message: "registry baseline was found, but current Klepto index key requires a new version".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if write_options.write_registry || write_options.ack_write_registry || write_options.overwrite_registry {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPR205".to_string(),
            message: "registry resolve is read-only; write/ack/overwrite registry flags were ignored".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }

    Ok(CodeKleptoIndexRegistryResolveReceipt {
        schema: "code_passenger.klepto_index_registry_resolve_receipt.v1".to_string(),
        graph_schema: version_plan.graph_schema.clone(),
        root: version_plan.root.clone(),
        root_exists: version_plan.root_exists,
        max_depth: version_plan.max_depth,
        registry_path: registry_path.to_string_lossy().to_string(),
        registry_path_exists,
        registry_file_schema: "code_passenger.klepto_index_registry_file.v1".to_string(),
        registry_file_digest,
        registry_entry_count: entries.len(),
        source_version_plan_schema: version_plan.schema.clone(),
        source_key_schema: version_plan.source_key_schema.clone(),
        source_klepto_schema: version_plan.source_klepto_schema.clone(),
        implementation_schema: version_plan.implementation_schema.clone(),
        registry: CodeKleptoIndexRegistryResolveStatus {
            model: "klepto_index_registry_resolver".to_string(),
            storage_model: "local_json_registry_file".to_string(),
            mutates_registry: false,
            mutates_index: false,
            resolve_model: "exact key first, exact component hashes second, then closest latest baseline for bump planning".to_string(),
            notes: vec![
                "resolver reads a persisted registry file and injects the matched entry as the registered baseline".to_string(),
                "exact key/hash matches recommend reuse of the registered version".to_string(),
                "near matches are used only as read-only baselines for patch/minor/major recommendations".to_string(),
                "resolver never writes registry files or mutates Tantivy disk indexes".to_string(),
            ],
        },
        current_key: current,
        matched_entry_id: matched_entry.as_ref().map(|entry| entry.entry_id.clone()),
        matched_key_version: matched_entry.as_ref().map(|entry| entry.key_version.clone()),
        matched_entry,
        match_model,
        matched_registered_key,
        version_plan_action: version_plan.recommended_action.clone(),
        version_plan_bump: version_plan.recommended_bump.clone(),
        version_plan_reason: version_plan.version_reason.clone(),
        same_registered_key: version_plan.same_registered_key,
        hash_delta_count: version_plan.hash_delta_count,
        hash_deltas: version_plan.hash_deltas.clone(),
        write_requested: write_options.write_registry,
        ack_write_registry: write_options.ack_write_registry,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        candidate_committed: version_plan.candidate_committed,
        candidate_search_hits: version_plan.candidate_search_hits,
        findings,
    })
}

pub(crate) fn build_klepto_index_registry_refresh_plan_receipt(
    registry_path: &Path,
    index_dir: &Path,
    registry_resolve: CodeKleptoIndexRegistryResolveReceipt,
    disk_refresh: CodeTantivyDiskRefreshPlanReceipt,
) -> CodeKleptoIndexRegistryRefreshPlanReceipt {
    let mut findings = registry_resolve.findings.clone();
    findings.extend(disk_refresh.findings.clone());

    let (combined_action, combined_reason) = if !registry_resolve.root_exists {
        (
            "defer_missing_root".to_string(),
            "root is missing, so no stable registry or refresh decision can be made".to_string(),
        )
    } else if registry_resolve.version_plan_action == "register_initial" && disk_refresh.refresh_recommended {
        (
            "register_initial_and_refresh_index".to_string(),
            "registry has no reusable key and the disk index needs a future refresh".to_string(),
        )
    } else if registry_resolve.version_plan_action == "register_initial" {
        (
            "register_initial_only".to_string(),
            "registry has no reusable key, but the disk index already matches the candidate shape".to_string(),
        )
    } else if registry_resolve.version_plan_action == "reuse_existing_version" && disk_refresh.refresh_recommended {
        (
            "refresh_index_for_existing_version".to_string(),
            "registry key is reusable, but the disk index differs from the fresh candidate".to_string(),
        )
    } else if registry_resolve.version_plan_action == "reuse_existing_version" {
        (
            "reuse_registry_and_index".to_string(),
            "registry key and existing disk index both match the fresh candidate".to_string(),
        )
    } else if registry_resolve.version_plan_action == "register_new_version" && disk_refresh.refresh_recommended {
        (
            "register_new_version_and_refresh_index".to_string(),
            "current Klepto/index key differs from registry baseline and the disk index needs a future refresh".to_string(),
        )
    } else if registry_resolve.version_plan_action == "register_new_version" {
        (
            "register_new_version_only".to_string(),
            "current Klepto/index key differs from registry baseline, but disk index contents already match the candidate".to_string(),
        )
    } else {
        (
            "review_required".to_string(),
            "registry and refresh decisions should be reviewed before a write phase".to_string(),
        )
    };

    if combined_action.contains("refresh_index") {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPX101".to_string(),
            message: "registry-aware refresh plan recommends a future disk index refresh".to_string(),
            path: Some(index_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if combined_action.contains("register_initial") || combined_action.contains("register_new_version") {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPX102".to_string(),
            message: "registry-aware refresh plan recommends a future registry version change".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if registry_resolve.write_requested || registry_resolve.ack_write_registry || disk_refresh.write_requested || disk_refresh.overwrite_requested {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPX103".to_string(),
            message: "registry refresh-plan mode is read-only; registry and index write flags were ignored".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }

    CodeKleptoIndexRegistryRefreshPlanReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_plan_receipt.v1".to_string(),
        graph_schema: registry_resolve.graph_schema.clone(),
        root: registry_resolve.root.clone(),
        root_exists: registry_resolve.root_exists,
        max_depth: registry_resolve.max_depth,
        registry_path: registry_path.to_string_lossy().to_string(),
        index_dir: index_dir.to_string_lossy().to_string(),
        source_registry_resolve_schema: registry_resolve.schema.clone(),
        source_disk_refresh_schema: disk_refresh.schema.clone(),
        plan: CodeKleptoIndexRegistryRefreshPlanStatus {
            model: "klepto_index_registry_refresh_plan".to_string(),
            registry_model: "persisted_registry_resolve_receipt".to_string(),
            refresh_model: "read_only_disk_manifest_vs_fresh_ram_candidate".to_string(),
            mutates_registry: false,
            mutates_index: false,
            notes: vec![
                "combined plan resolves the Klepto registry before deciding whether a disk index refresh is reusable".to_string(),
                "registry writes remain delegated to klepto-index-registry-write-json with explicit write+ack flags".to_string(),
                "disk index refresh execution is not performed by this command".to_string(),
                "schema/key drift and disk manifest drift are reported together so future write phases can gate safely".to_string(),
            ],
        },
        registry_match_model: registry_resolve.match_model.clone(),
        registry_version_action: registry_resolve.version_plan_action.clone(),
        registry_version_bump: registry_resolve.version_plan_bump.clone(),
        same_registered_key: registry_resolve.same_registered_key,
        hash_delta_count: registry_resolve.hash_delta_count,
        refresh_recommended: disk_refresh.refresh_recommended,
        refresh_reason: disk_refresh.refresh_reason.clone(),
        combined_action,
        combined_reason,
        write_requested: registry_resolve.write_requested || disk_refresh.write_requested,
        ack_write_registry: registry_resolve.ack_write_registry,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        registry_resolve,
        disk_refresh,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_refresh_exec_plan_receipt(
    registry_path: &Path,
    index_dir: &Path,
    refresh_plan: CodeKleptoIndexRegistryRefreshPlanReceipt,
    exec_options: CodeKleptoIndexRegistryRefreshExecOptions,
) -> CodeKleptoIndexRegistryRefreshExecPlanReceipt {
    let registry_write_required = matches!(
        refresh_plan.registry_version_action.as_str(),
        "register_initial" | "register_new_version"
    ) || refresh_plan.combined_action.contains("register_initial")
        || refresh_plan.combined_action.contains("register_new_version");
    let index_refresh_required = refresh_plan.refresh_recommended;

    let registry_ack_satisfied = !registry_write_required || refresh_plan.ack_write_registry;
    let index_ack_satisfied = !index_refresh_required || exec_options.ack_write_index;
    let any_write_requested = refresh_plan.write_requested || exec_options.ack_write_index;

    let execution_action = match (registry_write_required, index_refresh_required) {
        (false, false) => "noop_reuse_registry_and_index",
        (true, false) => "registry_write_only",
        (false, true) => "index_refresh_only",
        (true, true) => "registry_write_and_index_refresh",
    }
    .to_string();

    let execution_ready = refresh_plan.root_exists
        && (registry_write_required || index_refresh_required)
        && registry_ack_satisfied
        && index_ack_satisfied;

    let execution_reason = if !refresh_plan.root_exists {
        "root is missing; execution must be deferred".to_string()
    } else if !registry_write_required && !index_refresh_required {
        "registry and disk index already match the current Klepto/index key".to_string()
    } else if !registry_ack_satisfied || !index_ack_satisfied {
        "one or more required acknowledgement flags are missing".to_string()
    } else {
        "all required acknowledgement flags are present, but this command remains a read-only execution ticket".to_string()
    };

    let mut findings = refresh_plan.findings.clone();
    if !registry_write_required && !index_refresh_required {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPE101".to_string(),
            message: "registry-aware refresh execution plan is a no-op; registry and index are reusable".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }
    if registry_write_required {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPE102".to_string(),
            message: "execution would require a registry version write in a future write command".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if index_refresh_required {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPE103".to_string(),
            message: "execution would require a disk index refresh in a future write command".to_string(),
            path: Some(index_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if registry_write_required && !refresh_plan.ack_write_registry {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPE104".to_string(),
            message: "registry write acknowledgement is required before execution".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if index_refresh_required && !exec_options.ack_write_index {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPE105".to_string(),
            message: "index refresh acknowledgement is required before execution".to_string(),
            path: Some(index_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if any_write_requested {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPE106".to_string(),
            message: "refresh execution-plan mode is read-only; write and acknowledgement flags were recorded but no mutation was performed".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if execution_ready {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPE107".to_string(),
            message: "execution ticket has all required acknowledgements for a future writer phase".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }

    let token_input = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        refresh_plan.registry_path,
        refresh_plan.index_dir,
        refresh_plan.registry_version_action,
        refresh_plan.registry_version_bump,
        refresh_plan.refresh_reason,
        refresh_plan.combined_action,
        registry_write_required,
        index_refresh_required
    );

    CodeKleptoIndexRegistryRefreshExecPlanReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_exec_plan_receipt.v1".to_string(),
        graph_schema: refresh_plan.graph_schema.clone(),
        root: refresh_plan.root.clone(),
        root_exists: refresh_plan.root_exists,
        max_depth: refresh_plan.max_depth,
        registry_path: registry_path.to_string_lossy().to_string(),
        index_dir: index_dir.to_string_lossy().to_string(),
        source_refresh_plan_schema: refresh_plan.schema.clone(),
        execution: CodeKleptoIndexRegistryRefreshExecPlanStatus {
            model: "klepto_index_registry_refresh_exec_plan".to_string(),
            source_plan_model: "phase27_registry_refresh_plan".to_string(),
            execution_model: "read_only_execution_ticket".to_string(),
            mutates_registry: false,
            mutates_index: false,
            requires_registry_ack: registry_write_required,
            requires_index_ack: index_refresh_required,
            notes: vec![
                "execution ticket records exactly which writes a future executor would need".to_string(),
                "this command never writes registry files or refreshes Tantivy indexes".to_string(),
                "registry writes still require --write-registry and --ack-write-registry in the future writer".to_string(),
                "index refreshes require --write-index and --ack-write-index in the future writer".to_string(),
            ],
        },
        registry_version_action: refresh_plan.registry_version_action.clone(),
        registry_version_bump: refresh_plan.registry_version_bump.clone(),
        refresh_recommended: refresh_plan.refresh_recommended,
        refresh_reason: refresh_plan.refresh_reason.clone(),
        combined_plan_action: refresh_plan.combined_action.clone(),
        execution_action,
        execution_reason,
        registry_write_required,
        index_refresh_required,
        write_requested: any_write_requested,
        ack_write_registry: refresh_plan.ack_write_registry,
        ack_write_index: exec_options.ack_write_index,
        registry_ack_satisfied,
        index_ack_satisfied,
        execution_ready,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        execution_token: stable_id("kexec", &token_input),
        refresh_plan,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_refresh_exec_receipt(
    registry_path: &Path,
    index_dir: &Path,
    exec_plan: CodeKleptoIndexRegistryRefreshExecPlanReceipt,
    options: CodeKleptoIndexRegistryRefreshExecuteOptions,
) -> CodeKleptoIndexRegistryRefreshExecReceipt {
    let token_matches = options
        .expected_execution_token
        .as_ref()
        .map(|expected| expected == &exec_plan.execution_token)
        .unwrap_or(true);
    let execution_ack_satisfied = !options.execute_refresh || options.ack_execute_refresh;
    let plan_ready = exec_plan.execution_ready;
    let execution_permitted = options.execute_refresh && options.ack_execute_refresh && token_matches && plan_ready;

    let execution_action = if !exec_plan.registry_write_required && !exec_plan.index_refresh_required {
        "noop_reuse_registry_and_index".to_string()
    } else if !options.execute_refresh {
        "dry_run_execution_not_requested".to_string()
    } else if !options.ack_execute_refresh {
        "blocked_missing_execute_ack".to_string()
    } else if !token_matches {
        "blocked_execution_token_mismatch".to_string()
    } else if !plan_ready {
        "blocked_plan_not_ready".to_string()
    } else {
        "would_execute_in_future_writer".to_string()
    };

    let execution_reason = match execution_action.as_str() {
        "noop_reuse_registry_and_index" => "execution is unnecessary because registry and index already match".to_string(),
        "dry_run_execution_not_requested" => "execution receipt is a dry-run because --execute-refresh was not supplied".to_string(),
        "blocked_missing_execute_ack" => "execution was requested but --ack-execute-refresh was not supplied".to_string(),
        "blocked_execution_token_mismatch" => "execution token did not match the expected token".to_string(),
        "blocked_plan_not_ready" => "execution plan is not ready because lower-level registry/index acknowledgements are missing".to_string(),
        _ => "all execution acknowledgements match, but this phase still records a dry-run receipt only".to_string(),
    };

    let mut findings = exec_plan.findings.clone();
    if !options.execute_refresh {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPE201".to_string(),
            message: "refresh execution receipt was emitted in dry-run mode; --execute-refresh was not supplied".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }
    if options.execute_refresh && !options.ack_execute_refresh {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPE202".to_string(),
            message: "refresh execution was requested but --ack-execute-refresh was not supplied".to_string(),
            path: Some(registry_path.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if options.expected_execution_token.is_some() && !token_matches {
        findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPE203".to_string(),
            message: "expected execution token does not match the generated execution token".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }
    if execution_permitted {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPE204".to_string(),
            message: "execution would be permitted for a future writer, but phase 29 does not mutate registry or index state".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }
    if exec_plan.registry_write_required || exec_plan.index_refresh_required {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPE205".to_string(),
            message: "execution receipt captured required registry/index mutations without performing them".to_string(),
            path: Some(index_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }

    CodeKleptoIndexRegistryRefreshExecReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_exec_receipt.v1".to_string(),
        graph_schema: exec_plan.graph_schema.clone(),
        root: exec_plan.root.clone(),
        root_exists: exec_plan.root_exists,
        max_depth: exec_plan.max_depth,
        registry_path: registry_path.to_string_lossy().to_string(),
        index_dir: index_dir.to_string_lossy().to_string(),
        source_exec_plan_schema: exec_plan.schema.clone(),
        execution: CodeKleptoIndexRegistryRefreshExecStatus {
            model: "klepto_index_registry_refresh_exec_receipt".to_string(),
            source_plan_model: "phase28_refresh_exec_plan".to_string(),
            execution_model: "dry_run_execution_receipt".to_string(),
            mutates_registry: false,
            mutates_index: false,
            requires_execute_ack: exec_plan.registry_write_required || exec_plan.index_refresh_required,
            requires_execution_token_match: options.expected_execution_token.is_some(),
            notes: vec![
                "execution receipts consume the phase 28 ticket and verify final execution intent".to_string(),
                "this phase never writes registry files and never refreshes Tantivy disk indexes".to_string(),
                "a future writer can require this receipt plus matching execution_token before mutation".to_string(),
                "dry-run receipts make no-op, blocked, and would-execute states explicit".to_string(),
            ],
        },
        execution_token: exec_plan.execution_token.clone(),
        expected_execution_token: options.expected_execution_token,
        execution_token_matches: token_matches,
        plan_execution_action: exec_plan.execution_action.clone(),
        execution_action,
        execution_reason,
        registry_write_required: exec_plan.registry_write_required,
        index_refresh_required: exec_plan.index_refresh_required,
        execute_requested: options.execute_refresh,
        ack_execute_refresh: options.ack_execute_refresh,
        execution_ack_satisfied,
        plan_ready,
        execution_permitted,
        execution_performed: false,
        dry_run: true,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        registry_path_exists_after: registry_path.exists(),
        index_dir_exists_after: index_dir.exists(),
        exec_plan,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_refresh_stage_receipt(
    staging_dir: &Path,
    registry_path: &Path,
    index_dir: &Path,
    exec_receipt: CodeKleptoIndexRegistryRefreshExecReceipt,
    options: CodeKleptoIndexRegistryRefreshStageOptions,
) -> Result<CodeKleptoIndexRegistryRefreshStageReceipt> {
    let staging_dir_exists_before = staging_dir.exists();
    let stage_required = exec_receipt.registry_write_required || exec_receipt.index_refresh_required;
    let _stage_ack_satisfied = !options.stage_refresh || options.ack_stage_refresh;
    let staging_allowed = stage_required
        && options.stage_refresh
        && options.ack_stage_refresh
        && exec_receipt.execution_permitted;

    let staging_action = if !stage_required {
        "noop_no_stage_needed".to_string()
    } else if !options.stage_refresh {
        "dry_run_staging_not_requested".to_string()
    } else if !options.ack_stage_refresh {
        "blocked_missing_stage_ack".to_string()
    } else if !exec_receipt.execution_permitted {
        "blocked_execution_not_permitted".to_string()
    } else {
        "staged_for_future_swap".to_string()
    };

    let staging_reason = match staging_action.as_str() {
        "noop_no_stage_needed" => "registry and index already match; no staging files are needed".to_string(),
        "dry_run_staging_not_requested" => "staging receipt is dry-run because --stage-refresh was not supplied".to_string(),
        "blocked_missing_stage_ack" => "staging was requested but --ack-stage-refresh was not supplied".to_string(),
        "blocked_execution_not_permitted" => "phase 29 execution receipt was not permitted, so staging is blocked".to_string(),
        _ => "staging files were written to an isolated staging directory only".to_string(),
    };

    let mut findings = exec_receipt.findings.clone();
    if stage_required && !options.stage_refresh {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPSG001".to_string(),
            message: "refresh staging was not requested; no staging directory was created".to_string(),
            path: Some(staging_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if options.stage_refresh && !options.ack_stage_refresh {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPSG002".to_string(),
            message: "refresh staging was requested but --ack-stage-refresh was not supplied".to_string(),
            path: Some(staging_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }
    if options.stage_refresh && options.ack_stage_refresh && !exec_receipt.execution_permitted && stage_required {
        findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPSG003".to_string(),
            message: "refresh staging is blocked because the execution receipt is not permitted".to_string(),
            path: Some(staging_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }

    let staging_id = stable_id(
        "kstg",
        &format!(
            "{}|{}|{}|{}|{}|{}",
            staging_dir.to_string_lossy(),
            registry_path.to_string_lossy(),
            index_dir.to_string_lossy(),
            exec_receipt.execution_token,
            exec_receipt.plan_execution_action,
            exec_receipt.exec_plan.registry_version_bump
        ),
    );

    let mut staged_manifest_path = None;
    let mut staged_exec_receipt_path = None;
    let mut staged_registry_candidate_path = None;
    let mut staged_index_marker_path = None;
    let mut staged_file_count = 0usize;

    if staging_allowed {
        let stage_root = staging_dir.join(&staging_id);
        fs::create_dir_all(&stage_root)?;
        let exec_path = stage_root.join("execution_receipt.json");
        let registry_candidate_path = stage_root.join("registry_candidate.json");
        let index_marker_path = stage_root.join("index_refresh_marker.json");
        let manifest_path = stage_root.join("staging_manifest.json");

        fs::write(&exec_path, format!("{}\n", serde_json::to_string_pretty(&exec_receipt)?))?;
        staged_file_count += 1;

        if exec_receipt.registry_write_required {
            let registry_candidate = serde_json::json!({
                "schema": "code_passenger.klepto_index_registry_staged_candidate.v1",
                "source_exec_receipt_schema": exec_receipt.schema.clone(),
                "execution_token": exec_receipt.execution_token.clone(),
                "registry_path": registry_path.to_string_lossy().to_string(),
                "registry_action": exec_receipt.exec_plan.registry_version_action.clone(),
                "registry_bump": exec_receipt.exec_plan.registry_version_bump.clone(),
                "live_registry_mutated": false
            });
            fs::write(&registry_candidate_path, format!("{}\n", serde_json::to_string_pretty(&registry_candidate)?))?;
            staged_registry_candidate_path = Some(registry_candidate_path.to_string_lossy().to_string());
            staged_file_count += 1;
        }

        if exec_receipt.index_refresh_required {
            let index_marker = serde_json::json!({
                "schema": "code_passenger.klepto_index_refresh_staged_marker.v1",
                "source_exec_receipt_schema": exec_receipt.schema.clone(),
                "execution_token": exec_receipt.execution_token.clone(),
                "index_dir": index_dir.to_string_lossy().to_string(),
                "refresh_reason": exec_receipt.exec_plan.refresh_reason.clone(),
                "live_index_mutated": false
            });
            fs::write(&index_marker_path, format!("{}\n", serde_json::to_string_pretty(&index_marker)?))?;
            staged_index_marker_path = Some(index_marker_path.to_string_lossy().to_string());
            staged_file_count += 1;
        }

        let manifest = serde_json::json!({
            "schema": "code_passenger.klepto_index_registry_refresh_staging_manifest.v1",
            "staging_id": staging_id.clone(),
            "execution_token": exec_receipt.execution_token.clone(),
            "staging_action": staging_action.clone(),
            "registry_path": registry_path.to_string_lossy().to_string(),
            "index_dir": index_dir.to_string_lossy().to_string(),
            "live_registry_mutated": false,
            "live_index_mutated": false,
            "staged_file_count": staged_file_count + 1
        });
        fs::write(&manifest_path, format!("{}\n", serde_json::to_string_pretty(&manifest)?))?;
        staged_file_count += 1;
        staged_manifest_path = Some(manifest_path.to_string_lossy().to_string());
        staged_exec_receipt_path = Some(exec_path.to_string_lossy().to_string());

        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPSG004".to_string(),
            message: "refresh execution was staged in an isolated staging directory; live registry and index were not mutated".to_string(),
            path: Some(stage_root.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }

    let exec_permitted = exec_receipt.execution_permitted;

    Ok(CodeKleptoIndexRegistryRefreshStageReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_stage_receipt.v1".to_string(),
        graph_schema: exec_receipt.graph_schema.clone(),
        root: exec_receipt.root.clone(),
        root_exists: exec_receipt.root_exists,
        max_depth: exec_receipt.max_depth,
        staging_dir: staging_dir.to_string_lossy().to_string(),
        registry_path: registry_path.to_string_lossy().to_string(),
        index_dir: index_dir.to_string_lossy().to_string(),
        source_exec_receipt_schema: exec_receipt.schema.clone(),
        staging: CodeKleptoIndexRegistryRefreshStageStatus {
            model: "klepto_index_registry_refresh_staging_writer".to_string(),
            source_model: "phase29_refresh_exec_receipt".to_string(),
            staging_model: "isolated_json_staging_directory".to_string(),
            mutates_staging: staging_allowed,
            mutates_registry: false,
            mutates_index: false,
            requires_stage_ack: stage_required,
            notes: vec![
                "staging writes are isolated under the supplied staging directory".to_string(),
                "live registry files are never modified by this command".to_string(),
                "live Tantivy disk indexes are never refreshed or swapped by this command".to_string(),
                "future swap phases can consume the staged manifest after a separate policy gate".to_string(),
            ],
        },
        exec_receipt,
        staging_id,
        staging_action,
        staging_reason,
        stage_requested: options.stage_refresh,
        ack_stage_refresh: options.ack_stage_refresh,
        execution_permitted: exec_permitted,
        staging_allowed,
        staging_performed: staging_allowed,
        staging_dir_exists_before,
        staging_dir_exists_after: staging_dir.exists(),
        staged_manifest_path,
        staged_exec_receipt_path,
        staged_registry_candidate_path,
        staged_index_marker_path,
        staged_file_count,
        live_registry_path_exists_after: registry_path.exists(),
        live_index_dir_exists_after: index_dir.exists(),
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        findings,
    })
}


pub(crate) fn build_klepto_index_registry_refresh_stage_verify_receipt(
    staging_dir: &Path,
    options: CodeKleptoIndexRegistryRefreshStageVerifyOptions,
) -> Result<CodeKleptoIndexRegistryRefreshStageVerifyReceipt> {
    let staging_dir_exists = staging_dir.exists();
    let mut findings = Vec::<FindingNode>::new();
    let mut stage_roots = Vec::<PathBuf>::new();

    if staging_dir.join("staging_manifest.json").exists() {
        stage_roots.push(staging_dir.to_path_buf());
    }
    if staging_dir.is_dir() {
        for entry in fs::read_dir(staging_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && path.join("staging_manifest.json").exists() {
                stage_roots.push(path);
            }
        }
    }
    stage_roots.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    stage_roots.dedup();

    let discovered_stage_count = stage_roots.len();
    let selected_stage_path_buf = stage_roots.last().cloned();

    let mut selected_stage_id = None;
    let mut selected_stage_path = None;
    let mut manifest_path = None;
    let mut exec_receipt_path = None;
    let mut registry_candidate_path = None;
    let mut index_marker_path = None;
    let mut manifest_schema_ok = false;
    let mut exec_receipt_schema_ok = false;
    let mut registry_candidate_schema_ok = true;
    let mut index_marker_schema_ok = true;
    let mut manifest_execution_token = None;
    let mut exec_receipt_execution_token = None;
    let mut staged_file_count_manifest = None;
    let mut staged_file_count_observed = 0usize;
    let mut registry_candidate_required = false;
    let mut index_marker_required = false;

    if let Some(stage_root) = selected_stage_path_buf.as_ref() {
        selected_stage_id = stage_root.file_name().map(|name| name.to_string_lossy().to_string());
        selected_stage_path = Some(stage_root.to_string_lossy().to_string());

        let manifest_file = stage_root.join("staging_manifest.json");
        let exec_file = stage_root.join("execution_receipt.json");
        let registry_file = stage_root.join("registry_candidate.json");
        let index_file = stage_root.join("index_refresh_marker.json");

        manifest_path = Some(manifest_file.to_string_lossy().to_string());
        exec_receipt_path = Some(exec_file.to_string_lossy().to_string());

        if manifest_file.exists() {
            staged_file_count_observed += 1;
            let manifest_value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&manifest_file)?)?;
            manifest_schema_ok = json_value_str(&manifest_value, "schema").as_deref() == Some("code_passenger.klepto_index_registry_refresh_staging_manifest.v1");
            manifest_execution_token = json_value_str(&manifest_value, "execution_token");
            staged_file_count_manifest = manifest_value.get("staged_file_count").and_then(|value| value.as_u64()).map(|value| value as usize);
        }

        if exec_file.exists() {
            staged_file_count_observed += 1;
            let exec_value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&exec_file)?)?;
            exec_receipt_schema_ok = json_value_str(&exec_value, "schema").as_deref() == Some("code_passenger.klepto_index_registry_refresh_exec_receipt.v1");
            exec_receipt_execution_token = json_value_str(&exec_value, "execution_token");
            registry_candidate_required = exec_value.get("registry_write_required").and_then(|value| value.as_bool()).unwrap_or(false);
            index_marker_required = exec_value.get("index_refresh_required").and_then(|value| value.as_bool()).unwrap_or(false);
        }

        if registry_file.exists() {
            staged_file_count_observed += 1;
            registry_candidate_path = Some(registry_file.to_string_lossy().to_string());
            let registry_value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&registry_file)?)?;
            registry_candidate_schema_ok = json_value_str(&registry_value, "schema").as_deref() == Some("code_passenger.klepto_index_registry_staged_candidate.v1");
        } else if registry_candidate_required {
            registry_candidate_schema_ok = false;
        }

        if index_file.exists() {
            staged_file_count_observed += 1;
            index_marker_path = Some(index_file.to_string_lossy().to_string());
            let index_value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&index_file)?)?;
            index_marker_schema_ok = json_value_str(&index_value, "schema").as_deref() == Some("code_passenger.klepto_index_refresh_staged_marker.v1");
        } else if index_marker_required {
            index_marker_schema_ok = false;
        }
    }

    let execution_token = exec_receipt_execution_token.clone().or_else(|| manifest_execution_token.clone());
    let execution_token_matches = manifest_execution_token.is_some()
        && exec_receipt_execution_token.is_some()
        && manifest_execution_token == exec_receipt_execution_token;
    let expected_execution_token_matches = options.expected_execution_token.as_ref().map(|expected| execution_token.as_ref() == Some(expected)).unwrap_or(true);
    let staged_file_count_matches = staged_file_count_manifest.map(|expected| expected == staged_file_count_observed).unwrap_or(false);

    let verified = discovered_stage_count > 0
        && manifest_schema_ok
        && exec_receipt_schema_ok
        && registry_candidate_schema_ok
        && index_marker_schema_ok
        && execution_token_matches
        && expected_execution_token_matches
        && staged_file_count_matches;

    let verification_action = if discovered_stage_count == 0 {
        "missing_stage".to_string()
    } else if !expected_execution_token_matches {
        "blocked_token_mismatch".to_string()
    } else if verified {
        "verified_for_future_swap".to_string()
    } else {
        "invalid_stage_artifacts".to_string()
    };

    let verification_reason = match verification_action.as_str() {
        "missing_stage" => "no staged refresh manifest was found under the supplied staging directory".to_string(),
        "blocked_token_mismatch" => "the supplied expected execution token does not match the staged execution token".to_string(),
        "verified_for_future_swap" => "staged manifest, execution receipt, optional candidates, token, and file counts verified without mutation".to_string(),
        _ => "one or more staged artifacts failed schema, token, or file-count verification".to_string(),
    };

    match verification_action.as_str() {
        "missing_stage" => findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPSV001".to_string(),
            message: "no staged refresh manifest was found".to_string(),
            path: Some(staging_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        }),
        "blocked_token_mismatch" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPSV003".to_string(),
            message: "expected execution token does not match staged execution token".to_string(),
            path: selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "verified_for_future_swap" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPSV004".to_string(),
            message: "staged refresh artifacts verified for a future guarded swap".to_string(),
            path: selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        _ => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPSV002".to_string(),
            message: "staged refresh artifacts failed verification".to_string(),
            path: selected_stage_path.clone(),
            line: None,
            column: None,
        }),
    }

    Ok(CodeKleptoIndexRegistryRefreshStageVerifyReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_stage_verify_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        staging_dir_exists,
        verification: CodeKleptoIndexRegistryRefreshStageVerifyStatus {
            model: "klepto_index_registry_refresh_stage_verifier".to_string(),
            source_model: "phase30_isolated_staging_directory".to_string(),
            verification_model: "read_only_manifest_token_and_artifact_check".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            notes: vec![
                "verifier reads staged JSON artifacts only".to_string(),
                "verifier checks manifest schema, execution receipt schema, execution-token consistency, and staged-file counts".to_string(),
                "verifier never mutates staging, registry, or Tantivy index paths".to_string(),
                "future swap phases can require this receipt before consuming staged artifacts".to_string(),
            ],
        },
        expected_execution_token: options.expected_execution_token,
        discovered_stage_count,
        selected_stage_id,
        selected_stage_path,
        manifest_path,
        exec_receipt_path,
        registry_candidate_path,
        index_marker_path,
        manifest_schema_ok,
        exec_receipt_schema_ok,
        registry_candidate_schema_ok,
        index_marker_schema_ok,
        execution_token,
        manifest_execution_token,
        exec_receipt_execution_token,
        execution_token_matches,
        expected_execution_token_matches,
        staged_file_count_manifest,
        staged_file_count_observed,
        staged_file_count_matches,
        registry_candidate_required,
        index_marker_required,
        verified,
        verification_action,
        verification_reason,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        findings,
    })
}

pub(crate) fn build_klepto_index_registry_refresh_swap_plan_receipt(
    staging_dir: &Path,
    stage_verify: CodeKleptoIndexRegistryRefreshStageVerifyReceipt,
) -> CodeKleptoIndexRegistryRefreshSwapPlanReceipt {
    let registry_swap_required = stage_verify.registry_candidate_required;
    let index_swap_required = stage_verify.index_marker_required;
    let mut required_artifacts = vec![
        "staging_manifest.json".to_string(),
        "execution_receipt.json".to_string(),
    ];
    if registry_swap_required {
        required_artifacts.push("registry_candidate.json".to_string());
    }
    if index_swap_required {
        required_artifacts.push("index_refresh_marker.json".to_string());
    }

    let swap_ready = stage_verify.verified && (registry_swap_required || index_swap_required);
    let swap_action = if stage_verify.discovered_stage_count == 0 {
        "missing_stage".to_string()
    } else if !stage_verify.verified {
        "blocked_unverified_stage".to_string()
    } else if !registry_swap_required && !index_swap_required {
        "noop_verified_stage".to_string()
    } else {
        "ready_for_future_swap".to_string()
    };

    let swap_reason = match swap_action.as_str() {
        "missing_stage" => "no verified staging manifest was available to plan a swap".to_string(),
        "blocked_unverified_stage" => "staged artifacts must pass phase 31 verification before a swap can be planned".to_string(),
        "noop_verified_stage" => "the verified stage does not require registry or index swaps".to_string(),
        _ => "verified staged artifacts are ready for a future guarded swap/apply phase".to_string(),
    };

    let swap_plan_id = stable_id(
        "kswap",
        &format!(
            "{}|{}|{}|{}|{}",
            staging_dir.to_string_lossy(),
            stage_verify.execution_token.clone().unwrap_or_else(|| "none".to_string()),
            stage_verify.selected_stage_path.clone().unwrap_or_else(|| "none".to_string()),
            registry_swap_required,
            index_swap_required
        ),
    );

    let mut findings = stage_verify.findings.clone();
    match swap_action.as_str() {
        "missing_stage" => findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPSW001".to_string(),
            message: "no verified stage is available for swap planning".to_string(),
            path: Some(staging_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        }),
        "blocked_unverified_stage" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPSW002".to_string(),
            message: "stage verification failed, so swap planning is blocked".to_string(),
            path: stage_verify.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "ready_for_future_swap" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPSW003".to_string(),
            message: "verified stage is ready for a future guarded swap; no live paths were mutated".to_string(),
            path: stage_verify.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        _ => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPSW004".to_string(),
            message: "verified stage requires no registry or index swap".to_string(),
            path: stage_verify.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
    }

    CodeKleptoIndexRegistryRefreshSwapPlanReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_swap_plan_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_stage_verify_schema: stage_verify.schema.clone(),
        swap: CodeKleptoIndexRegistryRefreshSwapPlanStatus {
            model: "klepto_index_registry_refresh_swap_planner".to_string(),
            source_model: "phase31_verified_stage_receipt".to_string(),
            swap_model: "read_only_future_swap_plan".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            requires_verified_stage: true,
            notes: vec![
                "swap plans consume verified stage receipts only".to_string(),
                "this command never writes staging files, registry files, or Tantivy indexes".to_string(),
                "future apply phases can require swap_plan_id and execution_token before mutation".to_string(),
                "registry and index swap requirements are derived from staged artifacts, not command intent flags".to_string(),
            ],
        },
        selected_stage_id: stage_verify.selected_stage_id.clone(),
        selected_stage_path: stage_verify.selected_stage_path.clone(),
        execution_token: stage_verify.execution_token.clone(),
        swap_plan_id,
        swap_action,
        swap_reason,
        swap_ready,
        registry_swap_required,
        index_swap_required,
        required_artifacts,
        verified: stage_verify.verified,
        expected_execution_token_matches: stage_verify.expected_execution_token_matches,
        staging_dir_exists: stage_verify.staging_dir_exists,
        discovered_stage_count: stage_verify.discovered_stage_count,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        stage_verify,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_refresh_swap_exec_receipt(
    staging_dir: &Path,
    swap_plan: CodeKleptoIndexRegistryRefreshSwapPlanReceipt,
    options: CodeKleptoIndexRegistryRefreshSwapExecuteOptions,
) -> CodeKleptoIndexRegistryRefreshSwapExecReceipt {
    let execution_token_matches = options
        .expected_execution_token
        .as_ref()
        .map(|expected| swap_plan.execution_token.as_ref().map(|actual| actual == expected).unwrap_or(false))
        .unwrap_or(true);
    let swap_plan_id_matches = options
        .expected_swap_plan_id
        .as_ref()
        .map(|expected| expected == &swap_plan.swap_plan_id)
        .unwrap_or(true);
    let swap_ack_satisfied = !options.execute_swap || options.ack_execute_swap;
    let plan_ready = swap_plan.swap_ready;
    let swap_permitted = options.execute_swap
        && options.ack_execute_swap
        && execution_token_matches
        && swap_plan_id_matches
        && plan_ready;

    let swap_action = if !swap_plan.registry_swap_required && !swap_plan.index_swap_required && swap_plan.verified {
        "noop_no_swap_required".to_string()
    } else if !options.execute_swap {
        "dry_run_swap_not_requested".to_string()
    } else if !options.ack_execute_swap {
        "blocked_missing_swap_ack".to_string()
    } else if !execution_token_matches {
        "blocked_execution_token_mismatch".to_string()
    } else if !swap_plan_id_matches {
        "blocked_swap_plan_id_mismatch".to_string()
    } else if !plan_ready {
        "blocked_swap_plan_not_ready".to_string()
    } else {
        "would_swap_in_future_apply".to_string()
    };

    let swap_reason = match swap_action.as_str() {
        "noop_no_swap_required" => "the verified stage requires no registry or index swap".to_string(),
        "dry_run_swap_not_requested" => "swap execution receipt is dry-run because --execute-swap was not supplied".to_string(),
        "blocked_missing_swap_ack" => "swap execution was requested but --ack-execute-swap was not supplied".to_string(),
        "blocked_execution_token_mismatch" => "expected execution token did not match the verified stage token".to_string(),
        "blocked_swap_plan_id_mismatch" => "expected swap plan id did not match the generated swap plan id".to_string(),
        "blocked_swap_plan_not_ready" => "phase 32 swap plan is not ready, so swap execution is blocked".to_string(),
        _ => "all swap acknowledgements match, but this phase records a dry-run receipt only".to_string(),
    };

    let mut findings = swap_plan.findings.clone();
    if !options.execute_swap && (swap_plan.registry_swap_required || swap_plan.index_swap_required) {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPSX001".to_string(),
            message: "swap execution receipt was emitted in dry-run mode; --execute-swap was not supplied".to_string(),
            path: swap_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        });
    }
    if options.execute_swap && !options.ack_execute_swap {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPSX002".to_string(),
            message: "swap execution was requested but --ack-execute-swap was not supplied".to_string(),
            path: swap_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        });
    }
    if options.expected_execution_token.is_some() && !execution_token_matches {
        findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPSX003".to_string(),
            message: "expected execution token does not match the verified stage token".to_string(),
            path: swap_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        });
    }
    if options.expected_swap_plan_id.is_some() && !swap_plan_id_matches {
        findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPSX004".to_string(),
            message: "expected swap plan id does not match the generated swap plan id".to_string(),
            path: swap_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        });
    }
    if swap_permitted {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPSX005".to_string(),
            message: "swap would be permitted for a future guarded apply command, but phase 33 does not mutate live paths".to_string(),
            path: swap_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        });
    }
    if !swap_plan.swap_ready && options.execute_swap {
        findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPSX006".to_string(),
            message: "swap execution requested against a swap plan that is not ready".to_string(),
            path: Some(staging_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        });
    }

    CodeKleptoIndexRegistryRefreshSwapExecReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_swap_exec_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_swap_plan_schema: swap_plan.schema.clone(),
        swap_execution: CodeKleptoIndexRegistryRefreshSwapExecStatus {
            model: "klepto_index_registry_refresh_swap_exec_receipt".to_string(),
            source_model: "phase32_verified_stage_swap_plan".to_string(),
            execution_model: "dry_run_future_swap_execution".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            requires_swap_ack: swap_plan.registry_swap_required || swap_plan.index_swap_required,
            requires_execution_token_match: options.expected_execution_token.is_some(),
            requires_swap_plan_id_match: options.expected_swap_plan_id.is_some(),
            notes: vec![
                "swap execution receipts consume phase 32 swap plans and verify final apply intent".to_string(),
                "this phase never writes registry files and never mutates Tantivy disk indexes".to_string(),
                "future apply phases can require this receipt plus matching swap_plan_id and execution_token before mutation".to_string(),
                "dry-run receipts make no-op, blocked, and would-swap states explicit".to_string(),
            ],
        },
        selected_stage_id: swap_plan.selected_stage_id.clone(),
        selected_stage_path: swap_plan.selected_stage_path.clone(),
        execution_token: swap_plan.execution_token.clone(),
        swap_plan_id: swap_plan.swap_plan_id.clone(),
        expected_execution_token: options.expected_execution_token,
        expected_swap_plan_id: options.expected_swap_plan_id,
        execution_token_matches,
        swap_plan_id_matches,
        swap_requested: options.execute_swap,
        ack_swap: options.ack_execute_swap,
        swap_ack_satisfied,
        plan_ready,
        swap_action,
        swap_reason,
        registry_swap_required: swap_plan.registry_swap_required,
        index_swap_required: swap_plan.index_swap_required,
        swap_permitted,
        swap_performed: false,
        dry_run: true,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        swap_plan,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_refresh_apply_plan_receipt(
    staging_dir: &Path,
    swap_exec: CodeKleptoIndexRegistryRefreshSwapExecReceipt,
) -> CodeKleptoIndexRegistryRefreshApplyPlanReceipt {
    let registry_apply_required = swap_exec.registry_swap_required;
    let index_apply_required = swap_exec.index_swap_required;
    let apply_ready = swap_exec.swap_permitted
        && !swap_exec.swap_performed
        && (registry_apply_required || index_apply_required);

    let mut required_source_artifacts = vec![
        "staging_manifest.json".to_string(),
        "execution_receipt.json".to_string(),
    ];
    if registry_apply_required {
        required_source_artifacts.push("registry_candidate.json".to_string());
    }
    if index_apply_required {
        required_source_artifacts.push("index_refresh_marker.json".to_string());
    }

    let mut required_live_mutations = Vec::new();
    if registry_apply_required {
        required_live_mutations.push("atomically_replace_or_upsert_live_registry_from_staged_registry_candidate".to_string());
    }
    if index_apply_required {
        required_live_mutations.push("atomically_promote_staged_index_refresh_into_live_index_dir".to_string());
    }

    let apply_plan_id = stable_id(
        "kaply",
        &format!(
            "{}:{}:{}:{}:{}",
            staging_dir.to_string_lossy(),
            swap_exec.selected_stage_id.clone().unwrap_or_default(),
            swap_exec.execution_token.clone().unwrap_or_default(),
            swap_exec.swap_plan_id,
            required_live_mutations.join("|")
        ),
    );

    let apply_action = if !registry_apply_required && !index_apply_required && swap_exec.swap_plan.verified {
        "noop_no_apply_required".to_string()
    } else if !swap_exec.swap_requested {
        "dry_run_apply_not_requested".to_string()
    } else if !swap_exec.swap_permitted {
        "blocked_swap_execution_not_permitted".to_string()
    } else if apply_ready {
        "ready_for_future_apply".to_string()
    } else {
        "blocked_apply_plan_not_ready".to_string()
    };

    let apply_reason = match apply_action.as_str() {
        "noop_no_apply_required" => "verified swap execution path requires no registry or index apply".to_string(),
        "dry_run_apply_not_requested" => "apply plan is dry-run because upstream swap execution was not requested".to_string(),
        "blocked_swap_execution_not_permitted" => "phase 33 swap execution receipt did not permit future apply".to_string(),
        "ready_for_future_apply" => "phase 33 swap execution receipt is permitted; this phase only describes future live mutations".to_string(),
        _ => "apply plan is not ready for a future guarded writer".to_string(),
    };

    let mut findings = swap_exec.findings.clone();
    match apply_action.as_str() {
        "noop_no_apply_required" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPAP001".to_string(),
            message: "verified swap path requires no live registry or index apply".to_string(),
            path: swap_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "dry_run_apply_not_requested" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPAP002".to_string(),
            message: "apply plan emitted without requested swap execution; no future live mutation is permitted".to_string(),
            path: swap_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "blocked_swap_execution_not_permitted" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPAP003".to_string(),
            message: "apply plan blocked because swap execution was not permitted".to_string(),
            path: swap_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "ready_for_future_apply" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPAP004".to_string(),
            message: "apply plan is ready for a future guarded writer; phase 34 does not mutate live paths".to_string(),
            path: swap_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        _ => findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPAP005".to_string(),
            message: "apply plan is not ready for future live mutation".to_string(),
            path: swap_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
    }

    CodeKleptoIndexRegistryRefreshApplyPlanReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_apply_plan_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_swap_exec_schema: swap_exec.schema.clone(),
        apply: CodeKleptoIndexRegistryRefreshApplyPlanStatus {
            model: "klepto_index_registry_refresh_apply_planner".to_string(),
            source_model: "phase33_swap_execution_receipt".to_string(),
            apply_model: "read_only_future_live_apply_plan".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            requires_swap_execution_receipt: true,
            requires_permitted_swap: registry_apply_required || index_apply_required,
            notes: vec![
                "apply plans consume phase 33 swap execution receipts".to_string(),
                "this command describes future live registry/index mutations but never performs them".to_string(),
                "future apply writers can require apply_plan_id, swap_plan_id, and execution_token before mutation".to_string(),
                "required live mutations are derived from verified staged artifacts, not from live path flags".to_string(),
            ],
        },
        selected_stage_id: swap_exec.selected_stage_id.clone(),
        selected_stage_path: swap_exec.selected_stage_path.clone(),
        execution_token: swap_exec.execution_token.clone(),
        swap_plan_id: swap_exec.swap_plan_id.clone(),
        apply_plan_id,
        apply_action,
        apply_reason,
        apply_ready,
        registry_apply_required,
        index_apply_required,
        required_source_artifacts,
        required_live_mutations,
        swap_permitted: swap_exec.swap_permitted,
        swap_performed: swap_exec.swap_performed,
        dry_run: true,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        swap_exec,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_refresh_apply_exec_receipt(
    staging_dir: &Path,
    apply_plan: CodeKleptoIndexRegistryRefreshApplyPlanReceipt,
    options: CodeKleptoIndexRegistryRefreshApplyExecuteOptions,
) -> CodeKleptoIndexRegistryRefreshApplyExecReceipt {
    let plan_ready = apply_plan.apply_ready;
    let registry_apply_required = apply_plan.registry_apply_required;
    let index_apply_required = apply_plan.index_apply_required;
    let apply_ack_satisfied = !options.execute_apply || options.ack_execute_apply;
    let apply_plan_id_matches = options
        .expected_apply_plan_id
        .as_ref()
        .map(|expected| expected == &apply_plan.apply_plan_id)
        .unwrap_or(true);

    let apply_permitted = options.execute_apply
        && options.ack_execute_apply
        && apply_plan_id_matches
        && plan_ready
        && (registry_apply_required || index_apply_required);

    let apply_action = if !registry_apply_required && !index_apply_required && apply_plan.apply_action == "noop_no_apply_required" {
        "noop_no_apply_required".to_string()
    } else if !options.execute_apply {
        "dry_run_apply_not_requested".to_string()
    } else if !options.ack_execute_apply {
        "blocked_missing_apply_ack".to_string()
    } else if !apply_plan_id_matches {
        "blocked_apply_plan_id_mismatch".to_string()
    } else if !plan_ready {
        "blocked_apply_plan_not_ready".to_string()
    } else if apply_permitted {
        "would_apply_in_future_writer".to_string()
    } else {
        "blocked_apply_execution_not_permitted".to_string()
    };

    let apply_reason = match apply_action.as_str() {
        "noop_no_apply_required" => "verified staged path requires no live registry or index apply".to_string(),
        "dry_run_apply_not_requested" => "apply execution was not requested; receipt remains a dry-run".to_string(),
        "blocked_missing_apply_ack" => "apply execution requires --ack-execute-apply".to_string(),
        "blocked_apply_plan_id_mismatch" => "expected apply_plan_id did not match the computed phase 34 apply plan".to_string(),
        "blocked_apply_plan_not_ready" => "phase 34 apply plan was not ready for a future live writer".to_string(),
        "would_apply_in_future_writer" => "all dry-run gates are satisfied; a future writer would be allowed to apply staged artifacts".to_string(),
        _ => "apply execution is not permitted".to_string(),
    };

    let mut findings = apply_plan.findings.clone();
    match apply_action.as_str() {
        "noop_no_apply_required" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPAE001".to_string(),
            message: "apply execution path is a no-op because no live mutation is required".to_string(),
            path: apply_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "dry_run_apply_not_requested" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPAE002".to_string(),
            message: "apply execution receipt emitted without --execute-apply; no live mutation is permitted".to_string(),
            path: apply_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "blocked_missing_apply_ack" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPAE003".to_string(),
            message: "apply execution requested without --ack-execute-apply".to_string(),
            path: apply_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "blocked_apply_plan_id_mismatch" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPAE004".to_string(),
            message: "apply execution blocked because the expected apply_plan_id did not match".to_string(),
            path: apply_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "blocked_apply_plan_not_ready" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPAE005".to_string(),
            message: "apply execution blocked because phase 34 apply plan is not ready".to_string(),
            path: apply_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "would_apply_in_future_writer" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPAE006".to_string(),
            message: "apply execution is permitted for a future writer; phase 35 does not mutate live paths".to_string(),
            path: apply_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        _ => findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPAE007".to_string(),
            message: "apply execution is not permitted".to_string(),
            path: apply_plan.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
    }

    CodeKleptoIndexRegistryRefreshApplyExecReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_apply_exec_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_apply_plan_schema: apply_plan.schema.clone(),
        apply_execution: CodeKleptoIndexRegistryRefreshApplyExecStatus {
            model: "klepto_index_registry_refresh_apply_exec_receipt".to_string(),
            source_model: "phase34_apply_plan_receipt".to_string(),
            execution_model: "dry_run_future_live_apply_execution".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            requires_apply_ack: registry_apply_required || index_apply_required,
            requires_apply_plan_id_match: options.expected_apply_plan_id.is_some(),
            notes: vec![
                "apply execution receipts consume phase 34 apply plans and final explicit apply intent".to_string(),
                "this phase never writes registry files and never mutates Tantivy disk indexes".to_string(),
                "future live writers can require this receipt plus matching apply_plan_id, swap_plan_id, and execution_token".to_string(),
                "dry-run receipts make no-op, blocked, and would-apply states explicit".to_string(),
            ],
        },
        selected_stage_id: apply_plan.selected_stage_id.clone(),
        selected_stage_path: apply_plan.selected_stage_path.clone(),
        execution_token: apply_plan.execution_token.clone(),
        swap_plan_id: apply_plan.swap_plan_id.clone(),
        apply_plan_id: apply_plan.apply_plan_id.clone(),
        expected_apply_plan_id: options.expected_apply_plan_id,
        apply_plan_id_matches,
        apply_requested: options.execute_apply,
        ack_apply: options.ack_execute_apply,
        apply_ack_satisfied,
        plan_ready,
        apply_action,
        apply_reason,
        registry_apply_required,
        index_apply_required,
        required_source_artifacts: apply_plan.required_source_artifacts.clone(),
        required_live_mutations: apply_plan.required_live_mutations.clone(),
        apply_permitted,
        apply_performed: false,
        dry_run: true,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        apply_plan,
        findings,
    }
}


pub(crate) fn build_klepto_index_registry_refresh_live_apply_request_receipt(
    staging_dir: &Path,
    apply_exec: CodeKleptoIndexRegistryRefreshApplyExecReceipt,
    options: CodeKleptoIndexRegistryRefreshLiveApplyRequestOptions,
) -> CodeKleptoIndexRegistryRefreshLiveApplyRequestReceipt {
    let live_apply_ack_satisfied = !options.request_live_apply || options.ack_request_live_apply;
    let live_apply_request_permitted = options.request_live_apply
        && options.ack_request_live_apply
        && apply_exec.apply_permitted
        && !apply_exec.apply_performed
        && (apply_exec.registry_apply_required || apply_exec.index_apply_required);

    let live_apply_request_id = stable_id(
        "klive",
        &format!(
            "{}:{}:{}:{}:{}:{}",
            staging_dir.to_string_lossy(),
            apply_exec.selected_stage_id.clone().unwrap_or_default(),
            apply_exec.execution_token.clone().unwrap_or_default(),
            apply_exec.swap_plan_id,
            apply_exec.apply_plan_id,
            apply_exec.required_live_mutations.join("|")
        ),
    );

    let request_action = if !apply_exec.registry_apply_required && !apply_exec.index_apply_required && apply_exec.apply_action == "noop_no_apply_required" {
        "noop_no_live_apply_required".to_string()
    } else if !options.request_live_apply {
        "dry_run_live_apply_not_requested".to_string()
    } else if !options.ack_request_live_apply {
        "blocked_missing_live_apply_ack".to_string()
    } else if !apply_exec.apply_permitted {
        "blocked_apply_execution_not_permitted".to_string()
    } else if apply_exec.apply_performed {
        "blocked_apply_already_performed".to_string()
    } else if live_apply_request_permitted {
        "live_apply_request_recorded_for_future_writer".to_string()
    } else {
        "blocked_live_apply_request_not_permitted".to_string()
    };

    let request_reason = match request_action.as_str() {
        "noop_no_live_apply_required" => "verified path requires no live registry or index mutation".to_string(),
        "dry_run_live_apply_not_requested" => "live apply request was not requested; this receipt records dry-run state only".to_string(),
        "blocked_missing_live_apply_ack" => "live apply request requires --ack-request-live-apply".to_string(),
        "blocked_apply_execution_not_permitted" => "phase 35 apply execution receipt did not permit a future live writer".to_string(),
        "blocked_apply_already_performed" => "apply execution receipt indicates apply was already performed".to_string(),
        "live_apply_request_recorded_for_future_writer" => "operator intent is present and acknowledged; a future writer may require this request_id before mutation".to_string(),
        _ => "live apply request is not permitted".to_string(),
    };

    let mut findings = apply_exec.findings.clone();
    match request_action.as_str() {
        "noop_no_live_apply_required" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPLR001".to_string(),
            message: "live apply request path is a no-op because no live mutation is required".to_string(),
            path: apply_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "dry_run_live_apply_not_requested" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPLR002".to_string(),
            message: "live apply request receipt emitted without --request-live-apply".to_string(),
            path: apply_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "blocked_missing_live_apply_ack" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPLR003".to_string(),
            message: "live apply request blocked because --ack-request-live-apply was missing".to_string(),
            path: apply_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "blocked_apply_execution_not_permitted" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPLR004".to_string(),
            message: "live apply request blocked because phase 35 apply execution was not permitted".to_string(),
            path: apply_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "live_apply_request_recorded_for_future_writer" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPLR005".to_string(),
            message: "live apply request recorded for a future guarded writer; phase 36 does not mutate live paths".to_string(),
            path: apply_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        _ => findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPLR006".to_string(),
            message: "live apply request is not permitted".to_string(),
            path: apply_exec.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
    }

    CodeKleptoIndexRegistryRefreshLiveApplyRequestReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_live_apply_request_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_apply_exec_schema: apply_exec.schema.clone(),
        live_apply_request: CodeKleptoIndexRegistryRefreshLiveApplyRequestStatus {
            model: "klepto_index_registry_refresh_live_apply_request".to_string(),
            source_model: "phase35_apply_execution_receipt".to_string(),
            request_model: "auditable_future_live_writer_intent".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            requires_apply_execution_receipt: true,
            requires_apply_permitted: apply_exec.registry_apply_required || apply_exec.index_apply_required,
            requires_live_apply_ack: apply_exec.registry_apply_required || apply_exec.index_apply_required,
            notes: vec![
                "live apply requests consume phase 35 dry-run apply execution receipts".to_string(),
                "this phase records operator intent for a future live writer but never performs mutation".to_string(),
                "future writers can require live_apply_request_id, apply_plan_id, swap_plan_id, and execution_token".to_string(),
                "registry and index mutation remain disabled in phase 36".to_string(),
            ],
        },
        selected_stage_id: apply_exec.selected_stage_id.clone(),
        selected_stage_path: apply_exec.selected_stage_path.clone(),
        execution_token: apply_exec.execution_token.clone(),
        swap_plan_id: apply_exec.swap_plan_id.clone(),
        apply_plan_id: apply_exec.apply_plan_id.clone(),
        live_apply_request_id,
        request_live_apply: options.request_live_apply,
        ack_request_live_apply: options.ack_request_live_apply,
        live_apply_ack_satisfied,
        apply_permitted: apply_exec.apply_permitted,
        apply_performed: apply_exec.apply_performed,
        request_action,
        request_reason,
        registry_apply_required: apply_exec.registry_apply_required,
        index_apply_required: apply_exec.index_apply_required,
        required_source_artifacts: apply_exec.required_source_artifacts.clone(),
        required_live_mutations: apply_exec.required_live_mutations.clone(),
        live_apply_request_permitted,
        live_apply_request_recorded: live_apply_request_permitted,
        live_apply_performed: false,
        dry_run: true,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        apply_exec,
        findings,
    }
}


pub(crate) fn build_klepto_index_registry_refresh_live_apply_preflight_receipt(
    staging_dir: &Path,
    live_request: CodeKleptoIndexRegistryRefreshLiveApplyRequestReceipt,
    options: CodeKleptoIndexRegistryRefreshLiveApplyPreflightOptions,
) -> CodeKleptoIndexRegistryRefreshLiveApplyPreflightReceipt {
    let live_apply_preflight_ack_satisfied = !options.preflight_live_apply || options.ack_preflight_live_apply;
    let live_apply_request_id_matches = options
        .expected_live_apply_request_id
        .as_ref()
        .map(|expected| expected == &live_request.live_apply_request_id)
        .unwrap_or(true);
    let live_apply_preflight_permitted = options.preflight_live_apply
        && options.ack_preflight_live_apply
        && live_request.live_apply_request_recorded
        && !live_request.live_apply_performed
        && live_apply_request_id_matches
        && (live_request.registry_apply_required || live_request.index_apply_required);

    let preflight_action = if !live_request.registry_apply_required && !live_request.index_apply_required && live_request.request_action == "noop_no_live_apply_required" {
        "noop_no_live_apply_required".to_string()
    } else if !options.preflight_live_apply {
        "dry_run_live_apply_preflight_not_requested".to_string()
    } else if !options.ack_preflight_live_apply {
        "blocked_missing_live_apply_preflight_ack".to_string()
    } else if !live_request.live_apply_request_recorded {
        "blocked_live_apply_request_not_recorded".to_string()
    } else if live_request.live_apply_performed {
        "blocked_live_apply_already_performed".to_string()
    } else if !live_apply_request_id_matches {
        "blocked_live_apply_request_id_mismatch".to_string()
    } else if live_apply_preflight_permitted {
        "live_apply_preflight_ready_for_future_writer".to_string()
    } else {
        "blocked_live_apply_preflight_not_permitted".to_string()
    };

    let preflight_reason = match preflight_action.as_str() {
        "noop_no_live_apply_required" => "verified path requires no live registry or index mutation".to_string(),
        "dry_run_live_apply_preflight_not_requested" => "live apply preflight was not requested; this receipt records dry-run state only".to_string(),
        "blocked_missing_live_apply_preflight_ack" => "live apply preflight requires --ack-preflight-live-apply".to_string(),
        "blocked_live_apply_request_not_recorded" => "phase 36 live apply request was not recorded".to_string(),
        "blocked_live_apply_already_performed" => "phase 36 receipt indicates live apply was already performed".to_string(),
        "blocked_live_apply_request_id_mismatch" => "expected live_apply_request_id did not match the recorded request".to_string(),
        "live_apply_preflight_ready_for_future_writer" => "live apply request is recorded, acknowledged, request-id bound, and ready for a future real writer".to_string(),
        _ => "live apply preflight is not permitted".to_string(),
    };

    let mut findings = live_request.findings.clone();
    match preflight_action.as_str() {
        "noop_no_live_apply_required" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLP001".to_string(), message: "live apply preflight is a no-op because no live mutation is required".to_string(), path: live_request.selected_stage_path.clone(), line: None, column: None }),
        "dry_run_live_apply_preflight_not_requested" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLP002".to_string(), message: "live apply preflight receipt emitted without --preflight-live-apply".to_string(), path: live_request.selected_stage_path.clone(), line: None, column: None }),
        "blocked_missing_live_apply_preflight_ack" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLP003".to_string(), message: "live apply preflight blocked because --ack-preflight-live-apply was missing".to_string(), path: live_request.selected_stage_path.clone(), line: None, column: None }),
        "blocked_live_apply_request_not_recorded" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLP004".to_string(), message: "live apply preflight blocked because the phase 36 live request was not recorded".to_string(), path: live_request.selected_stage_path.clone(), line: None, column: None }),
        "blocked_live_apply_request_id_mismatch" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLP005".to_string(), message: "live apply preflight blocked because the expected live request id did not match".to_string(), path: live_request.selected_stage_path.clone(), line: None, column: None }),
        "live_apply_preflight_ready_for_future_writer" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLP006".to_string(), message: "live apply preflight is ready for a future guarded writer; phase 37 does not mutate live paths".to_string(), path: live_request.selected_stage_path.clone(), line: None, column: None }),
        _ => findings.push(FindingNode { severity: "warning".to_string(), code: "CPLP007".to_string(), message: "live apply preflight is not permitted".to_string(), path: live_request.selected_stage_path.clone(), line: None, column: None }),
    }

    CodeKleptoIndexRegistryRefreshLiveApplyPreflightReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_live_apply_preflight_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_live_apply_request_schema: live_request.schema.clone(),
        live_apply_preflight: CodeKleptoIndexRegistryRefreshLiveApplyPreflightStatus {
            model: "klepto_index_registry_refresh_live_apply_preflight".to_string(),
            source_model: "phase36_live_apply_request_receipt".to_string(),
            preflight_model: "final_non_mutating_gate_before_future_live_writer".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            requires_live_apply_request_receipt: true,
            requires_live_apply_request_recorded: live_request.registry_apply_required || live_request.index_apply_required,
            requires_live_apply_request_id_match: options.expected_live_apply_request_id.is_some(),
            requires_preflight_ack: live_request.registry_apply_required || live_request.index_apply_required,
            notes: vec![
                "live apply preflight consumes a phase 36 live-apply request receipt".to_string(),
                "preflight binds the live request id before a future real writer can touch live paths".to_string(),
                "this phase does not write staging files, registry files, or Tantivy indexes".to_string(),
                "future writers can require this preflight plus live_apply_request_id, apply_plan_id, swap_plan_id, and execution_token".to_string(),
            ],
        },
        selected_stage_id: live_request.selected_stage_id.clone(),
        selected_stage_path: live_request.selected_stage_path.clone(),
        execution_token: live_request.execution_token.clone(),
        swap_plan_id: live_request.swap_plan_id.clone(),
        apply_plan_id: live_request.apply_plan_id.clone(),
        live_apply_request_id: live_request.live_apply_request_id.clone(),
        expected_live_apply_request_id: options.expected_live_apply_request_id.clone(),
        live_apply_request_id_matches,
        preflight_live_apply: options.preflight_live_apply,
        ack_preflight_live_apply: options.ack_preflight_live_apply,
        live_apply_preflight_ack_satisfied,
        live_apply_request_recorded: live_request.live_apply_request_recorded,
        live_apply_performed: live_request.live_apply_performed,
        preflight_action,
        preflight_reason,
        registry_apply_required: live_request.registry_apply_required,
        index_apply_required: live_request.index_apply_required,
        required_source_artifacts: live_request.required_source_artifacts.clone(),
        required_live_mutations: live_request.required_live_mutations.clone(),
        live_apply_preflight_permitted,
        live_apply_preflight_ready: live_apply_preflight_permitted,
        dry_run: true,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        live_apply_request: live_request,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_refresh_live_apply_commit_plan_receipt(
    staging_dir: &Path,
    preflight: CodeKleptoIndexRegistryRefreshLiveApplyPreflightReceipt,
    options: CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanOptions,
) -> CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanReceipt {
    let live_commit_plan_ack_satisfied = !options.plan_live_commit || options.ack_plan_live_commit;
    let live_apply_request_id_matches = options
        .expected_live_apply_request_id
        .as_ref()
        .map(|expected| expected == &preflight.live_apply_request_id)
        .unwrap_or(true);
    let live_commit_plan_permitted = options.plan_live_commit
        && options.ack_plan_live_commit
        && preflight.live_apply_preflight_ready
        && preflight.live_apply_request_recorded
        && !preflight.live_apply_performed
        && live_apply_request_id_matches
        && (preflight.registry_apply_required || preflight.index_apply_required);

    let live_commit_action = if !preflight.registry_apply_required && !preflight.index_apply_required && preflight.preflight_action == "noop_no_live_apply_required" {
        "noop_no_live_commit_required".to_string()
    } else if !options.plan_live_commit {
        "dry_run_live_commit_plan_not_requested".to_string()
    } else if !options.ack_plan_live_commit {
        "blocked_missing_live_commit_plan_ack".to_string()
    } else if !preflight.live_apply_preflight_ready {
        "blocked_live_apply_preflight_not_ready".to_string()
    } else if !live_apply_request_id_matches {
        "blocked_live_apply_request_id_mismatch".to_string()
    } else if live_commit_plan_permitted {
        "live_apply_commit_plan_ready_for_future_writer".to_string()
    } else {
        "blocked_live_commit_plan_not_permitted".to_string()
    };

    let live_commit_reason = match live_commit_action.as_str() {
        "noop_no_live_commit_required" => "verified path requires no live registry or index commit".to_string(),
        "dry_run_live_commit_plan_not_requested" => "live commit planning was not requested; this receipt records dry-run state only".to_string(),
        "blocked_missing_live_commit_plan_ack" => "live commit planning requires --ack-plan-live-commit".to_string(),
        "blocked_live_apply_preflight_not_ready" => "phase 37 live apply preflight was not ready".to_string(),
        "blocked_live_apply_request_id_mismatch" => "expected live_apply_request_id did not match the preflight receipt".to_string(),
        "live_apply_commit_plan_ready_for_future_writer" => "live apply preflight is ready and a future writer can require this commit plan before live mutation".to_string(),
        _ => "live commit plan is not permitted".to_string(),
    };

    let selected_stage_path = preflight.selected_stage_path.clone();
    let stage_base = selected_stage_path.clone().unwrap_or_else(|| staging_dir.to_string_lossy().to_string());
    let live_commit_plan_id = stable_id(
        "kcmmt",
        &format!(
            "{}|{}|{}|{}|{}|{}|{}",
            staging_dir.to_string_lossy(),
            stage_base,
            preflight.execution_token.clone().unwrap_or_default(),
            preflight.swap_plan_id.clone(),
            preflight.apply_plan_id.clone(),
            preflight.live_apply_request_id.clone(),
            live_commit_action.clone()
        ),
    );

    let mut required_preflight_artifacts = vec![
        "staging_manifest.json".to_string(),
        "execution_receipt.json".to_string(),
        "live_apply_request_receipt".to_string(),
        "live_apply_preflight_receipt".to_string(),
    ];
    if preflight.registry_apply_required {
        required_preflight_artifacts.push("registry_candidate.json".to_string());
    }
    if preflight.index_apply_required {
        required_preflight_artifacts.push("index_refresh_marker.json".to_string());
    }

    let future_live_locks = vec![format!("{}/live_apply.lock", stage_base)];
    let mut future_backup_paths = Vec::<String>::new();
    if preflight.registry_apply_required {
        future_backup_paths.push(format!("{}/live_registry_backup.json", stage_base));
    }
    if preflight.index_apply_required {
        future_backup_paths.push(format!("{}/live_index_backup_marker.json", stage_base));
    }
    let future_commit_journal_path = Some(format!("{}/live_apply_commit_journal.json", stage_base));

    let mut findings = preflight.findings.clone();
    match live_commit_action.as_str() {
        "noop_no_live_commit_required" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLC001".to_string(), message: "live commit plan is a no-op because no live mutation is required".to_string(), path: preflight.selected_stage_path.clone(), line: None, column: None }),
        "dry_run_live_commit_plan_not_requested" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLC002".to_string(), message: "live commit plan receipt emitted without --plan-live-commit".to_string(), path: preflight.selected_stage_path.clone(), line: None, column: None }),
        "blocked_missing_live_commit_plan_ack" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLC003".to_string(), message: "live commit plan blocked because --ack-plan-live-commit was missing".to_string(), path: preflight.selected_stage_path.clone(), line: None, column: None }),
        "blocked_live_apply_preflight_not_ready" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLC004".to_string(), message: "live commit plan blocked because the phase 37 preflight was not ready".to_string(), path: preflight.selected_stage_path.clone(), line: None, column: None }),
        "blocked_live_apply_request_id_mismatch" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLC005".to_string(), message: "live commit plan blocked because the expected live request id did not match".to_string(), path: preflight.selected_stage_path.clone(), line: None, column: None }),
        "live_apply_commit_plan_ready_for_future_writer" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLC006".to_string(), message: "live commit plan is ready for a future guarded writer; phase 38 does not mutate live paths".to_string(), path: preflight.selected_stage_path.clone(), line: None, column: None }),
        _ => findings.push(FindingNode { severity: "warning".to_string(), code: "CPLC007".to_string(), message: "live commit plan is not permitted".to_string(), path: preflight.selected_stage_path.clone(), line: None, column: None }),
    }

    CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_live_apply_commit_plan_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_live_apply_preflight_schema: preflight.schema.clone(),
        live_apply_commit_plan: CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanStatus {
            model: "klepto_index_registry_refresh_live_apply_commit_plan".to_string(),
            source_model: "phase37_live_apply_preflight_receipt".to_string(),
            commit_plan_model: "non_mutating_future_writer_lock_backup_and_journal_plan".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            requires_live_apply_preflight_receipt: true,
            requires_live_apply_preflight_ready: preflight.registry_apply_required || preflight.index_apply_required,
            requires_live_apply_request_id_match: options.expected_live_apply_request_id.is_some(),
            requires_commit_plan_ack: preflight.registry_apply_required || preflight.index_apply_required,
            notes: vec![
                "live commit planning consumes a phase 37 live-apply preflight receipt".to_string(),
                "the plan enumerates future lock, backup, and commit-journal artifacts but writes none of them".to_string(),
                "future guarded writers can require live_commit_plan_id before mutating registry or index paths".to_string(),
                "this phase is still dry-run and never mutates live registry files or Tantivy indexes".to_string(),
            ],
        },
        selected_stage_id: preflight.selected_stage_id.clone(),
        selected_stage_path: preflight.selected_stage_path.clone(),
        execution_token: preflight.execution_token.clone(),
        swap_plan_id: preflight.swap_plan_id.clone(),
        apply_plan_id: preflight.apply_plan_id.clone(),
        live_apply_request_id: preflight.live_apply_request_id.clone(),
        expected_live_apply_request_id: options.expected_live_apply_request_id.clone(),
        live_apply_request_id_matches,
        live_commit_plan_id,
        plan_live_commit: options.plan_live_commit,
        ack_plan_live_commit: options.ack_plan_live_commit,
        live_commit_plan_ack_satisfied,
        live_apply_request_recorded: preflight.live_apply_request_recorded,
        live_apply_preflight_ready: preflight.live_apply_preflight_ready,
        live_apply_performed: preflight.live_apply_performed,
        live_commit_action,
        live_commit_reason,
        registry_apply_required: preflight.registry_apply_required,
        index_apply_required: preflight.index_apply_required,
        required_source_artifacts: preflight.required_source_artifacts.clone(),
        required_preflight_artifacts,
        required_live_mutations: preflight.required_live_mutations.clone(),
        future_live_locks,
        future_backup_paths,
        future_commit_journal_path,
        live_commit_plan_permitted,
        live_commit_plan_ready: live_commit_plan_permitted,
        live_commit_performed: false,
        dry_run: true,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        live_apply_preflight: preflight,
        findings,
    }
}

pub(crate) fn build_klepto_index_registry_refresh_live_apply_commit_exec_receipt(
    staging_dir: &Path,
    commit_plan: CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanReceipt,
    options: CodeKleptoIndexRegistryRefreshLiveApplyCommitExecOptions,
) -> CodeKleptoIndexRegistryRefreshLiveApplyCommitExecReceipt {
    let live_commit_execution_ack_satisfied = !options.execute_live_commit || options.ack_execute_live_commit;
    let live_commit_plan_id_matches = options
        .expected_live_commit_plan_id
        .as_ref()
        .map(|expected| expected == &commit_plan.live_commit_plan_id)
        .unwrap_or(true);
    let requires_live_commit = commit_plan.registry_apply_required || commit_plan.index_apply_required;
    let live_commit_execution_permitted = options.execute_live_commit
        && options.ack_execute_live_commit
        && commit_plan.live_commit_plan_ready
        && !commit_plan.live_commit_performed
        && live_commit_plan_id_matches
        && requires_live_commit;

    let live_commit_action = if !requires_live_commit && commit_plan.live_commit_action == "noop_no_live_commit_required" {
        "noop_no_live_commit_required".to_string()
    } else if !options.execute_live_commit {
        "dry_run_live_commit_not_requested".to_string()
    } else if !options.ack_execute_live_commit {
        "blocked_missing_live_commit_execution_ack".to_string()
    } else if !commit_plan.live_commit_plan_ready {
        "blocked_live_commit_plan_not_ready".to_string()
    } else if !live_commit_plan_id_matches {
        "blocked_live_commit_plan_id_mismatch".to_string()
    } else if live_commit_execution_permitted {
        "would_live_commit_in_future_writer".to_string()
    } else {
        "blocked_live_commit_not_permitted".to_string()
    };

    let live_commit_reason = match live_commit_action.as_str() {
        "noop_no_live_commit_required" => "verified path requires no live registry or index commit".to_string(),
        "dry_run_live_commit_not_requested" => "live commit execution was not requested; this receipt records dry-run state only".to_string(),
        "blocked_missing_live_commit_execution_ack" => "live commit execution requires --ack-execute-live-commit".to_string(),
        "blocked_live_commit_plan_not_ready" => "phase 38 live commit plan was not ready".to_string(),
        "blocked_live_commit_plan_id_mismatch" => "expected live_commit_plan_id did not match the commit plan receipt".to_string(),
        "would_live_commit_in_future_writer" => "live commit execution is permitted for a future writer, but phase 39 remains non-mutating".to_string(),
        _ => "live commit execution is not permitted".to_string(),
    };

    let live_commit_execution_id = stable_id(
        "kcmit",
        &format!(
            "{}|{}|{}|{}|{}|{}",
            staging_dir.to_string_lossy(),
            commit_plan.live_commit_plan_id.clone(),
            commit_plan.live_apply_request_id.clone(),
            commit_plan.apply_plan_id.clone(),
            commit_plan.swap_plan_id.clone(),
            live_commit_action.clone()
        ),
    );

    let mut findings = commit_plan.findings.clone();
    match live_commit_action.as_str() {
        "noop_no_live_commit_required" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLC101".to_string(), message: "live commit execution is a no-op because no live mutation is required".to_string(), path: commit_plan.selected_stage_path.clone(), line: None, column: None }),
        "dry_run_live_commit_not_requested" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLC102".to_string(), message: "live commit execution receipt emitted without --execute-live-commit".to_string(), path: commit_plan.selected_stage_path.clone(), line: None, column: None }),
        "blocked_missing_live_commit_execution_ack" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLC103".to_string(), message: "live commit execution blocked because --ack-execute-live-commit was missing".to_string(), path: commit_plan.selected_stage_path.clone(), line: None, column: None }),
        "blocked_live_commit_plan_not_ready" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLC104".to_string(), message: "live commit execution blocked because the phase 38 commit plan was not ready".to_string(), path: commit_plan.selected_stage_path.clone(), line: None, column: None }),
        "blocked_live_commit_plan_id_mismatch" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLC105".to_string(), message: "live commit execution blocked because the expected commit plan id did not match".to_string(), path: commit_plan.selected_stage_path.clone(), line: None, column: None }),
        "would_live_commit_in_future_writer" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLC106".to_string(), message: "live commit execution would be permitted for a future guarded writer; phase 39 does not mutate live paths".to_string(), path: commit_plan.selected_stage_path.clone(), line: None, column: None }),
        _ => findings.push(FindingNode { severity: "warning".to_string(), code: "CPLC107".to_string(), message: "live commit execution is not permitted".to_string(), path: commit_plan.selected_stage_path.clone(), line: None, column: None }),
    }

    CodeKleptoIndexRegistryRefreshLiveApplyCommitExecReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_live_apply_commit_exec_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_live_apply_commit_plan_schema: commit_plan.schema.clone(),
        live_apply_commit_exec: CodeKleptoIndexRegistryRefreshLiveApplyCommitExecStatus {
            model: "klepto_index_registry_refresh_live_apply_commit_exec".to_string(),
            source_model: "phase38_live_apply_commit_plan_receipt".to_string(),
            execution_model: "dry_run_future_live_writer_commit_execution".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            requires_live_commit_plan_receipt: true,
            requires_live_commit_plan_ready: requires_live_commit,
            requires_live_commit_plan_id_match: options.expected_live_commit_plan_id.is_some(),
            requires_execute_ack: requires_live_commit,
            notes: vec![
                "live commit execution consumes a phase 38 live commit plan receipt".to_string(),
                "the receipt records whether a future writer would be allowed to create locks, backups, journals, and mutate live paths".to_string(),
                "phase 39 remains dry-run and never creates lock files, backup files, journals, registry writes, or index swaps".to_string(),
                "future guarded writers can require live_commit_execution_id together with live_commit_plan_id before mutation".to_string(),
            ],
        },
        selected_stage_id: commit_plan.selected_stage_id.clone(),
        selected_stage_path: commit_plan.selected_stage_path.clone(),
        execution_token: commit_plan.execution_token.clone(),
        swap_plan_id: commit_plan.swap_plan_id.clone(),
        apply_plan_id: commit_plan.apply_plan_id.clone(),
        live_apply_request_id: commit_plan.live_apply_request_id.clone(),
        live_commit_plan_id: commit_plan.live_commit_plan_id.clone(),
        expected_live_commit_plan_id: options.expected_live_commit_plan_id.clone(),
        live_commit_plan_id_matches,
        live_commit_execution_id,
        execute_live_commit: options.execute_live_commit,
        ack_execute_live_commit: options.ack_execute_live_commit,
        live_commit_execution_ack_satisfied,
        live_apply_request_recorded: commit_plan.live_apply_request_recorded,
        live_apply_preflight_ready: commit_plan.live_apply_preflight_ready,
        live_commit_plan_ready: commit_plan.live_commit_plan_ready,
        live_commit_action,
        live_commit_reason,
        registry_apply_required: commit_plan.registry_apply_required,
        index_apply_required: commit_plan.index_apply_required,
        required_source_artifacts: commit_plan.required_source_artifacts.clone(),
        required_preflight_artifacts: commit_plan.required_preflight_artifacts.clone(),
        required_live_mutations: commit_plan.required_live_mutations.clone(),
        future_live_locks: commit_plan.future_live_locks.clone(),
        future_backup_paths: commit_plan.future_backup_paths.clone(),
        future_commit_journal_path: commit_plan.future_commit_journal_path.clone(),
        live_commit_execution_permitted,
        live_commit_performed: false,
        dry_run: true,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        live_apply_commit_plan: commit_plan,
        findings,
    }
}


pub(crate) fn build_klepto_index_registry_refresh_live_apply_commit_stage_receipt(
    staging_dir: &Path,
    commit_exec: CodeKleptoIndexRegistryRefreshLiveApplyCommitExecReceipt,
    options: CodeKleptoIndexRegistryRefreshLiveApplyCommitStageOptions,
) -> Result<CodeKleptoIndexRegistryRefreshLiveApplyCommitStageReceipt> {
    let live_commit_stage_ack_satisfied = !options.stage_live_commit || options.ack_stage_live_commit;
    let live_commit_execution_id_matches = options
        .expected_live_commit_execution_id
        .as_ref()
        .map(|expected| expected == &commit_exec.live_commit_execution_id)
        .unwrap_or(true);
    let requires_live_commit = commit_exec.registry_apply_required || commit_exec.index_apply_required;
    let live_commit_stage_permitted = options.stage_live_commit
        && options.ack_stage_live_commit
        && commit_exec.live_commit_execution_permitted
        && live_commit_execution_id_matches
        && requires_live_commit;

    let live_commit_stage_action = if !requires_live_commit && commit_exec.live_commit_action == "noop_no_live_commit_required" {
        "noop_no_live_commit_stage_required".to_string()
    } else if !options.stage_live_commit {
        "dry_run_live_commit_stage_not_requested".to_string()
    } else if !options.ack_stage_live_commit {
        "blocked_missing_live_commit_stage_ack".to_string()
    } else if !commit_exec.live_commit_execution_permitted {
        "blocked_live_commit_execution_not_permitted".to_string()
    } else if !live_commit_execution_id_matches {
        "blocked_live_commit_execution_id_mismatch".to_string()
    } else if live_commit_stage_permitted {
        "live_commit_staged_for_future_writer".to_string()
    } else {
        "blocked_live_commit_stage_not_permitted".to_string()
    };

    let live_commit_stage_reason = match live_commit_stage_action.as_str() {
        "noop_no_live_commit_stage_required" => "verified path requires no staged live commit journal".to_string(),
        "dry_run_live_commit_stage_not_requested" => "live commit staging was not requested; this receipt records dry-run state only".to_string(),
        "blocked_missing_live_commit_stage_ack" => "live commit staging requires --ack-stage-live-commit".to_string(),
        "blocked_live_commit_execution_not_permitted" => "phase 39 live commit execution receipt was not permitted".to_string(),
        "blocked_live_commit_execution_id_mismatch" => "expected live_commit_execution_id did not match the phase 39 receipt".to_string(),
        "live_commit_staged_for_future_writer" => "live commit journal artifacts were written only under the isolated staging directory".to_string(),
        _ => "live commit staging is not permitted".to_string(),
    };

    let mut staged_commit_files = Vec::<String>::new();
    if live_commit_stage_permitted {
        if let Some(stage_path) = commit_exec.selected_stage_path.as_ref() {
            let stage_path = PathBuf::from(stage_path);
            fs::create_dir_all(&stage_path)?;
            let exec_path = stage_path.join("live_commit_execution_receipt.json");
            let journal_path = commit_exec
                .future_commit_journal_path
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| stage_path.join("live_apply_commit_journal.json"));
            let manifest_path = stage_path.join("live_apply_commit_stage_manifest.json");

            fs::write(&exec_path, serde_json::to_string_pretty(&commit_exec)?)?;
            let journal = serde_json::json!({
                "schema": "code_passenger.klepto_index_registry_refresh_live_apply_commit_journal.v1",
                "live_commit_execution_id": commit_exec.live_commit_execution_id.clone(),
                "live_commit_plan_id": commit_exec.live_commit_plan_id.clone(),
                "live_apply_request_id": commit_exec.live_apply_request_id.clone(),
                "apply_plan_id": commit_exec.apply_plan_id.clone(),
                "swap_plan_id": commit_exec.swap_plan_id.clone(),
                "execution_token": commit_exec.execution_token.clone(),
                "registry_apply_required": commit_exec.registry_apply_required,
                "index_apply_required": commit_exec.index_apply_required,
                "required_live_mutations": commit_exec.required_live_mutations.clone(),
                "future_live_locks": commit_exec.future_live_locks.clone(),
                "future_backup_paths": commit_exec.future_backup_paths.clone(),
                "live_commit_performed": false,
                "registry_mutated": false,
                "index_mutated": false,
                "notes": [
                    "this journal is staged under the isolated staging directory only",
                    "phase 40 does not create live locks, live backups, registry writes, or index swaps"
                ]
            });
            fs::write(&journal_path, serde_json::to_string_pretty(&journal)?)?;

            staged_commit_files.push(exec_path.to_string_lossy().to_string());
            staged_commit_files.push(journal_path.to_string_lossy().to_string());

            let manifest = serde_json::json!({
                "schema": "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_manifest.v1",
                "stage_id": commit_exec.selected_stage_id.clone(),
                "live_commit_execution_id": journal["live_commit_execution_id"].clone(),
                "live_commit_plan_id": journal["live_commit_plan_id"].clone(),
                "live_apply_request_id": journal["live_apply_request_id"].clone(),
                "staged_commit_files": staged_commit_files.clone(),
                "staged_commit_file_count": staged_commit_files.len() + 1,
                "registry_mutated": false,
                "index_mutated": false,
                "live_commit_performed": false
            });
            fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
            staged_commit_files.push(manifest_path.to_string_lossy().to_string());
        }
    }

    let mut findings = commit_exec.findings.clone();
    match live_commit_stage_action.as_str() {
        "noop_no_live_commit_stage_required" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLS101".to_string(), message: "live commit staging is a no-op because no live commit is required".to_string(), path: commit_exec.selected_stage_path.clone(), line: None, column: None }),
        "dry_run_live_commit_stage_not_requested" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLS102".to_string(), message: "live commit stage receipt emitted without --stage-live-commit".to_string(), path: commit_exec.selected_stage_path.clone(), line: None, column: None }),
        "blocked_missing_live_commit_stage_ack" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLS103".to_string(), message: "live commit staging blocked because --ack-stage-live-commit was missing".to_string(), path: commit_exec.selected_stage_path.clone(), line: None, column: None }),
        "blocked_live_commit_execution_not_permitted" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLS104".to_string(), message: "live commit staging blocked because phase 39 execution was not permitted".to_string(), path: commit_exec.selected_stage_path.clone(), line: None, column: None }),
        "blocked_live_commit_execution_id_mismatch" => findings.push(FindingNode { severity: "error".to_string(), code: "CPLS105".to_string(), message: "live commit staging blocked because expected live commit execution id did not match".to_string(), path: commit_exec.selected_stage_path.clone(), line: None, column: None }),
        "live_commit_staged_for_future_writer" => findings.push(FindingNode { severity: "info".to_string(), code: "CPLS106".to_string(), message: "live commit journal artifacts were written under staging only; live registry and index paths were not mutated".to_string(), path: commit_exec.selected_stage_path.clone(), line: None, column: None }),
        _ => findings.push(FindingNode { severity: "warning".to_string(), code: "CPLS107".to_string(), message: "live commit staging is not permitted".to_string(), path: commit_exec.selected_stage_path.clone(), line: None, column: None }),
    }

    Ok(CodeKleptoIndexRegistryRefreshLiveApplyCommitStageReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_live_apply_commit_exec_schema: commit_exec.schema.clone(),
        live_apply_commit_stage: CodeKleptoIndexRegistryRefreshLiveApplyCommitStageStatus {
            model: "klepto_index_registry_refresh_live_apply_commit_stage".to_string(),
            source_model: "phase39_live_apply_commit_exec_receipt".to_string(),
            stage_model: "isolated_staging_commit_journal_writer".to_string(),
            mutates_staging: live_commit_stage_permitted,
            mutates_registry: false,
            mutates_index: false,
            requires_live_commit_exec_receipt: true,
            requires_live_commit_execution_permitted: requires_live_commit,
            requires_live_commit_execution_id_match: options.expected_live_commit_execution_id.is_some(),
            requires_stage_ack: requires_live_commit,
            notes: vec![
                "live commit staging consumes a phase 39 live commit execution receipt".to_string(),
                "phase 40 may write commit-intent artifacts only under the isolated staging directory".to_string(),
                "phase 40 never creates live locks, live backups, registry writes, or index swaps".to_string(),
                "a future guarded writer can require the staged commit journal plus live_commit_execution_id before mutation".to_string(),
            ],
        },
        selected_stage_id: commit_exec.selected_stage_id.clone(),
        selected_stage_path: commit_exec.selected_stage_path.clone(),
        execution_token: commit_exec.execution_token.clone(),
        swap_plan_id: commit_exec.swap_plan_id.clone(),
        apply_plan_id: commit_exec.apply_plan_id.clone(),
        live_apply_request_id: commit_exec.live_apply_request_id.clone(),
        live_commit_plan_id: commit_exec.live_commit_plan_id.clone(),
        live_commit_execution_id: commit_exec.live_commit_execution_id.clone(),
        expected_live_commit_execution_id: options.expected_live_commit_execution_id.clone(),
        live_commit_execution_id_matches,
        stage_live_commit: options.stage_live_commit,
        ack_stage_live_commit: options.ack_stage_live_commit,
        live_commit_stage_ack_satisfied,
        live_commit_execution_permitted: commit_exec.live_commit_execution_permitted,
        live_commit_stage_action,
        live_commit_stage_reason,
        registry_apply_required: commit_exec.registry_apply_required,
        index_apply_required: commit_exec.index_apply_required,
        required_source_artifacts: commit_exec.required_source_artifacts.clone(),
        required_preflight_artifacts: commit_exec.required_preflight_artifacts.clone(),
        required_live_mutations: commit_exec.required_live_mutations.clone(),
        future_live_locks: commit_exec.future_live_locks.clone(),
        future_backup_paths: commit_exec.future_backup_paths.clone(),
        future_commit_journal_path: commit_exec.future_commit_journal_path.clone(),
        staged_commit_file_count: staged_commit_files.len(),
        staged_commit_files,
        live_commit_stage_permitted,
        live_commit_stage_ready: live_commit_stage_permitted,
        live_commit_stage_performed: live_commit_stage_permitted,
        dry_run: !live_commit_stage_permitted,
        staging_write_allowed: live_commit_stage_permitted,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        live_commit_performed: false,
        live_apply_commit_exec: commit_exec,
        findings,
    })
}

pub(crate) fn build_klepto_index_registry_refresh_live_apply_commit_stage_verify_receipt(
    staging_dir: &Path,
    options: CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyOptions,
) -> Result<CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyReceipt> {
    let staging_dir_exists = staging_dir.exists();
    let mut findings = Vec::<FindingNode>::new();
    let mut stage_roots = Vec::<PathBuf>::new();

    if staging_dir.join("live_apply_commit_stage_manifest.json").exists() {
        stage_roots.push(staging_dir.to_path_buf());
    }
    if staging_dir.is_dir() {
        for entry in fs::read_dir(staging_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && path.join("live_apply_commit_stage_manifest.json").exists() {
                stage_roots.push(path);
            }
        }
    }
    stage_roots.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    stage_roots.dedup();

    let discovered_stage_count = stage_roots.len();
    let selected_stage_path_buf = stage_roots.last().cloned();

    let mut selected_stage_id = None;
    let mut selected_stage_path = None;
    let mut stage_manifest_path = None;
    let mut live_commit_exec_receipt_path = None;
    let mut live_commit_journal_path = None;
    let mut manifest_schema_ok = false;
    let mut commit_exec_receipt_schema_ok = false;
    let mut commit_journal_schema_ok = false;
    let mut manifest_live_commit_execution_id = None;
    let mut exec_live_commit_execution_id = None;
    let mut journal_live_commit_execution_id = None;
    let mut staged_commit_file_count_manifest = None;
    let mut staged_commit_file_count_observed = 0usize;
    let mut live_commit_stage_ready = false;

    if let Some(stage_root) = selected_stage_path_buf.as_ref() {
        selected_stage_id = stage_root.file_name().map(|name| name.to_string_lossy().to_string());
        selected_stage_path = Some(stage_root.to_string_lossy().to_string());

        let manifest_file = stage_root.join("live_apply_commit_stage_manifest.json");
        let exec_file = stage_root.join("live_commit_execution_receipt.json");
        let journal_file = stage_root.join("live_apply_commit_journal.json");

        stage_manifest_path = Some(manifest_file.to_string_lossy().to_string());
        live_commit_exec_receipt_path = Some(exec_file.to_string_lossy().to_string());
        live_commit_journal_path = Some(journal_file.to_string_lossy().to_string());

        if manifest_file.exists() {
            staged_commit_file_count_observed += 1;
            let manifest_value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&manifest_file)?)?;
            manifest_schema_ok = json_value_str(&manifest_value, "schema").as_deref() == Some("code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_manifest.v1");
            manifest_live_commit_execution_id = json_value_str(&manifest_value, "live_commit_execution_id");
            staged_commit_file_count_manifest = manifest_value.get("staged_commit_file_count").and_then(|value| value.as_u64()).map(|value| value as usize);
        }

        if exec_file.exists() {
            staged_commit_file_count_observed += 1;
            let exec_value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&exec_file)?)?;
            commit_exec_receipt_schema_ok = json_value_str(&exec_value, "schema").as_deref() == Some("code_passenger.klepto_index_registry_refresh_live_apply_commit_exec_receipt.v1");
            exec_live_commit_execution_id = json_value_str(&exec_value, "live_commit_execution_id");
            live_commit_stage_ready = exec_value.get("live_commit_execution_permitted").and_then(|value| value.as_bool()).unwrap_or(false);
        }

        if journal_file.exists() {
            staged_commit_file_count_observed += 1;
            let journal_value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&journal_file)?)?;
            commit_journal_schema_ok = json_value_str(&journal_value, "schema").as_deref() == Some("code_passenger.klepto_index_registry_refresh_live_apply_commit_journal.v1");
            journal_live_commit_execution_id = json_value_str(&journal_value, "live_commit_execution_id");
        }
    }

    let live_commit_execution_id = exec_live_commit_execution_id
        .clone()
        .or_else(|| journal_live_commit_execution_id.clone())
        .or_else(|| manifest_live_commit_execution_id.clone());
    let live_commit_execution_id_matches = manifest_live_commit_execution_id.is_some()
        && exec_live_commit_execution_id.is_some()
        && journal_live_commit_execution_id.is_some()
        && manifest_live_commit_execution_id == exec_live_commit_execution_id
        && exec_live_commit_execution_id == journal_live_commit_execution_id;
    let expected_live_commit_execution_id_matches = options
        .expected_live_commit_execution_id
        .as_ref()
        .map(|expected| live_commit_execution_id.as_ref() == Some(expected))
        .unwrap_or(true);
    let staged_commit_file_count_matches = staged_commit_file_count_manifest
        .map(|expected| expected == staged_commit_file_count_observed)
        .unwrap_or(false);

    let verified = discovered_stage_count > 0
        && manifest_schema_ok
        && commit_exec_receipt_schema_ok
        && commit_journal_schema_ok
        && live_commit_execution_id_matches
        && expected_live_commit_execution_id_matches
        && staged_commit_file_count_matches
        && live_commit_stage_ready;

    let verification_action = if discovered_stage_count == 0 {
        "missing_stage".to_string()
    } else if !expected_live_commit_execution_id_matches {
        "blocked_live_commit_execution_id_mismatch".to_string()
    } else if verified {
        "live_commit_stage_verified_for_future_writer".to_string()
    } else {
        "invalid_live_commit_stage_artifacts".to_string()
    };

    let verification_reason = match verification_action.as_str() {
        "missing_stage" => "no staged live commit manifest was found under the supplied staging directory".to_string(),
        "blocked_live_commit_execution_id_mismatch" => "the supplied expected live commit execution id does not match the staged commit execution id".to_string(),
        "live_commit_stage_verified_for_future_writer" => "staged live commit manifest, execution receipt, journal, execution id, and file counts verified without mutation".to_string(),
        _ => "one or more staged live commit artifacts failed schema, execution-id, readiness, or file-count verification".to_string(),
    };

    match verification_action.as_str() {
        "missing_stage" => findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPLV001".to_string(),
            message: "no staged live commit manifest was found".to_string(),
            path: Some(staging_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        }),
        "blocked_live_commit_execution_id_mismatch" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPLV003".to_string(),
            message: "expected live commit execution id does not match staged live commit execution id".to_string(),
            path: selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        "live_commit_stage_verified_for_future_writer" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPLV004".to_string(),
            message: "staged live commit journal artifacts verified for a future guarded writer".to_string(),
            path: selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        _ => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPLV002".to_string(),
            message: "staged live commit journal artifacts failed verification".to_string(),
            path: selected_stage_path.clone(),
            line: None,
            column: None,
        }),
    }

    Ok(CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_verify_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        staging_dir_exists,
        verification: CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyStatus {
            model: "klepto_index_registry_refresh_live_apply_commit_stage_verifier".to_string(),
            source_model: "phase40_isolated_live_commit_stage".to_string(),
            verification_model: "read_only_commit_manifest_execution_journal_check".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            notes: vec![
                "verifier reads staged live commit JSON artifacts only".to_string(),
                "verifier checks manifest schema, commit execution receipt schema, journal schema, live_commit_execution_id consistency, and staged file counts".to_string(),
                "verifier never mutates staging, registry, or Tantivy index paths".to_string(),
                "future live writer phases can require this receipt before consuming staged commit artifacts".to_string(),
            ],
        },
        expected_live_commit_execution_id: options.expected_live_commit_execution_id,
        discovered_stage_count,
        selected_stage_id,
        selected_stage_path,
        stage_manifest_path,
        live_commit_exec_receipt_path,
        live_commit_journal_path,
        manifest_schema_ok,
        commit_exec_receipt_schema_ok,
        commit_journal_schema_ok,
        live_commit_execution_id,
        manifest_live_commit_execution_id,
        exec_live_commit_execution_id,
        journal_live_commit_execution_id,
        live_commit_execution_id_matches,
        expected_live_commit_execution_id_matches,
        staged_commit_file_count_manifest,
        staged_commit_file_count_observed,
        staged_commit_file_count_matches,
        live_commit_stage_ready,
        verified,
        verification_action,
        verification_reason,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        live_commit_performed: false,
        findings,
    })
}

pub(crate) fn build_klepto_index_registry_refresh_live_apply_commit_gate_receipt(
    staging_dir: &Path,
    stage_verify: CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyReceipt,
    options: CodeKleptoIndexRegistryRefreshLiveApplyCommitGateOptions,
) -> Result<CodeKleptoIndexRegistryRefreshLiveApplyCommitGateReceipt> {
    let live_commit_gate_ack_satisfied = options.open_live_commit_gate && options.ack_open_live_commit_gate;
    let stage_verified = stage_verify.verified;

    let live_commit_gate_ready = stage_verified && live_commit_gate_ack_satisfied;
    let live_commit_gate_action = if !options.open_live_commit_gate {
        "dry_run_live_commit_gate_not_requested".to_string()
    } else if !stage_verified {
        "blocked_live_commit_stage_not_verified".to_string()
    } else if !options.ack_open_live_commit_gate {
        "blocked_missing_live_commit_gate_ack".to_string()
    } else {
        "live_commit_gate_ready_for_future_writer".to_string()
    };

    let live_commit_gate_reason = match live_commit_gate_action.as_str() {
        "dry_run_live_commit_gate_not_requested" => "live commit gate was not requested; staged artifacts were only verified/read".to_string(),
        "blocked_live_commit_stage_not_verified" => "live commit gate cannot open until Phase 41 verifies staged commit artifacts and execution id binding".to_string(),
        "blocked_missing_live_commit_gate_ack" => "live commit gate was requested without the explicit ack flag".to_string(),
        _ => "verified staged commit artifacts are gated for a future live writer without mutating live paths".to_string(),
    };

    let live_commit_gate_id = if live_commit_gate_ready {
        let raw = format!(
            "{}::{}::{}",
            stage_verify.live_commit_execution_id.clone().unwrap_or_default(),
            stage_verify.selected_stage_path.clone().unwrap_or_default(),
            stage_verify.staged_commit_file_count_observed
        );
        Some(stable_id("klcgt", &raw))
    } else {
        None
    };

    let mut findings = stage_verify.findings.clone();
    match live_commit_gate_action.as_str() {
        "dry_run_live_commit_gate_not_requested" => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPLG002".to_string(),
            message: "live commit gate was not requested; no live or staging paths were mutated".to_string(),
            path: Some(staging_dir.to_string_lossy().to_string()),
            line: None,
            column: None,
        }),
        "blocked_live_commit_stage_not_verified" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPLG101".to_string(),
            message: "live commit gate blocked because staged commit artifacts are not verified".to_string(),
            path: stage_verify.selected_stage_path.clone().or_else(|| Some(staging_dir.to_string_lossy().to_string())),
            line: None,
            column: None,
        }),
        "blocked_missing_live_commit_gate_ack" => findings.push(FindingNode {
            severity: "error".to_string(),
            code: "CPLG103".to_string(),
            message: "live commit gate requested without ack".to_string(),
            path: stage_verify.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
        _ => findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPLG106".to_string(),
            message: "live commit gate is ready for a future guarded writer; no live paths were mutated".to_string(),
            path: stage_verify.selected_stage_path.clone(),
            line: None,
            column: None,
        }),
    }

    Ok(CodeKleptoIndexRegistryRefreshLiveApplyCommitGateReceipt {
        schema: "code_passenger.klepto_index_registry_refresh_live_apply_commit_gate_receipt.v1".to_string(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        source_live_commit_stage_verify_schema: stage_verify.schema.clone(),
        gate: CodeKleptoIndexRegistryRefreshLiveApplyCommitGateStatus {
            model: "klepto_index_registry_refresh_live_apply_commit_gate".to_string(),
            source_model: "phase41_verified_live_commit_stage".to_string(),
            gate_model: "read_only_final_gate_before_live_writer".to_string(),
            mutates_staging: false,
            mutates_registry: false,
            mutates_index: false,
            requires_verified_commit_stage: true,
            requires_live_commit_execution_id_match: true,
            requires_gate_ack: true,
            notes: vec![
                "gate consumes a Phase 41 staged live commit verifier receipt".to_string(),
                "gate requires explicit open+ack flags before producing a future-writer gate id".to_string(),
                "gate records the exact artifacts and ids a future live writer must require".to_string(),
                "gate never mutates staging, registry, or Tantivy index paths".to_string(),
            ],
        },
        selected_stage_id: stage_verify.selected_stage_id.clone(),
        selected_stage_path: stage_verify.selected_stage_path.clone(),
        stage_manifest_path: stage_verify.stage_manifest_path.clone(),
        live_commit_exec_receipt_path: stage_verify.live_commit_exec_receipt_path.clone(),
        live_commit_journal_path: stage_verify.live_commit_journal_path.clone(),
        live_commit_execution_id: stage_verify.live_commit_execution_id.clone(),
        expected_live_commit_execution_id: options.expected_live_commit_execution_id,
        live_commit_execution_id_matches: stage_verify.live_commit_execution_id_matches,
        expected_live_commit_execution_id_matches: stage_verify.expected_live_commit_execution_id_matches,
        staged_commit_file_count_observed: stage_verify.staged_commit_file_count_observed,
        stage_verified,
        open_live_commit_gate: options.open_live_commit_gate,
        ack_open_live_commit_gate: options.ack_open_live_commit_gate,
        live_commit_gate_ack_satisfied,
        live_commit_gate_id,
        live_commit_gate_action,
        live_commit_gate_reason,
        required_verified_artifacts: vec![
            "live_apply_commit_stage_manifest.json".to_string(),
            "live_commit_execution_receipt.json".to_string(),
            "live_apply_commit_journal.json".to_string(),
        ],
        required_future_writer_inputs: vec![
            "live_commit_gate_id".to_string(),
            "live_commit_execution_id".to_string(),
            "live_apply_commit_stage_verify_receipt".to_string(),
            "live_apply_commit_journal".to_string(),
            "live_commit_execution_receipt".to_string(),
        ],
        live_commit_gate_ready,
        dry_run: true,
        write_allowed: false,
        registry_mutated: false,
        index_mutated: false,
        live_commit_performed: false,
        live_commit_stage_verify: stage_verify,
        findings,
    })
}
