//! The polymorphic index key, registry entry helpers and disk manifests; every digest through `crate::digest`.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn json_value_str(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|inner| inner.as_str()).map(|inner| inner.to_string())
}

pub(crate) fn select_klepto_registry_entry_for_key(
    entries: &[CodeKleptoIndexRegistryEntry],
    current: &CodeKleptoPolymorphicIndexKey,
) -> (Option<CodeKleptoIndexRegistryEntry>, String) {
    if let Some(entry) = entries.iter().rev().find(|entry| entry.key_id == current.key_id) {
        return (Some(entry.clone()), "exact_key_id".to_string());
    }
    if let Some(entry) = entries.iter().rev().find(|entry| klepto_entry_hashes_match_current(entry, current)) {
        return (Some(entry.clone()), "exact_component_hashes".to_string());
    }
    let best = entries
        .iter()
        .rev()
        .max_by_key(|entry| klepto_registry_similarity_score(entry, current))
        .cloned();
    let model = if best.is_some() { "closest_latest_baseline" } else { "no_registry_baseline" };
    (best, model.to_string())
}

pub(crate) fn klepto_registered_key_from_entry(entry: &CodeKleptoIndexRegistryEntry) -> CodeKleptoRegisteredIndexKey {
    CodeKleptoRegisteredIndexKey {
        key_id: Some(entry.key_id.clone()),
        key_version: Some(entry.key_version.clone()),
        schema_hash: Some(entry.schema_hash.clone()),
        field_contract_hash: Some(entry.field_contract_hash.clone()),
        analyzer_hash: Some(entry.analyzer_hash.clone()),
        tenant_scope_hash: Some(entry.tenant_scope_hash.clone()),
        feature_binding_hash: Some(entry.feature_binding_hash.clone()),
        document_shape_hash: Some(entry.document_shape_hash.clone()),
        source: "registry_file".to_string(),
    }
}

pub(crate) fn klepto_entry_hashes_match_current(entry: &CodeKleptoIndexRegistryEntry, current: &CodeKleptoPolymorphicIndexKey) -> bool {
    entry.schema_hash == current.schema_hash
        && entry.field_contract_hash == current.field_contract_hash
        && entry.analyzer_hash == current.analyzer_hash
        && entry.tenant_scope_hash == current.tenant_scope_hash
        && entry.feature_binding_hash == current.feature_binding_hash
        && entry.document_shape_hash == current.document_shape_hash
}

pub(crate) fn klepto_registry_similarity_score(entry: &CodeKleptoIndexRegistryEntry, current: &CodeKleptoPolymorphicIndexKey) -> usize {
    let mut score = 0usize;
    if entry.schema_hash == current.schema_hash { score += 1; }
    if entry.field_contract_hash == current.field_contract_hash { score += 1; }
    if entry.analyzer_hash == current.analyzer_hash { score += 1; }
    if entry.tenant_scope_hash == current.tenant_scope_hash { score += 1; }
    if entry.feature_binding_hash == current.feature_binding_hash { score += 1; }
    if entry.document_shape_hash == current.document_shape_hash { score += 1; }
    score
}

pub(crate) fn read_klepto_registry_entries(registry_path: &Path) -> Result<Vec<CodeKleptoIndexRegistryEntry>> {
    if !registry_path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(registry_path)?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(&text)?;
    if let Some(entries_value) = value.get("entries") {
        return Ok(serde_json::from_value(entries_value.clone())?);
    }
    if value.get("entry_id").is_some() {
        return Ok(vec![serde_json::from_value(value)?]);
    }
    Ok(Vec::new())
}

pub(crate) fn target_registry_version(
    action: &str,
    bump: &str,
    registered: &CodeKleptoRegisteredIndexKey,
    current: &CodeKleptoPolymorphicIndexKey,
) -> String {
    match action {
        "reuse_existing_version" => registered.key_version.clone().unwrap_or_else(|| current.key_version.clone()),
        "register_new_version" => bump_semver(registered.key_version.as_deref().unwrap_or(current.key_version.as_str()), bump),
        "defer" => "none".to_string(),
        _ => current.key_version.clone(),
    }
}

pub(crate) fn bump_semver(version: &str, bump: &str) -> String {
    let mut parts = version.split('.').map(|part| part.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    while parts.len() < 3 {
        parts.push(0);
    }
    match bump {
        "major" => {
            parts[0] += 1;
            parts[1] = 0;
            parts[2] = 0;
        }
        "minor" => {
            parts[1] += 1;
            parts[2] = 0;
        }
        "patch" => {
            parts[2] += 1;
        }
        _ => {}
    }
    format!("{}.{}.{}", parts[0], parts[1], parts[2])
}

pub(crate) fn collect_version_hash_delta(
    deltas: &mut Vec<CodeKleptoIndexVersionHashDelta>,
    component: &str,
    registered: &Option<String>,
    current: &str,
    bump_class: &str,
) {
    if let Some(registered_value) = registered {
        if registered_value != current {
            deltas.push(CodeKleptoIndexVersionHashDelta {
                component: component.to_string(),
                registered: registered_value.clone(),
                current: current.to_string(),
                bump_class: bump_class.to_string(),
            });
        }
    }
}

pub(crate) fn strongest_version_bump(deltas: &[CodeKleptoIndexVersionHashDelta]) -> String {
    if deltas.iter().any(|delta| delta.bump_class == "major") {
        "major".to_string()
    } else if deltas.iter().any(|delta| delta.bump_class == "minor") {
        "minor".to_string()
    } else if deltas.iter().any(|delta| delta.bump_class == "patch") {
        "patch".to_string()
    } else {
        "none".to_string()
    }
}

pub(crate) fn build_klepto_index_key_receipt(
    graph: &CodeGraph,
    plan: CodeKleptoFieldPlan,
    candidate: CodeTantivyIndexReceipt,
    requested_options: CodeTantivyIndexOptions,
) -> CodeKleptoIndexKeyReceipt {
    let field_bindings = build_klepto_index_field_bindings(&plan, &candidate);
    let stored_fields = stored_fields_from_indexed_documents(&candidate.indexed_documents);
    let query_fields = candidate.search_check.query_fields.clone();
    let candidate_search_hits = candidate.search_check.hit_count;

    let schema_hash = serialized_digest(&candidate.schema_fields);
    let field_contract_hash = serialized_digest(&field_bindings);
    let analyzer_hash = serialized_digest(&plan.analyzer);
    let tenant_scope_hash = serialized_digest(&plan.tenant_scope);
    let feature_binding_hash = serialized_digest(&plan.feature_binding);
    let document_shape_hash = serialized_digest(&stored_fields);
    let key_material = vec![
        format!("source_schema={}", plan.schema.as_str()),
        format!("implementation_schema={}", candidate.schema.as_str()),
        format!("schema_hash={schema_hash}"),
        format!("field_contract_hash={field_contract_hash}"),
        format!("analyzer_hash={analyzer_hash}"),
        format!("tenant_scope_hash={tenant_scope_hash}"),
        format!("feature_binding_hash={feature_binding_hash}"),
        format!("document_shape_hash={document_shape_hash}"),
    ];
    let combined_hash = serialized_digest(&key_material);
    let key_id = format!("kidx_{}", crate::digest::short(&combined_hash, 16));

    let mut findings = Vec::<FindingNode>::new();
    findings.extend(plan.findings.clone());
    findings.extend(candidate.findings.clone());
    if requested_options.write_index || requested_options.overwrite_index || requested_options.index_dir.is_some() {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPK001".to_string(),
            message: "klepto-index-key mode ignores index write flags and builds the candidate implementation schema in RAM only".to_string(),
            path: requested_options.index_dir.clone(),
            line: None,
            column: None,
        });
    }

    let version_bump_recommendation = if !graph.root_exists {
        "none".to_string()
    } else if plan.document_count == 0 {
        "patch".to_string()
    } else {
        "register_or_reuse".to_string()
    };
    let version_bump_reason = if !graph.root_exists {
        "missing root has no stable source schema to register".to_string()
    } else if plan.document_count == 0 {
        "schema binding exists but selected Klepto query produced no documents".to_string()
    } else {
        "stable key should be reused when hashes match and registered as a new index version when hashes differ".to_string()
    };

    CodeKleptoIndexKeyReceipt {
        schema: "code_passenger.klepto_index_key_receipt.v1".to_string(),
        graph_schema: graph.schema.clone(),
        root: graph.root.clone(),
        root_exists: graph.root_exists,
        max_depth: graph.max_depth,
        source_klepto_schema: plan.schema.clone(),
        candidate_index_schema: candidate.schema.clone(),
        binding: CodeKleptoIndexSchemaBindingStatus {
            model: "klepto_source_schema_to_polymorphic_index_key".to_string(),
            source_schema_role: "Klepto describes what can be grabbed, checked, tenant-scoped, feature-bound, and projected into index fields".to_string(),
            implementation_schema_role: "Tantivy is one concrete implementation schema for the same key; future engines can implement the same field contract".to_string(),
            polymorphic_index_key: true,
            mutates_index: false,
            notes: vec![
                "Klepto field specs are treated as the source contract, not as incidental index input".to_string(),
                "the key is derived from schema fields, field bindings, analyzer plan, tenant scope, feature binding, and document shape".to_string(),
                "versioning registries can compare key hashes before deciding reuse, patch, minor, or major bumps".to_string(),
                "ingestion receipts can later point at this key to prove which source schema produced searchable documents".to_string(),
            ],
        },
        key: CodeKleptoPolymorphicIndexKey {
            key_id,
            key_version: "0.1.0".to_string(),
            digest_algorithm: crate::digest::ALGORITHM.to_string(),
            schema_hash,
            field_contract_hash,
            analyzer_hash,
            tenant_scope_hash,
            feature_binding_hash,
            document_shape_hash,
            version_bump_recommendation,
            version_bump_reason,
            stable_key_parts: key_material,
        },
        tenant_scope: plan.tenant_scope,
        analyzer: plan.analyzer,
        feature_binding: plan.feature_binding,
        klepto_field_count: plan.field_count,
        klepto_document_count: plan.document_count,
        index_schema_field_count: candidate.schema_fields.len(),
        index_stored_field_count: stored_fields.len(),
        query_fields,
        field_bindings,
        write_requested: requested_options.write_index,
        overwrite_requested: requested_options.overwrite_index,
        write_allowed: false,
        index_mutated: false,
        candidate_committed: candidate.index.commit_succeeded,
        candidate_search_hits,
        findings,
    }
}

pub(crate) fn build_klepto_index_field_bindings(
    plan: &CodeKleptoFieldPlan,
    candidate: &CodeTantivyIndexReceipt,
) -> Vec<CodeKleptoIndexFieldBinding> {
    let plan_fields = plan
        .fields
        .iter()
        .map(|field| (field.name.clone(), field.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut bindings = Vec::<CodeKleptoIndexFieldBinding>::new();
    let mut bound_fields = BTreeSet::<String>::new();
    for schema_field in &candidate.schema_fields {
        let klepto_field = plan_fields.get(&schema_field.name);
        let (source, tenant_scoped, feature_scoped, extractable, customized_by_flag) = klepto_field
            .map(|field| {
                (
                    field.source.clone(),
                    field.tenant_scoped,
                    field.feature_scoped,
                    field.extractable,
                    field.customized_by_flag,
                )
            })
            .unwrap_or_else(|| {
                (
                    klepto_field_source(&schema_field.name).to_string(),
                    matches!(schema_field.name.as_str(), "tenant_id" | "namespace" | "workspace_id" | "project_id" | "visibility" | "role"),
                    matches!(schema_field.name.as_str(), "feature_candidates" | "feature_gate_target"),
                    false,
                    false,
                )
            });
        let compatibility = if klepto_field.is_some() {
            "klepto_declares_index_field"
        } else if is_index_system_field(&schema_field.name) {
            "index_system_field"
        } else {
            "candidate_field_without_klepto_spec"
        };
        let mut notes = Vec::<String>::new();
        if extractable {
            notes.push("field value is extractable from Klepto-selected context or attributes".to_string());
        }
        if customized_by_flag {
            notes.push("field was requested through --field customization".to_string());
        }
        if compatibility == "index_system_field" {
            notes.push("field is part of the index document envelope even when not requested as a Klepto custom field".to_string());
        }
        bound_fields.insert(schema_field.name.clone());
        bindings.push(CodeKleptoIndexFieldBinding {
            klepto_field: schema_field.name.clone(),
            source,
            field_type: schema_field.field_type.clone(),
            analyzer: schema_field.tokenizer.clone(),
            stored: schema_field.stored,
            indexed: schema_field.indexed,
            searchable: candidate.search_check.query_fields.contains(&schema_field.name),
            tenant_scoped,
            feature_scoped,
            tantivy_field: Some(schema_field.name.clone()),
            index_key_component: schema_field.stored || schema_field.indexed,
            compatibility: compatibility.to_string(),
            notes,
        });
    }

    for field in &plan.fields {
        if bound_fields.contains(&field.name) {
            continue;
        }
        let mut notes = Vec::<String>::new();
        if field.extractable {
            notes.push("field value is extractable from Klepto-selected context or attributes".to_string());
        }
        if field.customized_by_flag {
            notes.push("field was requested through --field customization".to_string());
        }
        notes.push("field is part of the Klepto source contract even when the current Tantivy implementation keeps it virtual".to_string());
        bindings.push(CodeKleptoIndexFieldBinding {
            klepto_field: field.name.clone(),
            source: field.source.clone(),
            field_type: field.field_type.clone(),
            analyzer: field.analyzer.clone(),
            stored: field.stored,
            indexed: field.indexed,
            searchable: field.searchable,
            tenant_scoped: field.tenant_scoped,
            feature_scoped: field.feature_scoped,
            tantivy_field: None,
            index_key_component: true,
            compatibility: "klepto_virtual_field_without_index_field".to_string(),
            notes,
        });
    }
    bindings.sort_by(|a, b| a.klepto_field.cmp(&b.klepto_field));
    bindings
}

pub(crate) fn is_index_system_field(name: &str) -> bool {
    matches!(
        name,
        "doc_id"
            | "tenant_id"
            | "namespace"
            | "workspace_id"
            | "project_id"
            | "visibility"
            | "role"
            | "extraction_mode"
            | "pin_id"
            | "source_result_id"
            | "anchor_line"
    )
}

/// A value's digest, `sha256:<hex>` of its canonical form (one rule: `crate::digest`).
pub(crate) fn serialized_digest<T: Serialize>(value: &T) -> String {
    crate::digest::of_value(value)
}

pub(crate) fn tenant_scopes_from_indexed_documents(documents: &[CodeTantivyIndexedDocumentReceipt]) -> Vec<CodeTantivyDiskTenantScopeStatus> {
    let mut tenant_counts = BTreeMap::<String, CodeTantivyDiskTenantScopeStatus>::new();
    for document in documents {
        let key = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            document.tenant_id, document.namespace, document.workspace_id, document.project_id, document.visibility
        );
        let entry = tenant_counts.entry(key).or_insert_with(|| CodeTantivyDiskTenantScopeStatus {
            tenant_id: document.tenant_id.clone(),
            namespace: document.namespace.clone(),
            workspace_id: document.workspace_id.clone(),
            project_id: document.project_id.clone(),
            visibility: document.visibility.clone(),
            document_count: 0,
            kind_counts: BTreeMap::new(),
        });
        entry.document_count += 1;
        *entry.kind_counts.entry(document.kind.clone()).or_insert(0) += 1;
    }
    tenant_counts.into_values().collect()
}

pub(crate) fn stored_fields_from_indexed_documents(documents: &[CodeTantivyIndexedDocumentReceipt]) -> Vec<String> {
    let mut fields = BTreeSet::<String>::new();
    for document in documents {
        fields.extend(document.indexed_fields.iter().cloned());
    }
    fields.into_iter().collect()
}

pub(crate) fn tenant_scope_map(scopes: &[CodeTantivyDiskTenantScopeStatus]) -> BTreeMap<String, CodeTantivyDiskTenantScopeStatus> {
    scopes
        .iter()
        .map(|scope| {
            (
                format!(
                    "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
                    scope.tenant_id, scope.namespace, scope.workspace_id, scope.project_id, scope.visibility
                ),
                scope.clone(),
            )
        })
        .collect()
}

pub(crate) fn collect_disk_manifest_files(index_dir: &Path) -> Result<(Vec<CodeTantivyDiskManifestFile>, u64, String)> {
    let mut paths = Vec::<PathBuf>::new();
    collect_disk_manifest_paths(index_dir, &mut paths)?;
    paths.sort_by(|a, b| {
        normalized_relative_path(index_dir, a).cmp(&normalized_relative_path(index_dir, b))
    });

    let mut files = Vec::<CodeTantivyDiskManifestFile>::new();
    let mut total_bytes = 0u64;
    for path in paths {
        let bytes = fs::read(&path)?;
        let relative_path = normalized_relative_path(index_dir, &path);
        let byte_count = bytes.len() as u64;
        let file_digest = crate::digest::tagged(&bytes);
        total_bytes = total_bytes.saturating_add(byte_count);
        files.push(CodeTantivyDiskManifestFile {
            relative_path,
            byte_count,
            digest: file_digest,
        });
    }
    // the manifest's canonical form: every file's path, size and digest, in path order
    let manifest_digest = crate::digest::of_value(&files.iter().map(|f| (&f.relative_path, f.byte_count, &f.digest)).collect::<Vec<_>>());
    Ok((files, total_bytes, manifest_digest))
}

pub(crate) fn collect_disk_manifest_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<std::result::Result<Vec<_>, std::io::Error>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::metadata(&path)?;
        if metadata.is_dir() {
            collect_disk_manifest_paths(&path, paths)?;
        } else if metadata.is_file() {
            paths.push(path);
        }
    }
    Ok(())
}

pub(crate) fn normalized_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}


