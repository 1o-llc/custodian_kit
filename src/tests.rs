use super::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project() -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("custodian_graph_test_{stamp}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        r#"
[package]
name = "graph-demo"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", optional = true, features = ["derive"] }
serde_json = { version = "1", optional = true }
regex = "1"

[features]
default = []
serde_support = ["dep:serde", "serde_json", "serde/derive"]
"#,
    )
    .unwrap();
    fs::write(
        root.join("src/lib.rs"),
        r#"
use regex::Regex;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config { pub name: String }

pub fn encode(cfg: &Config) -> Result<String, serde_json::Error> {
let _r = Regex::new("x").unwrap();
serde_json::to_string(cfg)
}
"#,
    )
    .unwrap();
    root
}

#[test]
fn graph_detects_features_deps_functions_imports_and_sites() {
    let root = temp_project();
    let graph = build_code_graph(&root, 6).unwrap();
    assert_eq!(graph.schema, "code_passenger.graph.v1");
    assert_eq!(graph.summary.crate_count, 1);
    assert!(graph.deps.iter().any(|d| d.name == "serde" && d.optional));
    assert!(graph.features.iter().any(|f| f.name == "serde_support" && f.enables_deps.contains(&"serde".to_string())));
    assert!(graph.features.iter().any(|f| f.forwards_dependency_features.iter().any(|fw| fw.dep == "serde" && fw.feature == "derive")));
    assert!(graph.functions.iter().any(|f| f.name == "encode"));
    assert!(graph.imports.iter().any(|i| i.root == "serde"));
    assert!(graph.use_sites.iter().any(|s| s.dep == "serde_json" && s.optional_dep));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_serializes_to_json() {
    let root = temp_project();
    let json = code_graph_json(&root, 6).unwrap();
    assert!(json.contains("\"schema\": \"code_passenger.graph.v1\""));
    assert!(json.contains("serde_support"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_nav_renders_graph_regions() {
    let root = temp_project();
    let nav = code_nav(&root, 6).unwrap();
    assert!(nav.contains("CODEPASSENGER :: CODE NAV"));
    assert!(nav.contains("CRTE"));
    assert!(nav.contains("FILE"));
    assert!(nav.contains("FEAT"));
    assert!(nav.contains("DEP"));
    assert!(nav.contains("FN"));
    assert!(nav.contains("USE"));
    assert!(nav.contains("SITE"));
    assert!(nav.contains("serde_support"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_nav_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_nav_root");
    let nav = code_nav(&root, 4).unwrap();
    assert!(nav.contains("MISS"));
    assert!(nav.contains("root path does not exist"));
}

#[test]
fn code_probe_dependency_shows_sites_and_features() {
    let root = temp_project();
    let probe = code_probe(&root, "serde", 6).unwrap();
    assert!(probe.contains("CODEPASSENGER :: CODE PROBE"));
    assert!(probe.contains("DEP"));
    assert!(probe.contains("SITE"));
    assert!(probe.contains("serde_support"));
    assert!(probe.contains("feature_candidates"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_probe_function_shows_signature_and_scope_counts() {
    let root = temp_project();
    let probe = code_probe(&root, "encode", 6).unwrap();
    assert!(probe.contains("FN"));
    assert!(probe.contains("signature="));
    assert!(probe.contains("scoped_use_sites"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_probe_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_probe_root");
    let probe = code_probe(&root, "serde", 4).unwrap();
    assert!(probe.contains("MISS"));
    assert!(probe.contains("root path does not exist"));
}

#[test]
fn feature_map_renders_optional_dep_feature_usage() {
    let root = temp_project();
    let map = feature_map(&root, 6).unwrap();
    assert!(map.contains("CODEPASSENGER :: FEATURE MAP"));
    assert!(map.contains("serde_support"));
    assert!(map.contains("optional_deps: 2"));
    assert!(map.contains("DEP serde"));
    assert!(map.contains("DEP serde_json"));
    assert!(map.contains("SITE"));
    assert!(map.contains("feature_candidates"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_map_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_feature_map_root");
    let map = feature_map(&root, 4).unwrap();
    assert!(map.contains("MISS"));
    assert!(map.contains("root path does not exist"));
}

#[test]
fn feature_sites_dependency_lists_exact_sites() {
    let root = temp_project();
    let sites = feature_sites(&root, "serde_json", 6).unwrap();
    assert!(sites.contains("CODEPASSENGER :: FEATURE SITES"));
    assert!(sites.contains("target: serde_json"));
    assert!(sites.contains("DEP  serde_json"));
    assert!(sites.contains("SITE"));
    // klepto names the scope by the function's full path (graph_demo::encode); the regex scan wrote the bare name
    assert!(sites.contains("scope=fn graph_demo::encode"));
    assert!(sites.contains("optional_dep=true"));
    assert!(sites.contains("feature_candidates=serde_support"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_sites_feature_lists_enabled_deps_and_sites() {
    let root = temp_project();
    let sites = feature_sites(&root, "serde_support", 6).unwrap();
    assert!(sites.contains("FEAT serde_support"));
    assert!(sites.contains("DEP  serde"));
    assert!(sites.contains("DEP  serde_json"));
    assert!(sites.contains("SITE"));
    assert!(sites.contains("scope=file"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_sites_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_feature_sites_root");
    let sites = feature_sites(&root, "serde", 4).unwrap();
    assert!(sites.contains("MISS"));
    assert!(sites.contains("root path does not exist"));
}

#[test]
fn feature_probe_optional_dep_summarizes_gate_review() {
    let root = temp_project();
    let probe = feature_probe(&root, "serde_json", 6).unwrap();
    assert!(probe.contains("CODEPASSENGER :: FEATURE PROBE"));
    assert!(probe.contains("target: serde_json"));
    assert!(probe.contains("review_level: optional_dep_gate_review"));
    assert!(probe.contains("optional_deps: 1"));
    assert!(probe.contains("scope_suggestion"));
    assert!(probe.contains("representative sites"));
    // klepto names the scope by the function's full path (graph_demo::encode); the regex scan wrote the bare name
    assert!(probe.contains("scope=fn graph_demo::encode"));
    assert!(probe.contains("writes: false"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_probe_feature_group_shows_forwarded_deps() {
    let root = temp_project();
    let probe = feature_probe(&root, "serde_support", 6).unwrap();
    assert!(probe.contains("matched_features: 1"));
    assert!(probe.contains("FEAT serde_support"));
    assert!(probe.contains("forwarded_dependency_features: serde/derive"));
    assert!(probe.contains("DEP  serde"));
    assert!(probe.contains("DEP  serde_json"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_probe_no_match_is_safe() {
    let root = temp_project();
    let probe = feature_probe(&root, "totally_missing_target", 6).unwrap();
    assert!(probe.contains("status: no_match"));
    assert!(probe.contains("writes: false"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_probe_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_feature_probe_root");
    let probe = feature_probe(&root, "serde", 4).unwrap();
    assert!(probe.contains("MISS"));
    assert!(probe.contains("root path does not exist"));
}


#[test]
fn code_query_json_function_public_result_filter_works() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "fn".to_string(),
        "--public".to_string(),
        "--returns".to_string(),
        "Result".to_string(),
    ];
    let json = code_query_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.query.v1");
    assert_eq!(value["query"]["kind"], "fn");
    assert!(value["result_count"].as_u64().unwrap() >= 1);
    assert!(json.contains("encode"));
    assert!(json.contains("signature_preview"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_query_json_use_site_dep_scope_filter_works() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
    ];
    let json = code_query_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.query.v1");
    assert_eq!(value["query"]["kind"], "use-site");
    assert!(value["result_count"].as_u64().unwrap() >= 1);
    assert!(json.contains("serde_json"));
    // klepto names the scope by the function's full path (graph_demo::encode); the regex scan wrote the bare name
    assert!(json.contains("fn graph_demo::encode"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_query_json_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_query_root");
    let args = vec!["--kind".to_string(), "fn".to_string()];
    let json = code_query_json(&root, 4, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.query.v1");
    assert_eq!(value["root_exists"], false);
    assert_eq!(value["result_count"].as_u64().unwrap(), 0);
    assert_eq!(value["findings"].as_array().unwrap().len(), 1);
}


#[test]
fn code_query_visual_use_site_dep_scope_filter_works() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
    ];
    let text = code_query(&root, 6, &args).unwrap();
    assert!(text.contains("CODEPASSENGER :: CODE QUERY"));
    assert!(text.contains("kind: use-site"));
    assert!(text.contains("SITE"));
    assert!(text.contains("serde_json"));
    // klepto names the scope by the function's full path (graph_demo::encode); the regex scan wrote the bare name
    assert!(text.contains("scope=fn graph_demo::encode"));
    assert!(text.contains("safe next commands"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_query_visual_function_filter_renders_regions() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "fn".to_string(),
        "--public".to_string(),
        "--returns".to_string(),
        "Result".to_string(),
    ];
    let text = code_query(&root, 6, &args).unwrap();
    assert!(text.contains("CODEPASSENGER :: CODE QUERY"));
    assert!(text.contains("FN"));
    assert!(text.contains("encode"));
    assert!(text.contains("signature_preview"));
    assert!(text.contains("result"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_query_visual_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_visual_query_root");
    let args = vec!["--kind".to_string(), "fn".to_string()];
    let text = code_query(&root, 4, &args).unwrap();
    assert!(text.contains("CODEPASSENGER :: CODE QUERY"));
    assert!(text.contains("MISS"));
    assert!(text.contains("root path does not exist"));
    assert!(text.contains("safe next commands"));
}

#[test]
fn pin_query_json_function_results_make_pin_set() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "fn".to_string(),
        "--public".to_string(),
        "--returns".to_string(),
        "Result".to_string(),
    ];
    let json = pin_query_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.pin_set.v1");
    assert!(value["pin_count"].as_u64().unwrap() >= 1);
    assert!(json.contains("pin_001"));
    assert!(json.contains("source_result_id"));
    assert!(json.contains("copy_hint"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pin_query_visual_use_site_filter_renders_pins() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
    ];
    let text = pin_query(&root, 6, &args).unwrap();
    assert!(text.contains("CODEPASSENGER :: PIN QUERY"));
    assert!(text.contains("pin_001"));
    assert!(text.contains("SITE"));
    assert!(text.contains("serde_json"));
    assert!(text.contains("copy_hint"));
    assert!(text.contains("safe next commands"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pin_query_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_pin_query_root");
    let args = vec!["--kind".to_string(), "fn".to_string()];
    let json = pin_query_json(&root, 4, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.pin_set.v1");
    assert_eq!(value["root_exists"], false);
    assert_eq!(value["pin_count"].as_u64().unwrap(), 0);
    let text = pin_query(&root, 4, &args).unwrap();
    assert!(text.contains("MISS"));
    assert!(text.contains("pin set is intentionally empty"));
}


#[test]
fn code_context_json_use_site_query_emits_context_windows() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
    ];
    let json = code_context_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.context.v1");
    assert_eq!(value["source_query"]["kind"], "use-site");
    assert_eq!(value["context_options"]["before_lines"].as_u64().unwrap(), 1);
    assert_eq!(value["context_options"]["after_lines"].as_u64().unwrap(), 1);
    assert!(value["pin_count"].as_u64().unwrap() >= 2);
    assert!(value["context_count"].as_u64().unwrap() >= 2);
    assert!(value["total_estimated_tokens"].as_u64().unwrap() > 0);
    assert!(value["contexts"][0]["pin_id"].as_str().unwrap().starts_with("pin_"));
    assert!(value["contexts"][0]["content"].as_str().unwrap().contains("serde_json"));
    assert!(!json.contains("--context-lines"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_context_json_respects_window_byte_cap() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "fn".to_string(),
        "--name".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "4".to_string(),
        "--max-window-bytes".to_string(),
        "48".to_string(),
    ];
    let json = code_context_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.context.v1");
    assert_eq!(value["context_options"]["max_window_bytes"].as_u64().unwrap(), 48);
    assert!(value["context_count"].as_u64().unwrap() >= 1);
    assert!(value["contexts"][0]["byte_count"].as_u64().unwrap() <= 48);
    assert_eq!(value["contexts"][0]["truncated"], true);
    assert_eq!(value["truncated"], true);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_context_json_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_context_root");
    let args = vec!["--kind".to_string(), "fn".to_string(), "--context-lines".to_string(), "2".to_string()];
    let json = code_context_json(&root, 4, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.context.v1");
    assert_eq!(value["root_exists"], false);
    assert_eq!(value["pin_count"].as_u64().unwrap(), 0);
    assert_eq!(value["context_count"].as_u64().unwrap(), 0);
    assert_eq!(value["summary"]["finding_count"].as_u64().unwrap(), 1);
}


#[test]
fn code_search_plan_json_emits_multi_tenant_documents() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--namespace".to_string(),
        "code".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--role".to_string(),
        "viewer".to_string(),
        "--extract-mode".to_string(),
        "preview".to_string(),
        "--language".to_string(),
        "english".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
    ];
    let json = code_search_plan_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.search_plan.v1");
    assert_eq!(value["tenant_scope"]["tenant_id"], "acme-dev");
    assert_eq!(value["tenant_scope"]["workspace_id"], "workspace-a");
    assert_eq!(value["tenant_scope"]["project_id"], "graph-demo");
    assert_eq!(value["analyzer"]["tantivy_language"], "English");
    assert!(value["document_count"].as_u64().unwrap() >= 2);
    assert_eq!(value["documents"][0]["tenant_id"], "acme-dev");
    assert!(value["documents"][0]["content"].as_str().unwrap().contains("serde_json"));
    assert!(json.contains("tenant_scope must be applied as an AND filter before extraction"));
    assert!(json.contains("search/full_text/result_extraction multi-tenant extraction config"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_plan_json_writer_memory_is_bounded_and_schema_is_searchable() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "fn".to_string(),
        "--name".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--writer-docs".to_string(),
        "5000".to_string(),
    ];
    let json = code_search_plan_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mem = value["writer_memory"]["writer_memory_bytes"].as_u64().unwrap();
    assert!((50_000_000..=1_000_000_000).contains(&mem));
    assert_eq!(value["writer_memory"]["estimated_document_count"].as_u64().unwrap(), 5000);
    let fields = value["schema_fields"].as_array().unwrap();
    assert!(fields.iter().any(|field| field["name"] == "tenant_id" && field["tenant_scoped"] == true));
    assert!(fields.iter().any(|field| field["name"] == "content" && field["searchable"] == true));
    assert!(fields.iter().any(|field| field["name"] == "anchor_line" && field["field_type"] == "u64"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_plan_json_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_search_plan_root");
    let args = vec!["--tenant".to_string(), "missing-tenant".to_string(), "--kind".to_string(), "fn".to_string()];
    let json = code_search_plan_json(&root, 4, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.search_plan.v1");
    assert_eq!(value["root_exists"], false);
    assert_eq!(value["tenant_scope"]["tenant_id"], "missing-tenant");
    assert_eq!(value["document_count"].as_u64().unwrap(), 0);
    assert_eq!(value["documents"].as_array().unwrap().len(), 0);
    assert_eq!(value["summary"]["finding_count"].as_u64().unwrap(), 1);
}

#[test]
fn edit_plan_json_use_site_gate_plan_is_safe() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--intent".to_string(),
        "gate serde_json encode usage".to_string(),
    ];
    let json = edit_plan_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.edit_plan.v1");
    assert_eq!(value["policy"]["operation"], "cfg_gate");
    assert_eq!(value["policy"]["target_feature"], "serde_support");
    assert_eq!(value["policy"]["writes_allowed"], false);
    assert_eq!(value["safety"]["writes"], false);
    assert!(value["candidate_count"].as_u64().unwrap() >= 2);
    assert_eq!(value["candidates"][0]["proposed_change"]["write_allowed"], false);
    assert!(value["candidates"][0]["proposed_change"]["proposed_preview"].as_str().unwrap().contains("#[cfg(feature = \"serde_support\")]") );
    assert!(!json.contains("--gate-feature"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn edit_plan_json_write_request_stays_blocked() {
    let root = temp_project();
    let args = vec![
        "--kind".to_string(),
        "fn".to_string(),
        "--name".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--write".to_string(),
    ];
    let json = edit_plan_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.edit_plan.v1");
    assert_eq!(value["policy"]["write_requested"], true);
    assert_eq!(value["policy"]["writes_allowed"], false);
    assert_eq!(value["safety"]["applies_patches"], false);
    assert!(json.contains("CPE001"));
    assert!(json.contains("not_available_in_step_12"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn edit_plan_json_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_edit_plan_root");
    let args = vec!["--kind".to_string(), "fn".to_string(), "--context-lines".to_string(), "2".to_string()];
    let json = edit_plan_json(&root, 4, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.edit_plan.v1");
    assert_eq!(value["root_exists"], false);
    assert_eq!(value["candidate_count"].as_u64().unwrap(), 0);
    assert_eq!(value["policy"]["writes_allowed"], false);
    assert_eq!(value["summary"]["finding_count"].as_u64().unwrap(), 1);
}

#[test]
fn klepto_field_plan_json_assembles_feature_bound_fields() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
    ];
    let json = klepto_field_plan_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_field_plan.v1");
    assert_eq!(value["tenant_scope"]["tenant_id"], "acme-dev");
    assert_eq!(value["feature_binding"]["target_feature"], "serde_support");
    assert_eq!(value["feature_binding"]["feature_gate_requested"], true);
    assert!(value["feature_binding"]["matched_deps"].as_array().unwrap().iter().any(|dep| dep == "serde_json"));
    assert!(value["document_count"].as_u64().unwrap() >= 2);
    assert_eq!(value["documents"][0]["fields"]["dep"], "serde_json");
    assert!(value["documents"][0]["fields"]["full_path"].as_str().unwrap().contains("serde_json"));
    assert!(json.contains("klepto_context_field_assembly"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_field_plan_json_custom_fields_and_filters_are_planned() {
    let root = temp_project();
    let args = vec![
        "--clear-fields".to_string(),
        "--field".to_string(),
        "dep,scope,symbols".to_string(),
        "--klepto-filter".to_string(),
        "lowercase,remove-long-tokens-40".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
    ];
    let json = klepto_field_plan_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_field_plan.v1");
    assert_eq!(value["field_count"].as_u64().unwrap(), 3);
    let fields = value["fields"].as_array().unwrap();
    assert!(fields.iter().any(|field| field["name"] == "dep" && field["customized_by_flag"] == true));
    assert!(fields.iter().any(|field| field["name"] == "symbols" && field["searchable"] == true));
    assert!(value["analyzer"]["filters"].as_array().unwrap().iter().any(|filter| filter == "remove_long_tokens_40"));
    assert!(value["documents"][0]["fields"].get("content").is_none());
    assert!(value["documents"][0]["fields"]["symbols"].as_str().unwrap().contains("serde_json"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_field_plan_json_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_klepto_field_root");
    let args = vec![
        "--tenant".to_string(),
        "missing-tenant".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "fn".to_string(),
    ];
    let json = klepto_field_plan_json(&root, 4, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_field_plan.v1");
    assert_eq!(value["root_exists"], false);
    assert_eq!(value["tenant_scope"]["tenant_id"], "missing-tenant");
    assert_eq!(value["document_count"].as_u64().unwrap(), 0);
    assert_eq!(value["documents"].as_array().unwrap().len(), 0);
    assert_eq!(value["summary"]["finding_count"].as_u64().unwrap(), 1);
}

#[test]
fn klepto_index_key_json_binds_klepto_fields_to_index_schema() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--namespace".to_string(),
        "code".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_key_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_key_receipt.v1");
    assert_eq!(value["source_klepto_schema"], "code_passenger.klepto_field_plan.v1");
    assert_eq!(value["candidate_index_schema"], "code_passenger.tantivy_index_receipt.v1");
    assert_eq!(value["binding"]["polymorphic_index_key"], true);
    assert_eq!(value["binding"]["mutates_index"], false);
    assert_eq!(value["tenant_scope"]["tenant_id"], "acme-dev");
    assert_eq!(value["feature_binding"]["target_feature"], "serde_support");
    assert!(value["key"]["key_id"].as_str().unwrap().starts_with("kidx_"));
    // every digest is stable_digest (sha256) now, one rule (crate::digest); fnv1a64 is gone
    assert!(value["key"]["schema_hash"].as_str().unwrap().starts_with("sha256:"));
    // every digest is stable_digest (sha256) now, one rule (crate::digest); fnv1a64 is gone
    assert!(value["key"]["field_contract_hash"].as_str().unwrap().starts_with("sha256:"));
    assert!(value["klepto_document_count"].as_u64().unwrap() >= 2);
    assert_eq!(value["index_schema_field_count"].as_u64().unwrap(), 23);
    let bindings = value["field_bindings"].as_array().unwrap();
    assert!(bindings.iter().any(|field| field["klepto_field"] == "dep" && field["compatibility"] == "klepto_declares_index_field"));
    assert!(bindings.iter().any(|field| field["klepto_field"] == "doc_id" && field["compatibility"] == "index_system_field"));
    assert!(bindings.iter().any(|field| field["klepto_field"] == "content" && field["searchable"] == true));
    assert!(bindings.iter().any(|field| field["klepto_field"] == "head" && field["compatibility"] == "klepto_virtual_field_without_index_field"));
    assert!(bindings.iter().any(|field| field["klepto_field"] == "site_kind" && field["index_key_component"] == true));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_key_json_ignores_write_flags() {
    let root = temp_project();
    let should_not_write = std::env::temp_dir().join("custodian_klepto_key_should_not_write");
    let _ = fs::remove_dir_all(&should_not_write);
    let args = vec![
        "--write-index".to_string(),
        "--overwrite-index".to_string(),
        "--index-dir".to_string(),
        should_not_write.to_string_lossy().to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_key_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_key_receipt.v1");
    assert_eq!(value["write_requested"], true);
    assert_eq!(value["overwrite_requested"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPK001"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_key_json_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_klepto_key_root");
    let args = vec![
        "--tenant".to_string(),
        "missing-tenant".to_string(),
        "--kind".to_string(),
        "fn".to_string(),
        "--search-query".to_string(),
        "encode".to_string(),
    ];
    let json = klepto_index_key_json(&root, 4, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_key_receipt.v1");
    assert_eq!(value["root_exists"], false);
    assert_eq!(value["tenant_scope"]["tenant_id"], "missing-tenant");
    assert_eq!(value["klepto_document_count"].as_u64().unwrap(), 0);
    assert_eq!(value["candidate_committed"], true);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPG001"));
}


#[test]
fn klepto_index_version_plan_json_registers_initial_key() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_version_plan_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_version_plan_receipt.v1");
    assert_eq!(value["source_key_schema"], "code_passenger.klepto_index_key_receipt.v1");
    assert_eq!(value["source_klepto_schema"], "code_passenger.klepto_field_plan.v1");
    assert_eq!(value["implementation_schema"], "code_passenger.tantivy_index_receipt.v1");
    assert_eq!(value["recommended_action"], "register_initial");
    assert_eq!(value["recommended_bump"], "initial");
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["current_key"]["key_id"].as_str().unwrap().starts_with("kidx_"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPV002"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_version_plan_json_reuses_matching_registered_key() {
    let root = temp_project();
    let base_args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let key_json = klepto_index_key_json(&root, 6, &base_args).unwrap();
    let key_value: serde_json::Value = serde_json::from_str(&key_json).unwrap();
    let key = &key_value["key"];
    let mut version_args = base_args.clone();
    version_args.extend([
        "--registered-key".to_string(), key["key_id"].as_str().unwrap().to_string(),
        "--registered-version".to_string(), "0.1.0".to_string(),
        "--registered-schema-hash".to_string(), key["schema_hash"].as_str().unwrap().to_string(),
        "--registered-field-contract-hash".to_string(), key["field_contract_hash"].as_str().unwrap().to_string(),
        "--registered-analyzer-hash".to_string(), key["analyzer_hash"].as_str().unwrap().to_string(),
        "--registered-tenant-scope-hash".to_string(), key["tenant_scope_hash"].as_str().unwrap().to_string(),
        "--registered-feature-binding-hash".to_string(), key["feature_binding_hash"].as_str().unwrap().to_string(),
        "--registered-document-shape-hash".to_string(), key["document_shape_hash"].as_str().unwrap().to_string(),
    ]);
    let json = klepto_index_version_plan_json(&root, 6, &version_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_version_plan_receipt.v1");
    assert_eq!(value["same_registered_key"], true);
    assert_eq!(value["hash_delta_count"].as_u64().unwrap(), 0);
    assert_eq!(value["recommended_action"], "reuse_existing_version");
    assert_eq!(value["recommended_bump"], "none");
    assert_eq!(value["registered_hash_count"].as_u64().unwrap(), 6);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_version_plan_json_detects_analyzer_change_and_ignores_write_flags() {
    let root = temp_project();
    let should_not_write = std::env::temp_dir().join("custodian_klepto_version_should_not_write");
    let _ = fs::remove_dir_all(&should_not_write);
    let args = vec![
        "--write-index".to_string(),
        "--overwrite-index".to_string(),
        "--index-dir".to_string(),
        should_not_write.to_string_lossy().to_string(),
        "--registered-key".to_string(),
        "kidx_old".to_string(),
        "--registered-analyzer-hash".to_string(),
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_version_plan_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_version_plan_receipt.v1");
    assert_eq!(value["recommended_action"], "register_new_version");
    assert_eq!(value["recommended_bump"], "minor");
    assert_eq!(value["hash_delta_count"].as_u64().unwrap(), 1);
    assert_eq!(value["hash_deltas"][0]["component"], "analyzer_hash");
    assert_eq!(value["write_requested"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPV001"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPV003"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_entry_json_builds_initial_entry() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_entry_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_entry_receipt.v1");
    assert_eq!(value["source_version_plan_schema"], "code_passenger.klepto_index_version_plan_receipt.v1");
    assert_eq!(value["source_key_schema"], "code_passenger.klepto_index_key_receipt.v1");
    assert_eq!(value["entry"]["recommended_action"], "register_initial");
    assert_eq!(value["entry"]["key_version"], "0.1.0");
    assert_eq!(value["entry"]["state"], "candidate");
    assert_eq!(value["entry"]["read_only"], true);
    assert_eq!(value["registry_mutated"], false);
    assert!(value["entry"]["entry_id"].as_str().unwrap().starts_with("kver_"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR101"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_entry_json_reuses_existing_entry() {
    let root = temp_project();
    let base_args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let key_json = klepto_index_key_json(&root, 6, &base_args).unwrap();
    let key_value: serde_json::Value = serde_json::from_str(&key_json).unwrap();
    let key = &key_value["key"];
    let mut args = base_args.clone();
    args.extend([
        "--registered-key".to_string(), key["key_id"].as_str().unwrap().to_string(),
        "--registered-version".to_string(), "0.3.7".to_string(),
        "--registered-schema-hash".to_string(), key["schema_hash"].as_str().unwrap().to_string(),
        "--registered-field-contract-hash".to_string(), key["field_contract_hash"].as_str().unwrap().to_string(),
        "--registered-analyzer-hash".to_string(), key["analyzer_hash"].as_str().unwrap().to_string(),
        "--registered-tenant-scope-hash".to_string(), key["tenant_scope_hash"].as_str().unwrap().to_string(),
        "--registered-feature-binding-hash".to_string(), key["feature_binding_hash"].as_str().unwrap().to_string(),
        "--registered-document-shape-hash".to_string(), key["document_shape_hash"].as_str().unwrap().to_string(),
    ]);
    let json = klepto_index_registry_entry_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_entry_receipt.v1");
    assert_eq!(value["same_registered_key"], true);
    assert_eq!(value["hash_delta_count"].as_u64().unwrap(), 0);
    assert_eq!(value["entry"]["recommended_action"], "reuse_existing_version");
    assert_eq!(value["entry"]["key_version"], "0.3.7");
    assert_eq!(value["entry"]["state"], "existing");
    assert_eq!(value["entry"]["previous_key_id"].as_str().unwrap(), key["key_id"].as_str().unwrap());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR100"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_entry_json_bumps_minor_and_ignores_write_flags() {
    let root = temp_project();
    let should_not_write = std::env::temp_dir().join("custodian_klepto_registry_should_not_write");
    let _ = fs::remove_dir_all(&should_not_write);
    let args = vec![
        "--write-index".to_string(),
        "--overwrite-index".to_string(),
        "--index-dir".to_string(),
        should_not_write.to_string_lossy().to_string(),
        "--registered-key".to_string(),
        "kidx_old".to_string(),
        "--registered-version".to_string(),
        "1.2.3".to_string(),
        "--registered-analyzer-hash".to_string(),
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_entry_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_entry_receipt.v1");
    assert_eq!(value["entry"]["recommended_action"], "register_new_version");
    assert_eq!(value["entry"]["recommended_bump"], "minor");
    assert_eq!(value["entry"]["key_version"], "1.3.0");
    assert_eq!(value["write_requested"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR102"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_write_json_dry_run_does_not_write() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_dry_run.json");
    let _ = fs::remove_file(&registry_path);
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_write_json(&registry_path, &root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_write_receipt.v1");
    assert_eq!(value["source_registry_entry_schema"], "code_passenger.klepto_index_registry_entry_receipt.v1");
    assert_eq!(value["entry"]["state"], "candidate");
    assert_eq!(value["write_requested"], false);
    assert_eq!(value["ack_write_registry"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["registry_path_exists_after"], false);
    assert!(!registry_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_write_json_requires_ack() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_no_ack.json");
    let _ = fs::remove_file(&registry_path);
    let args = vec![
        "--write-registry".to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_write_json(&registry_path, &root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_write_receipt.v1");
    assert_eq!(value["write_requested"], true);
    assert_eq!(value["ack_write_registry"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert!(!registry_path.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPW001"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_write_json_writes_with_ack() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_write_with_ack.json");
    let _ = fs::remove_file(&registry_path);
    let args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_write_json(&registry_path, &root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_write_receipt.v1");
    assert_eq!(value["write_requested"], true);
    assert_eq!(value["ack_write_registry"], true);
    assert_eq!(value["write_allowed"], true);
    assert_eq!(value["registry_mutated"], true);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["written_entry_count"].as_u64().unwrap(), 1);
    assert_eq!(value["entry_count_after"].as_u64().unwrap(), 1);
    assert_eq!(value["registry_path_exists_after"], true);
    assert!(registry_path.exists());
    let registry_text = fs::read_to_string(&registry_path).unwrap();
    let registry_value: serde_json::Value = serde_json::from_str(&registry_text).unwrap();
    assert_eq!(registry_value["schema"], "code_passenger.klepto_index_registry_file.v1");
    assert_eq!(registry_value["entry_count"].as_u64().unwrap(), 1);
    assert_eq!(registry_value["entries"].as_array().unwrap().len(), 1);
    // every digest is stable_digest (sha256) now, one rule (crate::digest); fnv1a64 is gone
    assert!(value["registry_file_digest"].as_str().unwrap().starts_with("sha256:"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPW004"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_resolve_json_missing_registry_is_safe() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_resolve_missing.json");
    let _ = fs::remove_file(&registry_path);
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_resolve_json(&registry_path, &root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_resolve_receipt.v1");
    assert_eq!(value["registry_path_exists"], false);
    assert_eq!(value["registry_entry_count"].as_u64().unwrap(), 0);
    assert_eq!(value["match_model"], "no_registry_baseline");
    assert_eq!(value["version_plan_action"], "register_initial");
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR201"));
    assert!(!registry_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_resolve_json_reuses_persisted_entry() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_resolve_reuse.json");
    let _ = fs::remove_file(&registry_path);
    let args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let write_json = klepto_index_registry_write_json(&registry_path, &root, 6, &args).unwrap();
    let write_value: serde_json::Value = serde_json::from_str(&write_json).unwrap();
    assert_eq!(write_value["registry_mutated"], true);
    let resolve_args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_resolve_json(&registry_path, &root, 6, &resolve_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_resolve_receipt.v1");
    assert_eq!(value["registry_path_exists"], true);
    assert_eq!(value["registry_entry_count"].as_u64().unwrap(), 1);
    assert_eq!(value["same_registered_key"], true);
    assert_eq!(value["version_plan_action"], "reuse_existing_version");
    assert_eq!(value["version_plan_bump"], "none");
    assert_eq!(value["hash_delta_count"].as_u64().unwrap(), 0);
    assert_eq!(value["matched_key_version"], "0.1.0");
    assert!(value["match_model"].as_str().unwrap().starts_with("exact"));
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR203"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_resolve_json_plans_drift_and_ignores_write_flags() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_resolve_drift.json");
    let should_not_write = std::env::temp_dir().join("custodian_klepto_registry_resolve_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&should_not_write);
    let write_args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let write_json = klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap();
    let write_value: serde_json::Value = serde_json::from_str(&write_json).unwrap();
    assert_eq!(write_value["registry_mutated"], true);
    let resolve_args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--index-dir".to_string(),
        should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_resolve_json(&registry_path, &root, 6, &resolve_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_resolve_receipt.v1");
    assert_eq!(value["registry_path_exists"], true);
    assert_eq!(value["same_registered_key"], false);
    assert_eq!(value["version_plan_action"], "register_new_version");
    assert_eq!(value["version_plan_bump"], "major");
    assert!(value["hash_delta_count"].as_u64().unwrap() >= 1);
    assert_eq!(value["match_model"], "closest_latest_baseline");
    assert_eq!(value["write_requested"], true);
    assert_eq!(value["ack_write_registry"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR204"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR205"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_plan_json_reuses_registry_and_matching_index() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_refresh_reuse.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_registry_refresh_reuse_index");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let common_args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let mut write_args = vec!["--write-registry".to_string(), "--ack-write-registry".to_string()];
    write_args.extend(common_args.clone());
    let write_json = klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap();
    let write_value: serde_json::Value = serde_json::from_str(&write_json).unwrap();
    assert_eq!(write_value["registry_mutated"], true);

    let mut index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
    ];
    index_args.extend(common_args.clone());
    let index_json = code_search_index_json(&root, 6, &index_args).unwrap();
    let index_value: serde_json::Value = serde_json::from_str(&index_json).unwrap();
    assert_eq!(index_value["index"]["index_written"], true);

    let json = klepto_index_registry_refresh_plan_json(&registry_path, &index_dir, &root, 6, &common_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_plan_receipt.v1");
    assert_eq!(value["registry_version_action"], "reuse_existing_version");
    assert_eq!(value["same_registered_key"], true);
    assert_eq!(value["refresh_recommended"], false);
    assert_eq!(value["combined_action"], "reuse_registry_and_index");
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["registry_resolve"]["schema"], "code_passenger.klepto_index_registry_resolve_receipt.v1");
    assert_eq!(value["disk_refresh"]["schema"], "code_passenger.tantivy_disk_refresh_plan_receipt.v1");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_plan_json_missing_registry_and_index_is_safe() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_refresh_missing.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_registry_refresh_missing_index");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_refresh_plan_json(&registry_path, &index_dir, &root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_plan_receipt.v1");
    assert_eq!(value["registry_resolve"]["registry_path_exists"], false);
    assert_eq!(value["disk_refresh"]["index_exists"], false);
    assert_eq!(value["registry_version_action"], "register_initial");
    assert_eq!(value["refresh_recommended"], true);
    assert_eq!(value["combined_action"], "register_initial_and_refresh_index");
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR201"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPX101"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_plan_json_detects_schema_drift_and_ignores_writes() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_refresh_drift.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_registry_refresh_drift_index");
    let should_not_write = std::env::temp_dir().join("custodian_klepto_registry_refresh_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&should_not_write);
    let write_args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let write_json = klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap();
    let write_value: serde_json::Value = serde_json::from_str(&write_json).unwrap();
    assert_eq!(write_value["registry_mutated"], true);

    let resolve_args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--write-index".to_string(),
        "--overwrite-index".to_string(),
        "--index-dir".to_string(),
        should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_refresh_plan_json(&registry_path, &index_dir, &root, 6, &resolve_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_plan_receipt.v1");
    assert_eq!(value["registry_version_action"], "register_new_version");
    assert_eq!(value["registry_version_bump"], "major");
    assert_eq!(value["same_registered_key"], false);
    assert_eq!(value["refresh_recommended"], true);
    assert_eq!(value["combined_action"], "register_new_version_and_refresh_index");
    assert_eq!(value["write_requested"], true);
    assert_eq!(value["ack_write_registry"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPX103"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_exec_plan_json_noops_when_registry_and_index_match() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_exec_reuse.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_registry_exec_reuse_index");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let common_args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let mut write_args = vec!["--write-registry".to_string(), "--ack-write-registry".to_string()];
    write_args.extend(common_args.clone());
    let write_json = klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap();
    let write_value: serde_json::Value = serde_json::from_str(&write_json).unwrap();
    assert_eq!(write_value["registry_mutated"], true);

    let mut index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
    ];
    index_args.extend(common_args.clone());
    let index_json = code_search_index_json(&root, 6, &index_args).unwrap();
    let index_value: serde_json::Value = serde_json::from_str(&index_json).unwrap();
    assert_eq!(index_value["index"]["index_written"], true);

    let json = klepto_index_registry_refresh_exec_plan_json(&registry_path, &index_dir, &root, 6, &common_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_exec_plan_receipt.v1");
    assert_eq!(value["combined_plan_action"], "reuse_registry_and_index");
    assert_eq!(value["execution_action"], "noop_reuse_registry_and_index");
    assert_eq!(value["registry_write_required"], false);
    assert_eq!(value["index_refresh_required"], false);
    assert_eq!(value["execution_ready"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["execution_token"].as_str().unwrap().starts_with("kexec_"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE101"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_exec_plan_json_missing_registry_and_index_requires_acks() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_exec_missing.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_registry_exec_missing_index");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_refresh_exec_plan_json(&registry_path, &index_dir, &root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_exec_plan_receipt.v1");
    assert_eq!(value["combined_plan_action"], "register_initial_and_refresh_index");
    assert_eq!(value["execution_action"], "registry_write_and_index_refresh");
    assert_eq!(value["registry_write_required"], true);
    assert_eq!(value["index_refresh_required"], true);
    assert_eq!(value["registry_ack_satisfied"], false);
    assert_eq!(value["index_ack_satisfied"], false);
    assert_eq!(value["execution_ready"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE104"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE105"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_exec_plan_json_schema_drift_can_be_ticketed_without_writes() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_exec_drift.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_registry_exec_drift_index");
    let should_not_write = std::env::temp_dir().join("custodian_klepto_registry_exec_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&should_not_write);
    let write_args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let write_json = klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap();
    let write_value: serde_json::Value = serde_json::from_str(&write_json).unwrap();
    assert_eq!(write_value["registry_mutated"], true);

    let exec_args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--write-index".to_string(),
        "--ack-write-index".to_string(),
        "--overwrite-index".to_string(),
        "--index-dir".to_string(),
        should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_refresh_exec_plan_json(&registry_path, &index_dir, &root, 6, &exec_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_exec_plan_receipt.v1");
    assert_eq!(value["combined_plan_action"], "register_new_version_and_refresh_index");
    assert_eq!(value["registry_version_bump"], "major");
    assert_eq!(value["execution_action"], "registry_write_and_index_refresh");
    assert_eq!(value["registry_ack_satisfied"], true);
    assert_eq!(value["index_ack_satisfied"], true);
    assert_eq!(value["execution_ready"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE106"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE107"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_exec_json_noops_when_registry_and_index_match() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_execute_reuse.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_registry_execute_reuse_index");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let common_args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let mut write_args = vec!["--write-registry".to_string(), "--ack-write-registry".to_string()];
    write_args.extend(common_args.clone());
    let write_json = klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap();
    let write_value: serde_json::Value = serde_json::from_str(&write_json).unwrap();
    assert_eq!(write_value["registry_mutated"], true);

    let mut index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
    ];
    index_args.extend(common_args.clone());
    let index_json = code_search_index_json(&root, 6, &index_args).unwrap();
    let index_value: serde_json::Value = serde_json::from_str(&index_json).unwrap();
    assert_eq!(index_value["index"]["index_written"], true);

    let json = klepto_index_registry_refresh_exec_json(&registry_path, &index_dir, &root, 6, &common_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_exec_receipt.v1");
    assert_eq!(value["plan_execution_action"], "noop_reuse_registry_and_index");
    assert_eq!(value["execution_action"], "noop_reuse_registry_and_index");
    assert_eq!(value["execution_permitted"], false);
    assert_eq!(value["execution_performed"], false);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["execution_token"].as_str().unwrap().starts_with("kexec_"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE201"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_exec_json_missing_registry_and_index_blocks_execution() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_execute_missing.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_registry_execute_missing_index");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_refresh_exec_json(&registry_path, &index_dir, &root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_exec_receipt.v1");
    assert_eq!(value["exec_plan"]["execution_action"], "registry_write_and_index_refresh");
    assert_eq!(value["execution_action"], "dry_run_execution_not_requested");
    assert_eq!(value["registry_write_required"], true);
    assert_eq!(value["index_refresh_required"], true);
    assert_eq!(value["execute_requested"], false);
    assert_eq!(value["execution_permitted"], false);
    assert_eq!(value["execution_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE201"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE205"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_exec_json_schema_drift_would_execute_but_does_not_write() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_registry_execute_drift.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_registry_execute_drift_index");
    let should_not_write = std::env::temp_dir().join("custodian_klepto_registry_execute_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&should_not_write);
    let write_args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--field".to_string(),
        "head,site_kind".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let write_json = klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap();
    let write_value: serde_json::Value = serde_json::from_str(&write_json).unwrap();
    assert_eq!(write_value["registry_mutated"], true);

    let exec_args = vec![
        "--write-registry".to_string(),
        "--ack-write-registry".to_string(),
        "--write-index".to_string(),
        "--ack-write-index".to_string(),
        "--execute-refresh".to_string(),
        "--ack-execute-refresh".to_string(),
        "--overwrite-index".to_string(),
        "--index-dir".to_string(),
        should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = klepto_index_registry_refresh_exec_json(&registry_path, &index_dir, &root, 6, &exec_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_exec_receipt.v1");
    assert_eq!(value["exec_plan"]["registry_version_bump"], "major");
    assert_eq!(value["plan_execution_action"], "registry_write_and_index_refresh");
    assert_eq!(value["execution_action"], "would_execute_in_future_writer");
    assert_eq!(value["execute_requested"], true);
    assert_eq!(value["ack_execute_refresh"], true);
    assert_eq!(value["plan_ready"], true);
    assert_eq!(value["execution_permitted"], true);
    assert_eq!(value["execution_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE204"));
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPE205"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_stage_json_noops_when_registry_and_index_match() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_stage_reuse_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_stage_reuse_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_stage_reuse_stage");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let common_args = vec![
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let mut write_args = vec!["--write-registry".to_string(), "--ack-write-registry".to_string()];
    write_args.extend(common_args.clone());
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let mut index_args = vec!["--write-index".to_string(), "--index-dir".to_string(), index_dir.to_string_lossy().to_string()];
    index_args.extend(common_args.clone());
    assert_eq!(serde_json::from_str::<serde_json::Value>(&code_search_index_json(&root, 6, &index_args).unwrap()).unwrap()["index"]["index_written"], true);
    let json = klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &common_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_stage_receipt.v1");
    assert_eq!(value["exec_receipt"]["execution_action"], "noop_reuse_registry_and_index");
    assert_eq!(value["staging_action"], "noop_no_stage_needed");
    assert_eq!(value["staging_performed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!staging_dir.exists());
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_stage_json_blocks_without_stage_ack() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_stage_missing_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_stage_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_stage_missing_stage");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let args = vec![
        "--stage-refresh".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let json = klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_stage_receipt.v1");
    assert_eq!(value["staging_action"], "blocked_missing_stage_ack");
    assert_eq!(value["staging_allowed"], false);
    assert_eq!(value["staging_performed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!staging_dir.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSG002"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_stage_json_writes_isolated_stage_only() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_stage_drift_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_stage_drift_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_stage_drift_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_stage_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let json = klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_stage_receipt.v1");
    assert_eq!(value["staging_action"], "staged_for_future_swap");
    assert_eq!(value["exec_receipt"]["execution_action"], "would_execute_in_future_writer");
    assert_eq!(value["staging_allowed"], true);
    assert_eq!(value["staging_performed"], true);
    assert_eq!(value["staged_file_count"].as_u64().unwrap(), 4);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(PathBuf::from(value["staged_manifest_path"].as_str().unwrap()).exists());
    assert!(PathBuf::from(value["staged_exec_receipt_path"].as_str().unwrap()).exists());
    assert!(PathBuf::from(value["staged_registry_candidate_path"].as_str().unwrap()).exists());
    assert!(PathBuf::from(value["staged_index_marker_path"].as_str().unwrap()).exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSG004"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}


#[test]
fn klepto_index_registry_refresh_stage_verify_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join("custodian_klepto_stage_verify_missing_stage");
    let _ = fs::remove_dir_all(&staging_dir);
    let json = klepto_index_registry_refresh_stage_verify_json(&staging_dir, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_stage_verify_receipt.v1");
    assert_eq!(value["staging_dir_exists"], false);
    assert_eq!(value["discovered_stage_count"], 0);
    assert_eq!(value["verification_action"], "missing_stage");
    assert_eq!(value["verified"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSV001"));
}

#[test]
fn klepto_index_registry_refresh_stage_verify_json_verifies_isolated_stage() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_stage_verify_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_stage_verify_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_stage_verify_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_stage_verify_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let json = klepto_index_registry_refresh_stage_verify_json(&staging_dir, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_stage_verify_receipt.v1");
    assert_eq!(value["verification_action"], "verified_for_future_swap");
    assert_eq!(value["verified"], true);
    assert_eq!(value["manifest_schema_ok"], true);
    assert_eq!(value["exec_receipt_schema_ok"], true);
    assert_eq!(value["registry_candidate_schema_ok"], true);
    assert_eq!(value["index_marker_schema_ok"], true);
    assert_eq!(value["execution_token_matches"], true);
    assert_eq!(value["staged_file_count_matches"], true);
    assert_eq!(value["staged_file_count_observed"].as_u64().unwrap(), 4);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSV004"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_stage_verify_json_blocks_token_mismatch() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_stage_verify_token_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_stage_verify_token_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_stage_verify_token_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_stage_verify_token_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let verify_args = vec!["--expected-execution-token".to_string(), "kexec_wrong".to_string()];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_verify_json(&staging_dir, &verify_args).unwrap()).unwrap();
    assert_eq!(value["verification_action"], "blocked_token_mismatch");
    assert_eq!(value["verified"], false);
    assert_eq!(value["expected_execution_token_matches"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSV003"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_swap_plan_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join("custodian_klepto_swap_missing_stage");
    let _ = fs::remove_dir_all(&staging_dir);
    let json = klepto_index_registry_refresh_swap_plan_json(&staging_dir, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_swap_plan_receipt.v1");
    assert_eq!(value["source_stage_verify_schema"], "code_passenger.klepto_index_registry_refresh_stage_verify_receipt.v1");
    assert_eq!(value["staging_dir_exists"], false);
    assert_eq!(value["discovered_stage_count"], 0);
    assert_eq!(value["swap_action"], "missing_stage");
    assert_eq!(value["swap_ready"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSW001"));
}

#[test]
fn klepto_index_registry_refresh_swap_plan_json_readies_verified_stage() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_swap_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_swap_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_swap_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_swap_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_swap_plan_json(&staging_dir, &[]).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_swap_plan_receipt.v1");
    assert_eq!(value["swap_action"], "ready_for_future_swap");
    assert_eq!(value["swap_ready"], true);
    assert_eq!(value["verified"], true);
    assert_eq!(value["registry_swap_required"], true);
    assert_eq!(value["index_swap_required"], true);
    assert_eq!(value["required_artifacts"].as_array().unwrap().len(), 4);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSW003"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_swap_plan_json_blocks_unverified_stage() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_swap_block_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_swap_block_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_swap_block_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_swap_block_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let swap_args = vec!["--expected-execution-token".to_string(), "kexec_wrong".to_string()];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_swap_plan_json(&staging_dir, &swap_args).unwrap()).unwrap();
    assert_eq!(value["swap_action"], "blocked_unverified_stage");
    assert_eq!(value["swap_ready"], false);
    assert_eq!(value["verified"], false);
    assert_eq!(value["stage_verify"]["verification_action"], "blocked_token_mismatch");
    assert_eq!(value["expected_execution_token_matches"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSW002"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_swap_exec_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join("custodian_klepto_swap_exec_missing_stage");
    let _ = fs::remove_dir_all(&staging_dir);
    let json = klepto_index_registry_refresh_swap_exec_json(&staging_dir, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_swap_exec_receipt.v1");
    assert_eq!(value["source_swap_plan_schema"], "code_passenger.klepto_index_registry_refresh_swap_plan_receipt.v1");
    assert_eq!(value["plan_ready"], false);
    assert_eq!(value["swap_action"], "dry_run_swap_not_requested");
    assert_eq!(value["swap_permitted"], false);
    assert_eq!(value["swap_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
}

#[test]
fn klepto_index_registry_refresh_swap_exec_json_verified_stage_would_swap_without_mutation() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_swap_exec_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_swap_exec_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_swap_exec_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_swap_exec_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let plan_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_swap_plan_json(&staging_dir, &[]).unwrap()).unwrap();
    let exec_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--expected-execution-token".to_string(), plan_value["execution_token"].as_str().unwrap().to_string(),
        "--expected-swap-plan-id".to_string(), plan_value["swap_plan_id"].as_str().unwrap().to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_swap_exec_json(&staging_dir, &exec_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_swap_exec_receipt.v1");
    assert_eq!(value["source_swap_plan_schema"], "code_passenger.klepto_index_registry_refresh_swap_plan_receipt.v1");
    assert_eq!(value["swap_action"], "would_swap_in_future_apply");
    assert_eq!(value["swap_permitted"], true);
    assert_eq!(value["swap_performed"], false);
    assert_eq!(value["execution_token_matches"], true);
    assert_eq!(value["swap_plan_id_matches"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSX005"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_swap_exec_json_blocks_swap_plan_id_mismatch() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_swap_exec_block_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_swap_exec_block_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_swap_exec_block_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_swap_exec_block_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let exec_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--expected-swap-plan-id".to_string(), "kswap_wrong".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_swap_exec_json(&staging_dir, &exec_args).unwrap()).unwrap();
    assert_eq!(value["swap_action"], "blocked_swap_plan_id_mismatch");
    assert_eq!(value["swap_permitted"], false);
    assert_eq!(value["swap_performed"], false);
    assert_eq!(value["swap_plan_id_matches"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPSX004"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_apply_plan_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join("custodian_klepto_apply_plan_missing_stage");
    let _ = fs::remove_dir_all(&staging_dir);
    let json = klepto_index_registry_refresh_apply_plan_json(&staging_dir, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_apply_plan_receipt.v1");
    assert_eq!(value["apply_action"], "dry_run_apply_not_requested");
    assert_eq!(value["apply_ready"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPAP002"));
}

#[test]
fn klepto_index_registry_refresh_apply_plan_json_verified_stage_is_ready_without_mutation() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_apply_plan_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_apply_plan_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_apply_plan_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_apply_plan_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let swap_plan_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_swap_plan_json(&staging_dir, &[]).unwrap()).unwrap();
    let expected_execution_token = swap_plan_value["execution_token"].as_str().unwrap().to_string();
    let expected_swap_plan_id = swap_plan_value["swap_plan_id"].as_str().unwrap().to_string();
    let apply_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--expected-execution-token".to_string(), expected_execution_token,
        "--expected-swap-plan-id".to_string(), expected_swap_plan_id,
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_apply_plan_json(&staging_dir, &apply_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_apply_plan_receipt.v1");
    assert_eq!(value["apply_action"], "ready_for_future_apply");
    assert_eq!(value["apply_ready"], true);
    assert_eq!(value["swap_permitted"], true);
    assert_eq!(value["swap_performed"], false);
    assert_eq!(value["registry_apply_required"], true);
    assert_eq!(value["index_apply_required"], true);
    assert!(value["required_live_mutations"].as_array().unwrap().len() >= 2);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPAP004"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_apply_plan_json_blocks_bad_swap_plan() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_apply_plan_block_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_apply_plan_block_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_apply_plan_block_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_apply_plan_block_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let apply_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--expected-swap-plan-id".to_string(), "kswap_wrong".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_apply_plan_json(&staging_dir, &apply_args).unwrap()).unwrap();
    assert_eq!(value["apply_action"], "blocked_swap_execution_not_permitted");
    assert_eq!(value["apply_ready"], false);
    assert_eq!(value["swap_permitted"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPAP003"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_apply_exec_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join("custodian_klepto_apply_exec_missing_stage");
    let _ = fs::remove_dir_all(&staging_dir);
    let json = klepto_index_registry_refresh_apply_exec_json(&staging_dir, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_apply_exec_receipt.v1");
    assert_eq!(value["source_apply_plan_schema"], "code_passenger.klepto_index_registry_refresh_apply_plan_receipt.v1");
    assert_eq!(value["apply_action"], "dry_run_apply_not_requested");
    assert_eq!(value["apply_requested"], false);
    assert_eq!(value["apply_permitted"], false);
    assert_eq!(value["apply_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPAE002"));
}

#[test]
fn klepto_index_registry_refresh_apply_exec_json_verified_stage_would_apply_without_mutation() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_apply_exec_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_apply_exec_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_apply_exec_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_apply_exec_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let apply_plan_args = vec!["--execute-swap".to_string(), "--ack-execute-swap".to_string()];
    let apply_plan_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_apply_plan_json(&staging_dir, &apply_plan_args).unwrap()).unwrap();
    assert_eq!(apply_plan_value["apply_action"], "ready_for_future_apply");
    let expected_apply_plan_id = apply_plan_value["apply_plan_id"].as_str().unwrap().to_string();
    let apply_exec_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--expected-apply-plan-id".to_string(), expected_apply_plan_id,
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_apply_exec_json(&staging_dir, &apply_exec_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_apply_exec_receipt.v1");
    assert_eq!(value["apply_action"], "would_apply_in_future_writer");
    assert_eq!(value["apply_requested"], true);
    assert_eq!(value["ack_apply"], true);
    assert_eq!(value["apply_plan_id_matches"], true);
    assert_eq!(value["plan_ready"], true);
    assert_eq!(value["apply_permitted"], true);
    assert_eq!(value["apply_performed"], false);
    assert_eq!(value["registry_apply_required"], true);
    assert_eq!(value["index_apply_required"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPAE006"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_apply_exec_json_blocks_apply_plan_id_mismatch() {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join("custodian_klepto_apply_exec_block_registry.json");
    let index_dir = std::env::temp_dir().join("custodian_klepto_apply_exec_block_missing_index");
    let staging_dir = std::env::temp_dir().join("custodian_klepto_apply_exec_block_stage");
    let live_should_not_write = std::env::temp_dir().join("custodian_klepto_apply_exec_block_live_should_not_write");
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    let apply_exec_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--expected-apply-plan-id".to_string(), "kaply_wrong".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_apply_exec_json(&staging_dir, &apply_exec_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_apply_exec_receipt.v1");
    assert_eq!(value["apply_action"], "blocked_apply_plan_id_mismatch");
    assert_eq!(value["apply_requested"], true);
    assert_eq!(value["ack_apply"], true);
    assert_eq!(value["apply_plan_id_matches"], false);
    assert_eq!(value["apply_permitted"], false);
    assert_eq!(value["apply_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPAE004"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

fn stage_live_apply_request_fixture(prefix: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let root = temp_project();
    let registry_path = std::env::temp_dir().join(format!("{prefix}_registry.json"));
    let index_dir = std::env::temp_dir().join(format!("{prefix}_missing_index"));
    let staging_dir = std::env::temp_dir().join(format!("{prefix}_stage"));
    let live_should_not_write = std::env::temp_dir().join(format!("{prefix}_live_should_not_write"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    assert!(staging_dir.exists());
    assert!(!live_should_not_write.exists());
    (root, registry_path, staging_dir, live_should_not_write)
}

#[test]
fn klepto_index_registry_refresh_live_apply_request_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join("custodian_klepto_live_request_missing_stage");
    let _ = fs::remove_dir_all(&staging_dir);
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_request_json(&staging_dir, &[]).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_request_receipt.v1");
    assert_eq!(value["source_apply_exec_schema"], "code_passenger.klepto_index_registry_refresh_apply_exec_receipt.v1");
    assert_eq!(value["request_action"], "dry_run_live_apply_not_requested");
    assert_eq!(value["request_live_apply"], false);
    assert_eq!(value["ack_request_live_apply"], false);
    assert_eq!(value["live_apply_request_permitted"], false);
    assert_eq!(value["live_apply_request_recorded"], false);
    assert_eq!(value["live_apply_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!staging_dir.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLR002"));
}

#[test]
fn klepto_index_registry_refresh_live_apply_request_json_blocks_missing_live_ack() {
    let (root, registry_path, staging_dir, live_should_not_write) = stage_live_apply_request_fixture("custodian_klepto_live_request_block");
    let args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_request_json(&staging_dir, &args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_request_receipt.v1");
    assert_eq!(value["request_action"], "blocked_missing_live_apply_ack");
    assert_eq!(value["request_live_apply"], true);
    assert_eq!(value["ack_request_live_apply"], false);
    assert_eq!(value["apply_permitted"], true);
    assert_eq!(value["live_apply_ack_satisfied"], false);
    assert_eq!(value["live_apply_request_permitted"], false);
    assert_eq!(value["live_apply_request_recorded"], false);
    assert_eq!(value["live_apply_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLR003"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_request_json_records_acknowledged_request_without_mutation() {
    let (root, registry_path, staging_dir, live_should_not_write) = stage_live_apply_request_fixture("custodian_klepto_live_request_record");
    let apply_plan_args = vec!["--execute-swap".to_string(), "--ack-execute-swap".to_string()];
    let apply_plan_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_apply_plan_json(&staging_dir, &apply_plan_args).unwrap()).unwrap();
    assert_eq!(apply_plan_value["apply_action"], "ready_for_future_apply");
    let expected_apply_plan_id = apply_plan_value["apply_plan_id"].as_str().unwrap().to_string();
    let args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--expected-apply-plan-id".to_string(), expected_apply_plan_id,
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_request_json(&staging_dir, &args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_request_receipt.v1");
    assert_eq!(value["request_action"], "live_apply_request_recorded_for_future_writer");
    assert_eq!(value["request_live_apply"], true);
    assert_eq!(value["ack_request_live_apply"], true);
    assert_eq!(value["live_apply_ack_satisfied"], true);
    assert_eq!(value["apply_permitted"], true);
    assert_eq!(value["apply_performed"], false);
    assert_eq!(value["registry_apply_required"], true);
    assert_eq!(value["index_apply_required"], true);
    assert_eq!(value["live_apply_request_permitted"], true);
    assert_eq!(value["live_apply_request_recorded"], true);
    assert_eq!(value["live_apply_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["live_apply_request_id"].as_str().unwrap().starts_with("klive_"));
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLR005"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}


#[test]
fn klepto_index_registry_refresh_live_apply_preflight_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join("custodian_klepto_live_preflight_missing_stage");
    let _ = fs::remove_dir_all(&staging_dir);
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_preflight_json(&staging_dir, &[]).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_preflight_receipt.v1");
    assert_eq!(value["source_live_apply_request_schema"], "code_passenger.klepto_index_registry_refresh_live_apply_request_receipt.v1");
    assert_eq!(value["preflight_action"], "dry_run_live_apply_preflight_not_requested");
    assert_eq!(value["preflight_live_apply"], false);
    assert_eq!(value["ack_preflight_live_apply"], false);
    assert_eq!(value["live_apply_preflight_permitted"], false);
    assert_eq!(value["live_apply_preflight_ready"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!staging_dir.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLP002"));
}

#[test]
fn klepto_index_registry_refresh_live_apply_preflight_json_blocks_missing_preflight_ack() {
    let (root, registry_path, staging_dir, live_should_not_write) = stage_live_apply_request_fixture("custodian_klepto_live_preflight_block");
    let args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_preflight_json(&staging_dir, &args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_preflight_receipt.v1");
    assert_eq!(value["preflight_action"], "blocked_missing_live_apply_preflight_ack");
    assert_eq!(value["preflight_live_apply"], true);
    assert_eq!(value["ack_preflight_live_apply"], false);
    assert_eq!(value["live_apply_request_recorded"], true);
    assert_eq!(value["live_apply_preflight_ack_satisfied"], false);
    assert_eq!(value["live_apply_preflight_permitted"], false);
    assert_eq!(value["live_apply_preflight_ready"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLP003"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_preflight_json_readies_acknowledged_request_without_mutation() {
    let (root, registry_path, staging_dir, live_should_not_write) = stage_live_apply_request_fixture("custodian_klepto_live_preflight_ready");
    let request_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
    ];
    let request_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_request_json(&staging_dir, &request_args).unwrap()).unwrap();
    assert_eq!(request_value["request_action"], "live_apply_request_recorded_for_future_writer");
    let expected_request_id = request_value["live_apply_request_id"].as_str().unwrap().to_string();
    let args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--expected-live-apply-request-id".to_string(), expected_request_id,
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_preflight_json(&staging_dir, &args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_preflight_receipt.v1");
    assert_eq!(value["preflight_action"], "live_apply_preflight_ready_for_future_writer");
    assert_eq!(value["preflight_live_apply"], true);
    assert_eq!(value["ack_preflight_live_apply"], true);
    assert_eq!(value["live_apply_preflight_ack_satisfied"], true);
    assert_eq!(value["live_apply_request_recorded"], true);
    assert_eq!(value["live_apply_request_id_matches"], true);
    assert_eq!(value["live_apply_preflight_permitted"], true);
    assert_eq!(value["live_apply_preflight_ready"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLP006"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_index_json_uses_real_tantivy_in_ram() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = code_search_index_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_index_receipt.v1");
    assert_eq!(value["index"]["engine"], "tantivy");
    assert_eq!(value["index"]["tantivy_used"], true);
    assert_eq!(value["index"]["index_kind"], "ram");
    assert_eq!(value["index"]["commit_succeeded"], true);
    assert!(value["committed_document_count"].as_u64().unwrap() >= 2);
    assert!(value["search_check"]["hit_count"].as_u64().unwrap() >= 1);
    assert_eq!(value["tenant_scope"]["tenant_id"], "acme-dev");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_index_json_missing_root_still_commits_empty_ram_index() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_tantivy_index_root");
    let args = vec![
        "--tenant".to_string(),
        "missing-tenant".to_string(),
        "--kind".to_string(),
        "fn".to_string(),
        "--search-query".to_string(),
        "encode".to_string(),
    ];
    let json = code_search_index_json(&root, 4, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_index_receipt.v1");
    assert_eq!(value["root_exists"], false);
    assert_eq!(value["index"]["tantivy_used"], true);
    assert_eq!(value["document_count"].as_u64().unwrap(), 0);
    assert_eq!(value["committed_document_count"].as_u64().unwrap(), 0);
    assert_eq!(value["search_check"]["hit_count"].as_u64().unwrap(), 0);
    assert_eq!(value["summary"]["finding_count"].as_u64().unwrap(), 1);
}

#[test]
fn code_search_index_json_write_requires_index_dir() {
    let root = temp_project();
    let args = vec![
        "--write-index".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
    ];
    let err = code_search_index_json(&root, 6, &args).unwrap_err();
    assert!(err.to_string().contains("--write-index requires --index-dir"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_query_json_returns_hydrated_hits() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--gate-feature".to_string(),
        "serde_support".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
        "--top".to_string(),
        "5".to_string(),
    ];
    let json = code_search_query_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_query_receipt.v1");
    assert_eq!(value["source_index_receipt_schema"], "code_passenger.tantivy_index_receipt.v1");
    assert_eq!(value["index"]["tantivy_used"], true);
    assert_eq!(value["index"]["source_index_kind"], "ram");
    assert_eq!(value["index"]["disk_write_allowed"], false);
    assert!(value["hit_count"].as_u64().unwrap() >= 1);
    assert!(value["hits"].as_array().unwrap().iter().any(|hit| hit["document"]["label"] == "serde_json"));
    let hydrated_ids = value["hits"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|hit| hit["document"]["id"].as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        hydrated_ids.len() >= 2,
        "expected Tantivy hits to hydrate to distinct Klepto documents, got {hydrated_ids:?}"
    );
    assert_eq!(value["tenant_scope"]["tenant_id"], "acme-dev");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_query_json_ignores_disk_write_request() {
    let root = temp_project();
    let args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        std::env::temp_dir().join("codepassenger_query_should_not_write_index").to_string_lossy().to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = code_search_query_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_query_receipt.v1");
    assert_eq!(value["index"]["source_index_kind"], "ram");
    assert_eq!(value["index"]["disk_write_requested"], true);
    assert_eq!(value["index"]["disk_write_ignored"], true);
    assert_eq!(value["index"]["disk_write_allowed"], false);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_query_json_missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_tantivy_query_root");
    let args = vec![
        "--tenant".to_string(),
        "missing-tenant".to_string(),
        "--kind".to_string(),
        "fn".to_string(),
        "--search-query".to_string(),
        "encode".to_string(),
    ];
    let json = code_search_query_json(&root, 4, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_query_receipt.v1");
    assert_eq!(value["root_exists"], false);
    assert_eq!(value["hit_count"].as_u64().unwrap(), 0);
    assert_eq!(value["indexed_document_count"].as_u64().unwrap(), 0);
    assert_eq!(value["summary"]["finding_count"].as_u64().unwrap(), 1);
}


#[test]
fn code_search_query_json_enforces_default_tenant_filter() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--namespace".to_string(),
        "code".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = code_search_query_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_query_receipt.v1");
    assert_eq!(value["tenant_filter"]["enforced"], true);
    assert_eq!(value["tenant_filter"]["tenant_id"], "acme-dev");
    assert_eq!(value["tenant_filter"]["workspace_id"], "workspace-a");
    assert!(value["tenant_filter"]["accepted_hit_count"].as_u64().unwrap() >= 1);
    assert_eq!(value["tenant_filter"]["rejected_hit_count"].as_u64().unwrap(), 0);
    assert!(value["hits"].as_array().unwrap().iter().all(|hit| hit["document"]["tenant_id"] == "acme-dev"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_query_json_rejects_mismatched_tenant_filter() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--filter-tenant".to_string(),
        "other-tenant".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = code_search_query_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["tenant_filter"]["enforced"], true);
    assert_eq!(value["tenant_filter"]["tenant_id"], "other-tenant");
    assert_eq!(value["hit_count"].as_u64().unwrap(), 0);
    assert_eq!(value["hits"].as_array().unwrap().len(), 0);
    assert!(value["tenant_filter"]["rejected_hit_count"].as_u64().unwrap() >= 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_query_json_can_disable_tenant_filter_for_diagnostics() {
    let root = temp_project();
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--filter-tenant".to_string(),
        "other-tenant".to_string(),
        "--no-tenant-filter".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = code_search_query_json(&root, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["tenant_filter"]["enforced"], false);
    assert_eq!(value["tenant_filter"]["mode"], "disabled");
    assert!(value["hit_count"].as_u64().unwrap() >= 1);
    assert_eq!(value["tenant_filter"]["rejected_hit_count"].as_u64().unwrap(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_query_json_opens_persisted_index_and_hydrates_hits() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join(format!(
        "codepassenger_disk_query_index_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let index_json = code_search_index_json(&root, 6, &index_args).unwrap();
    let index_value: serde_json::Value = serde_json::from_str(&index_json).unwrap();
    assert_eq!(index_value["index"]["index_written"], true);

    let query_args = vec![
        "--filter-tenant".to_string(),
        "acme-dev".to_string(),
        "--filter-workspace".to_string(),
        "workspace-a".to_string(),
        "--filter-project".to_string(),
        "graph-demo".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
        "--top".to_string(),
        "5".to_string(),
    ];
    let json = code_search_disk_query_json(&index_dir, &query_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_query_receipt.v1");
    assert_eq!(value["index_exists"], true);
    assert_eq!(value["index"]["tantivy_used"], true);
    assert_eq!(value["tenant_filter"]["enforced"], true);
    assert!(value["hit_count"].as_u64().unwrap() >= 1);
    assert!(value["hits"].as_array().unwrap().iter().all(|hit| hit["document"]["tenant_id"] == "acme-dev"));
    let hydrated_ids = value["hits"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|hit| hit["source_document_id"].as_str())
        .collect::<BTreeSet<_>>();
    assert!(hydrated_ids.len() >= 1);
    let _ = fs::remove_dir_all(index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_query_json_rejects_mismatched_tenant_filter() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join(format!(
        "codepassenger_disk_query_mismatch_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let _ = code_search_index_json(&root, 6, &index_args).unwrap();
    let query_args = vec![
        "--filter-tenant".to_string(),
        "other-tenant".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = code_search_disk_query_json(&index_dir, &query_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_query_receipt.v1");
    assert_eq!(value["hit_count"].as_u64().unwrap(), 0);
    assert!(value["tenant_filter"]["rejected_hit_count"].as_u64().unwrap() >= 1);
    let _ = fs::remove_dir_all(index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_query_json_missing_index_is_safe() {
    let index_dir = std::env::temp_dir().join("definitely_missing_codepassenger_disk_index");
    let query_args = vec![
        "--filter-tenant".to_string(),
        "missing-tenant".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = code_search_disk_query_json(&index_dir, &query_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_query_receipt.v1");
    assert_eq!(value["index_exists"], false);
    assert_eq!(value["hit_count"].as_u64().unwrap(), 0);
    assert_eq!(value["findings"].as_array().unwrap().len(), 1);
}

#[test]
fn code_search_disk_status_json_opens_persisted_index_and_lists_tenants() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join(format!(
        "codepassenger_disk_status_index_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--namespace".to_string(),
        "code".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let _ = code_search_index_json(&root, 6, &index_args).unwrap();
    let status_args = vec!["--sample".to_string(), "2".to_string()];
    let json = code_search_disk_status_json(&index_dir, &status_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_status_receipt.v1");
    assert_eq!(value["index_exists"], true);
    assert_eq!(value["index"]["opened_read_only"], true);
    assert_eq!(value["index"]["mutates_index"], false);
    assert!(value["document_count"].as_u64().unwrap() >= 1);
    assert!(value["schema_field_count"].as_u64().unwrap() >= 1);
    assert!(value["tenant_scope_count"].as_u64().unwrap() >= 1);
    assert_eq!(value["tenant_scopes"][0]["tenant_id"], "acme-dev");
    assert_eq!(value["tenant_scopes"][0]["workspace_id"], "workspace-a");
    assert!(value["stored_field_names"].as_array().unwrap().iter().any(|name| name == "tenant_id"));
    assert!(value["query_fields"].as_array().unwrap().iter().any(|name| name == "content"));
    let _ = fs::remove_dir_all(index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_status_json_respects_sample_limit() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join(format!(
        "codepassenger_disk_status_sample_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let _ = code_search_index_json(&root, 6, &index_args).unwrap();
    let status_args = vec!["--sample".to_string(), "1".to_string()];
    let json = code_search_disk_status_json(&index_dir, &status_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_status_receipt.v1");
    assert_eq!(value["sample_document_count"].as_u64().unwrap(), 1);
    assert_eq!(value["sample_documents"].as_array().unwrap().len(), 1);
    let _ = fs::remove_dir_all(index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_status_json_missing_index_is_safe() {
    let index_dir = std::env::temp_dir().join("definitely_missing_codepassenger_disk_status_index");
    let json = code_search_disk_status_json(&index_dir, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_status_receipt.v1");
    assert_eq!(value["index_exists"], false);
    assert_eq!(value["document_count"].as_u64().unwrap(), 0);
    assert_eq!(value["findings"].as_array().unwrap().len(), 1);
}

#[test]
fn code_search_disk_manifest_json_opens_persisted_index_and_hashes_files() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join(format!(
        "codepassenger_disk_manifest_index_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--namespace".to_string(),
        "code".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--context-lines".to_string(),
        "1".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let _ = code_search_index_json(&root, 6, &index_args).unwrap();
    let manifest_args = vec!["--sample".to_string(), "2".to_string(), "--max-files".to_string(), "256".to_string()];
    let json = code_search_disk_manifest_json(&index_dir, &manifest_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_manifest_receipt.v1");
    assert_eq!(value["index_exists"], true);
    assert_eq!(value["index"]["opened_read_only"], true);
    assert_eq!(value["index"]["mutates_index"], false);
    assert_eq!(value["status_schema"], "code_passenger.tantivy_disk_status_receipt.v1");
    assert!(value["document_count"].as_u64().unwrap() >= 1);
    assert!(value["tenant_scope_count"].as_u64().unwrap() >= 1);
    assert!(value["file_count"].as_u64().unwrap() >= 1);
    assert!(value["total_bytes"].as_u64().unwrap() >= 1);
    // every digest is stable_digest (sha256) now, one rule (crate::digest); fnv1a64 is gone
    assert!(value["manifest_digest"].as_str().unwrap().starts_with("sha256:"));
    // every digest is stable_digest (sha256) now, one rule (crate::digest); fnv1a64 is gone
    assert!(value["files"].as_array().unwrap().iter().any(|file| file["digest"].as_str().unwrap().starts_with("sha256:")));
    assert!(value["stored_field_names"].as_array().unwrap().iter().any(|name| name == "tenant_id"));
    let _ = fs::remove_dir_all(index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_manifest_json_digest_is_stable_for_same_index() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join(format!(
        "codepassenger_disk_manifest_stable_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let _ = code_search_index_json(&root, 6, &index_args).unwrap();
    let json_a = code_search_disk_manifest_json(&index_dir, &[]).unwrap();
    let json_b = code_search_disk_manifest_json(&index_dir, &[]).unwrap();
    let value_a: serde_json::Value = serde_json::from_str(&json_a).unwrap();
    let value_b: serde_json::Value = serde_json::from_str(&json_b).unwrap();
    assert_eq!(value_a["schema"], "code_passenger.tantivy_disk_manifest_receipt.v1");
    assert_eq!(value_a["manifest_digest"], value_b["manifest_digest"]);
    assert_eq!(value_a["file_count"], value_b["file_count"]);
    let _ = fs::remove_dir_all(index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_manifest_json_missing_index_is_safe() {
    let index_dir = std::env::temp_dir().join("definitely_missing_codepassenger_disk_manifest_index");
    let json = code_search_disk_manifest_json(&index_dir, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_manifest_receipt.v1");
    assert_eq!(value["index_exists"], false);
    assert_eq!(value["file_count"].as_u64().unwrap(), 0);
    assert_eq!(value["total_bytes"].as_u64().unwrap(), 0);
    assert_eq!(value["findings"].as_array().unwrap().len(), 1);
}

#[test]
fn code_search_disk_compare_json_same_index_is_equal() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join(format!(
        "codepassenger_disk_compare_same_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let _ = code_search_index_json(&root, 6, &index_args).unwrap();
    let compare_args = vec!["--sample".to_string(), "1".to_string(), "--max-files".to_string(), "256".to_string()];
    let json = code_search_disk_compare_json(&index_dir, &index_dir, &compare_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_compare_receipt.v1");
    assert_eq!(value["left_exists"], true);
    assert_eq!(value["right_exists"], true);
    assert_eq!(value["index"]["mutates_index"], false);
    assert_eq!(value["same_manifest_digest"], true);
    assert_eq!(value["same_tenant_scopes"], true);
    assert_eq!(value["added_files"].as_array().unwrap().len(), 0);
    assert_eq!(value["removed_files"].as_array().unwrap().len(), 0);
    assert_eq!(value["changed_files"].as_array().unwrap().len(), 0);
    let _ = fs::remove_dir_all(index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_compare_json_detects_different_tenant_index() {
    let root = temp_project();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let left_index = std::env::temp_dir().join(format!("codepassenger_disk_compare_left_{stamp}"));
    let right_index = std::env::temp_dir().join(format!("codepassenger_disk_compare_right_{stamp}"));
    let common_tail = vec![
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let mut left_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        left_index.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
    ];
    left_args.extend(common_tail.clone());
    let mut right_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        right_index.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "other-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
    ];
    right_args.extend(common_tail);
    let _ = code_search_index_json(&root, 6, &left_args).unwrap();
    let _ = code_search_index_json(&root, 6, &right_args).unwrap();
    let json = code_search_disk_compare_json(&left_index, &right_index, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_compare_receipt.v1");
    assert_eq!(value["left_exists"], true);
    assert_eq!(value["right_exists"], true);
    assert_eq!(value["same_manifest_digest"], false);
    assert_eq!(value["same_tenant_scopes"], false);
    assert!(value["tenant_scope_deltas"].as_array().unwrap().len() >= 1);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPC003"));
    let _ = fs::remove_dir_all(left_index);
    let _ = fs::remove_dir_all(right_index);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_compare_json_missing_indexes_are_safe() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let left = std::env::temp_dir().join(format!("missing_codepassenger_compare_left_{stamp}"));
    let right = std::env::temp_dir().join(format!("missing_codepassenger_compare_right_{stamp}"));
    let json = code_search_disk_compare_json(&left, &right, &[]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_compare_receipt.v1");
    assert_eq!(value["left_exists"], false);
    assert_eq!(value["right_exists"], false);
    assert_eq!(value["same_manifest_digest"], false);
    assert_eq!(value["left_file_count"].as_u64().unwrap(), 0);
    assert_eq!(value["right_file_count"].as_u64().unwrap(), 0);
    assert!(value["findings"].as_array().unwrap().len() >= 2);
}

#[test]
fn code_search_disk_refresh_plan_json_matching_candidate_needs_no_refresh() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join(format!(
        "codepassenger_disk_refresh_same_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let common_args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--workspace".to_string(),
        "workspace-a".to_string(),
        "--project".to_string(),
        "graph-demo".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let mut index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
    ];
    index_args.extend(common_args.clone());
    let _ = code_search_index_json(&root, 6, &index_args).unwrap();
    let json = code_search_disk_refresh_plan_json(&root, &index_dir, 6, &common_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_refresh_plan_receipt.v1");
    assert_eq!(value["index_exists"], true);
    assert_eq!(value["index"]["mutates_index"], false);
    assert_eq!(value["same_document_count"], true);
    assert_eq!(value["same_tenant_scopes"], true);
    assert_eq!(value["same_schema_field_count"], true);
    assert_eq!(value["same_stored_fields"], true);
    assert_eq!(value["refresh_recommended"], false);
    assert_eq!(value["refresh_reason"], "existing_index_matches_candidate_plan");
    let _ = fs::remove_dir_all(index_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_refresh_plan_json_missing_index_is_safe_and_recommends_refresh() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join("definitely_missing_codepassenger_refresh_index");
    let args = vec![
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = code_search_disk_refresh_plan_json(&root, &index_dir, 6, &args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_refresh_plan_receipt.v1");
    assert_eq!(value["index_exists"], false);
    assert_eq!(value["candidate_committed"], true);
    assert_eq!(value["refresh_recommended"], true);
    assert_eq!(value["refresh_reason"], "existing_index_missing");
    assert!(value["candidate_document_count"].as_u64().unwrap() >= 1);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR001"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn code_search_disk_refresh_plan_json_ignores_write_flags() {
    let root = temp_project();
    let index_dir = std::env::temp_dir().join(format!(
        "codepassenger_disk_refresh_write_flags_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let candidate_write_dir = std::env::temp_dir().join(format!(
        "codepassenger_refresh_should_not_write_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let index_args = vec![
        "--write-index".to_string(),
        "--index-dir".to_string(),
        index_dir.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let _ = code_search_index_json(&root, 6, &index_args).unwrap();
    let refresh_args = vec![
        "--write-index".to_string(),
        "--overwrite-index".to_string(),
        "--index-dir".to_string(),
        candidate_write_dir.to_string_lossy().to_string(),
        "--tenant".to_string(),
        "acme-dev".to_string(),
        "--kind".to_string(),
        "use-site".to_string(),
        "--dep".to_string(),
        "serde_json".to_string(),
        "--scope".to_string(),
        "encode".to_string(),
        "--search-query".to_string(),
        "serde_json".to_string(),
    ];
    let json = code_search_disk_refresh_plan_json(&root, &index_dir, 6, &refresh_args).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], "code_passenger.tantivy_disk_refresh_plan_receipt.v1");
    assert_eq!(value["write_requested"], true);
    assert_eq!(value["overwrite_requested"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["index"]["candidate_index_kind"], "ram");
    assert!(!candidate_write_dir.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPR002"));
    let _ = fs::remove_dir_all(index_dir);
    let _ = fs::remove_dir_all(candidate_write_dir);
    let _ = fs::remove_dir_all(root);
}

fn phase38_live_commit_fixture(label: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = temp_project();
    let registry_path = std::env::temp_dir().join(format!("custodian_klepto_live_commit_{label}_{stamp}_registry.json"));
    let index_dir = std::env::temp_dir().join(format!("custodian_klepto_live_commit_{label}_{stamp}_missing_index"));
    let staging_dir = std::env::temp_dir().join(format!("custodian_klepto_live_commit_{label}_{stamp}_stage"));
    let live_should_not_write = std::env::temp_dir().join(format!("custodian_klepto_live_commit_{label}_{stamp}_live_should_not_write"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&live_should_not_write);
    let write_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--field".to_string(), "head,site_kind".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    assert_eq!(serde_json::from_str::<serde_json::Value>(&klepto_index_registry_write_json(&registry_path, &root, 6, &write_args).unwrap()).unwrap()["registry_mutated"], true);
    let stage_args = vec![
        "--write-registry".to_string(), "--ack-write-registry".to_string(),
        "--write-index".to_string(), "--ack-write-index".to_string(),
        "--execute-refresh".to_string(), "--ack-execute-refresh".to_string(),
        "--stage-refresh".to_string(), "--ack-stage-refresh".to_string(),
        "--overwrite-index".to_string(), "--index-dir".to_string(), live_should_not_write.to_string_lossy().to_string(),
        "--tenant".to_string(), "acme-dev".to_string(),
        "--workspace".to_string(), "workspace-a".to_string(),
        "--project".to_string(), "graph-demo".to_string(),
        "--gate-feature".to_string(), "serde_support".to_string(),
        "--kind".to_string(), "use-site".to_string(),
        "--dep".to_string(), "serde_json".to_string(),
        "--scope".to_string(), "encode".to_string(),
        "--context-lines".to_string(), "1".to_string(),
        "--search-query".to_string(), "serde_json".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_stage_json(&staging_dir, &registry_path, &index_dir, &root, 6, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["staging_action"], "staged_for_future_swap");
    assert!(!live_should_not_write.exists());
    (root, registry_path, index_dir, staging_dir, live_should_not_write)
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_plan_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join(format!(
        "custodian_klepto_live_commit_missing_{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    let _ = fs::remove_dir_all(&staging_dir);
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_plan_json(&staging_dir, &[]).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_plan_receipt.v1");
    assert_eq!(value["source_live_apply_preflight_schema"], "code_passenger.klepto_index_registry_refresh_live_apply_preflight_receipt.v1");
    assert_eq!(value["live_commit_action"], "dry_run_live_commit_plan_not_requested");
    assert_eq!(value["plan_live_commit"], false);
    assert_eq!(value["ack_plan_live_commit"], false);
    assert_eq!(value["live_commit_plan_ready"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!staging_dir.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLC002"));
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_plan_json_blocks_missing_commit_ack() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("blocked");
    let args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--plan-live-commit".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_plan_json(&staging_dir, &args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_plan_receipt.v1");
    assert_eq!(value["live_commit_action"], "blocked_missing_live_commit_plan_ack");
    assert_eq!(value["plan_live_commit"], true);
    assert_eq!(value["ack_plan_live_commit"], false);
    assert_eq!(value["live_commit_plan_ack_satisfied"], false);
    assert_eq!(value["live_apply_preflight_ready"], true);
    assert_eq!(value["live_commit_plan_ready"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLC003"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_plan_json_readies_acknowledged_preflight_without_mutation() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("ready");
    let preflight_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
    ];
    let preflight_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_preflight_json(&staging_dir, &preflight_args).unwrap()).unwrap();
    assert_eq!(preflight_value["preflight_action"], "live_apply_preflight_ready_for_future_writer");
    let req_id = preflight_value["live_apply_request_id"].as_str().unwrap().to_string();
    let commit_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--expected-live-apply-request-id".to_string(), req_id.clone(),
        "--plan-live-commit".to_string(), "--ack-plan-live-commit".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_plan_json(&staging_dir, &commit_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_plan_receipt.v1");
    assert_eq!(value["live_commit_action"], "live_apply_commit_plan_ready_for_future_writer");
    assert_eq!(value["live_apply_request_id"], req_id);
    assert_eq!(value["live_apply_request_id_matches"], true);
    assert_eq!(value["plan_live_commit"], true);
    assert_eq!(value["ack_plan_live_commit"], true);
    assert_eq!(value["live_commit_plan_ack_satisfied"], true);
    assert_eq!(value["live_apply_preflight_ready"], true);
    assert_eq!(value["live_commit_plan_permitted"], true);
    assert_eq!(value["live_commit_plan_ready"], true);
    assert_eq!(value["live_commit_performed"], false);
    assert_eq!(value["registry_apply_required"], true);
    assert_eq!(value["index_apply_required"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["live_commit_plan_id"].as_str().unwrap().starts_with("kcmmt_"));
    assert!(value["future_live_locks"].as_array().unwrap().len() >= 1);
    assert!(value["future_backup_paths"].as_array().unwrap().len() >= 2);
    assert!(value["future_commit_journal_path"].as_str().unwrap().ends_with("live_apply_commit_journal.json"));
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLC006"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_exec_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join(format!(
        "custodian_klepto_live_commit_exec_missing_{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    let _ = fs::remove_dir_all(&staging_dir);
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_exec_json(&staging_dir, &[]).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_exec_receipt.v1");
    assert_eq!(value["source_live_apply_commit_plan_schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_plan_receipt.v1");
    assert_eq!(value["live_commit_action"], "dry_run_live_commit_not_requested");
    assert_eq!(value["execute_live_commit"], false);
    assert_eq!(value["ack_execute_live_commit"], false);
    assert_eq!(value["live_commit_execution_permitted"], false);
    assert_eq!(value["live_commit_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!staging_dir.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLC102"));
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_exec_json_blocks_commit_plan_id_mismatch() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("exec_mismatch");
    let args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--plan-live-commit".to_string(), "--ack-plan-live-commit".to_string(),
        "--execute-live-commit".to_string(), "--ack-execute-live-commit".to_string(),
        "--expected-live-commit-plan-id".to_string(), "kcmmt_wrong".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_exec_json(&staging_dir, &args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_exec_receipt.v1");
    assert_eq!(value["live_commit_action"], "blocked_live_commit_plan_id_mismatch");
    assert_eq!(value["execute_live_commit"], true);
    assert_eq!(value["ack_execute_live_commit"], true);
    assert_eq!(value["live_commit_plan_ready"], true);
    assert_eq!(value["live_commit_plan_id_matches"], false);
    assert_eq!(value["live_commit_execution_permitted"], false);
    assert_eq!(value["live_commit_performed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLC105"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_exec_json_ready_plan_would_commit_without_mutation() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("exec_ready");
    let plan_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--plan-live-commit".to_string(), "--ack-plan-live-commit".to_string(),
    ];
    let plan_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_plan_json(&staging_dir, &plan_args).unwrap()).unwrap();
    assert_eq!(plan_value["live_commit_action"], "live_apply_commit_plan_ready_for_future_writer");
    let commit_plan_id = plan_value["live_commit_plan_id"].as_str().unwrap().to_string();
    let mut exec_args = plan_args.clone();
    exec_args.extend([
        "--execute-live-commit".to_string(),
        "--ack-execute-live-commit".to_string(),
        "--expected-live-commit-plan-id".to_string(),
        commit_plan_id.clone(),
    ]);
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_exec_json(&staging_dir, &exec_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_exec_receipt.v1");
    assert_eq!(value["live_commit_action"], "would_live_commit_in_future_writer");
    assert_eq!(value["live_commit_plan_id"], commit_plan_id);
    assert_eq!(value["live_commit_plan_id_matches"], true);
    assert_eq!(value["execute_live_commit"], true);
    assert_eq!(value["ack_execute_live_commit"], true);
    assert_eq!(value["live_commit_execution_ack_satisfied"], true);
    assert_eq!(value["live_commit_plan_ready"], true);
    assert_eq!(value["live_commit_execution_permitted"], true);
    assert_eq!(value["live_commit_performed"], false);
    assert_eq!(value["registry_apply_required"], true);
    assert_eq!(value["index_apply_required"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(value["live_commit_execution_id"].as_str().unwrap().starts_with("kcmit_"));
    assert!(value["future_live_locks"].as_array().unwrap().len() >= 1);
    assert!(value["future_backup_paths"].as_array().unwrap().len() >= 2);
    assert!(value["future_commit_journal_path"].as_str().unwrap().ends_with("live_apply_commit_journal.json"));
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLC106"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_stage_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join(format!(
        "custodian_klepto_live_commit_stage_missing_{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    let _ = fs::remove_dir_all(&staging_dir);
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_json(&staging_dir, &[]).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_receipt.v1");
    assert_eq!(value["source_live_apply_commit_exec_schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_exec_receipt.v1");
    assert_eq!(value["live_commit_stage_action"], "dry_run_live_commit_stage_not_requested");
    assert_eq!(value["stage_live_commit"], false);
    assert_eq!(value["ack_stage_live_commit"], false);
    assert_eq!(value["live_commit_stage_permitted"], false);
    assert_eq!(value["live_commit_stage_performed"], false);
    assert_eq!(value["staging_write_allowed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert!(!staging_dir.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLS102"));
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_stage_json_blocks_missing_stage_ack() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("stage_blocked");
    let args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--plan-live-commit".to_string(), "--ack-plan-live-commit".to_string(),
        "--execute-live-commit".to_string(), "--ack-execute-live-commit".to_string(),
        "--stage-live-commit".to_string(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_json(&staging_dir, &args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_receipt.v1");
    assert_eq!(value["live_commit_stage_action"], "blocked_missing_live_commit_stage_ack");
    assert_eq!(value["stage_live_commit"], true);
    assert_eq!(value["ack_stage_live_commit"], false);
    assert_eq!(value["live_commit_stage_ack_satisfied"], false);
    assert_eq!(value["live_commit_execution_permitted"], true);
    assert_eq!(value["live_commit_stage_permitted"], false);
    assert_eq!(value["live_commit_stage_performed"], false);
    assert_eq!(value["staged_commit_file_count"], 0);
    assert_eq!(value["staging_write_allowed"], false);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    let stage_path = PathBuf::from(value["selected_stage_path"].as_str().unwrap());
    assert!(!stage_path.join("live_apply_commit_stage_manifest.json").exists());
    assert!(!stage_path.join("live_apply_commit_journal.json").exists());
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLS103"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_stage_json_writes_isolated_stage_only() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("stage_ready");
    let base_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--plan-live-commit".to_string(), "--ack-plan-live-commit".to_string(),
        "--execute-live-commit".to_string(), "--ack-execute-live-commit".to_string(),
    ];
    let exec_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_exec_json(&staging_dir, &base_args).unwrap()).unwrap();
    assert_eq!(exec_value["live_commit_action"], "would_live_commit_in_future_writer");
    let commit_exec_id = exec_value["live_commit_execution_id"].as_str().unwrap().to_string();
    let mut stage_args = base_args.clone();
    stage_args.extend([
        "--stage-live-commit".to_string(),
        "--ack-stage-live-commit".to_string(),
        "--expected-live-commit-execution-id".to_string(),
        commit_exec_id.clone(),
    ]);
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_json(&staging_dir, &stage_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_receipt.v1");
    assert_eq!(value["live_commit_stage_action"], "live_commit_staged_for_future_writer");
    assert_eq!(value["live_commit_execution_id"], commit_exec_id);
    assert_eq!(value["live_commit_execution_id_matches"], true);
    assert_eq!(value["stage_live_commit"], true);
    assert_eq!(value["ack_stage_live_commit"], true);
    assert_eq!(value["live_commit_stage_ack_satisfied"], true);
    assert_eq!(value["live_commit_execution_permitted"], true);
    assert_eq!(value["live_commit_stage_permitted"], true);
    assert_eq!(value["live_commit_stage_ready"], true);
    assert_eq!(value["live_commit_stage_performed"], true);
    assert_eq!(value["staging_write_allowed"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["live_commit_performed"], false);
    assert_eq!(value["staged_commit_file_count"], 3);
    let stage_path = PathBuf::from(value["selected_stage_path"].as_str().unwrap());
    assert!(stage_path.join("live_commit_execution_receipt.json").exists());
    assert!(stage_path.join("live_apply_commit_journal.json").exists());
    assert!(stage_path.join("live_apply_commit_stage_manifest.json").exists());
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLS106"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_stage_verify_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join(format!(
        "custodian_klepto_live_commit_stage_verify_missing_{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    let _ = fs::remove_dir_all(&staging_dir);
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_verify_json(&staging_dir, &[]).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_verify_receipt.v1");
    assert_eq!(value["verification_action"], "missing_stage");
    assert_eq!(value["verified"], false);
    assert_eq!(value["discovered_stage_count"], 0);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["live_commit_performed"], false);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLV001"));
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_stage_verify_json_verifies_isolated_stage() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("stage_verify_ready");
    let base_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--plan-live-commit".to_string(), "--ack-plan-live-commit".to_string(),
        "--execute-live-commit".to_string(), "--ack-execute-live-commit".to_string(),
    ];
    let exec_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_exec_json(&staging_dir, &base_args).unwrap()).unwrap();
    let commit_exec_id = exec_value["live_commit_execution_id"].as_str().unwrap().to_string();
    let mut stage_args = base_args.clone();
    stage_args.extend([
        "--stage-live-commit".to_string(),
        "--ack-stage-live-commit".to_string(),
        "--expected-live-commit-execution-id".to_string(),
        commit_exec_id.clone(),
    ]);
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_json(&staging_dir, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["live_commit_stage_action"], "live_commit_staged_for_future_writer");

    let verify_args = vec!["--expected-live-commit-execution-id".to_string(), commit_exec_id.clone()];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_verify_json(&staging_dir, &verify_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_verify_receipt.v1");
    assert_eq!(value["verification_action"], "live_commit_stage_verified_for_future_writer");
    assert_eq!(value["verified"], true);
    assert_eq!(value["manifest_schema_ok"], true);
    assert_eq!(value["commit_exec_receipt_schema_ok"], true);
    assert_eq!(value["commit_journal_schema_ok"], true);
    assert_eq!(value["live_commit_execution_id"], commit_exec_id);
    assert_eq!(value["live_commit_execution_id_matches"], true);
    assert_eq!(value["expected_live_commit_execution_id_matches"], true);
    assert_eq!(value["staged_commit_file_count_manifest"], 3);
    assert_eq!(value["staged_commit_file_count_observed"], 3);
    assert_eq!(value["staged_commit_file_count_matches"], true);
    assert_eq!(value["live_commit_stage_ready"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["live_commit_performed"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLV004"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_stage_verify_json_blocks_execution_id_mismatch() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("stage_verify_mismatch");
    let base_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--plan-live-commit".to_string(), "--ack-plan-live-commit".to_string(),
        "--execute-live-commit".to_string(), "--ack-execute-live-commit".to_string(),
        "--stage-live-commit".to_string(), "--ack-stage-live-commit".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_json(&staging_dir, &base_args).unwrap()).unwrap();
    assert_eq!(stage_value["live_commit_stage_action"], "live_commit_staged_for_future_writer");
    let verify_args = vec!["--expected-live-commit-execution-id".to_string(), "kcmit_wrong".to_string()];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_verify_json(&staging_dir, &verify_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_verify_receipt.v1");
    assert_eq!(value["verification_action"], "blocked_live_commit_execution_id_mismatch");
    assert_eq!(value["verified"], false);
    assert_eq!(value["expected_live_commit_execution_id_matches"], false);
    assert_eq!(value["live_commit_execution_id_matches"], true);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["live_commit_performed"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLV003"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_gate_json_missing_stage_is_safe() {
    let staging_dir = std::env::temp_dir().join(format!(
        "custodian_klepto_live_commit_gate_missing_{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    let _ = fs::remove_dir_all(&staging_dir);
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_gate_json(&staging_dir, &[]).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_gate_receipt.v1");
    assert_eq!(value["source_live_commit_stage_verify_schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_stage_verify_receipt.v1");
    assert_eq!(value["live_commit_gate_action"], "dry_run_live_commit_gate_not_requested");
    assert_eq!(value["stage_verified"], false);
    assert_eq!(value["open_live_commit_gate"], false);
    assert_eq!(value["ack_open_live_commit_gate"], false);
    assert_eq!(value["live_commit_gate_ready"], false);
    assert_eq!(value["live_commit_gate_id"], serde_json::Value::Null);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["live_commit_performed"], false);
    assert!(!staging_dir.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLG002"));
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_gate_json_blocks_missing_gate_ack() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("gate_blocked");
    let base_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--plan-live-commit".to_string(), "--ack-plan-live-commit".to_string(),
        "--execute-live-commit".to_string(), "--ack-execute-live-commit".to_string(),
        "--stage-live-commit".to_string(), "--ack-stage-live-commit".to_string(),
    ];
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_json(&staging_dir, &base_args).unwrap()).unwrap();
    assert_eq!(stage_value["live_commit_stage_action"], "live_commit_staged_for_future_writer");
    let commit_exec_id = stage_value["live_commit_execution_id"].as_str().unwrap().to_string();
    let gate_args = vec![
        "--open-live-commit-gate".to_string(),
        "--expected-live-commit-execution-id".to_string(), commit_exec_id.clone(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_gate_json(&staging_dir, &gate_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_gate_receipt.v1");
    assert_eq!(value["live_commit_gate_action"], "blocked_missing_live_commit_gate_ack");
    assert_eq!(value["stage_verified"], true);
    assert_eq!(value["open_live_commit_gate"], true);
    assert_eq!(value["ack_open_live_commit_gate"], false);
    assert_eq!(value["live_commit_gate_ack_satisfied"], false);
    assert_eq!(value["live_commit_gate_ready"], false);
    assert_eq!(value["live_commit_gate_id"], serde_json::Value::Null);
    assert_eq!(value["live_commit_execution_id"], commit_exec_id);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["live_commit_performed"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLG103"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn klepto_index_registry_refresh_live_apply_commit_gate_json_readies_verified_stage_without_mutation() {
    let (root, registry_path, index_dir, staging_dir, live_should_not_write) = phase38_live_commit_fixture("gate_ready");
    let base_args = vec![
        "--execute-swap".to_string(), "--ack-execute-swap".to_string(),
        "--execute-apply".to_string(), "--ack-execute-apply".to_string(),
        "--request-live-apply".to_string(), "--ack-request-live-apply".to_string(),
        "--preflight-live-apply".to_string(), "--ack-preflight-live-apply".to_string(),
        "--plan-live-commit".to_string(), "--ack-plan-live-commit".to_string(),
        "--execute-live-commit".to_string(), "--ack-execute-live-commit".to_string(),
    ];
    let exec_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_exec_json(&staging_dir, &base_args).unwrap()).unwrap();
    let commit_exec_id = exec_value["live_commit_execution_id"].as_str().unwrap().to_string();
    let mut stage_args = base_args.clone();
    stage_args.extend([
        "--stage-live-commit".to_string(),
        "--ack-stage-live-commit".to_string(),
        "--expected-live-commit-execution-id".to_string(),
        commit_exec_id.clone(),
    ]);
    let stage_value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_stage_json(&staging_dir, &stage_args).unwrap()).unwrap();
    assert_eq!(stage_value["live_commit_stage_action"], "live_commit_staged_for_future_writer");

    let gate_args = vec![
        "--open-live-commit-gate".to_string(),
        "--ack-open-live-commit-gate".to_string(),
        "--expected-live-commit-execution-id".to_string(), commit_exec_id.clone(),
    ];
    let value: serde_json::Value = serde_json::from_str(&klepto_index_registry_refresh_live_apply_commit_gate_json(&staging_dir, &gate_args).unwrap()).unwrap();
    assert_eq!(value["schema"], "code_passenger.klepto_index_registry_refresh_live_apply_commit_gate_receipt.v1");
    assert_eq!(value["live_commit_gate_action"], "live_commit_gate_ready_for_future_writer");
    assert_eq!(value["stage_verified"], true);
    assert_eq!(value["open_live_commit_gate"], true);
    assert_eq!(value["ack_open_live_commit_gate"], true);
    assert_eq!(value["live_commit_gate_ack_satisfied"], true);
    assert_eq!(value["live_commit_gate_ready"], true);
    assert!(value["live_commit_gate_id"].as_str().unwrap().starts_with("klcgt_"));
    assert_eq!(value["live_commit_execution_id"], commit_exec_id);
    assert_eq!(value["live_commit_execution_id_matches"], true);
    assert_eq!(value["expected_live_commit_execution_id_matches"], true);
    assert_eq!(value["staged_commit_file_count_observed"], 3);
    assert_eq!(value["write_allowed"], false);
    assert_eq!(value["registry_mutated"], false);
    assert_eq!(value["index_mutated"], false);
    assert_eq!(value["live_commit_performed"], false);
    assert!(!live_should_not_write.exists());
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| finding["code"] == "CPLG106"));
    let _ = fs::remove_file(&registry_path);
    let _ = fs::remove_dir_all(&index_dir);
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn missing_root_is_safe() {
    let root = std::env::temp_dir().join("definitely_missing_custodian_graph_root");
    let graph = build_code_graph(&root, 4).unwrap();
    assert!(!graph.root_exists);
    assert_eq!(graph.summary.crate_count, 0);
    assert_eq!(graph.summary.finding_count, 1);
}
