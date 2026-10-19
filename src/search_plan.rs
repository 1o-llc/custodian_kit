//! Tantivy-ready multi-tenant search plans.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_code_search_plan_args(args: &[String], root: &str) -> (CodeSearchPlanOptions, Vec<String>) {
    let mut options = CodeSearchPlanOptions::default();
    options.workspace_id = safe_scope_component(root, "workspace");
    options.project_id = infer_project_id(root);
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--tenant" | "--tenant-id" => {
                if let Some(value) = args.get(idx + 1) {
                    options.tenant_id = safe_scope_component(value, "default");
                    idx += 2;
                    continue;
                }
            }
            "--namespace" => {
                if let Some(value) = args.get(idx + 1) {
                    options.namespace = safe_scope_component(value, "codepassenger");
                    idx += 2;
                    continue;
                }
            }
            "--workspace" | "--workspace-id" => {
                if let Some(value) = args.get(idx + 1) {
                    options.workspace_id = safe_scope_component(value, "workspace");
                    idx += 2;
                    continue;
                }
            }
            "--project" | "--project-id" => {
                if let Some(value) = args.get(idx + 1) {
                    options.project_id = safe_scope_component(value, "project");
                    idx += 2;
                    continue;
                }
            }
            "--visibility" => {
                if let Some(value) = args.get(idx + 1) {
                    options.visibility = normalize_visibility(value);
                    idx += 2;
                    continue;
                }
            }
            "--role" => {
                if let Some(value) = args.get(idx + 1) {
                    options.role = safe_scope_component(value, "developer");
                    idx += 2;
                    continue;
                }
            }
            "--extract-mode" | "--extraction-mode" => {
                if let Some(value) = args.get(idx + 1) {
                    options.extraction_mode = normalize_extraction_mode(value);
                    idx += 2;
                    continue;
                }
            }
            "--language" | "--lang" => {
                if let Some(value) = args.get(idx + 1) {
                    options.language = value.to_lowercase();
                    idx += 2;
                    continue;
                }
            }
            "--tokenizer" => {
                if let Some(value) = args.get(idx + 1) {
                    options.tokenizer = safe_scope_component(value, "code_default");
                    idx += 2;
                    continue;
                }
            }
            "--writer-memory" | "--writer-memory-bytes" => {
                if let Some(value) = args.get(idx + 1).and_then(|s| s.parse::<usize>().ok()) {
                    options.requested_writer_memory = Some(value);
                    idx += 2;
                    continue;
                }
            }
            "--writer-docs" | "--estimated-docs" => {
                if let Some(value) = args.get(idx + 1).and_then(|s| s.parse::<usize>().ok()) {
                    options.estimated_doc_count_override = Some(value.max(1));
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

pub(crate) fn build_search_index_plan_from_context(context: &CodeContextReport, options: CodeSearchPlanOptions) -> CodeSearchIndexPlan {
    let tenant_scope = CodeSearchTenantScope {
        tenant_id: options.tenant_id.clone(),
        namespace: options.namespace.clone(),
        workspace_id: options.workspace_id.clone(),
        project_id: options.project_id.clone(),
        visibility: options.visibility.clone(),
        extraction_mode: options.extraction_mode.clone(),
        role: options.role.clone(),
    };
    let schema_fields = search_schema_fields(&options.tokenizer);
    let analyzer = CodeSearchAnalyzerPlan {
        tokenizer: options.tokenizer.clone(),
        language: options.language.clone(),
        tantivy_language: map_tantivy_language_name(&options.language).map(|s| s.to_string()),
        filters: vec![
            "lowercase".to_string(),
            "ascii_folding".to_string(),
            "remove_long_tokens_40".to_string(),
            "code_path_boundaries".to_string(),
            "optional_language_stemmer".to_string(),
        ],
        query_parser_fields: vec![
            "content".to_string(),
            "label".to_string(),
            "path".to_string(),
            "crate_name".to_string(),
            "dep".to_string(),
            "scope".to_string(),
            "full_path".to_string(),
        ],
        source_influences: vec![
            "search/full_text/result_extraction multi-tenant extraction config".to_string(),
            "search/full_text/tantivy_query QueryContext/filter/scoring separation".to_string(),
            "search/full_text/schemas schema-field config pattern".to_string(),
            "provided writer-memory clamp helpers".to_string(),
        ],
    };
    let extraction_policy = extraction_policy_for_scope(&tenant_scope);
    let document_count = context.contexts.len();
    let writer_memory = estimate_search_writer_memory(
        &schema_fields,
        options.estimated_doc_count_override.unwrap_or(document_count.max(1)),
        options.requested_writer_memory,
    );
    let documents = context.contexts.iter().enumerate().map(|(idx, ctx)| {
        CodeSearchDocument {
            id: format!("sdoc_{:03}", idx + 1),
            tenant_id: tenant_scope.tenant_id.clone(),
            namespace: tenant_scope.namespace.clone(),
            workspace_id: tenant_scope.workspace_id.clone(),
            project_id: tenant_scope.project_id.clone(),
            visibility: tenant_scope.visibility.clone(),
            pin_id: ctx.pin_id.clone(),
            source_result_id: ctx.source_result_id.clone(),
            kind: ctx.kind.clone(),
            label: ctx.label.clone(),
            crate_name: ctx.crate_name.clone(),
            path: ctx.path.clone(),
            anchor_line: ctx.anchor_line,
            start_line: ctx.start_line,
            end_line: ctx.end_line,
            copy_hint: ctx.copy_hint.clone(),
            content: ctx.content.clone(),
            estimated_tokens: ctx.estimated_tokens,
            attributes: ctx.attributes.clone(),
        }
    }).collect::<Vec<_>>();

    CodeSearchIndexPlan {
        schema: "code_passenger.search_plan.v1".to_string(),
        graph_schema: context.graph_schema.clone(),
        root: context.root.clone(),
        root_exists: context.root_exists,
        max_depth: context.max_depth,
        source_query: context.source_query.clone(),
        context_options: context.context_options.clone(),
        tenant_scope,
        analyzer,
        schema_fields,
        extraction_policy,
        writer_memory,
        document_count,
        documents,
        summary: context.summary.clone(),
        findings: context.findings.clone(),
    }
}

pub(crate) fn extraction_policy_for_scope(scope: &CodeSearchTenantScope) -> CodeSearchExtractionPolicy {
    let searchable_fields = vec![
        "content".to_string(),
        "label".to_string(),
        "path".to_string(),
        "crate_name".to_string(),
        "dep".to_string(),
        "scope".to_string(),
        "full_path".to_string(),
    ];
    let mut extractable_fields = vec![
        "tenant_id".to_string(),
        "namespace".to_string(),
        "workspace_id".to_string(),
        "project_id".to_string(),
        "pin_id".to_string(),
        "kind".to_string(),
        "label".to_string(),
        "path".to_string(),
        "copy_hint".to_string(),
    ];
    let mut hidden_fields = Vec::<String>::new();
    if scope.extraction_mode == "full" || scope.role == "admin" || scope.role == "developer" {
        extractable_fields.push("content".to_string());
        extractable_fields.push("attributes".to_string());
    } else {
        hidden_fields.push("content".to_string());
        hidden_fields.push("attributes".to_string());
    }

    CodeSearchExtractionPolicy {
        mode: scope.extraction_mode.clone(),
        role: scope.role.clone(),
        searchable_fields,
        extractable_fields,
        hidden_fields,
        notes: vec![
            "tenant_scope must be applied as an AND filter before extraction".to_string(),
            "preview mode may index content but should not extract full content for restricted roles".to_string(),
            "documents are context windows, not raw repository blobs".to_string(),
        ],
    }
}

pub(crate) fn search_schema_fields(tokenizer: &str) -> Vec<CodeSearchSchemaField> {
    let exact = |name: &str, extraction_type: &str, tenant_scoped: bool| CodeSearchSchemaField {
        name: name.to_string(),
        field_type: "text_exact".to_string(),
        stored: true,
        indexed: true,
        fast: true,
        tokenizer: None,
        searchable: false,
        extraction_type: extraction_type.to_string(),
        tenant_scoped,
    };
    let text = |name: &str, searchable: bool, extraction_type: &str| CodeSearchSchemaField {
        name: name.to_string(),
        field_type: "text".to_string(),
        stored: true,
        indexed: true,
        fast: false,
        tokenizer: Some(tokenizer.to_string()),
        searchable,
        extraction_type: extraction_type.to_string(),
        tenant_scoped: false,
    };
    let u64_field = |name: &str| CodeSearchSchemaField {
        name: name.to_string(),
        field_type: "u64".to_string(),
        stored: true,
        indexed: true,
        fast: true,
        tokenizer: None,
        searchable: false,
        extraction_type: "numeric".to_string(),
        tenant_scoped: false,
    };

    vec![
        exact("tenant_id", "full_text", true),
        exact("namespace", "full_text", true),
        exact("workspace_id", "full_text", true),
        exact("project_id", "full_text", true),
        exact("visibility", "full_text", true),
        exact("role", "full_text", true),
        exact("pin_id", "full_text", false),
        exact("source_result_id", "full_text", false),
        exact("kind", "full_text", false),
        text("label", true, "snippet"),
        text("crate_name", true, "snippet"),
        text("path", true, "snippet"),
        u64_field("anchor_line"),
        u64_field("start_line"),
        u64_field("end_line"),
        u64_field("estimated_tokens"),
        text("copy_hint", false, "snippet"),
        text("content", true, "snippet"),
        text("dep", true, "snippet"),
        text("scope", true, "snippet"),
        text("full_path", true, "snippet"),
        text("feature_candidates", true, "snippet"),
    ]
}

pub(crate) fn estimate_search_writer_memory(fields: &[CodeSearchSchemaField], doc_count: usize, requested: Option<usize>) -> CodeSearchWriterMemoryPlan {
    const MIN_BYTES: usize = 50_000_000;
    const MAX_BYTES: usize = 1_000_000_000;
    let avg = estimate_search_avg_doc_size(fields);
    let estimated_payload_bytes = avg.saturating_mul(doc_count.max(1));
    let computed = estimated_payload_bytes.saturating_mul(2).clamp(MIN_BYTES, MAX_BYTES);
    let writer_memory_bytes = requested.unwrap_or(computed).clamp(MIN_BYTES, MAX_BYTES);
    let source = if requested.is_some() {
        "explicit_flag_clamped"
    } else {
        "schema_doc_estimate_x2_clamped"
    };
    CodeSearchWriterMemoryPlan {
        estimated_avg_doc_size: avg,
        estimated_document_count: doc_count.max(1),
        estimated_payload_bytes,
        requested_memory_bytes: requested,
        writer_memory_bytes,
        min_bytes: MIN_BYTES,
        max_bytes: MAX_BYTES,
        source: source.to_string(),
    }
}

pub(crate) fn estimate_search_avg_doc_size(fields: &[CodeSearchSchemaField]) -> usize {
    fields.iter().map(|field| match field.field_type.as_str() {
        "text" | "text_exact" => 256,
        "u64" | "i64" | "f64" => 8,
        "bool" => 1,
        _ => 32,
    }).sum()
}

pub(crate) fn map_tantivy_language_name(lang: &str) -> Option<&'static str> {
    match lang.to_lowercase().as_str() {
        "english" | "en" => Some("English"),
        "french" | "fr" => Some("French"),
        "german" | "de" => Some("German"),
        "italian" | "it" => Some("Italian"),
        "spanish" | "es" => Some("Spanish"),
        "dutch" | "nl" => Some("Dutch"),
        "portuguese" | "pt" => Some("Portuguese"),
        "russian" | "ru" => Some("Russian"),
        "swedish" | "sv" => Some("Swedish"),
        "danish" | "da" => Some("Danish"),
        "norwegian" | "no" => Some("Norwegian"),
        "romanian" | "ro" => Some("Romanian"),
        "hungarian" | "hu" => Some("Hungarian"),
        "finnish" | "fi" => Some("Finnish"),
        "turkish" | "tr" => Some("Turkish"),
        "arabic" | "ar" => Some("Arabic"),
        _ => None,
    }
}

pub(crate) fn normalize_visibility(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "public" => "public".to_string(),
        "internal" => "internal".to_string(),
        "private" => "private".to_string(),
        other => safe_scope_component(other, "private"),
    }
}

pub(crate) fn normalize_extraction_mode(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "full" => "full".to_string(),
        "preview" => "preview".to_string(),
        _ => "preview".to_string(),
    }
}

pub(crate) fn safe_scope_component(value: &str, fallback: &str) -> String {
    let mut out = value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' { ch.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    if out.is_empty() { fallback.to_string() } else { out }
}

pub(crate) fn infer_project_id(root: &str) -> String {
    let path = Path::new(root);
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| safe_scope_component(name, "project"))
        .unwrap_or_else(|| "project".to_string())
}
