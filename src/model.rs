//! The receipt and graph types every command reads and writes.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CustodianError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml error: {0}")]
    Toml(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("search index error: {0}")]
    SearchIndex(String),
    #[error("tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    /// A refusal of `reading` (codes in `reading::ReadingRefusal`).
    #[error("[{code}] {message}")]
    Reading { code: String, message: String },
}


pub type Result<T> = std::result::Result<T, CustodianError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGraph {
    pub schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub crates: Vec<CrateNode>,
    pub files: Vec<FileNode>,
    pub features: Vec<FeatureNode>,
    pub deps: Vec<DependencyNode>,
    pub functions: Vec<FunctionNode>,
    pub imports: Vec<ImportNode>,
    pub use_sites: Vec<UseSiteNode>,
    pub findings: Vec<FindingNode>,
    pub summary: CodeGraphSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeGraphSummary {
    pub crate_count: usize,
    pub file_count: usize,
    pub feature_count: usize,
    pub dependency_count: usize,
    pub optional_dependency_count: usize,
    pub function_count: usize,
    pub import_count: usize,
    pub use_site_count: usize,
    pub finding_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateNode {
    pub id: String,
    pub name: String,
    pub crate_root: String,
    pub manifest_path: String,
    pub src_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub id: String,
    pub crate_id: String,
    pub crate_name: String,
    pub path: String,
    pub relative_path: String,
    pub line_count: usize,
    pub byte_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureNode {
    pub id: String,
    pub crate_id: String,
    pub crate_name: String,
    pub name: String,
    pub members: Vec<String>,
    pub enables_deps: Vec<String>,
    pub forwards_dependency_features: Vec<ForwardedDependencyFeature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardedDependencyFeature {
    pub dep: String,
    pub feature: String,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyNode {
    pub id: String,
    pub crate_id: String,
    pub crate_name: String,
    pub name: String,
    pub cargo_key: String,
    pub optional: bool,
    pub source_table: String,
    pub enabled_by_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionNode {
    pub id: String,
    pub crate_id: String,
    pub crate_name: String,
    pub path: String,
    pub name: String,
    pub line: u32,
    pub column: u32,
    pub visibility: String,
    pub signature_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportNode {
    pub id: String,
    pub crate_id: String,
    pub crate_name: String,
    pub path: String,
    pub root: String,
    pub full_path: String,
    pub line: u32,
    pub column: u32,
    pub is_public_use: bool,
    pub origin: ImportOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportOrigin {
    Internal,
    Std,
    Core,
    Alloc,
    Dependency,
    WorkspaceMember,
    UnknownExternal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseSiteNode {
    pub id: String,
    pub crate_id: String,
    pub crate_name: String,
    pub path: String,
    pub dep: String,
    pub full_path: String,
    pub head: String,
    pub kind: UseSiteKind,
    pub line: u32,
    pub column: u32,
    pub scope: String,
    pub feature_candidates: Vec<String>,
    pub optional_dep: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UseSiteKind {
    UseStmt,
    Path,
    MacroCall,
    InternalPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingNode {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeQueryReport {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub query: CodeQuerySpec,
    pub result_count: usize,
    pub truncated: bool,
    pub results: Vec<CodeQueryResult>,
    pub summary: CodeGraphSummary,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeQuerySpec {
    pub raw_args: Vec<String>,
    pub kind: String,
    pub name_contains: Option<String>,
    pub text_contains: Option<String>,
    pub path_contains: Option<String>,
    pub dep: Option<String>,
    pub feature: Option<String>,
    pub origin: Option<String>,
    pub scope_contains: Option<String>,
    pub returns_contains: Option<String>,
    pub takes_contains: Option<String>,
    pub public_only: bool,
    pub private_only: bool,
    pub limit: usize,
    /// `--item`: the exact item a use-site or import names — its full path, or
    /// its trailing `::`-segments (`serve_metrics`, `metrics::serve_metrics`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
}

impl Default for CodeQuerySpec {
    fn default() -> Self {
        Self {
            raw_args: Vec::new(),
            kind: "all".to_string(),
            name_contains: None,
            text_contains: None,
            path_contains: None,
            dep: None,
            feature: None,
            origin: None,
            scope_contains: None,
            returns_contains: None,
            takes_contains: None,
            public_only: false,
            private_only: false,
            limit: 100,
            item: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeQueryResult {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub crate_name: Option<String>,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub score_reason: Vec<String>,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePinSet {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_query: CodeQuerySpec,
    pub pin_count: usize,
    pub truncated: bool,
    pub pins: Vec<CodePin>,
    pub summary: CodeGraphSummary,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePin {
    pub id: String,
    pub source_result_id: String,
    pub kind: String,
    pub label: String,
    pub crate_name: Option<String>,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub range: Option<CodePinRange>,
    pub copy_hint: String,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePinRange {
    pub path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub range_kind: String,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContextReport {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_query: CodeQuerySpec,
    pub context_options: CodeContextOptions,
    pub pin_count: usize,
    pub context_count: usize,
    pub truncated: bool,
    pub total_bytes: usize,
    pub total_estimated_tokens: usize,
    pub pins: Vec<CodePin>,
    pub contexts: Vec<CodeContextWindow>,
    pub summary: CodeGraphSummary,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContextOptions {
    pub before_lines: usize,
    pub after_lines: usize,
    pub max_window_bytes: usize,
    pub max_total_bytes: usize,
}

impl Default for CodeContextOptions {
    fn default() -> Self {
        Self {
            before_lines: 3,
            after_lines: 3,
            max_window_bytes: 4096,
            max_total_bytes: 16384,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContextWindow {
    pub id: String,
    pub pin_id: String,
    pub source_result_id: String,
    pub kind: String,
    pub label: String,
    pub crate_name: Option<String>,
    pub path: String,
    pub anchor_line: u32,
    pub anchor_column: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub byte_count: usize,
    pub estimated_tokens: usize,
    pub truncated: bool,
    pub copy_hint: String,
    pub content: String,
    pub attributes: BTreeMap<String, String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchIndexPlan {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_query: CodeQuerySpec,
    pub context_options: CodeContextOptions,
    pub tenant_scope: CodeSearchTenantScope,
    pub analyzer: CodeSearchAnalyzerPlan,
    pub schema_fields: Vec<CodeSearchSchemaField>,
    pub extraction_policy: CodeSearchExtractionPolicy,
    pub writer_memory: CodeSearchWriterMemoryPlan,
    pub document_count: usize,
    pub documents: Vec<CodeSearchDocument>,
    pub summary: CodeGraphSummary,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchTenantScope {
    pub tenant_id: String,
    pub namespace: String,
    pub workspace_id: String,
    pub project_id: String,
    pub visibility: String,
    pub extraction_mode: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchAnalyzerPlan {
    pub tokenizer: String,
    pub language: String,
    pub tantivy_language: Option<String>,
    pub filters: Vec<String>,
    pub query_parser_fields: Vec<String>,
    pub source_influences: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchSchemaField {
    pub name: String,
    pub field_type: String,
    pub stored: bool,
    pub indexed: bool,
    pub fast: bool,
    pub tokenizer: Option<String>,
    pub searchable: bool,
    pub extraction_type: String,
    pub tenant_scoped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchExtractionPolicy {
    pub mode: String,
    pub role: String,
    pub searchable_fields: Vec<String>,
    pub extractable_fields: Vec<String>,
    pub hidden_fields: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchWriterMemoryPlan {
    pub estimated_avg_doc_size: usize,
    pub estimated_document_count: usize,
    pub estimated_payload_bytes: usize,
    pub requested_memory_bytes: Option<usize>,
    pub writer_memory_bytes: usize,
    pub min_bytes: usize,
    pub max_bytes: usize,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchDocument {
    pub id: String,
    pub tenant_id: String,
    pub namespace: String,
    pub workspace_id: String,
    pub project_id: String,
    pub visibility: String,
    pub pin_id: String,
    pub source_result_id: String,
    pub kind: String,
    pub label: String,
    pub crate_name: Option<String>,
    pub path: String,
    pub anchor_line: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub copy_hint: String,
    pub content: String,
    pub estimated_tokens: usize,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoFieldPlan {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_query: CodeQuerySpec,
    pub context_options: CodeContextOptions,
    pub tenant_scope: CodeSearchTenantScope,
    pub analyzer: CodeSearchAnalyzerPlan,
    pub feature_binding: CodeKleptoFeatureBinding,
    pub recipe: CodeKleptoRecipePlan,
    pub writer_memory: CodeSearchWriterMemoryPlan,
    pub field_count: usize,
    pub fields: Vec<CodeKleptoFieldSpec>,
    pub document_count: usize,
    pub documents: Vec<CodeKleptoFieldDocument>,
    pub summary: CodeGraphSummary,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoFeatureBinding {
    pub target_feature: Option<String>,
    pub source: String,
    pub matched_deps: Vec<String>,
    pub matched_feature_candidates: Vec<String>,
    pub feature_gate_requested: bool,
    pub feature_gate_plausible: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoRecipePlan {
    pub name: String,
    pub purpose: String,
    pub extraction_model: String,
    pub grab_sequence: Vec<String>,
    pub checks: Vec<String>,
    pub custom_fields_requested: Vec<String>,
    pub source_influences: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoFieldSpec {
    pub name: String,
    pub field_type: String,
    pub source: String,
    pub analyzer: Option<String>,
    pub filters: Vec<String>,
    pub stored: bool,
    pub indexed: bool,
    pub fast: bool,
    pub searchable: bool,
    pub extractable: bool,
    pub tenant_scoped: bool,
    pub feature_scoped: bool,
    pub customized_by_flag: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoFieldDocument {
    pub id: String,
    pub tenant_id: String,
    pub namespace: String,
    pub workspace_id: String,
    pub project_id: String,
    pub visibility: String,
    pub role: String,
    pub extraction_mode: String,
    pub pin_id: String,
    pub source_result_id: String,
    pub kind: String,
    pub label: String,
    pub crate_name: Option<String>,
    pub path: String,
    pub copy_hint: String,
    pub anchor_line: u32,
    pub fields: BTreeMap<String, String>,
    pub checks: Vec<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexKeyReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_klepto_schema: String,
    pub candidate_index_schema: String,
    pub binding: CodeKleptoIndexSchemaBindingStatus,
    pub key: CodeKleptoPolymorphicIndexKey,
    pub tenant_scope: CodeSearchTenantScope,
    pub analyzer: CodeSearchAnalyzerPlan,
    pub feature_binding: CodeKleptoFeatureBinding,
    pub klepto_field_count: usize,
    pub klepto_document_count: usize,
    pub index_schema_field_count: usize,
    pub index_stored_field_count: usize,
    pub query_fields: Vec<String>,
    pub field_bindings: Vec<CodeKleptoIndexFieldBinding>,
    pub write_requested: bool,
    pub overwrite_requested: bool,
    pub write_allowed: bool,
    pub index_mutated: bool,
    pub candidate_committed: bool,
    pub candidate_search_hits: usize,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexSchemaBindingStatus {
    pub model: String,
    pub source_schema_role: String,
    pub implementation_schema_role: String,
    pub polymorphic_index_key: bool,
    pub mutates_index: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoPolymorphicIndexKey {
    pub key_id: String,
    pub key_version: String,
    pub digest_algorithm: String,
    pub schema_hash: String,
    pub field_contract_hash: String,
    pub analyzer_hash: String,
    pub tenant_scope_hash: String,
    pub feature_binding_hash: String,
    pub document_shape_hash: String,
    pub version_bump_recommendation: String,
    pub version_bump_reason: String,
    pub stable_key_parts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexFieldBinding {
    pub klepto_field: String,
    pub source: String,
    pub field_type: String,
    pub analyzer: Option<String>,
    pub stored: bool,
    pub indexed: bool,
    pub searchable: bool,
    pub tenant_scoped: bool,
    pub feature_scoped: bool,
    pub tantivy_field: Option<String>,
    pub index_key_component: bool,
    pub compatibility: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexVersionPlanReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_key_schema: String,
    pub source_klepto_schema: String,
    pub implementation_schema: String,
    pub version: CodeKleptoIndexVersionPlanStatus,
    pub current_key: CodeKleptoPolymorphicIndexKey,
    pub registered_key: CodeKleptoRegisteredIndexKey,
    pub registered_hash_count: usize,
    pub same_registered_key: bool,
    pub hash_delta_count: usize,
    pub hash_deltas: Vec<CodeKleptoIndexVersionHashDelta>,
    pub recommended_action: String,
    pub recommended_bump: String,
    pub version_reason: String,
    pub registry_mutated: bool,
    pub write_requested: bool,
    pub overwrite_requested: bool,
    pub write_allowed: bool,
    pub index_mutated: bool,
    pub candidate_committed: bool,
    pub candidate_search_hits: usize,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexVersionPlanStatus {
    pub model: String,
    pub registry_model: String,
    pub source_key_model: String,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeKleptoRegisteredIndexKey {
    pub key_id: Option<String>,
    pub key_version: Option<String>,
    pub schema_hash: Option<String>,
    pub field_contract_hash: Option<String>,
    pub analyzer_hash: Option<String>,
    pub tenant_scope_hash: Option<String>,
    pub feature_binding_hash: Option<String>,
    pub document_shape_hash: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexVersionHashDelta {
    pub component: String,
    pub registered: String,
    pub current: String,
    pub bump_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryEntryReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_version_plan_schema: String,
    pub source_key_schema: String,
    pub source_klepto_schema: String,
    pub implementation_schema: String,
    pub registry: CodeKleptoIndexRegistryEntryStatus,
    pub current_key: CodeKleptoPolymorphicIndexKey,
    pub registered_key: CodeKleptoRegisteredIndexKey,
    pub entry: CodeKleptoIndexRegistryEntry,
    pub version_plan_action: String,
    pub version_plan_bump: String,
    pub version_plan_reason: String,
    pub same_registered_key: bool,
    pub hash_delta_count: usize,
    pub hash_deltas: Vec<CodeKleptoIndexVersionHashDelta>,
    pub registry_mutated: bool,
    pub write_requested: bool,
    pub overwrite_requested: bool,
    pub write_allowed: bool,
    pub index_mutated: bool,
    pub candidate_committed: bool,
    pub candidate_search_hits: usize,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryEntryStatus {
    pub model: String,
    pub source: String,
    pub entry_model: String,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryEntry {
    pub entry_id: String,
    pub key_id: String,
    pub key_version: String,
    pub previous_key_id: Option<String>,
    pub previous_key_version: Option<String>,
    pub source_key_schema: String,
    pub source_klepto_schema: String,
    pub implementation_schema: String,
    pub schema_hash: String,
    pub field_contract_hash: String,
    pub analyzer_hash: String,
    pub tenant_scope_hash: String,
    pub feature_binding_hash: String,
    pub document_shape_hash: String,
    pub recommended_action: String,
    pub recommended_bump: String,
    pub state: String,
    pub storage_mode: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryWriteReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub registry_path: String,
    pub registry_path_exists_before: bool,
    pub registry_path_exists_after: bool,
    pub source_registry_entry_schema: String,
    pub source_version_plan_schema: String,
    pub source_key_schema: String,
    pub registry: CodeKleptoIndexRegistryWriteStatus,
    pub entry: CodeKleptoIndexRegistryEntry,
    pub write_requested: bool,
    pub ack_write_registry: bool,
    pub overwrite_requested: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub entry_count_before: usize,
    pub entry_count_after: usize,
    pub written_entry_count: usize,
    pub registry_file_schema: String,
    pub registry_file_digest: String,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryWriteStatus {
    pub model: String,
    pub storage_model: String,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_write_flag: bool,
    pub requires_ack_flag: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryResolveReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub registry_path: String,
    pub registry_path_exists: bool,
    pub registry_file_schema: String,
    pub registry_file_digest: String,
    pub registry_entry_count: usize,
    pub source_version_plan_schema: String,
    pub source_key_schema: String,
    pub source_klepto_schema: String,
    pub implementation_schema: String,
    pub registry: CodeKleptoIndexRegistryResolveStatus,
    pub current_key: CodeKleptoPolymorphicIndexKey,
    pub matched_entry: Option<CodeKleptoIndexRegistryEntry>,
    pub matched_entry_id: Option<String>,
    pub matched_key_version: Option<String>,
    pub match_model: String,
    pub matched_registered_key: CodeKleptoRegisteredIndexKey,
    pub version_plan_action: String,
    pub version_plan_bump: String,
    pub version_plan_reason: String,
    pub same_registered_key: bool,
    pub hash_delta_count: usize,
    pub hash_deltas: Vec<CodeKleptoIndexVersionHashDelta>,
    pub write_requested: bool,
    pub ack_write_registry: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub candidate_committed: bool,
    pub candidate_search_hits: usize,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryResolveStatus {
    pub model: String,
    pub storage_model: String,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub resolve_model: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshPlanReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub registry_path: String,
    pub index_dir: String,
    pub source_registry_resolve_schema: String,
    pub source_disk_refresh_schema: String,
    pub plan: CodeKleptoIndexRegistryRefreshPlanStatus,
    pub registry_resolve: CodeKleptoIndexRegistryResolveReceipt,
    pub disk_refresh: CodeTantivyDiskRefreshPlanReceipt,
    pub registry_match_model: String,
    pub registry_version_action: String,
    pub registry_version_bump: String,
    pub same_registered_key: bool,
    pub hash_delta_count: usize,
    pub refresh_recommended: bool,
    pub refresh_reason: String,
    pub combined_action: String,
    pub combined_reason: String,
    pub write_requested: bool,
    pub ack_write_registry: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshPlanStatus {
    pub model: String,
    pub registry_model: String,
    pub refresh_model: String,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshExecPlanReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub registry_path: String,
    pub index_dir: String,
    pub source_refresh_plan_schema: String,
    pub execution: CodeKleptoIndexRegistryRefreshExecPlanStatus,
    pub refresh_plan: CodeKleptoIndexRegistryRefreshPlanReceipt,
    pub registry_version_action: String,
    pub registry_version_bump: String,
    pub refresh_recommended: bool,
    pub refresh_reason: String,
    pub combined_plan_action: String,
    pub execution_action: String,
    pub execution_reason: String,
    pub registry_write_required: bool,
    pub index_refresh_required: bool,
    pub write_requested: bool,
    pub ack_write_registry: bool,
    pub ack_write_index: bool,
    pub registry_ack_satisfied: bool,
    pub index_ack_satisfied: bool,
    pub execution_ready: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub execution_token: String,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshExecPlanStatus {
    pub model: String,
    pub source_plan_model: String,
    pub execution_model: String,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_registry_ack: bool,
    pub requires_index_ack: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshExecReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub registry_path: String,
    pub index_dir: String,
    pub source_exec_plan_schema: String,
    pub execution: CodeKleptoIndexRegistryRefreshExecStatus,
    pub exec_plan: CodeKleptoIndexRegistryRefreshExecPlanReceipt,
    pub execution_token: String,
    pub expected_execution_token: Option<String>,
    pub execution_token_matches: bool,
    pub plan_execution_action: String,
    pub execution_action: String,
    pub execution_reason: String,
    pub registry_write_required: bool,
    pub index_refresh_required: bool,
    pub execute_requested: bool,
    pub ack_execute_refresh: bool,
    pub execution_ack_satisfied: bool,
    pub plan_ready: bool,
    pub execution_permitted: bool,
    pub execution_performed: bool,
    pub dry_run: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub registry_path_exists_after: bool,
    pub index_dir_exists_after: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshExecStatus {
    pub model: String,
    pub source_plan_model: String,
    pub execution_model: String,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_execute_ack: bool,
    pub requires_execution_token_match: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshStageReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub staging_dir: String,
    pub registry_path: String,
    pub index_dir: String,
    pub source_exec_receipt_schema: String,
    pub staging: CodeKleptoIndexRegistryRefreshStageStatus,
    pub exec_receipt: CodeKleptoIndexRegistryRefreshExecReceipt,
    pub staging_id: String,
    pub staging_action: String,
    pub staging_reason: String,
    pub stage_requested: bool,
    pub ack_stage_refresh: bool,
    pub execution_permitted: bool,
    pub staging_allowed: bool,
    pub staging_performed: bool,
    pub staging_dir_exists_before: bool,
    pub staging_dir_exists_after: bool,
    pub staged_manifest_path: Option<String>,
    pub staged_exec_receipt_path: Option<String>,
    pub staged_registry_candidate_path: Option<String>,
    pub staged_index_marker_path: Option<String>,
    pub staged_file_count: usize,
    pub live_registry_path_exists_after: bool,
    pub live_index_dir_exists_after: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshStageStatus {
    pub model: String,
    pub source_model: String,
    pub staging_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_stage_ack: bool,
    pub notes: Vec<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshStageVerifyReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub staging_dir_exists: bool,
    pub verification: CodeKleptoIndexRegistryRefreshStageVerifyStatus,
    pub expected_execution_token: Option<String>,
    pub discovered_stage_count: usize,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub manifest_path: Option<String>,
    pub exec_receipt_path: Option<String>,
    pub registry_candidate_path: Option<String>,
    pub index_marker_path: Option<String>,
    pub manifest_schema_ok: bool,
    pub exec_receipt_schema_ok: bool,
    pub registry_candidate_schema_ok: bool,
    pub index_marker_schema_ok: bool,
    pub execution_token: Option<String>,
    pub manifest_execution_token: Option<String>,
    pub exec_receipt_execution_token: Option<String>,
    pub execution_token_matches: bool,
    pub expected_execution_token_matches: bool,
    pub staged_file_count_manifest: Option<usize>,
    pub staged_file_count_observed: usize,
    pub staged_file_count_matches: bool,
    pub registry_candidate_required: bool,
    pub index_marker_required: bool,
    pub verified: bool,
    pub verification_action: String,
    pub verification_reason: String,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshStageVerifyStatus {
    pub model: String,
    pub source_model: String,
    pub verification_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshSwapPlanReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_stage_verify_schema: String,
    pub swap: CodeKleptoIndexRegistryRefreshSwapPlanStatus,
    pub stage_verify: CodeKleptoIndexRegistryRefreshStageVerifyReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub execution_token: Option<String>,
    pub swap_plan_id: String,
    pub swap_action: String,
    pub swap_reason: String,
    pub swap_ready: bool,
    pub registry_swap_required: bool,
    pub index_swap_required: bool,
    pub required_artifacts: Vec<String>,
    pub verified: bool,
    pub expected_execution_token_matches: bool,
    pub staging_dir_exists: bool,
    pub discovered_stage_count: usize,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshSwapPlanStatus {
    pub model: String,
    pub source_model: String,
    pub swap_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_verified_stage: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshSwapExecReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_swap_plan_schema: String,
    pub swap_execution: CodeKleptoIndexRegistryRefreshSwapExecStatus,
    pub swap_plan: CodeKleptoIndexRegistryRefreshSwapPlanReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub execution_token: Option<String>,
    pub swap_plan_id: String,
    pub expected_execution_token: Option<String>,
    pub expected_swap_plan_id: Option<String>,
    pub execution_token_matches: bool,
    pub swap_plan_id_matches: bool,
    pub swap_requested: bool,
    pub ack_swap: bool,
    pub swap_ack_satisfied: bool,
    pub plan_ready: bool,
    pub swap_action: String,
    pub swap_reason: String,
    pub registry_swap_required: bool,
    pub index_swap_required: bool,
    pub swap_permitted: bool,
    pub swap_performed: bool,
    pub dry_run: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshSwapExecStatus {
    pub model: String,
    pub source_model: String,
    pub execution_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_swap_ack: bool,
    pub requires_execution_token_match: bool,
    pub requires_swap_plan_id_match: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshApplyPlanReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_swap_exec_schema: String,
    pub apply: CodeKleptoIndexRegistryRefreshApplyPlanStatus,
    pub swap_exec: CodeKleptoIndexRegistryRefreshSwapExecReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub execution_token: Option<String>,
    pub swap_plan_id: String,
    pub apply_plan_id: String,
    pub apply_action: String,
    pub apply_reason: String,
    pub apply_ready: bool,
    pub registry_apply_required: bool,
    pub index_apply_required: bool,
    pub required_source_artifacts: Vec<String>,
    pub required_live_mutations: Vec<String>,
    pub swap_permitted: bool,
    pub swap_performed: bool,
    pub dry_run: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshApplyPlanStatus {
    pub model: String,
    pub source_model: String,
    pub apply_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_swap_execution_receipt: bool,
    pub requires_permitted_swap: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshApplyExecReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_apply_plan_schema: String,
    pub apply_execution: CodeKleptoIndexRegistryRefreshApplyExecStatus,
    pub apply_plan: CodeKleptoIndexRegistryRefreshApplyPlanReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub execution_token: Option<String>,
    pub swap_plan_id: String,
    pub apply_plan_id: String,
    pub expected_apply_plan_id: Option<String>,
    pub apply_plan_id_matches: bool,
    pub apply_requested: bool,
    pub ack_apply: bool,
    pub apply_ack_satisfied: bool,
    pub plan_ready: bool,
    pub apply_action: String,
    pub apply_reason: String,
    pub registry_apply_required: bool,
    pub index_apply_required: bool,
    pub required_source_artifacts: Vec<String>,
    pub required_live_mutations: Vec<String>,
    pub apply_permitted: bool,
    pub apply_performed: bool,
    pub dry_run: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshApplyExecStatus {
    pub model: String,
    pub source_model: String,
    pub execution_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_apply_ack: bool,
    pub requires_apply_plan_id_match: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyRequestReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_apply_exec_schema: String,
    pub live_apply_request: CodeKleptoIndexRegistryRefreshLiveApplyRequestStatus,
    pub apply_exec: CodeKleptoIndexRegistryRefreshApplyExecReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub execution_token: Option<String>,
    pub swap_plan_id: String,
    pub apply_plan_id: String,
    pub live_apply_request_id: String,
    pub request_live_apply: bool,
    pub ack_request_live_apply: bool,
    pub live_apply_ack_satisfied: bool,
    pub apply_permitted: bool,
    pub apply_performed: bool,
    pub request_action: String,
    pub request_reason: String,
    pub registry_apply_required: bool,
    pub index_apply_required: bool,
    pub required_source_artifacts: Vec<String>,
    pub required_live_mutations: Vec<String>,
    pub live_apply_request_permitted: bool,
    pub live_apply_request_recorded: bool,
    pub live_apply_performed: bool,
    pub dry_run: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyRequestStatus {
    pub model: String,
    pub source_model: String,
    pub request_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_apply_execution_receipt: bool,
    pub requires_apply_permitted: bool,
    pub requires_live_apply_ack: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshApplyExecuteOptions {
    pub(crate) execute_apply: bool,
    pub(crate) ack_execute_apply: bool,
    pub(crate) expected_apply_plan_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshLiveApplyRequestOptions {
    pub(crate) request_live_apply: bool,
    pub(crate) ack_request_live_apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyPreflightReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_live_apply_request_schema: String,
    pub live_apply_preflight: CodeKleptoIndexRegistryRefreshLiveApplyPreflightStatus,
    pub live_apply_request: CodeKleptoIndexRegistryRefreshLiveApplyRequestReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub execution_token: Option<String>,
    pub swap_plan_id: String,
    pub apply_plan_id: String,
    pub live_apply_request_id: String,
    pub expected_live_apply_request_id: Option<String>,
    pub live_apply_request_id_matches: bool,
    pub preflight_live_apply: bool,
    pub ack_preflight_live_apply: bool,
    pub live_apply_preflight_ack_satisfied: bool,
    pub live_apply_request_recorded: bool,
    pub live_apply_performed: bool,
    pub preflight_action: String,
    pub preflight_reason: String,
    pub registry_apply_required: bool,
    pub index_apply_required: bool,
    pub required_source_artifacts: Vec<String>,
    pub required_live_mutations: Vec<String>,
    pub live_apply_preflight_permitted: bool,
    pub live_apply_preflight_ready: bool,
    pub dry_run: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyPreflightStatus {
    pub model: String,
    pub source_model: String,
    pub preflight_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_live_apply_request_receipt: bool,
    pub requires_live_apply_request_recorded: bool,
    pub requires_live_apply_request_id_match: bool,
    pub requires_preflight_ack: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshLiveApplyPreflightOptions {
    pub(crate) preflight_live_apply: bool,
    pub(crate) ack_preflight_live_apply: bool,
    pub(crate) expected_live_apply_request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_live_apply_preflight_schema: String,
    pub live_apply_commit_plan: CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanStatus,
    pub live_apply_preflight: CodeKleptoIndexRegistryRefreshLiveApplyPreflightReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub execution_token: Option<String>,
    pub swap_plan_id: String,
    pub apply_plan_id: String,
    pub live_apply_request_id: String,
    pub expected_live_apply_request_id: Option<String>,
    pub live_apply_request_id_matches: bool,
    pub live_commit_plan_id: String,
    pub plan_live_commit: bool,
    pub ack_plan_live_commit: bool,
    pub live_commit_plan_ack_satisfied: bool,
    pub live_apply_request_recorded: bool,
    pub live_apply_preflight_ready: bool,
    pub live_apply_performed: bool,
    pub live_commit_action: String,
    pub live_commit_reason: String,
    pub registry_apply_required: bool,
    pub index_apply_required: bool,
    pub required_source_artifacts: Vec<String>,
    pub required_preflight_artifacts: Vec<String>,
    pub required_live_mutations: Vec<String>,
    pub future_live_locks: Vec<String>,
    pub future_backup_paths: Vec<String>,
    pub future_commit_journal_path: Option<String>,
    pub live_commit_plan_permitted: bool,
    pub live_commit_plan_ready: bool,
    pub live_commit_performed: bool,
    pub dry_run: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanStatus {
    pub model: String,
    pub source_model: String,
    pub commit_plan_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_live_apply_preflight_receipt: bool,
    pub requires_live_apply_preflight_ready: bool,
    pub requires_live_apply_request_id_match: bool,
    pub requires_commit_plan_ack: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanOptions {
    pub(crate) plan_live_commit: bool,
    pub(crate) ack_plan_live_commit: bool,
    pub(crate) expected_live_apply_request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitExecReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_live_apply_commit_plan_schema: String,
    pub live_apply_commit_exec: CodeKleptoIndexRegistryRefreshLiveApplyCommitExecStatus,
    pub live_apply_commit_plan: CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub execution_token: Option<String>,
    pub swap_plan_id: String,
    pub apply_plan_id: String,
    pub live_apply_request_id: String,
    pub live_commit_plan_id: String,
    pub expected_live_commit_plan_id: Option<String>,
    pub live_commit_plan_id_matches: bool,
    pub live_commit_execution_id: String,
    pub execute_live_commit: bool,
    pub ack_execute_live_commit: bool,
    pub live_commit_execution_ack_satisfied: bool,
    pub live_apply_request_recorded: bool,
    pub live_apply_preflight_ready: bool,
    pub live_commit_plan_ready: bool,
    pub live_commit_action: String,
    pub live_commit_reason: String,
    pub registry_apply_required: bool,
    pub index_apply_required: bool,
    pub required_source_artifacts: Vec<String>,
    pub required_preflight_artifacts: Vec<String>,
    pub required_live_mutations: Vec<String>,
    pub future_live_locks: Vec<String>,
    pub future_backup_paths: Vec<String>,
    pub future_commit_journal_path: Option<String>,
    pub live_commit_execution_permitted: bool,
    pub live_commit_performed: bool,
    pub dry_run: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitExecStatus {
    pub model: String,
    pub source_model: String,
    pub execution_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_live_commit_plan_receipt: bool,
    pub requires_live_commit_plan_ready: bool,
    pub requires_live_commit_plan_id_match: bool,
    pub requires_execute_ack: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshLiveApplyCommitExecOptions {
    pub(crate) execute_live_commit: bool,
    pub(crate) ack_execute_live_commit: bool,
    pub(crate) expected_live_commit_plan_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitStageReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_live_apply_commit_exec_schema: String,
    pub live_apply_commit_stage: CodeKleptoIndexRegistryRefreshLiveApplyCommitStageStatus,
    pub live_apply_commit_exec: CodeKleptoIndexRegistryRefreshLiveApplyCommitExecReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub execution_token: Option<String>,
    pub swap_plan_id: String,
    pub apply_plan_id: String,
    pub live_apply_request_id: String,
    pub live_commit_plan_id: String,
    pub live_commit_execution_id: String,
    pub expected_live_commit_execution_id: Option<String>,
    pub live_commit_execution_id_matches: bool,
    pub stage_live_commit: bool,
    pub ack_stage_live_commit: bool,
    pub live_commit_stage_ack_satisfied: bool,
    pub live_commit_execution_permitted: bool,
    pub live_commit_stage_action: String,
    pub live_commit_stage_reason: String,
    pub registry_apply_required: bool,
    pub index_apply_required: bool,
    pub required_source_artifacts: Vec<String>,
    pub required_preflight_artifacts: Vec<String>,
    pub required_live_mutations: Vec<String>,
    pub future_live_locks: Vec<String>,
    pub future_backup_paths: Vec<String>,
    pub future_commit_journal_path: Option<String>,
    pub staged_commit_files: Vec<String>,
    pub staged_commit_file_count: usize,
    pub live_commit_stage_permitted: bool,
    pub live_commit_stage_ready: bool,
    pub live_commit_stage_performed: bool,
    pub dry_run: bool,
    pub staging_write_allowed: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub live_commit_performed: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitStageStatus {
    pub model: String,
    pub source_model: String,
    pub stage_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_live_commit_exec_receipt: bool,
    pub requires_live_commit_execution_permitted: bool,
    pub requires_live_commit_execution_id_match: bool,
    pub requires_stage_ack: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshLiveApplyCommitStageOptions {
    pub(crate) stage_live_commit: bool,
    pub(crate) ack_stage_live_commit: bool,
    pub(crate) expected_live_commit_execution_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub staging_dir_exists: bool,
    pub verification: CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyStatus,
    pub expected_live_commit_execution_id: Option<String>,
    pub discovered_stage_count: usize,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub stage_manifest_path: Option<String>,
    pub live_commit_exec_receipt_path: Option<String>,
    pub live_commit_journal_path: Option<String>,
    pub manifest_schema_ok: bool,
    pub commit_exec_receipt_schema_ok: bool,
    pub commit_journal_schema_ok: bool,
    pub live_commit_execution_id: Option<String>,
    pub manifest_live_commit_execution_id: Option<String>,
    pub exec_live_commit_execution_id: Option<String>,
    pub journal_live_commit_execution_id: Option<String>,
    pub live_commit_execution_id_matches: bool,
    pub expected_live_commit_execution_id_matches: bool,
    pub staged_commit_file_count_manifest: Option<usize>,
    pub staged_commit_file_count_observed: usize,
    pub staged_commit_file_count_matches: bool,
    pub live_commit_stage_ready: bool,
    pub verified: bool,
    pub verification_action: String,
    pub verification_reason: String,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub live_commit_performed: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyStatus {
    pub model: String,
    pub source_model: String,
    pub verification_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyOptions {
    pub(crate) expected_live_commit_execution_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitGateReceipt {
    pub schema: String,
    pub staging_dir: String,
    pub source_live_commit_stage_verify_schema: String,
    pub gate: CodeKleptoIndexRegistryRefreshLiveApplyCommitGateStatus,
    pub live_commit_stage_verify: CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyReceipt,
    pub selected_stage_id: Option<String>,
    pub selected_stage_path: Option<String>,
    pub stage_manifest_path: Option<String>,
    pub live_commit_exec_receipt_path: Option<String>,
    pub live_commit_journal_path: Option<String>,
    pub live_commit_execution_id: Option<String>,
    pub expected_live_commit_execution_id: Option<String>,
    pub live_commit_execution_id_matches: bool,
    pub expected_live_commit_execution_id_matches: bool,
    pub staged_commit_file_count_observed: usize,
    pub stage_verified: bool,
    pub open_live_commit_gate: bool,
    pub ack_open_live_commit_gate: bool,
    pub live_commit_gate_ack_satisfied: bool,
    pub live_commit_gate_id: Option<String>,
    pub live_commit_gate_action: String,
    pub live_commit_gate_reason: String,
    pub required_verified_artifacts: Vec<String>,
    pub required_future_writer_inputs: Vec<String>,
    pub live_commit_gate_ready: bool,
    pub dry_run: bool,
    pub write_allowed: bool,
    pub registry_mutated: bool,
    pub index_mutated: bool,
    pub live_commit_performed: bool,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryRefreshLiveApplyCommitGateStatus {
    pub model: String,
    pub source_model: String,
    pub gate_model: String,
    pub mutates_staging: bool,
    pub mutates_registry: bool,
    pub mutates_index: bool,
    pub requires_verified_commit_stage: bool,
    pub requires_live_commit_execution_id_match: bool,
    pub requires_gate_ack: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshLiveApplyCommitGateOptions {
    pub(crate) open_live_commit_gate: bool,
    pub(crate) ack_open_live_commit_gate: bool,
    pub(crate) expected_live_commit_execution_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshStageVerifyOptions {
    pub(crate) expected_execution_token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshSwapExecuteOptions {
    pub(crate) execute_swap: bool,
    pub(crate) ack_execute_swap: bool,
    pub(crate) expected_execution_token: Option<String>,
    pub(crate) expected_swap_plan_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshExecOptions {
    pub(crate) ack_write_index: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshExecuteOptions {
    pub(crate) execute_refresh: bool,
    pub(crate) ack_execute_refresh: bool,
    pub(crate) expected_execution_token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryRefreshStageOptions {
    pub(crate) stage_refresh: bool,
    pub(crate) ack_stage_refresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKleptoIndexRegistryFile {
    pub schema: String,
    pub registry_version: String,
    pub storage_model: String,
    pub entry_count: usize,
    pub latest_entry_id: Option<String>,
    pub entries: Vec<CodeKleptoIndexRegistryEntry>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexVersionOptions {
    pub(crate) registered_key: CodeKleptoRegisteredIndexKey,
}


#[derive(Debug, Clone, Default)]
pub(crate) struct CodeKleptoIndexRegistryWriteOptions {
    pub(crate) write_registry: bool,
    pub(crate) ack_write_registry: bool,
    pub(crate) overwrite_registry: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyIndexReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_field_plan_schema: String,
    pub tenant_scope: CodeSearchTenantScope,
    pub analyzer: CodeSearchAnalyzerPlan,
    pub index: CodeTantivyIndexStatus,
    pub writer_memory: CodeSearchWriterMemoryPlan,
    pub schema_fields: Vec<CodeTantivySchemaFieldReceipt>,
    pub document_count: usize,
    pub committed_document_count: usize,
    pub search_check: CodeTantivySearchCheck,
    pub indexed_documents: Vec<CodeTantivyIndexedDocumentReceipt>,
    pub summary: CodeGraphSummary,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyIndexStatus {
    pub engine: String,
    pub tantivy_used: bool,
    pub index_kind: String,
    pub index_dir: Option<String>,
    pub write_requested: bool,
    pub index_written: bool,
    pub overwrite_requested: bool,
    pub commit_succeeded: bool,
    pub tokenizer_registered: String,
    pub tenant_filter_model: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivySchemaFieldReceipt {
    pub name: String,
    pub field_type: String,
    pub stored: bool,
    pub indexed: bool,
    pub tokenizer: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivySearchCheck {
    pub query: String,
    pub top_k: usize,
    pub hit_count: usize,
    pub top_hits: Vec<CodeTantivySearchHit>,
    pub query_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivySearchHit {
    pub rank: usize,
    pub score: f32,
    pub segment_ord: u32,
    pub doc_id: u32,
    pub source_document_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyIndexedDocumentReceipt {
    pub id: String,
    pub pin_id: String,
    pub kind: String,
    pub label: String,
    pub path: String,
    pub tenant_id: String,
    pub namespace: String,
    pub workspace_id: String,
    pub project_id: String,
    pub visibility: String,
    pub indexed_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyQueryReceipt {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_index_receipt_schema: String,
    pub tenant_scope: CodeSearchTenantScope,
    pub analyzer: CodeSearchAnalyzerPlan,
    pub query: String,
    pub top_k: usize,
    pub hit_count: usize,
    pub index: CodeTantivyQueryIndexStatus,
    pub tenant_filter: CodeTantivyTenantQueryFilter,
    pub query_fields: Vec<String>,
    pub hits: Vec<CodeTantivyHydratedHit>,
    pub indexed_document_count: usize,
    pub indexed_documents: Vec<CodeTantivyIndexedDocumentReceipt>,
    pub summary: CodeGraphSummary,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskQueryReceipt {
    pub schema: String,
    pub index_dir: String,
    pub index_exists: bool,
    pub query: String,
    pub top_k: usize,
    pub hit_count: usize,
    pub index: CodeTantivyQueryIndexStatus,
    pub tenant_filter: CodeTantivyTenantQueryFilter,
    pub query_fields: Vec<String>,
    pub hits: Vec<CodeTantivyHydratedHit>,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskStatusReceipt {
    pub schema: String,
    pub index_dir: String,
    pub index_exists: bool,
    pub index: CodeTantivyDiskStatus,
    pub document_count: usize,
    pub schema_field_count: usize,
    pub schema_fields: Vec<CodeTantivyDiskSchemaFieldStatus>,
    pub query_fields: Vec<String>,
    pub tenant_scope_count: usize,
    pub tenant_scopes: Vec<CodeTantivyDiskTenantScopeStatus>,
    pub kind_counts: BTreeMap<String, usize>,
    pub stored_field_names: Vec<String>,
    pub sample_document_count: usize,
    pub sample_documents: Vec<CodeTantivyIndexedDocumentReceipt>,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskManifestReceipt {
    pub schema: String,
    pub index_dir: String,
    pub index_exists: bool,
    pub status_schema: String,
    pub index: CodeTantivyDiskManifestStatus,
    pub document_count: usize,
    pub tenant_scope_count: usize,
    pub tenant_scopes: Vec<CodeTantivyDiskTenantScopeStatus>,
    pub schema_field_count: usize,
    pub query_fields: Vec<String>,
    pub stored_field_names: Vec<String>,
    pub file_count: usize,
    pub listed_file_count: usize,
    pub total_bytes: u64,
    pub manifest_digest: String,
    pub hash_algorithm: String,
    pub files: Vec<CodeTantivyDiskManifestFile>,
    pub sample_document_count: usize,
    pub sample_documents: Vec<CodeTantivyIndexedDocumentReceipt>,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskManifestStatus {
    pub engine: String,
    pub tantivy_used: bool,
    pub opened_read_only: bool,
    pub mutates_index: bool,
    pub hash_algorithm: String,
    pub digest_model: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskManifestFile {
    pub relative_path: String,
    pub byte_count: u64,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskCompareReceipt {
    pub schema: String,
    pub left_index_dir: String,
    pub right_index_dir: String,
    pub left_exists: bool,
    pub right_exists: bool,
    pub index: CodeTantivyDiskCompareStatus,
    pub left_manifest_schema: String,
    pub right_manifest_schema: String,
    pub left_manifest_digest: String,
    pub right_manifest_digest: String,
    pub same_manifest_digest: bool,
    pub same_document_count: bool,
    pub same_tenant_scopes: bool,
    pub same_schema_field_count: bool,
    pub same_stored_fields: bool,
    pub left_document_count: usize,
    pub right_document_count: usize,
    pub left_tenant_scope_count: usize,
    pub right_tenant_scope_count: usize,
    pub left_schema_field_count: usize,
    pub right_schema_field_count: usize,
    pub left_file_count: usize,
    pub right_file_count: usize,
    pub left_total_bytes: u64,
    pub right_total_bytes: u64,
    pub byte_delta: i128,
    pub common_file_count: usize,
    pub added_files: Vec<CodeTantivyDiskManifestFile>,
    pub removed_files: Vec<CodeTantivyDiskManifestFile>,
    pub changed_files: Vec<CodeTantivyDiskCompareChangedFile>,
    pub tenant_scope_deltas: Vec<CodeTantivyDiskCompareTenantDelta>,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskCompareStatus {
    pub engine: String,
    pub tantivy_used: bool,
    pub opened_read_only: bool,
    pub mutates_index: bool,
    pub compare_model: String,
    pub hash_algorithm: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskCompareChangedFile {
    pub relative_path: String,
    pub left_byte_count: u64,
    pub right_byte_count: u64,
    pub left_digest: String,
    pub right_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskCompareTenantDelta {
    pub tenant_key: String,
    pub left_document_count: usize,
    pub right_document_count: usize,
    pub left_kind_counts: BTreeMap<String, usize>,
    pub right_kind_counts: BTreeMap<String, usize>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskRefreshPlanReceipt {
    pub schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub index_dir: String,
    pub index_exists: bool,
    pub index: CodeTantivyDiskRefreshPlanStatus,
    pub existing_manifest_schema: String,
    pub candidate_index_schema: String,
    pub existing_manifest_digest: String,
    pub existing_document_count: usize,
    pub candidate_document_count: usize,
    pub existing_tenant_scope_count: usize,
    pub candidate_tenant_scope_count: usize,
    pub existing_schema_field_count: usize,
    pub candidate_schema_field_count: usize,
    pub existing_stored_fields: Vec<String>,
    pub candidate_stored_fields: Vec<String>,
    pub same_document_count: bool,
    pub same_tenant_scopes: bool,
    pub same_schema_field_count: bool,
    pub same_stored_fields: bool,
    pub refresh_recommended: bool,
    pub refresh_reason: String,
    pub write_requested: bool,
    pub overwrite_requested: bool,
    pub write_allowed: bool,
    pub index_mutated: bool,
    pub candidate_committed: bool,
    pub candidate_search_hits: usize,
    pub tenant_scope_deltas: Vec<CodeTantivyDiskCompareTenantDelta>,
    pub existing_findings: Vec<FindingNode>,
    pub candidate_findings: Vec<FindingNode>,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskRefreshPlanStatus {
    pub engine: String,
    pub tantivy_used: bool,
    pub existing_opened_read_only: bool,
    pub candidate_index_kind: String,
    pub mutates_index: bool,
    pub refresh_model: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskStatus {
    pub engine: String,
    pub tantivy_used: bool,
    pub opened_read_only: bool,
    pub source_index_kind: String,
    pub mutates_index: bool,
    pub tenant_filter_model: String,
    pub hydration_model: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskSchemaFieldStatus {
    pub name: String,
    pub field_ord: u32,
    pub field_type: String,
    pub queryable_by_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyDiskTenantScopeStatus {
    pub tenant_id: String,
    pub namespace: String,
    pub workspace_id: String,
    pub project_id: String,
    pub visibility: String,
    pub document_count: usize,
    pub kind_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyQueryIndexStatus {
    pub engine: String,
    pub tantivy_used: bool,
    pub source_index_kind: String,
    pub disk_write_allowed: bool,
    pub disk_write_requested: bool,
    pub disk_write_ignored: bool,
    pub commit_succeeded: bool,
    pub tenant_filter_model: String,
    pub hydration_model: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyTenantQueryFilter {
    pub enforced: bool,
    pub mode: String,
    pub tenant_id: Option<String>,
    pub namespace: Option<String>,
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub visibility: Option<String>,
    pub exact_fields: Vec<String>,
    pub accepted_hit_count: usize,
    pub rejected_hit_count: usize,
    pub source: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeTantivyHydratedHit {
    pub rank: usize,
    pub score: f32,
    pub segment_ord: u32,
    pub tantivy_doc_id: u32,
    pub source_document_id: Option<String>,
    pub document: Option<CodeTantivyIndexedDocumentReceipt>,
}

#[derive(Debug, Clone)]
pub(crate) struct CodeTantivyIndexOptions {
    pub(crate) index_dir: Option<String>,
    pub(crate) write_index: bool,
    pub(crate) overwrite_index: bool,
    pub(crate) search_query: Option<String>,
    pub(crate) top_k: usize,
    pub(crate) filter_tenant_id: Option<String>,
    pub(crate) filter_namespace: Option<String>,
    pub(crate) filter_workspace_id: Option<String>,
    pub(crate) filter_project_id: Option<String>,
    pub(crate) filter_visibility: Option<String>,
    pub(crate) disable_tenant_filter: bool,
}

impl Default for CodeTantivyIndexOptions {
    fn default() -> Self {
        Self {
            index_dir: None,
            write_index: false,
            overwrite_index: false,
            search_query: None,
            top_k: 10,
            filter_tenant_id: None,
            filter_namespace: None,
            filter_workspace_id: None,
            filter_project_id: None,
            filter_visibility: None,
            disable_tenant_filter: false,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CodeTantivyDiskStatusOptions {
    pub(crate) sample_limit: usize,
    pub(crate) max_files: usize,
}

impl Default for CodeTantivyDiskStatusOptions {
    fn default() -> Self {
        Self { sample_limit: 5, max_files: 128 }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CodeKleptoFieldPlanOptions {
    pub(crate) requested_fields: Vec<String>,
    pub(crate) clear_default_fields: bool,
    pub(crate) filters: Vec<String>,
    pub(crate) target_feature: Option<String>,
    pub(crate) checks: Vec<String>,
}

impl Default for CodeKleptoFieldPlanOptions {
    fn default() -> Self {
        Self {
            requested_fields: Vec::new(),
            clear_default_fields: false,
            filters: vec![
                "lowercase".to_string(),
                "remove_long_tokens_40".to_string(),
                "code_path_boundaries".to_string(),
            ],
            target_feature: None,
            checks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CodeSearchPlanOptions {
    pub(crate) tenant_id: String,
    pub(crate) namespace: String,
    pub(crate) workspace_id: String,
    pub(crate) project_id: String,
    pub(crate) visibility: String,
    pub(crate) extraction_mode: String,
    pub(crate) role: String,
    pub(crate) language: String,
    pub(crate) tokenizer: String,
    pub(crate) requested_writer_memory: Option<usize>,
    pub(crate) estimated_doc_count_override: Option<usize>,
}

impl Default for CodeSearchPlanOptions {
    fn default() -> Self {
        Self {
            tenant_id: "default".to_string(),
            namespace: "codepassenger".to_string(),
            workspace_id: "workspace".to_string(),
            project_id: "project".to_string(),
            visibility: "private".to_string(),
            extraction_mode: "preview".to_string(),
            role: "developer".to_string(),
            language: "english".to_string(),
            tokenizer: "code_default".to_string(),
            requested_writer_memory: None,
            estimated_doc_count_override: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEditPlan {
    pub schema: String,
    pub graph_schema: String,
    pub root: String,
    pub root_exists: bool,
    pub max_depth: usize,
    pub source_query: CodeQuerySpec,
    pub context_options: CodeContextOptions,
    pub policy: CodeEditPlanPolicy,
    pub safety: CodeEditSafety,
    pub candidate_count: usize,
    pub candidates: Vec<CodeEditCandidate>,
    pub summary: CodeGraphSummary,
    pub findings: Vec<FindingNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEditPlanPolicy {
    pub mode: String,
    pub intent: String,
    pub operation: String,
    pub target_feature: Option<String>,
    pub risk_budget: String,
    pub write_requested: bool,
    pub writes_allowed: bool,
    pub requires_ack_before_apply: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEditSafety {
    pub writes: bool,
    pub applies_patches: bool,
    pub shell_execution: bool,
    pub requires_human_review: bool,
    pub unsupported_targets: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEditCandidate {
    pub id: String,
    pub pin_id: String,
    pub source_result_id: String,
    pub kind: String,
    pub label: String,
    pub crate_name: Option<String>,
    pub path: String,
    pub anchor_line: u32,
    pub anchor_column: u32,
    pub scope: Option<String>,
    pub dep: Option<String>,
    pub feature_candidates: Vec<String>,
    pub operation: String,
    pub rationale: String,
    pub preconditions: Vec<String>,
    pub expected_effects: Vec<String>,
    pub context: CodeEditCandidateContext,
    pub proposed_change: CodeProposedChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEditCandidateContext {
    pub context_id: String,
    pub copy_hint: String,
    pub byte_count: usize,
    pub estimated_tokens: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeProposedChange {
    pub id: String,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub patch_kind: String,
    pub current_preview: String,
    pub proposed_preview: String,
    pub apply_command: String,
    pub write_allowed: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CodeEditPlanOptions {
    pub(crate) intent: String,
    pub(crate) operation: String,
    pub(crate) target_feature: Option<String>,
    pub(crate) risk_budget: String,
    pub(crate) write_requested: bool,
}

impl Default for CodeEditPlanOptions {
    fn default() -> Self {
        Self {
            intent: "manual_review".to_string(),
            operation: "context_review".to_string(),
            target_feature: None,
            risk_budget: "low".to_string(),
            write_requested: false,
        }
    }
}


