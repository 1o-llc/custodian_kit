//! Tantivy index receipts (RAM and disk) and the disk status / manifest / compare / refresh-plan receipts.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[allow(clippy::too_many_lines)]
pub(crate) fn build_tantivy_index_receipt_from_klepto_plan(
    plan: &CodeKleptoFieldPlan,
    options: CodeTantivyIndexOptions,
) -> Result<CodeTantivyIndexReceipt> {
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, INDEXED, STORED, STRING, TEXT, FAST};
    use tantivy::tokenizer::{LowerCaser, RemoveLongFilter, SimpleTokenizer, TextAnalyzer};
    use tantivy::{Document, Index, TantivyDocument};

    let mut schema_builder = Schema::builder();
    let doc_id = schema_builder.add_text_field("doc_id", STRING | STORED);
    let tenant_id = schema_builder.add_text_field("tenant_id", STRING | STORED);
    let namespace = schema_builder.add_text_field("namespace", STRING | STORED);
    let workspace_id = schema_builder.add_text_field("workspace_id", STRING | STORED);
    let project_id = schema_builder.add_text_field("project_id", STRING | STORED);
    let visibility = schema_builder.add_text_field("visibility", STRING | STORED);
    let role = schema_builder.add_text_field("role", STRING | STORED);
    let extraction_mode = schema_builder.add_text_field("extraction_mode", STRING | STORED);
    let pin_id = schema_builder.add_text_field("pin_id", STRING | STORED);
    let source_result_id = schema_builder.add_text_field("source_result_id", STRING | STORED);
    let kind = schema_builder.add_text_field("kind", STRING | STORED);
    let label = schema_builder.add_text_field("label", TEXT | STORED);
    let crate_name = schema_builder.add_text_field("crate_name", STRING | STORED);
    let path = schema_builder.add_text_field("path", STRING | STORED);
    let copy_hint = schema_builder.add_text_field("copy_hint", STRING | STORED);
    let dep = schema_builder.add_text_field("dep", STRING | STORED);
    let scope = schema_builder.add_text_field("scope", TEXT | STORED);
    let full_path = schema_builder.add_text_field("full_path", TEXT | STORED);
    let feature_candidates = schema_builder.add_text_field("feature_candidates", TEXT | STORED);
    let feature_gate_target = schema_builder.add_text_field("feature_gate_target", STRING | STORED);
    let symbols = schema_builder.add_text_field("symbols", TEXT | STORED);
    let content_indexing = TextFieldIndexing::default()
        .set_tokenizer("code_default")
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    let content_options = TextOptions::default().set_indexing_options(content_indexing).set_stored();
    let content = schema_builder.add_text_field("content", content_options);
    let anchor_line = schema_builder.add_u64_field("anchor_line", INDEXED | STORED | FAST);
    let schema = schema_builder.build();

    let index = if options.write_index {
        let dir = options.index_dir.as_ref().ok_or_else(|| {
            CustodianError::SearchIndex("--write-index requires --index-dir".to_string())
        })?;
        let path = Path::new(dir);
        if path.exists() {
            if options.overwrite_index {
                fs::remove_dir_all(path)?;
            } else {
                return Err(CustodianError::SearchIndex(format!(
                    "index dir already exists: {dir}; pass --overwrite-index to replace it"
                )));
            }
        }
        fs::create_dir_all(path)?;
        Index::create_in_dir(path, schema.clone())?
    } else {
        Index::create_in_ram(schema.clone())
    };

    let analyzer = TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(40))
        .filter(LowerCaser)
        .build();
    index.tokenizers().register("code_default", analyzer);

    let mut writer = index.writer(plan.writer_memory.writer_memory_bytes)?;
    let base_fields: [(&str, Field); 24] = [
        ("doc_id", doc_id),
        ("tenant_id", tenant_id),
        ("namespace", namespace),
        ("workspace_id", workspace_id),
        ("project_id", project_id),
        ("visibility", visibility),
        ("role", role),
        ("extraction_mode", extraction_mode),
        ("pin_id", pin_id),
        ("source_result_id", source_result_id),
        ("kind", kind),
        ("label", label),
        ("crate_name", crate_name),
        ("path", path),
        ("copy_hint", copy_hint),
        ("dep", dep),
        ("scope", scope),
        ("full_path", full_path),
        ("feature_candidates", feature_candidates),
        ("feature_gate_target", feature_gate_target),
        ("symbols", symbols),
        ("content", content),
        ("anchor_line", anchor_line),
        ("namespace", namespace),
    ];
    let field_map: BTreeMap<&str, Field> = base_fields.iter().copied().collect();
    let mut indexed_documents = Vec::<CodeTantivyIndexedDocumentReceipt>::new();

    for source_doc in &plan.documents {
        let mut doc = TantivyDocument::new();
        let mut stored_names = [
            "doc_id",
            "tenant_id",
            "namespace",
            "workspace_id",
            "project_id",
            "visibility",
            "role",
            "extraction_mode",
            "pin_id",
            "source_result_id",
            "kind",
            "label",
            "crate_name",
            "path",
            "copy_hint",
        ]
        .into_iter()
        .map(String::from)
        .collect::<BTreeSet<_>>();
        add_text_field(&mut doc, &field_map, "doc_id", &source_doc.id);
        add_text_field(&mut doc, &field_map, "tenant_id", &source_doc.tenant_id);
        add_text_field(&mut doc, &field_map, "namespace", &source_doc.namespace);
        add_text_field(&mut doc, &field_map, "workspace_id", &source_doc.workspace_id);
        add_text_field(&mut doc, &field_map, "project_id", &source_doc.project_id);
        add_text_field(&mut doc, &field_map, "visibility", &source_doc.visibility);
        add_text_field(&mut doc, &field_map, "role", &source_doc.role);
        add_text_field(&mut doc, &field_map, "extraction_mode", &source_doc.extraction_mode);
        add_text_field(&mut doc, &field_map, "pin_id", &source_doc.pin_id);
        add_text_field(&mut doc, &field_map, "source_result_id", &source_doc.source_result_id);
        add_text_field(&mut doc, &field_map, "kind", &source_doc.kind);
        add_text_field(&mut doc, &field_map, "label", &source_doc.label);
        add_text_field(&mut doc, &field_map, "crate_name", source_doc.crate_name.as_deref().unwrap_or(""));
        add_text_field(&mut doc, &field_map, "path", &source_doc.path);
        add_text_field(&mut doc, &field_map, "copy_hint", &source_doc.copy_hint);
        for name in ["dep", "scope", "full_path", "feature_candidates", "symbols", "content"] {
            if let Some(value) = source_doc.fields.get(name) {
                add_text_field(&mut doc, &field_map, name, value);
                stored_names.insert(name.to_string());
            }
        }
        if let Some(target) = plan.feature_binding.target_feature.as_deref() {
            add_text_field(&mut doc, &field_map, "feature_gate_target", target);
            stored_names.insert("feature_gate_target".to_string());
        }
        if let Some(field) = field_map.get("anchor_line") {
            doc.add_u64(*field, source_doc.anchor_line as u64);
        }
        writer.add_document(doc)?;
        indexed_documents.push(CodeTantivyIndexedDocumentReceipt {
            id: source_doc.id.clone(),
            pin_id: source_doc.pin_id.clone(),
            kind: source_doc.kind.clone(),
            label: source_doc.label.clone(),
            path: source_doc.path.clone(),
            tenant_id: source_doc.tenant_id.clone(),
            namespace: source_doc.namespace.clone(),
            workspace_id: source_doc.workspace_id.clone(),
            project_id: source_doc.project_id.clone(),
            visibility: source_doc.visibility.clone(),
            indexed_fields: stored_names.into_iter().collect(),
        });
    }
    writer.commit()?;

    let query_text = options
        .search_query
        .clone()
        .or_else(|| plan.feature_binding.matched_deps.first().cloned())
        .or_else(|| plan.feature_binding.target_feature.clone())
        .or_else(|| plan.documents.first().map(|doc| doc.label.clone()))
        .unwrap_or_else(|| "code".to_string());
    let query_fields = vec![label, path, dep, scope, full_path, feature_candidates, symbols, content];
    let query_field_names = vec![
        "label".to_string(),
        "path".to_string(),
        "dep".to_string(),
        "scope".to_string(),
        "full_path".to_string(),
        "feature_candidates".to_string(),
        "symbols".to_string(),
        "content".to_string(),
    ];
    let reader = index.reader()?;
    let searcher = reader.searcher();
    let parser = QueryParser::for_index(&index, query_fields);
    let parsed_query = parser
        .parse_query(&query_text)
        .map_err(|err| CustodianError::SearchIndex(format!("tantivy query parse failed: {err}")))?;
    let top_docs = searcher.search(&parsed_query, &TopDocs::with_limit(options.top_k).order_by_score())?;
    let mut top_hits = Vec::new();
    for (idx, (score, addr)) in top_docs.iter().enumerate() {
        let retrieved_doc: TantivyDocument = searcher.doc(*addr)?;
        let source_document_id = stored_text_from_tantivy_json(&retrieved_doc.to_json(&schema), "doc_id");
        top_hits.push(CodeTantivySearchHit {
            rank: idx + 1,
            score: *score,
            segment_ord: addr.segment_ord,
            doc_id: addr.doc_id,
            source_document_id,
        });
    }

    let schema_fields = vec![
        tantivy_field_receipt("doc_id", "text_exact", true, true, None, "klepto_document_id"),
        tantivy_field_receipt("tenant_id", "text_exact", true, true, None, "tenant_scope"),
        tantivy_field_receipt("namespace", "text_exact", true, true, None, "tenant_scope"),
        tantivy_field_receipt("workspace_id", "text_exact", true, true, None, "tenant_scope"),
        tantivy_field_receipt("project_id", "text_exact", true, true, None, "tenant_scope"),
        tantivy_field_receipt("visibility", "text_exact", true, true, None, "tenant_scope"),
        tantivy_field_receipt("role", "text_exact", true, true, None, "tenant_scope"),
        tantivy_field_receipt("extraction_mode", "text_exact", true, true, None, "tenant_scope"),
        tantivy_field_receipt("pin_id", "text_exact", true, true, None, "pin"),
        tantivy_field_receipt("source_result_id", "text_exact", true, true, None, "pin"),
        tantivy_field_receipt("kind", "text_exact", true, true, None, "query_result"),
        tantivy_field_receipt("label", "text", true, true, Some("default"), "query_result"),
        tantivy_field_receipt("crate_name", "text_exact", true, true, None, "query_result"),
        tantivy_field_receipt("path", "text_exact", true, true, None, "query_result"),
        tantivy_field_receipt("copy_hint", "text_exact", true, true, None, "query_result"),
        tantivy_field_receipt("dep", "text_exact", true, true, None, "klepto_field"),
        tantivy_field_receipt("scope", "text", true, true, Some("default"), "klepto_field"),
        tantivy_field_receipt("full_path", "text", true, true, Some("default"), "klepto_field"),
        tantivy_field_receipt("feature_candidates", "text", true, true, Some("default"), "klepto_field"),
        tantivy_field_receipt("feature_gate_target", "text_exact", true, true, None, "feature_binding"),
        tantivy_field_receipt("symbols", "text", true, true, Some("default"), "klepto_field"),
        tantivy_field_receipt("content", "text", true, true, Some("code_default"), "context_window"),
        tantivy_field_receipt("anchor_line", "u64", true, true, None, "context_window"),
    ];

    Ok(CodeTantivyIndexReceipt {
        schema: "code_passenger.tantivy_index_receipt.v1".to_string(),
        graph_schema: plan.graph_schema.clone(),
        root: plan.root.clone(),
        root_exists: plan.root_exists,
        max_depth: plan.max_depth,
        source_field_plan_schema: plan.schema.clone(),
        tenant_scope: plan.tenant_scope.clone(),
        analyzer: plan.analyzer.clone(),
        index: CodeTantivyIndexStatus {
            engine: "tantivy".to_string(),
            tantivy_used: true,
            index_kind: if options.write_index { "directory" } else { "ram" }.to_string(),
            index_dir: options.index_dir.clone(),
            write_requested: options.write_index,
            index_written: options.write_index,
            overwrite_requested: options.overwrite_index,
            commit_succeeded: true,
            tokenizer_registered: "code_default".to_string(),
            tenant_filter_model: "documents are scoped by tenant_id/namespace/workspace_id/project_id before indexing; exact filter fields are indexed for later query enforcement".to_string(),
            notes: vec![
                "real Tantivy Index was created and committed".to_string(),
                "default mode uses Index::create_in_ram; disk index requires --write-index --index-dir".to_string(),
                "repo files are never modified by this command".to_string(),
            ],
        },
        writer_memory: plan.writer_memory.clone(),
        schema_fields,
        document_count: plan.document_count,
        committed_document_count: indexed_documents.len(),
        search_check: CodeTantivySearchCheck {
            query: query_text,
            top_k: options.top_k,
            hit_count: top_hits.len(),
            top_hits,
            query_fields: query_field_names,
        },
        indexed_documents,
        summary: plan.summary.clone(),
        findings: plan.findings.clone(),
    })
}

pub(crate) fn parse_code_search_disk_status_args(args: &[String]) -> CodeTantivyDiskStatusOptions {
    let mut options = CodeTantivyDiskStatusOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--sample" | "--sample-docs" | "--sample-limit" => {
                if let Some(value) = args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) {
                    options.sample_limit = value;
                    i += 1;
                }
            }
            "--max-files" | "--file-limit" => {
                if let Some(value) = args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) {
                    options.max_files = value;
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    options
}

#[allow(clippy::too_many_lines)]
pub(crate) fn build_tantivy_disk_status_receipt(index_dir: &Path, options: CodeTantivyDiskStatusOptions) -> Result<CodeTantivyDiskStatusReceipt> {
    use tantivy::collector::TopDocs;
    use tantivy::query::AllQuery;
    use tantivy::{Document, Index, TantivyDocument};

    let index_dir_text = index_dir.to_string_lossy().to_string();
    if !index_dir.exists() {
        return Ok(CodeTantivyDiskStatusReceipt {
            schema: "code_passenger.tantivy_disk_status_receipt.v1".to_string(),
            index_dir: index_dir_text.clone(),
            index_exists: false,
            index: CodeTantivyDiskStatus {
                engine: "tantivy".to_string(),
                tantivy_used: false,
                opened_read_only: false,
                source_index_kind: "directory".to_string(),
                mutates_index: false,
                tenant_filter_model: "status cannot inspect tenant scopes because index dir does not exist".to_string(),
                hydration_model: "no stored-field hydration performed because index dir does not exist".to_string(),
                notes: vec!["index dir does not exist; status receipt is intentionally empty".to_string()],
            },
            document_count: 0,
            schema_field_count: 0,
            schema_fields: Vec::new(),
            query_fields: Vec::new(),
            tenant_scope_count: 0,
            tenant_scopes: Vec::new(),
            kind_counts: BTreeMap::new(),
            stored_field_names: Vec::new(),
            sample_document_count: 0,
            sample_documents: Vec::new(),
            findings: vec![FindingNode {
                severity: "warn".to_string(),
                code: "CPS001".to_string(),
                message: "index dir does not exist; disk status is intentionally empty".to_string(),
                path: Some(index_dir_text),
                line: None,
                column: None,
            }],
        });
    }

    let index = Index::open_in_dir(index_dir)?;
    let schema = index.schema();
    let query_fields = disk_query_field_names(&schema);
    let schema_fields = schema
        .fields()
        .enumerate()
        .map(|(ord, field)| {
            let entry = schema.get_field_entry(field.0);
            let name = entry.name().to_string();
            CodeTantivyDiskSchemaFieldStatus {
                name: name.clone(),
                field_ord: ord as u32,
                field_type: format!("{:?}", entry.field_type()),
                queryable_by_default: query_fields.iter().any(|candidate| candidate == &name),
            }
        })
        .collect::<Vec<_>>();

    let reader = index.reader()?;
    let searcher = reader.searcher();
    let document_count = searcher.num_docs() as usize;
    let mut documents = Vec::<CodeTantivyIndexedDocumentReceipt>::new();
    if document_count > 0 {
        let all_docs = searcher.search(&AllQuery, &TopDocs::with_limit(document_count).order_by_score())?;
        for (_score, addr) in all_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(addr)?;
            let document_json = retrieved_doc.to_json(&schema);
            if let Some(document) = hydrate_indexed_document_from_tantivy_json(&document_json) {
                documents.push(document);
            }
        }
    }

    let mut tenant_counts = BTreeMap::<String, CodeTantivyDiskTenantScopeStatus>::new();
    let mut kind_counts = BTreeMap::<String, usize>::new();
    let mut stored_field_names = BTreeSet::<String>::new();
    for document in documents.iter() {
        *kind_counts.entry(document.kind.clone()).or_insert(0) += 1;
        for name in document.indexed_fields.iter() {
            stored_field_names.insert(name.clone());
        }
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
    let tenant_scopes = tenant_counts.into_values().collect::<Vec<_>>();
    let sample_documents = documents.into_iter().take(options.sample_limit).collect::<Vec<_>>();

    Ok(CodeTantivyDiskStatusReceipt {
        schema: "code_passenger.tantivy_disk_status_receipt.v1".to_string(),
        index_dir: index_dir_text,
        index_exists: true,
        index: CodeTantivyDiskStatus {
            engine: "tantivy".to_string(),
            tantivy_used: true,
            opened_read_only: true,
            source_index_kind: "directory".to_string(),
            mutates_index: false,
            tenant_filter_model: "status enumerates stored tenant_id/namespace/workspace_id/project_id/visibility values without returning unfiltered query hits".to_string(),
            hydration_model: "stored Tantivy fields are hydrated into compact document receipts for audit only".to_string(),
            notes: vec![
                "opened existing Tantivy index read-only".to_string(),
                "status command never creates, overwrites, or commits an index".to_string(),
                "tenant scopes and kind counts are derived from stored fields".to_string(),
            ],
        },
        document_count,
        schema_field_count: schema_fields.len(),
        schema_fields,
        query_fields,
        tenant_scope_count: tenant_scopes.len(),
        tenant_scopes,
        kind_counts,
        stored_field_names: stored_field_names.into_iter().collect(),
        sample_document_count: sample_documents.len(),
        sample_documents,
        findings: Vec::new(),
    })
}


pub(crate) fn build_tantivy_disk_manifest_receipt(index_dir: &Path, options: CodeTantivyDiskStatusOptions) -> Result<CodeTantivyDiskManifestReceipt> {
    let index_dir_text = index_dir.to_string_lossy().to_string();
    let status = build_tantivy_disk_status_receipt(index_dir, options.clone())?;
    let mut findings = status.findings.clone();
    if !index_dir.exists() {
        return Ok(CodeTantivyDiskManifestReceipt {
            schema: "code_passenger.tantivy_disk_manifest_receipt.v1".to_string(),
            index_dir: index_dir_text,
            index_exists: false,
            status_schema: status.schema,
            index: CodeTantivyDiskManifestStatus {
                engine: "tantivy".to_string(),
                tantivy_used: false,
                opened_read_only: false,
                mutates_index: false,
                hash_algorithm: crate::digest::ALGORITHM.to_string(),
                digest_model: "no digest because index dir does not exist".to_string(),
                notes: vec!["index dir does not exist; manifest receipt is intentionally empty".to_string()],
            },
            document_count: 0,
            tenant_scope_count: 0,
            tenant_scopes: Vec::new(),
            schema_field_count: 0,
            query_fields: Vec::new(),
            stored_field_names: Vec::new(),
            file_count: 0,
            listed_file_count: 0,
            total_bytes: 0,
            manifest_digest: "none".to_string(),
            hash_algorithm: crate::digest::ALGORITHM.to_string(),
            files: Vec::new(),
            sample_document_count: 0,
            sample_documents: Vec::new(),
            findings,
        });
    }

    let (all_files, total_bytes, manifest_digest) = collect_disk_manifest_files(index_dir)?;
    let file_count = all_files.len();
    let listed_files = all_files.into_iter().take(options.max_files).collect::<Vec<_>>();
    if file_count > listed_files.len() {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPM002".to_string(),
            message: format!("manifest file listing truncated from {file_count} to {} entries", listed_files.len()),
            path: Some(index_dir_text.clone()),
            line: None,
            column: None,
        });
    }

    Ok(CodeTantivyDiskManifestReceipt {
        schema: "code_passenger.tantivy_disk_manifest_receipt.v1".to_string(),
        index_dir: index_dir_text,
        index_exists: true,
        status_schema: status.schema,
        index: CodeTantivyDiskManifestStatus {
            engine: "tantivy".to_string(),
            tantivy_used: true,
            opened_read_only: true,
            mutates_index: false,
            hash_algorithm: crate::digest::ALGORITHM.to_string(),
            digest_model: "sorted relative path + byte length + per-file digest folded into one directory digest".to_string(),
            notes: vec![
                "opened existing Tantivy index through the read-only status path".to_string(),
                "manifest command never creates, overwrites, commits, or queries an index".to_string(),
                "digest is intended as a stable local comparison receipt, not a cryptographic signature".to_string(),
            ],
        },
        document_count: status.document_count,
        tenant_scope_count: status.tenant_scope_count,
        tenant_scopes: status.tenant_scopes,
        schema_field_count: status.schema_field_count,
        query_fields: status.query_fields,
        stored_field_names: status.stored_field_names,
        file_count,
        listed_file_count: listed_files.len(),
        total_bytes,
        manifest_digest,
        hash_algorithm: crate::digest::ALGORITHM.to_string(),
        files: listed_files,
        sample_document_count: status.sample_document_count,
        sample_documents: status.sample_documents,
        findings,
    })
}

pub(crate) fn build_tantivy_disk_compare_receipt(left_dir: &Path, right_dir: &Path, options: CodeTantivyDiskStatusOptions) -> Result<CodeTantivyDiskCompareReceipt> {
    let left = build_tantivy_disk_manifest_receipt(left_dir, options.clone())?;
    let right = build_tantivy_disk_manifest_receipt(right_dir, options)?;
    let left_text = left_dir.to_string_lossy().to_string();
    let right_text = right_dir.to_string_lossy().to_string();
    let mut findings = Vec::<FindingNode>::new();
    findings.extend(left.findings.clone());
    findings.extend(right.findings.clone());

    if !left.index_exists {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPC001".to_string(),
            message: "left index dir does not exist; compare receipt is partial".to_string(),
            path: Some(left_text.clone()),
            line: None,
            column: None,
        });
    }
    if !right.index_exists {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPC002".to_string(),
            message: "right index dir does not exist; compare receipt is partial".to_string(),
            path: Some(right_text.clone()),
            line: None,
            column: None,
        });
    }

    let left_files = left.files.iter().map(|file| (file.relative_path.clone(), file.clone())).collect::<BTreeMap<_, _>>();
    let right_files = right.files.iter().map(|file| (file.relative_path.clone(), file.clone())).collect::<BTreeMap<_, _>>();
    let left_paths = left_files.keys().cloned().collect::<BTreeSet<_>>();
    let right_paths = right_files.keys().cloned().collect::<BTreeSet<_>>();
    let common_paths = left_paths.intersection(&right_paths).cloned().collect::<Vec<_>>();
    let added_files = right_paths
        .difference(&left_paths)
        .filter_map(|path| right_files.get(path).cloned())
        .collect::<Vec<_>>();
    let removed_files = left_paths
        .difference(&right_paths)
        .filter_map(|path| left_files.get(path).cloned())
        .collect::<Vec<_>>();
    let changed_files = common_paths
        .iter()
        .filter_map(|path| {
            let left_file = left_files.get(path)?;
            let right_file = right_files.get(path)?;
            if left_file.byte_count == right_file.byte_count && left_file.digest == right_file.digest {
                None
            } else {
                Some(CodeTantivyDiskCompareChangedFile {
                    relative_path: path.clone(),
                    left_byte_count: left_file.byte_count,
                    right_byte_count: right_file.byte_count,
                    left_digest: left_file.digest.clone(),
                    right_digest: right_file.digest.clone(),
                })
            }
        })
        .collect::<Vec<_>>();

    let left_tenants = tenant_scope_map(&left.tenant_scopes);
    let right_tenants = tenant_scope_map(&right.tenant_scopes);
    let mut tenant_keys = left_tenants.keys().cloned().collect::<BTreeSet<_>>();
    tenant_keys.extend(right_tenants.keys().cloned());
    let mut tenant_scope_deltas = Vec::<CodeTantivyDiskCompareTenantDelta>::new();
    for key in tenant_keys {
        let left_scope = left_tenants.get(&key);
        let right_scope = right_tenants.get(&key);
        let left_document_count = left_scope.map(|scope| scope.document_count).unwrap_or(0);
        let right_document_count = right_scope.map(|scope| scope.document_count).unwrap_or(0);
        let left_kind_counts = left_scope.map(|scope| scope.kind_counts.clone()).unwrap_or_default();
        let right_kind_counts = right_scope.map(|scope| scope.kind_counts.clone()).unwrap_or_default();
        if left_document_count != right_document_count || left_kind_counts != right_kind_counts {
            tenant_scope_deltas.push(CodeTantivyDiskCompareTenantDelta {
                tenant_key: key,
                left_document_count,
                right_document_count,
                left_kind_counts,
                right_kind_counts,
            });
        }
    }

    let same_manifest_digest = left.index_exists && right.index_exists && left.manifest_digest == right.manifest_digest;
    let same_document_count = left.document_count == right.document_count;
    let same_tenant_scopes = left.tenant_scopes.len() == right.tenant_scopes.len() && tenant_scope_deltas.is_empty();
    let same_schema_field_count = left.schema_field_count == right.schema_field_count;
    let same_stored_fields = left.stored_field_names == right.stored_field_names;
    let byte_delta = i128::from(right.total_bytes) - i128::from(left.total_bytes);

    if left.index_exists && right.index_exists && !same_manifest_digest {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPC003".to_string(),
            message: "disk index manifests differ".to_string(),
            path: Some(format!("{left_text} <> {right_text}")),
            line: None,
            column: None,
        });
    }

    Ok(CodeTantivyDiskCompareReceipt {
        schema: "code_passenger.tantivy_disk_compare_receipt.v1".to_string(),
        left_index_dir: left_text,
        right_index_dir: right_text,
        left_exists: left.index_exists,
        right_exists: right.index_exists,
        index: CodeTantivyDiskCompareStatus {
            engine: "tantivy".to_string(),
            tantivy_used: left.index.tantivy_used || right.index.tantivy_used,
            opened_read_only: true,
            mutates_index: false,
            compare_model: "left and right read-only manifest receipts are compared by digest, file list, stored fields, schema counts, tenant scopes, and document counts".to_string(),
            hash_algorithm: crate::digest::ALGORITHM.to_string(),
            notes: vec![
                "compare command never creates, overwrites, commits, queries, or mutates an index".to_string(),
                "same_manifest_digest=true means the sorted file inventory digest matched".to_string(),
                "tenant_scope_deltas expose document/kind count differences by stored tenant scope".to_string(),
            ],
        },
        left_manifest_schema: left.schema,
        right_manifest_schema: right.schema,
        left_manifest_digest: left.manifest_digest,
        right_manifest_digest: right.manifest_digest,
        same_manifest_digest,
        same_document_count,
        same_tenant_scopes,
        same_schema_field_count,
        same_stored_fields,
        left_document_count: left.document_count,
        right_document_count: right.document_count,
        left_tenant_scope_count: left.tenant_scope_count,
        right_tenant_scope_count: right.tenant_scope_count,
        left_schema_field_count: left.schema_field_count,
        right_schema_field_count: right.schema_field_count,
        left_file_count: left.file_count,
        right_file_count: right.file_count,
        left_total_bytes: left.total_bytes,
        right_total_bytes: right.total_bytes,
        byte_delta,
        common_file_count: common_paths.len(),
        added_files,
        removed_files,
        changed_files,
        tenant_scope_deltas,
        findings,
    })
}

pub(crate) fn parse_code_search_refresh_plan_args(args: &[String]) -> (CodeTantivyDiskStatusOptions, Vec<String>) {
    let mut options = CodeTantivyDiskStatusOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--sample" | "--sample-docs" | "--sample-limit" => {
                if let Some(value) = args.get(i + 1) {
                    if let Ok(parsed) = value.parse::<usize>() {
                        options.sample_limit = parsed;
                    }
                    i += 2;
                    continue;
                }
            }
            "--max-files" | "--file-limit" => {
                if let Some(value) = args.get(i + 1) {
                    if let Ok(parsed) = value.parse::<usize>() {
                        options.max_files = parsed;
                    }
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        passthrough.push(args[i].clone());
        i += 1;
    }
    (options, passthrough)
}

pub(crate) fn build_tantivy_disk_refresh_plan_receipt(
    graph: &CodeGraph,
    index_dir: &Path,
    existing: CodeTantivyDiskManifestReceipt,
    candidate: CodeTantivyIndexReceipt,
    requested_options: CodeTantivyIndexOptions,
) -> CodeTantivyDiskRefreshPlanReceipt {
    let index_dir_text = index_dir.to_string_lossy().to_string();
    let candidate_tenant_scopes = tenant_scopes_from_indexed_documents(&candidate.indexed_documents);
    let candidate_stored_fields = stored_fields_from_indexed_documents(&candidate.indexed_documents);
    let candidate_tenants = tenant_scope_map(&candidate_tenant_scopes);
    let existing_tenants = tenant_scope_map(&existing.tenant_scopes);
    let mut tenant_keys = existing_tenants.keys().cloned().collect::<BTreeSet<_>>();
    tenant_keys.extend(candidate_tenants.keys().cloned());
    let mut tenant_scope_deltas = Vec::<CodeTantivyDiskCompareTenantDelta>::new();
    for key in tenant_keys {
        let existing_scope = existing_tenants.get(&key);
        let candidate_scope = candidate_tenants.get(&key);
        let left_document_count = existing_scope.map(|scope| scope.document_count).unwrap_or(0);
        let right_document_count = candidate_scope.map(|scope| scope.document_count).unwrap_or(0);
        let left_kind_counts = existing_scope.map(|scope| scope.kind_counts.clone()).unwrap_or_default();
        let right_kind_counts = candidate_scope.map(|scope| scope.kind_counts.clone()).unwrap_or_default();
        if left_document_count != right_document_count || left_kind_counts != right_kind_counts {
            tenant_scope_deltas.push(CodeTantivyDiskCompareTenantDelta {
                tenant_key: key,
                left_document_count,
                right_document_count,
                left_kind_counts,
                right_kind_counts,
            });
        }
    }

    let same_document_count = existing.index_exists && existing.document_count == candidate.committed_document_count;
    let same_tenant_scopes = existing.index_exists && existing.tenant_scope_count == candidate_tenant_scopes.len() && tenant_scope_deltas.is_empty();
    let same_schema_field_count = existing.index_exists && existing.schema_field_count == candidate.schema_fields.len();
    let same_stored_fields = existing.index_exists && existing.stored_field_names == candidate_stored_fields;
    let refresh_recommended = !existing.index_exists || !same_document_count || !same_tenant_scopes || !same_schema_field_count || !same_stored_fields;
    let candidate_search_hits = candidate.search_check.hit_count;
    let refresh_reason = if !existing.index_exists {
        "existing_index_missing".to_string()
    } else if !same_document_count {
        "document_count_changed".to_string()
    } else if !same_tenant_scopes {
        "tenant_scope_changed".to_string()
    } else if !same_schema_field_count {
        "schema_field_count_changed".to_string()
    } else if !same_stored_fields {
        "stored_fields_changed".to_string()
    } else {
        "existing_index_matches_candidate_plan".to_string()
    };

    let mut findings = Vec::<FindingNode>::new();
    findings.extend(existing.findings.clone());
    findings.extend(candidate.findings.clone());
    if !existing.index_exists {
        findings.push(FindingNode {
            severity: "warning".to_string(),
            code: "CPR001".to_string(),
            message: "existing index dir does not exist; refresh would create a replacement index in a future write phase".to_string(),
            path: Some(index_dir_text.clone()),
            line: None,
            column: None,
        });
    }
    if requested_options.write_index || requested_options.overwrite_index || requested_options.index_dir.is_some() {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPR002".to_string(),
            message: "refresh-plan mode ignores index write flags and builds the candidate in RAM only".to_string(),
            path: requested_options.index_dir.clone(),
            line: None,
            column: None,
        });
    }
    if existing.index_exists && refresh_recommended {
        findings.push(FindingNode {
            severity: "info".to_string(),
            code: "CPR003".to_string(),
            message: format!("candidate index differs from existing disk index: {refresh_reason}"),
            path: Some(index_dir_text.clone()),
            line: None,
            column: None,
        });
    }

    CodeTantivyDiskRefreshPlanReceipt {
        schema: "code_passenger.tantivy_disk_refresh_plan_receipt.v1".to_string(),
        root: graph.root.clone(),
        root_exists: graph.root_exists,
        max_depth: graph.max_depth,
        index_dir: index_dir_text,
        index_exists: existing.index_exists,
        index: CodeTantivyDiskRefreshPlanStatus {
            engine: "tantivy".to_string(),
            tantivy_used: existing.index.tantivy_used || candidate.index.tantivy_used,
            existing_opened_read_only: existing.index.opened_read_only,
            candidate_index_kind: candidate.index.index_kind.clone(),
            mutates_index: false,
            refresh_model: "existing disk manifest is compared to a freshly assembled candidate RAM index receipt; no index directory is created, overwritten, committed, swapped, or deleted".to_string(),
            notes: vec![
                "refresh plan opens the existing index through the read-only manifest path".to_string(),
                "candidate index is built in RAM even when write flags are supplied".to_string(),
                "future refresh execution should require a separate explicit write/swap command".to_string(),
            ],
        },
        existing_manifest_schema: existing.schema,
        candidate_index_schema: candidate.schema,
        existing_manifest_digest: existing.manifest_digest,
        existing_document_count: existing.document_count,
        candidate_document_count: candidate.committed_document_count,
        existing_tenant_scope_count: existing.tenant_scope_count,
        candidate_tenant_scope_count: candidate_tenant_scopes.len(),
        existing_schema_field_count: existing.schema_field_count,
        candidate_schema_field_count: candidate.schema_fields.len(),
        existing_stored_fields: existing.stored_field_names,
        candidate_stored_fields,
        same_document_count,
        same_tenant_scopes,
        same_schema_field_count,
        same_stored_fields,
        refresh_recommended,
        refresh_reason,
        write_requested: requested_options.write_index,
        overwrite_requested: requested_options.overwrite_index,
        write_allowed: false,
        index_mutated: false,
        candidate_committed: candidate.index.commit_succeeded,
        candidate_search_hits,
        tenant_scope_deltas,
        existing_findings: existing.findings,
        candidate_findings: candidate.findings,
        findings,
    }
}
