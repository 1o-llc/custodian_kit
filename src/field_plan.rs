//! The klepto field plan: tenant-scoped search fields from query results, pins and context windows.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_klepto_field_plan_args(args: &[String]) -> (CodeKleptoFieldPlanOptions, Vec<String>) {
    let mut options = CodeKleptoFieldPlanOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--field" | "--klepto-field" | "--extract-field" => {
                if let Some(value) = args.get(idx + 1) {
                    for field in value.split(',').map(normalize_klepto_field_name).filter(|s| !s.is_empty()) {
                        if !options.requested_fields.contains(&field) {
                            options.requested_fields.push(field);
                        }
                    }
                    idx += 2;
                    continue;
                }
            }
            "--clear-fields" | "--fields-only" => {
                options.clear_default_fields = true;
                idx += 1;
                continue;
            }
            "--text-filter" | "--field-filter" | "--klepto-filter" => {
                if let Some(value) = args.get(idx + 1) {
                    for filter in value.split(',').map(normalize_klepto_filter_name).filter(|s| !s.is_empty()) {
                        if !options.filters.contains(&filter) {
                            options.filters.push(filter);
                        }
                    }
                    idx += 2;
                    continue;
                }
            }
            "--gate-feature" | "--cfg-feature" | "--target-feature" | "--feature-gate" => {
                if let Some(value) = args.get(idx + 1) {
                    options.target_feature = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--check" | "--klepto-check" => {
                if let Some(value) = args.get(idx + 1) {
                    options.checks.push(value.clone());
                    idx += 2;
                    continue;
                }
            }
            _ => {}
        }
        passthrough.push(args[idx].clone());
        idx += 1;
    }
    (options, passthrough)
}

pub(crate) fn build_klepto_field_plan_from_context(
    context: &CodeContextReport,
    search_options: CodeSearchPlanOptions,
    klepto_options: CodeKleptoFieldPlanOptions,
) -> CodeKleptoFieldPlan {
    let tenant_scope = CodeSearchTenantScope {
        tenant_id: search_options.tenant_id.clone(),
        namespace: search_options.namespace.clone(),
        workspace_id: search_options.workspace_id.clone(),
        project_id: search_options.project_id.clone(),
        visibility: search_options.visibility.clone(),
        extraction_mode: search_options.extraction_mode.clone(),
        role: search_options.role.clone(),
    };
    let analyzer = CodeSearchAnalyzerPlan {
        tokenizer: search_options.tokenizer.clone(),
        language: search_options.language.clone(),
        tantivy_language: map_tantivy_language_name(&search_options.language).map(|s| s.to_string()),
        filters: klepto_options.filters.clone(),
        query_parser_fields: effective_klepto_field_names(&klepto_options),
        source_influences: vec![
            "klepto cached functions/imports/paths/calls are modeled as grab sources".to_string(),
            "CodePassenger feature/use-site graph supplies dep and feature_candidates fields".to_string(),
            "search full_text analyzers/filters are represented as field-level plans".to_string(),
            "Step 10 context windows bound what can be extracted into search fields".to_string(),
        ],
    };
    let feature_binding = build_klepto_feature_binding(context, &klepto_options);
    let fields = build_klepto_field_specs(&klepto_options, &search_options.tokenizer);
    let schema_equivalent = klepto_fields_as_search_fields(&fields);
    let writer_memory = estimate_search_writer_memory(
        &schema_equivalent,
        search_options.estimated_doc_count_override.unwrap_or(context.contexts.len().max(1)),
        search_options.requested_writer_memory,
    );
    let recipe = CodeKleptoRecipePlan {
        name: "klepto_context_field_assembly".to_string(),
        purpose: "assemble searchable/indexable fields from prescribed graph/query/context facts before a future Tantivy write".to_string(),
        extraction_model: "query_results -> pins -> bounded_context_windows -> klepto_field_documents".to_string(),
        grab_sequence: vec![
            "grab tenant/workspace/project scope before extraction".to_string(),
            "grab pins from the selected query result set".to_string(),
            "grab bounded source snippets from context windows".to_string(),
            "grab dep/scope/full_path/feature_candidates from use-site attributes when present".to_string(),
            "grab requested custom fields and apply analyzer/filter hints".to_string(),
            "emit a field plan only; no Tantivy index is created here".to_string(),
        ],
        checks: klepto_recipe_checks(&feature_binding, &klepto_options),
        custom_fields_requested: klepto_options.requested_fields.clone(),
        source_influences: vec![
            "Klepto::functions/imports/find_paths/find_calls style cached extraction".to_string(),
            "Klepto Snapshot/FnFinger idea for stable field fingerprints".to_string(),
            "multi-tenant search result extraction must filter before returning fields".to_string(),
            "provided Tantivy analyzer/filter/memory helpers inform later index wiring".to_string(),
        ],
    };
    let documents = build_klepto_field_documents(context, &tenant_scope, &fields, &feature_binding, &recipe.checks);

    CodeKleptoFieldPlan {
        schema: "code_passenger.klepto_field_plan.v1".to_string(),
        graph_schema: context.graph_schema.clone(),
        root: context.root.clone(),
        root_exists: context.root_exists,
        max_depth: context.max_depth,
        source_query: context.source_query.clone(),
        context_options: context.context_options.clone(),
        tenant_scope,
        analyzer,
        feature_binding,
        recipe,
        writer_memory,
        field_count: fields.len(),
        fields,
        document_count: documents.len(),
        documents,
        summary: context.summary.clone(),
        findings: context.findings.clone(),
    }
}

pub(crate) fn effective_klepto_field_names(options: &CodeKleptoFieldPlanOptions) -> Vec<String> {
    let mut fields = if options.clear_default_fields { Vec::new() } else { default_klepto_fields() };
    for field in &options.requested_fields {
        if !fields.contains(field) {
            fields.push(field.clone());
        }
    }
    fields
}

pub(crate) fn build_klepto_field_specs(options: &CodeKleptoFieldPlanOptions, tokenizer: &str) -> Vec<CodeKleptoFieldSpec> {
    let requested = effective_klepto_field_names(options);
    requested.into_iter().map(|name| {
        let tenant_scoped = matches!(name.as_str(), "tenant_id" | "namespace" | "workspace_id" | "project_id" | "visibility" | "role");
        let numeric = matches!(name.as_str(), "anchor_line" | "start_line" | "end_line" | "estimated_tokens");
        let fast = tenant_scoped || numeric || matches!(name.as_str(), "kind" | "pin_id" | "source_result_id");
        let searchable = matches!(name.as_str(), "content" | "label" | "crate_name" | "path" | "dep" | "scope" | "full_path" | "feature_candidates" | "symbols");
        let feature_scoped = matches!(name.as_str(), "dep" | "scope" | "full_path" | "feature_candidates" | "content" | "symbols" | "feature_gate_target");
        CodeKleptoFieldSpec {
            source: klepto_field_source(&name).to_string(),
            field_type: if numeric { "u64" } else if fast { "text_exact" } else { "text" }.to_string(),
            analyzer: if searchable { Some(tokenizer.to_string()) } else { None },
            filters: if searchable { options.filters.clone() } else { Vec::new() },
            stored: true,
            indexed: true,
            fast,
            searchable,
            extractable: true,
            tenant_scoped,
            feature_scoped,
            customized_by_flag: options.requested_fields.contains(&name),
            name,
        }
    }).collect()
}

pub(crate) fn klepto_fields_as_search_fields(fields: &[CodeKleptoFieldSpec]) -> Vec<CodeSearchSchemaField> {
    fields.iter().map(|field| CodeSearchSchemaField {
        name: field.name.clone(),
        field_type: field.field_type.clone(),
        stored: field.stored,
        indexed: field.indexed,
        fast: field.fast,
        tokenizer: field.analyzer.clone(),
        searchable: field.searchable,
        extraction_type: field.source.clone(),
        tenant_scoped: field.tenant_scoped,
    }).collect()
}

pub(crate) fn default_klepto_fields() -> Vec<String> {
    [
        "tenant_id",
        "namespace",
        "workspace_id",
        "project_id",
        "visibility",
        "kind",
        "label",
        "crate_name",
        "path",
        "copy_hint",
        "dep",
        "scope",
        "full_path",
        "feature_candidates",
        "feature_gate_target",
        "content",
        "symbols",
    ].into_iter().map(str::to_string).collect()
}

pub(crate) fn klepto_field_source(name: &str) -> &'static str {
    match name {
        "tenant_id" | "namespace" | "workspace_id" | "project_id" | "visibility" | "role" => "tenant_scope",
        "pin_id" | "source_result_id" | "copy_hint" => "pin",
        "content" | "anchor_line" | "start_line" | "end_line" | "estimated_tokens" => "context_window",
        "dep" | "scope" | "full_path" | "feature_candidates" | "site_kind" | "head" => "klepto_use_site_attributes",
        "feature_gate_target" => "feature_binding",
        "symbols" => "klepto_symbol_grab",
        _ => "custom_field",
    }
}

pub(crate) fn build_klepto_feature_binding(context: &CodeContextReport, options: &CodeKleptoFieldPlanOptions) -> CodeKleptoFeatureBinding {
    let mut matched_deps = BTreeSet::<String>::new();
    let mut matched_features = BTreeSet::<String>::new();
    for ctx in &context.contexts {
        if let Some(dep) = ctx.attributes.get("dep").filter(|s| !s.is_empty() && s.as_str() != "-") {
            matched_deps.insert(dep.clone());
        }
        if let Some(raw) = ctx.attributes.get("feature_candidates") {
            for item in raw.split(',').map(str::trim).filter(|s| !s.is_empty() && *s != "-") {
                matched_features.insert(item.to_string());
            }
        }
    }
    let target_feature = options.target_feature.clone()
        .or_else(|| context.source_query.feature.clone())
        .or_else(|| matched_features.iter().next().cloned());
    let feature_gate_requested = target_feature.is_some();
    let feature_gate_plausible = feature_gate_requested && (!matched_deps.is_empty() || !matched_features.is_empty());
    let source = if options.target_feature.is_some() {
        "explicit_gate_feature_flag"
    } else if context.source_query.feature.is_some() {
        "source_query_feature_filter"
    } else if !matched_features.is_empty() {
        "context_feature_candidates"
    } else {
        "not_bound"
    };
    let mut notes = vec![
        "feature binding is advisory; no cfg or manifest writes are performed".to_string(),
        "deps and feature_candidates are grabbed from use-site/query attributes when available".to_string(),
    ];
    if feature_gate_requested && !feature_gate_plausible {
        notes.push("target feature was requested but no dependency or feature-candidate evidence was found in selected contexts".to_string());
    }

    CodeKleptoFeatureBinding {
        target_feature,
        source: source.to_string(),
        matched_deps: matched_deps.into_iter().collect(),
        matched_feature_candidates: matched_features.into_iter().collect(),
        feature_gate_requested,
        feature_gate_plausible,
        notes,
    }
}

pub(crate) fn klepto_recipe_checks(binding: &CodeKleptoFeatureBinding, options: &CodeKleptoFieldPlanOptions) -> Vec<String> {
    let mut checks = vec![
        "tenant_scope_filter_before_extraction".to_string(),
        "pin_range_exists_before_context_grab".to_string(),
        "context_window_byte_caps_respected".to_string(),
        "analyzer_filters_declared_per_searchable_field".to_string(),
    ];
    if binding.feature_gate_requested {
        checks.push("feature_binding_has_dep_or_feature_candidate_evidence".to_string());
    }
    checks.extend(options.checks.iter().cloned());
    checks
}

pub(crate) fn build_klepto_field_documents(
    context: &CodeContextReport,
    tenant_scope: &CodeSearchTenantScope,
    fields: &[CodeKleptoFieldSpec],
    feature_binding: &CodeKleptoFeatureBinding,
    recipe_checks: &[String],
) -> Vec<CodeKleptoFieldDocument> {
    context.contexts.iter().enumerate().map(|(idx, ctx)| {
        let mut values = BTreeMap::<String, String>::new();
        for field in fields {
            values.insert(
                field.name.clone(),
                klepto_field_value(&field.name, ctx, tenant_scope, feature_binding),
            );
        }
        CodeKleptoFieldDocument {
            id: format!("kdoc_{:03}", idx + 1),
            tenant_id: tenant_scope.tenant_id.clone(),
            namespace: tenant_scope.namespace.clone(),
            workspace_id: tenant_scope.workspace_id.clone(),
            project_id: tenant_scope.project_id.clone(),
            visibility: tenant_scope.visibility.clone(),
            role: tenant_scope.role.clone(),
            extraction_mode: tenant_scope.extraction_mode.clone(),
            pin_id: ctx.pin_id.clone(),
            source_result_id: ctx.source_result_id.clone(),
            kind: ctx.kind.clone(),
            label: ctx.label.clone(),
            crate_name: ctx.crate_name.clone(),
            path: ctx.path.clone(),
            copy_hint: ctx.copy_hint.clone(),
            anchor_line: ctx.anchor_line,
            fields: values,
            checks: recipe_checks.to_vec(),
        }
    }).collect()
}

pub(crate) fn klepto_field_value(
    name: &str,
    ctx: &CodeContextWindow,
    tenant_scope: &CodeSearchTenantScope,
    feature_binding: &CodeKleptoFeatureBinding,
) -> String {
    match name {
        "tenant_id" => tenant_scope.tenant_id.clone(),
        "namespace" => tenant_scope.namespace.clone(),
        "workspace_id" => tenant_scope.workspace_id.clone(),
        "project_id" => tenant_scope.project_id.clone(),
        "visibility" => tenant_scope.visibility.clone(),
        "role" => tenant_scope.role.clone(),
        "pin_id" => ctx.pin_id.clone(),
        "source_result_id" => ctx.source_result_id.clone(),
        "kind" => ctx.kind.clone(),
        "label" => ctx.label.clone(),
        "crate_name" => ctx.crate_name.clone().unwrap_or_else(|| "-".to_string()),
        "path" => ctx.path.clone(),
        "copy_hint" => ctx.copy_hint.clone(),
        "anchor_line" => ctx.anchor_line.to_string(),
        "start_line" => ctx.start_line.to_string(),
        "end_line" => ctx.end_line.to_string(),
        "estimated_tokens" => ctx.estimated_tokens.to_string(),
        "content" => ctx.content.clone(),
        "dep" | "scope" | "full_path" | "feature_candidates" | "site_kind" | "head" => ctx.attributes.get(name).cloned().unwrap_or_else(|| "-".to_string()),
        "feature_gate_target" => feature_binding.target_feature.clone().unwrap_or_else(|| "-".to_string()),
        "symbols" => extract_klepto_symbols(&ctx.content).join(","),
        other => ctx.attributes.get(other).cloned().unwrap_or_else(|| "-".to_string()),
    }
}

pub(crate) fn extract_klepto_symbols(content: &str) -> Vec<String> {
    let mut symbols = BTreeSet::<String>::new();
    let mut current = String::new();
    for ch in content.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' {
            current.push(ch);
        } else {
            push_symbol_if_interesting(&mut symbols, &mut current);
        }
    }
    push_symbol_if_interesting(&mut symbols, &mut current);
    symbols.into_iter().take(32).collect()
}

pub(crate) fn push_symbol_if_interesting(symbols: &mut BTreeSet<String>, current: &mut String) {
    if current.len() >= 3 && !current.chars().all(|c| c.is_ascii_digit()) {
        symbols.insert(current.clone());
    }
    current.clear();
}

pub(crate) fn normalize_klepto_field_name(value: &str) -> String {
    value.trim().to_lowercase().replace('-', "_")
}

pub(crate) fn normalize_klepto_filter_name(value: &str) -> String {
    safe_scope_component(&value.trim().to_lowercase().replace('-', "_"), "filter")
}
