//! Tantivy queries over RAM and disk indexes, tenant filters, hydrated hits.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_code_search_disk_query_args(args: &[String]) -> CodeTantivyIndexOptions {
    let (mut options, _passthrough) = parse_code_search_index_args(args);
    options.write_index = false;
    options.overwrite_index = false;
    options.index_dir = None;
    options
}

#[allow(clippy::too_many_lines)]
pub(crate) fn build_tantivy_disk_query_receipt(index_dir: &Path, options: CodeTantivyIndexOptions) -> Result<CodeTantivyDiskQueryReceipt> {
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::schema::Field;
    use tantivy::tokenizer::{LowerCaser, RemoveLongFilter, SimpleTokenizer, TextAnalyzer};
    use tantivy::{Document, Index, TantivyDocument};

    let index_dir_text = index_dir.to_string_lossy().to_string();
    let query_text = options.search_query.clone().unwrap_or_else(|| "code".to_string());
    let top_k = options.top_k;
    if !index_dir.exists() {
        return Ok(CodeTantivyDiskQueryReceipt {
            schema: "code_passenger.tantivy_disk_query_receipt.v1".to_string(),
            index_dir: index_dir_text.clone(),
            index_exists: false,
            query: query_text,
            top_k,
            hit_count: 0,
            index: CodeTantivyQueryIndexStatus {
                engine: "tantivy".to_string(),
                tantivy_used: false,
                source_index_kind: "directory".to_string(),
                disk_write_allowed: false,
                disk_write_requested: false,
                disk_write_ignored: false,
                commit_succeeded: false,
                tenant_filter_model: "disk query can only enforce tenant filters after an existing index opens".to_string(),
                hydration_model: "no hydration performed because index dir does not exist".to_string(),
                notes: vec!["index dir does not exist; query receipt is intentionally empty".to_string()],
            },
            tenant_filter: build_tantivy_disk_query_filter(&options),
            query_fields: Vec::new(),
            hits: Vec::new(),
            findings: vec![FindingNode {
                severity: "warn".to_string(),
                code: "CPQ001".to_string(),
                message: "index dir does not exist; disk query is intentionally empty".to_string(),
                path: Some(index_dir_text),
                line: None,
                column: None,
            }],
        });
    }

    let index = Index::open_in_dir(index_dir)?;
    let analyzer = TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(40))
        .filter(LowerCaser)
        .build();
    index.tokenizers().register("code_default", analyzer);
    let schema = index.schema();
    let query_field_names = disk_query_field_names(&schema);
    let query_fields = query_field_names
        .iter()
        .filter_map(|name| schema.get_field(name).ok())
        .collect::<Vec<Field>>();

    let mut findings = Vec::<FindingNode>::new();
    if query_fields.is_empty() {
        findings.push(FindingNode {
            severity: "warn".to_string(),
            code: "CPQ002".to_string(),
            message: "opened Tantivy index has no supported query fields".to_string(),
            path: Some(index_dir_text.clone()),
            line: None,
            column: None,
        });
        return Ok(CodeTantivyDiskQueryReceipt {
            schema: "code_passenger.tantivy_disk_query_receipt.v1".to_string(),
            index_dir: index_dir_text,
            index_exists: true,
            query: query_text,
            top_k,
            hit_count: 0,
            index: CodeTantivyQueryIndexStatus {
                engine: "tantivy".to_string(),
                tantivy_used: true,
                source_index_kind: "directory".to_string(),
                disk_write_allowed: false,
                disk_write_requested: false,
                disk_write_ignored: false,
                commit_succeeded: true,
                tenant_filter_model: "no hits returned because no query fields were available".to_string(),
                hydration_model: "stored-field hydration unavailable without queryable fields".to_string(),
                notes: vec!["opened existing Tantivy index read-only".to_string()],
            },
            tenant_filter: build_tantivy_disk_query_filter(&options),
            query_fields: query_field_names,
            hits: Vec::new(),
            findings,
        });
    }

    let reader = index.reader()?;
    let searcher = reader.searcher();
    let parser = QueryParser::for_index(&index, query_fields);
    let parsed_query = parser
        .parse_query(&query_text)
        .map_err(|err| CustodianError::SearchIndex(format!("tantivy disk query parse failed: {err}")))?;
    let top_docs = searcher.search(&parsed_query, &TopDocs::with_limit(top_k).order_by_score())?;
    let base_filter = build_tantivy_disk_query_filter(&options);
    let mut accepted_hits = Vec::<CodeTantivyHydratedHit>::new();
    let mut rejected_hit_count = 0usize;

    for (score, addr) in top_docs {
        let retrieved_doc: TantivyDocument = searcher.doc(addr)?;
        let document_json = retrieved_doc.to_json(&schema);
        let source_document_id = stored_text_from_tantivy_json(&document_json, "doc_id");
        let document = hydrate_indexed_document_from_tantivy_json(&document_json);
        let hit = CodeTantivyHydratedHit {
            rank: accepted_hits.len() + 1,
            score,
            segment_ord: addr.segment_ord,
            tantivy_doc_id: addr.doc_id,
            source_document_id,
            document,
        };
        if tenant_query_filter_accepts_hit(&base_filter, &hit) {
            accepted_hits.push(hit);
        } else {
            rejected_hit_count += 1;
        }
    }

    let mut tenant_filter = base_filter;
    tenant_filter.accepted_hit_count = accepted_hits.len();
    tenant_filter.rejected_hit_count = rejected_hit_count;

    Ok(CodeTantivyDiskQueryReceipt {
        schema: "code_passenger.tantivy_disk_query_receipt.v1".to_string(),
        index_dir: index_dir_text,
        index_exists: true,
        query: query_text,
        top_k,
        hit_count: accepted_hits.len(),
        index: CodeTantivyQueryIndexStatus {
            engine: "tantivy".to_string(),
            tantivy_used: true,
            source_index_kind: "directory".to_string(),
            disk_write_allowed: false,
            disk_write_requested: false,
            disk_write_ignored: false,
            commit_succeeded: true,
            tenant_filter_model: "opened disk-index hits are filtered against explicit tenant/namespace/workspace/project/visibility values before return".to_string(),
            hydration_model: "hits are hydrated only from stored Tantivy fields in the opened disk index".to_string(),
            notes: vec![
                "opened existing Tantivy index read-only".to_string(),
                "disk query never creates, overwrites, or commits an index".to_string(),
                "use code-search-index-json --write-index --index-dir to create the index first".to_string(),
            ],
        },
        tenant_filter,
        query_fields: query_field_names,
        hits: accepted_hits,
        findings,
    })
}

pub(crate) fn disk_query_field_names(schema: &tantivy::schema::Schema) -> Vec<String> {
    ["label", "path", "dep", "scope", "full_path", "feature_candidates", "symbols", "content"]
        .iter()
        .filter(|name| schema.get_field(name).is_ok())
        .map(|name| (*name).to_string())
        .collect()
}

pub(crate) fn build_tantivy_disk_query_filter(options: &CodeTantivyIndexOptions) -> CodeTantivyTenantQueryFilter {
    let enforced = !options.disable_tenant_filter;
    CodeTantivyTenantQueryFilter {
        enforced,
        mode: if enforced { "disk_hydration_stage_exact_match" } else { "disabled" }.to_string(),
        tenant_id: enforced.then(|| options.filter_tenant_id.clone()).flatten(),
        namespace: enforced.then(|| options.filter_namespace.clone()).flatten(),
        workspace_id: enforced.then(|| options.filter_workspace_id.clone()).flatten(),
        project_id: enforced.then(|| options.filter_project_id.clone()).flatten(),
        visibility: enforced.then(|| options.filter_visibility.clone()).flatten(),
        exact_fields: if enforced {
            vec![
                "tenant_id".to_string(),
                "namespace".to_string(),
                "workspace_id".to_string(),
                "project_id".to_string(),
                "visibility".to_string(),
            ]
        } else {
            Vec::new()
        },
        accepted_hit_count: 0,
        rejected_hit_count: 0,
        source: "disk_query_filter_flags".to_string(),
        notes: if enforced {
            vec![
                "disk queries have no implicit tenant scope; pass --filter-tenant/namespace/workspace/project/visibility to narrow returned hits".to_string(),
                "unset filter fields are treated as wildcards".to_string(),
                "--no-tenant-filter disables return-path enforcement for diagnostics".to_string(),
            ]
        } else {
            vec!["tenant filter disabled by --no-tenant-filter".to_string()]
        },
    }
}

pub(crate) fn hydrate_indexed_document_from_tantivy_json(json: &str) -> Option<CodeTantivyIndexedDocumentReceipt> {
    let id = stored_text_from_tantivy_json(json, "doc_id")?;
    Some(CodeTantivyIndexedDocumentReceipt {
        id,
        pin_id: stored_text_from_tantivy_json(json, "pin_id").unwrap_or_default(),
        kind: stored_text_from_tantivy_json(json, "kind").unwrap_or_default(),
        label: stored_text_from_tantivy_json(json, "label").unwrap_or_default(),
        path: stored_text_from_tantivy_json(json, "path").unwrap_or_default(),
        tenant_id: stored_text_from_tantivy_json(json, "tenant_id").unwrap_or_default(),
        namespace: stored_text_from_tantivy_json(json, "namespace").unwrap_or_default(),
        workspace_id: stored_text_from_tantivy_json(json, "workspace_id").unwrap_or_default(),
        project_id: stored_text_from_tantivy_json(json, "project_id").unwrap_or_default(),
        visibility: stored_text_from_tantivy_json(json, "visibility").unwrap_or_default(),
        indexed_fields: stored_field_names_from_tantivy_json(json),
    })
}

pub(crate) fn stored_field_names_from_tantivy_json(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    object
        .keys()
        .filter(|name| name.as_str() != "anchor_line")
        .cloned()
        .collect()
}

pub(crate) fn build_tantivy_query_receipt_from_index_receipt(
    index_receipt: CodeTantivyIndexReceipt,
    requested_options: CodeTantivyIndexOptions,
) -> CodeTantivyQueryReceipt {
    let documents_by_id = index_receipt
        .indexed_documents
        .iter()
        .map(|doc| (doc.id.clone(), doc.clone()))
        .collect::<BTreeMap<_, _>>();
    let base_filter = build_tantivy_tenant_query_filter(&index_receipt.tenant_scope, &requested_options);
    let mut raw_hits = Vec::<CodeTantivyHydratedHit>::new();
    for hit in index_receipt.search_check.top_hits.iter() {
        raw_hits.push(CodeTantivyHydratedHit {
            rank: hit.rank,
            score: hit.score,
            segment_ord: hit.segment_ord,
            tantivy_doc_id: hit.doc_id,
            source_document_id: hit.source_document_id.clone(),
            document: hit
                .source_document_id
                .as_ref()
                .and_then(|id| documents_by_id.get(id).cloned()),
        });
    }

    let mut accepted_hits = Vec::<CodeTantivyHydratedHit>::new();
    let mut rejected_hit_count = 0usize;
    for hit in raw_hits {
        if tenant_query_filter_accepts_hit(&base_filter, &hit) {
            let mut accepted = hit;
            accepted.rank = accepted_hits.len() + 1;
            accepted_hits.push(accepted);
        } else {
            rejected_hit_count += 1;
        }
    }
    let mut tenant_filter = base_filter;
    tenant_filter.accepted_hit_count = accepted_hits.len();
    tenant_filter.rejected_hit_count = rejected_hit_count;

    CodeTantivyQueryReceipt {
        schema: "code_passenger.tantivy_query_receipt.v1".to_string(),
        graph_schema: index_receipt.graph_schema.clone(),
        root: index_receipt.root.clone(),
        root_exists: index_receipt.root_exists,
        max_depth: index_receipt.max_depth,
        source_index_receipt_schema: index_receipt.schema.clone(),
        tenant_scope: index_receipt.tenant_scope.clone(),
        analyzer: index_receipt.analyzer.clone(),
        query: index_receipt.search_check.query.clone(),
        top_k: index_receipt.search_check.top_k,
        hit_count: accepted_hits.len(),
        index: CodeTantivyQueryIndexStatus {
            engine: index_receipt.index.engine.clone(),
            tantivy_used: index_receipt.index.tantivy_used,
            source_index_kind: index_receipt.index.index_kind.clone(),
            disk_write_allowed: false,
            disk_write_requested: requested_options.write_index,
            disk_write_ignored: requested_options.write_index,
            commit_succeeded: index_receipt.index.commit_succeeded,
            tenant_filter_model: "hits are filtered against tenant_id/namespace/workspace_id/project_id/visibility before they are returned; future persistent mixed-tenant indexes can push the same exact fields into the Tantivy query".to_string(),
            hydration_model: "hits are hydrated by reading the stored doc_id from Tantivy and joining back to the in-process Klepto document receipt".to_string(),
            notes: vec![
                "real Tantivy query executed against a freshly committed RAM index".to_string(),
                "query mode never writes a disk index or repository files".to_string(),
                "tenant filters are enforced before hydrated hits are returned".to_string(),
                "use code-search-index-json when an explicit index write receipt is desired".to_string(),
            ],
        },
        tenant_filter,
        query_fields: index_receipt.search_check.query_fields.clone(),
        hits: accepted_hits,
        indexed_document_count: index_receipt.indexed_documents.len(),
        indexed_documents: index_receipt.indexed_documents.clone(),
        summary: index_receipt.summary.clone(),
        findings: index_receipt.findings.clone(),
    }
}

pub(crate) fn build_tantivy_tenant_query_filter(
    scope: &CodeSearchTenantScope,
    options: &CodeTantivyIndexOptions,
) -> CodeTantivyTenantQueryFilter {
    let enforced = !options.disable_tenant_filter;
    CodeTantivyTenantQueryFilter {
        enforced,
        mode: if enforced { "hydration_stage_exact_match" } else { "disabled" }.to_string(),
        tenant_id: enforced.then(|| options.filter_tenant_id.clone().unwrap_or_else(|| scope.tenant_id.clone())),
        namespace: enforced.then(|| options.filter_namespace.clone().unwrap_or_else(|| scope.namespace.clone())),
        workspace_id: enforced.then(|| options.filter_workspace_id.clone().unwrap_or_else(|| scope.workspace_id.clone())),
        project_id: enforced.then(|| options.filter_project_id.clone().unwrap_or_else(|| scope.project_id.clone())),
        visibility: enforced.then(|| options.filter_visibility.clone().unwrap_or_else(|| scope.visibility.clone())),
        exact_fields: if enforced {
            vec![
                "tenant_id".to_string(),
                "namespace".to_string(),
                "workspace_id".to_string(),
                "project_id".to_string(),
                "visibility".to_string(),
            ]
        } else {
            Vec::new()
        },
        accepted_hit_count: 0,
        rejected_hit_count: 0,
        source: "query_filter_flags_or_index_tenant_scope".to_string(),
        notes: if enforced {
            vec![
                "default filter values come from the indexed tenant scope".to_string(),
                "--filter-tenant/namespace/workspace/project/visibility can narrow or intentionally mismatch returned hits".to_string(),
                "--no-tenant-filter disables this return-path enforcement for diagnostics".to_string(),
            ]
        } else {
            vec!["tenant filter disabled by --no-tenant-filter".to_string()]
        },
    }
}

pub(crate) fn tenant_query_filter_accepts_hit(filter: &CodeTantivyTenantQueryFilter, hit: &CodeTantivyHydratedHit) -> bool {
    if !filter.enforced {
        return true;
    }
    let Some(doc) = hit.document.as_ref() else {
        return false;
    };
    filter.tenant_id.as_deref().map_or(true, |expected| doc.tenant_id == expected)
        && filter.namespace.as_deref().map_or(true, |expected| doc.namespace == expected)
        && filter.workspace_id.as_deref().map_or(true, |expected| doc.workspace_id == expected)
        && filter.project_id.as_deref().map_or(true, |expected| doc.project_id == expected)
        && filter.visibility.as_deref().map_or(true, |expected| doc.visibility == expected)
}

pub(crate) fn stored_text_from_tantivy_json(json: &str, field: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let field_value = value.get(field)?;
    match field_value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(values) => values.iter().find_map(|item| {
            item.as_str()
                .map(ToOwned::to_owned)
                .or_else(|| item.get("Str").and_then(|v| v.as_str()).map(ToOwned::to_owned))
                .or_else(|| item.get("text").and_then(|v| v.as_str()).map(ToOwned::to_owned))
        }),
        serde_json::Value::Object(map) => map
            .get("Str")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .or_else(|| map.get("text").and_then(|v| v.as_str()).map(ToOwned::to_owned)),
        _ => None,
    }
}

pub(crate) fn add_text_field(doc: &mut tantivy::TantivyDocument, field_map: &BTreeMap<&str, tantivy::schema::Field>, name: &str, value: &str) {
    if let Some(field) = field_map.get(name) {
        doc.add_text(*field, value);
    }
}

pub(crate) fn tantivy_field_receipt(
    name: &str,
    field_type: &str,
    stored: bool,
    indexed: bool,
    tokenizer: Option<&str>,
    source: &str,
) -> CodeTantivySchemaFieldReceipt {
    CodeTantivySchemaFieldReceipt {
        name: name.to_string(),
        field_type: field_type.to_string(),
        stored,
        indexed,
        tokenizer: tokenizer.map(str::to_string),
        source: source.to_string(),
    }
}
