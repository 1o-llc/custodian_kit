//! Flag parsing for the index, registry and live-apply commands.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_klepto_index_version_args(args: &[String]) -> (CodeKleptoIndexVersionOptions, Vec<String>) {
    let mut options = CodeKleptoIndexVersionOptions::default();
    options.registered_key.source = "none".to_string();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--registered-key" | "--registered-key-id" | "--previous-key" => {
                if let Some(value) = args.get(idx + 1) {
                    options.registered_key.key_id = Some(value.clone());
                    options.registered_key.source = "version_flags".to_string();
                    idx += 2;
                    continue;
                }
            }
            "--registered-version" | "--registered-key-version" | "--previous-version" => {
                if let Some(value) = args.get(idx + 1) {
                    options.registered_key.key_version = Some(value.clone());
                    options.registered_key.source = "version_flags".to_string();
                    idx += 2;
                    continue;
                }
            }
            "--registered-schema-hash" => {
                if let Some(value) = args.get(idx + 1) {
                    options.registered_key.schema_hash = Some(value.clone());
                    options.registered_key.source = "version_flags".to_string();
                    idx += 2;
                    continue;
                }
            }
            "--registered-field-contract-hash" | "--registered-field-hash" => {
                if let Some(value) = args.get(idx + 1) {
                    options.registered_key.field_contract_hash = Some(value.clone());
                    options.registered_key.source = "version_flags".to_string();
                    idx += 2;
                    continue;
                }
            }
            "--registered-analyzer-hash" => {
                if let Some(value) = args.get(idx + 1) {
                    options.registered_key.analyzer_hash = Some(value.clone());
                    options.registered_key.source = "version_flags".to_string();
                    idx += 2;
                    continue;
                }
            }
            "--registered-tenant-scope-hash" | "--registered-tenant-hash" => {
                if let Some(value) = args.get(idx + 1) {
                    options.registered_key.tenant_scope_hash = Some(value.clone());
                    options.registered_key.source = "version_flags".to_string();
                    idx += 2;
                    continue;
                }
            }
            "--registered-feature-binding-hash" | "--registered-feature-hash" => {
                if let Some(value) = args.get(idx + 1) {
                    options.registered_key.feature_binding_hash = Some(value.clone());
                    options.registered_key.source = "version_flags".to_string();
                    idx += 2;
                    continue;
                }
            }
            "--registered-document-shape-hash" | "--registered-doc-shape-hash" => {
                if let Some(value) = args.get(idx + 1) {
                    options.registered_key.document_shape_hash = Some(value.clone());
                    options.registered_key.source = "version_flags".to_string();
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


pub(crate) fn parse_klepto_index_registry_write_args(args: &[String]) -> (CodeKleptoIndexRegistryWriteOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryWriteOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--write-registry" | "--persist-registry" => {
                options.write_registry = true;
                idx += 1;
                continue;
            }
            "--ack-write-registry" | "--ack-registry-write" | "--yes-write-registry" => {
                options.ack_write_registry = true;
                idx += 1;
                continue;
            }
            "--overwrite-registry" | "--replace-registry" => {
                options.overwrite_registry = true;
                idx += 1;
                continue;
            }
            _ => {}
        }
        passthrough.push(args[idx].clone());
        idx += 1;
    }
    (options, passthrough)
}





pub(crate) fn parse_klepto_index_registry_refresh_exec_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshExecOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshExecOptions::default();
    let mut passthrough = Vec::<String>::new();
    for arg in args {
        match arg.as_str() {
            "--ack-write-index" | "--ack-index-refresh" | "--ack-refresh-index" | "--yes-write-index" => {
                options.ack_write_index = true;
            }
            _ => passthrough.push(arg.clone()),
        }
    }
    (options, passthrough)
}

pub(crate) fn parse_klepto_index_registry_refresh_execute_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshExecuteOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshExecuteOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--execute-refresh" | "--execute" | "--run-refresh" | "--apply-refresh" => {
                options.execute_refresh = true;
                idx += 1;
                continue;
            }
            "--ack-execute-refresh" | "--ack-execute" | "--ack-execution" | "--ack-refresh-execution" => {
                options.ack_execute_refresh = true;
                idx += 1;
                continue;
            }
            "--expected-execution-token" | "--execution-token" | "--expect-token" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_execution_token = Some(value.clone());
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

pub(crate) fn parse_klepto_index_registry_refresh_stage_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshStageOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshStageOptions::default();
    let mut passthrough = Vec::<String>::new();
    for arg in args {
        match arg.as_str() {
            "--stage-refresh" | "--stage" | "--stage-execution" | "--stage-refresh-execution" => {
                options.stage_refresh = true;
            }
            "--ack-stage-refresh" | "--ack-stage" | "--ack-staging" | "--ack-refresh-stage" => {
                options.ack_stage_refresh = true;
            }
            _ => passthrough.push(arg.clone()),
        }
    }
    (options, passthrough)
}


pub(crate) fn parse_klepto_index_registry_refresh_stage_verify_args(args: &[String]) -> CodeKleptoIndexRegistryRefreshStageVerifyOptions {
    let mut options = CodeKleptoIndexRegistryRefreshStageVerifyOptions::default();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--expected-execution-token" | "--execution-token" | "--expect-token" | "--expected-token" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_execution_token = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    options
}

pub(crate) fn parse_klepto_index_registry_refresh_swap_execute_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshSwapExecuteOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshSwapExecuteOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--execute-swap" | "--execute" | "--apply-swap" | "--swap" => {
                options.execute_swap = true;
                idx += 1;
                continue;
            }
            "--ack-execute-swap" | "--ack-swap" | "--ack-apply-swap" | "--ack-swap-execution" => {
                options.ack_execute_swap = true;
                idx += 1;
                continue;
            }
            "--expected-execution-token" | "--execution-token" | "--expect-token" | "--expected-token" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_execution_token = Some(value.clone());
                    passthrough.push(args[idx].clone());
                    passthrough.push(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--expected-swap-plan" | "--expected-swap-plan-id" | "--swap-plan-id" | "--expect-swap-plan" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_swap_plan_id = Some(value.clone());
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

pub(crate) fn parse_klepto_index_registry_refresh_apply_execute_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshApplyExecuteOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshApplyExecuteOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--execute-apply" | "--apply" | "--apply-live" | "--execute-live-apply" => {
                options.execute_apply = true;
                idx += 1;
                continue;
            }
            "--ack-execute-apply" | "--ack-apply" | "--ack-live-apply" | "--ack-apply-execution" => {
                options.ack_execute_apply = true;
                idx += 1;
                continue;
            }
            "--expected-apply-plan" | "--expected-apply-plan-id" | "--apply-plan-id" | "--expect-apply-plan" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_apply_plan_id = Some(value.clone());
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


pub(crate) fn parse_klepto_index_registry_refresh_live_apply_request_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshLiveApplyRequestOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshLiveApplyRequestOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--request-live-apply" | "--request-apply" | "--live-apply-request" | "--request-live-writer" => {
                options.request_live_apply = true;
                idx += 1;
                continue;
            }
            "--ack-request-live-apply" | "--ack-live-apply-request" | "--ack-request-apply" | "--ack-live-writer" => {
                options.ack_request_live_apply = true;
                idx += 1;
                continue;
            }
            _ => {}
        }
        passthrough.push(args[idx].clone());
        idx += 1;
    }
    (options, passthrough)
}

pub(crate) fn parse_klepto_index_registry_refresh_live_apply_preflight_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshLiveApplyPreflightOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshLiveApplyPreflightOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--preflight-live-apply" | "--preflight-apply" | "--live-apply-preflight" | "--preflight-live-writer" => {
                options.preflight_live_apply = true;
                idx += 1;
                continue;
            }
            "--ack-preflight-live-apply" | "--ack-live-apply-preflight" | "--ack-preflight-apply" | "--ack-preflight-live-writer" => {
                options.ack_preflight_live_apply = true;
                idx += 1;
                continue;
            }
            "--expected-live-apply-request" | "--expected-live-apply-request-id" | "--live-apply-request-id" | "--expect-live-apply-request" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_live_apply_request_id = Some(value.clone());
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

pub(crate) fn parse_klepto_index_registry_refresh_live_apply_commit_plan_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshLiveApplyCommitPlanOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--plan-live-commit" | "--plan-live-apply-commit" | "--live-commit-plan" | "--plan-live-writer" => {
                options.plan_live_commit = true;
                idx += 1;
                continue;
            }
            "--ack-plan-live-commit" | "--ack-live-commit-plan" | "--ack-live-apply-commit" | "--ack-plan-live-writer" => {
                options.ack_plan_live_commit = true;
                idx += 1;
                continue;
            }
            "--expected-live-apply-request" | "--expected-live-apply-request-id" | "--live-apply-request-id" | "--expect-live-apply-request" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_live_apply_request_id = Some(value.clone());
                    passthrough.push(args[idx].clone());
                    passthrough.push(value.clone());
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

pub(crate) fn parse_klepto_index_registry_refresh_live_apply_commit_exec_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshLiveApplyCommitExecOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshLiveApplyCommitExecOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--execute-live-commit" | "--execute-live-apply-commit" | "--run-live-commit" | "--commit-live" | "--execute-live-writer" => {
                options.execute_live_commit = true;
                idx += 1;
                continue;
            }
            "--ack-execute-live-commit" | "--ack-live-commit-execution" | "--ack-execute-live-apply-commit" | "--ack-live-writer" => {
                options.ack_execute_live_commit = true;
                idx += 1;
                continue;
            }
            "--expected-live-commit-plan" | "--expected-live-commit-plan-id" | "--live-commit-plan-id" | "--expect-live-commit-plan" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_live_commit_plan_id = Some(value.clone());
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

pub(crate) fn parse_klepto_index_registry_refresh_live_apply_commit_stage_args(args: &[String]) -> (CodeKleptoIndexRegistryRefreshLiveApplyCommitStageOptions, Vec<String>) {
    let mut options = CodeKleptoIndexRegistryRefreshLiveApplyCommitStageOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--stage-live-commit" | "--stage-live-apply-commit" | "--stage-commit" | "--stage-live-writer" => {
                options.stage_live_commit = true;
                idx += 1;
                continue;
            }
            "--ack-stage-live-commit" | "--ack-stage-live-apply-commit" | "--ack-live-commit-stage" | "--ack-stage-live-writer" => {
                options.ack_stage_live_commit = true;
                idx += 1;
                continue;
            }
            "--expected-live-commit-execution" | "--expected-live-commit-execution-id" | "--live-commit-execution-id" | "--expect-live-commit-execution" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_live_commit_execution_id = Some(value.clone());
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

pub(crate) fn parse_klepto_index_registry_refresh_live_apply_commit_stage_verify_args(args: &[String]) -> CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyOptions {
    let mut options = CodeKleptoIndexRegistryRefreshLiveApplyCommitStageVerifyOptions::default();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--expected-live-commit-execution" | "--expected-live-commit-execution-id" | "--live-commit-execution-id" | "--expect-live-commit-execution" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_live_commit_execution_id = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    options
}

pub(crate) fn parse_klepto_index_registry_refresh_live_apply_commit_gate_args(args: &[String]) -> CodeKleptoIndexRegistryRefreshLiveApplyCommitGateOptions {
    let mut options = CodeKleptoIndexRegistryRefreshLiveApplyCommitGateOptions::default();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--open-live-commit-gate" | "--gate-live-commit" | "--open-live-writer-gate" | "--gate-live-writer" => {
                options.open_live_commit_gate = true;
                idx += 1;
                continue;
            }
            "--ack-open-live-commit-gate" | "--ack-gate-live-commit" | "--ack-open-live-writer-gate" | "--ack-gate-live-writer" => {
                options.ack_open_live_commit_gate = true;
                idx += 1;
                continue;
            }
            "--expected-live-commit-execution" | "--expected-live-commit-execution-id" | "--live-commit-execution-id" | "--expect-live-commit-execution" => {
                if let Some(value) = args.get(idx + 1) {
                    options.expected_live_commit_execution_id = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    options
}

pub(crate) fn parse_code_search_index_args(args: &[String]) -> (CodeTantivyIndexOptions, Vec<String>) {
    let mut options = CodeTantivyIndexOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--index-dir" | "--tantivy-dir" => {
                if let Some(value) = args.get(idx + 1) {
                    options.index_dir = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--write-index" | "--persist-index" => {
                options.write_index = true;
                idx += 1;
                continue;
            }
            "--overwrite-index" | "--reset-index" => {
                options.overwrite_index = true;
                idx += 1;
                continue;
            }
            "--search-query" | "--query-check" | "--tantivy-query" => {
                if let Some(value) = args.get(idx + 1) {
                    options.search_query = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--top" | "--top-k" | "--limit-hits" => {
                if let Some(value) = args.get(idx + 1) {
                    if let Ok(parsed) = value.parse::<usize>() {
                        options.top_k = parsed.clamp(1, 100);
                    }
                    idx += 2;
                    continue;
                }
            }
            "--filter-tenant" | "--query-tenant" | "--tenant-filter" => {
                if let Some(value) = args.get(idx + 1) {
                    options.filter_tenant_id = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--filter-namespace" | "--query-namespace" => {
                if let Some(value) = args.get(idx + 1) {
                    options.filter_namespace = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--filter-workspace" | "--query-workspace" | "--workspace-filter" => {
                if let Some(value) = args.get(idx + 1) {
                    options.filter_workspace_id = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--filter-project" | "--query-project" | "--project-filter" => {
                if let Some(value) = args.get(idx + 1) {
                    options.filter_project_id = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--filter-visibility" | "--query-visibility" => {
                if let Some(value) = args.get(idx + 1) {
                    options.filter_visibility = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--no-tenant-filter" | "--disable-tenant-filter" => {
                options.disable_tenant_filter = true;
                idx += 1;
                continue;
            }
            _ => {}
        }
        passthrough.push(args[idx].clone());
        idx += 1;
    }
    (options, passthrough)
}
