//! The code query engine (code_passenger.query.v1) and pin sets.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_code_query_spec(args: &[String]) -> CodeQuerySpec {
    let mut spec = CodeQuerySpec { raw_args: args.to_vec(), ..CodeQuerySpec::default() };
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--kind" | "-k" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.kind = normalize_query_kind(value);
                    idx += 1;
                }
            }
            "--name" | "--name-contains" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.name_contains = Some(value.clone());
                    idx += 1;
                }
            }
            "--contains" | "--text" | "--text-contains" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.text_contains = Some(value.clone());
                    idx += 1;
                }
            }
            "--path" | "--path-contains" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.path_contains = Some(value.clone());
                    idx += 1;
                }
            }
            "--dep" | "--dependency" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.dep = Some(value.clone());
                    idx += 1;
                }
            }
            "--feature" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.feature = Some(value.clone());
                    idx += 1;
                }
            }
            "--origin" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.origin = Some(value.clone());
                    idx += 1;
                }
            }
            "--scope" | "--scope-contains" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.scope_contains = Some(value.clone());
                    idx += 1;
                }
            }
            "--returns" | "--return" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.returns_contains = Some(value.clone());
                    idx += 1;
                }
            }
            "--takes" | "--arg" | "--argument" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.takes_contains = Some(value.clone());
                    idx += 1;
                }
            }
            "--item" => {
                if let Some(value) = args.get(idx + 1) {
                    spec.item = Some(value.clone());
                    idx += 1;
                }
            }
            "--public" | "--public-only" => spec.public_only = true,
            "--private" | "--private-only" => spec.private_only = true,
            "--limit" => {
                if let Some(value) = args.get(idx + 1).and_then(|s| s.parse::<usize>().ok()) {
                    spec.limit = value.max(1);
                    idx += 1;
                }
            }
            value if !value.starts_with('-') && spec.text_contains.is_none() => {
                spec.text_contains = Some(value.to_string());
            }
            _ => {}
        }
        idx += 1;
    }
    spec
}

pub(crate) fn normalize_query_kind(kind: &str) -> String {
    match kind.trim().to_lowercase().replace('_', "-").as_str() {
        "function" | "functions" | "fn" | "fns" => "fn".to_string(),
        "import" | "imports" | "use" | "uses" => "import".to_string(),
        "use-site" | "use-sites" | "site" | "sites" | "usage" => "use-site".to_string(),
        "dependency" | "dependencies" | "dep" | "deps" => "dep".to_string(),
        "feature" | "features" | "feat" | "feats" => "feature".to_string(),
        "file" | "files" => "file".to_string(),
        "crate" | "crates" | "package" | "packages" => "crate".to_string(),
        "finding" | "findings" | "warn" | "warning" => "finding".to_string(),
        "all" | "*" | "any" => "all".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn run_code_query(graph: &CodeGraph, spec: CodeQuerySpec) -> CodeQueryReport {
    let mut results = Vec::<CodeQueryResult>::new();
    let mut findings = Vec::<FindingNode>::new();
    findings.extend(graph.findings.clone());

    if !graph.root_exists {
        return CodeQueryReport {
            schema: "code_passenger.query.v1".to_string(),
            graph_schema: graph.schema.clone(),
            root: graph.root.clone(),
            root_exists: graph.root_exists,
            max_depth: graph.max_depth,
            query: spec,
            result_count: 0,
            truncated: false,
            results,
            summary: graph.summary.clone(),
            findings,
        };
    }

    let kind = spec.kind.as_str();
    if kind == "all" || kind == "crate" {
        for krate in &graph.crates {
            let mut attrs = BTreeMap::new();
            attrs.insert("manifest_path".to_string(), krate.manifest_path.clone());
            attrs.insert("src_root".to_string(), krate.src_root.clone().unwrap_or_else(|| "-".to_string()));
            let hay = [&krate.id, &krate.name, &krate.crate_root, &krate.manifest_path];
            if query_text_matches(&spec, &hay) && path_filter_matches(&spec, Some(&krate.crate_root)) {
                results.push(CodeQueryResult {
                    id: krate.id.clone(),
                    kind: "crate".to_string(),
                    label: krate.name.clone(),
                    crate_name: Some(krate.name.clone()),
                    path: Some(krate.crate_root.clone()),
                    line: None,
                    column: None,
                    score_reason: vec!["crate metadata matched query".to_string()],
                    attributes: attrs,
                });
            }
        }
    }

    if kind == "all" || kind == "feature" {
        for feature in &graph.features {
            let forwards = feature.forwards_dependency_features.iter().map(|fw| fw.raw.clone()).collect::<Vec<_>>().join(",");
            let members_joined = feature.members.join(",");
            let enables_joined = feature.enables_deps.join(",");
            let hay = [&feature.id, &feature.name, &feature.crate_name, &members_joined, &enables_joined, &forwards];
            if !query_text_matches(&spec, &hay) || !feature_filter_matches(&spec, &feature.name) || !dep_list_filter_matches(&spec, &feature.enables_deps) {
                continue;
            }
            let mut attrs = BTreeMap::new();
            attrs.insert("members".to_string(), dash_if_empty(&feature.members));
            attrs.insert("enables_deps".to_string(), dash_if_empty(&feature.enables_deps));
            attrs.insert("forwards_dependency_features".to_string(), if forwards.is_empty() { "-".to_string() } else { forwards });
            results.push(CodeQueryResult {
                id: feature.id.clone(),
                kind: "feature".to_string(),
                label: feature.name.clone(),
                crate_name: Some(feature.crate_name.clone()),
                path: None,
                line: None,
                column: None,
                score_reason: vec!["feature declaration matched query".to_string()],
                attributes: attrs,
            });
        }
    }

    if kind == "all" || kind == "dep" {
        for dep in &graph.deps {
            let enabled_joined = dep.enabled_by_features.join(",");
            let hay = [&dep.id, &dep.name, &dep.cargo_key, &dep.crate_name, &dep.source_table, &enabled_joined];
            if !query_text_matches(&spec, &hay) || !dep_filter_matches(&spec, &dep.name) || !feature_list_filter_matches(&spec, &dep.enabled_by_features) {
                continue;
            }
            let mut attrs = BTreeMap::new();
            attrs.insert("cargo_key".to_string(), dep.cargo_key.clone());
            attrs.insert("optional".to_string(), dep.optional.to_string());
            attrs.insert("source_table".to_string(), dep.source_table.clone());
            attrs.insert("enabled_by_features".to_string(), dash_if_empty(&dep.enabled_by_features));
            attrs.insert("matching_use_sites".to_string(), matching_dep_sites(graph, &dep.crate_id, &dep.name).len().to_string());
            results.push(CodeQueryResult {
                id: dep.id.clone(),
                kind: "dep".to_string(),
                label: dep.name.clone(),
                crate_name: Some(dep.crate_name.clone()),
                path: None,
                line: None,
                column: None,
                score_reason: vec!["dependency metadata matched query".to_string()],
                attributes: attrs,
            });
        }
    }

    if kind == "all" || kind == "file" {
        for file in &graph.files {
            let hay = [&file.id, &file.path, &file.relative_path, &file.crate_name];
            if !query_text_matches(&spec, &hay) || !path_filter_matches(&spec, Some(&file.path)) {
                continue;
            }
            let mut attrs = BTreeMap::new();
            attrs.insert("relative_path".to_string(), file.relative_path.clone());
            attrs.insert("line_count".to_string(), file.line_count.to_string());
            attrs.insert("byte_count".to_string(), file.byte_count.to_string());
            results.push(CodeQueryResult {
                id: file.id.clone(),
                kind: "file".to_string(),
                label: file.relative_path.clone(),
                crate_name: Some(file.crate_name.clone()),
                path: Some(file.path.clone()),
                line: None,
                column: None,
                score_reason: vec!["file path matched query".to_string()],
                attributes: attrs,
            });
        }
    }

    if kind == "all" || kind == "fn" {
        for func in &graph.functions {
            let is_public = func.visibility == "pub" || func.visibility.starts_with("pub(");
            let hay = [&func.id, &func.name, &func.path, &func.crate_name, &func.signature_preview];
            if !query_text_matches(&spec, &hay)
                || !name_filter_matches(&spec, &func.name)
                || !path_filter_matches(&spec, Some(&func.path))
                || !visibility_filter_matches(&spec, is_public)
                || !signature_filter_matches(&spec, &func.signature_preview)
            {
                continue;
            }
            let mut attrs = BTreeMap::new();
            attrs.insert("visibility".to_string(), func.visibility.clone());
            attrs.insert("signature_preview".to_string(), func.signature_preview.clone());
            attrs.insert("public".to_string(), is_public.to_string());
            results.push(CodeQueryResult {
                id: func.id.clone(),
                kind: "fn".to_string(),
                label: func.name.clone(),
                crate_name: Some(func.crate_name.clone()),
                path: Some(func.path.clone()),
                line: Some(func.line),
                column: Some(func.column),
                score_reason: vec!["function signature matched query".to_string()],
                attributes: attrs,
            });
        }
    }

    if kind == "all" || kind == "import" {
        for import in &graph.imports {
            let origin = format!("{:?}", import.origin);
            let hay = [&import.id, &import.root, &import.full_path, &import.path, &import.crate_name, &origin];
            if !query_text_matches(&spec, &hay)
                || !name_filter_matches(&spec, &import.root)
                || !path_filter_matches(&spec, Some(&import.path))
                || !visibility_filter_matches(&spec, import.is_public_use)
                || !dep_filter_matches(&spec, &import.root)
                || !origin_filter_matches(&spec, &origin)
                || !item_filter_matches(&spec, &import.full_path)
            {
                continue;
            }
            let mut attrs = BTreeMap::new();
            attrs.insert("root".to_string(), import.root.clone());
            attrs.insert("full_path".to_string(), import.full_path.clone());
            attrs.insert("origin".to_string(), origin);
            attrs.insert("public".to_string(), import.is_public_use.to_string());
            results.push(CodeQueryResult {
                id: import.id.clone(),
                kind: "import".to_string(),
                label: import.root.clone(),
                crate_name: Some(import.crate_name.clone()),
                path: Some(import.path.clone()),
                line: Some(import.line),
                column: Some(import.column),
                score_reason: vec!["import matched query".to_string()],
                attributes: attrs,
            });
        }
    }

    if kind == "all" || kind == "use-site" {
        for site in &graph.use_sites {
            let kind_name = format!("{:?}", site.kind);
            let features_joined = site.feature_candidates.join(",");
            let hay = [&site.id, &site.dep, &site.full_path, &site.head, &site.path, &site.scope, &features_joined, &kind_name];
            if !query_text_matches(&spec, &hay)
                || !name_filter_matches(&spec, &site.head)
                || !path_filter_matches(&spec, Some(&site.path))
                || !dep_filter_matches(&spec, &site.dep)
                || !feature_list_filter_matches(&spec, &site.feature_candidates)
                || !scope_filter_matches(&spec, &site.scope)
                || !item_filter_matches(&spec, &site.full_path)
            {
                continue;
            }
            let mut attrs = BTreeMap::new();
            attrs.insert("dep".to_string(), site.dep.clone());
            attrs.insert("full_path".to_string(), site.full_path.clone());
            attrs.insert("head".to_string(), site.head.clone());
            attrs.insert("site_kind".to_string(), kind_name);
            attrs.insert("scope".to_string(), site.scope.clone());
            attrs.insert("optional_dep".to_string(), site.optional_dep.to_string());
            attrs.insert("feature_candidates".to_string(), dash_if_empty(&site.feature_candidates));
            results.push(CodeQueryResult {
                id: site.id.clone(),
                kind: "use-site".to_string(),
                label: site.dep.clone(),
                crate_name: Some(site.crate_name.clone()),
                path: Some(site.path.clone()),
                line: Some(site.line),
                column: Some(site.column),
                score_reason: vec!["use-site matched query".to_string()],
                attributes: attrs,
            });
        }
    }

    if kind == "all" || kind == "finding" {
        for finding in &graph.findings {
            let path = finding.path.clone().unwrap_or_else(|| "-".to_string());
            let hay = [&finding.severity, &finding.code, &finding.message, &path];
            if !query_text_matches(&spec, &hay) || !path_filter_matches(&spec, finding.path.as_ref()) {
                continue;
            }
            let mut attrs = BTreeMap::new();
            attrs.insert("severity".to_string(), finding.severity.clone());
            attrs.insert("code".to_string(), finding.code.clone());
            attrs.insert("message".to_string(), finding.message.clone());
            results.push(CodeQueryResult {
                id: format!("finding_{}", stable_id("", &format!("{}{}{}", finding.severity, finding.code, finding.message))),
                kind: "finding".to_string(),
                label: finding.code.clone(),
                crate_name: None,
                path: finding.path.clone(),
                line: finding.line,
                column: finding.column,
                score_reason: vec!["finding matched query".to_string()],
                attributes: attrs,
            });
        }
    }

    results.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.path.cmp(&b.path)).then(a.line.cmp(&b.line)).then(a.label.cmp(&b.label)));
    let total = results.len();
    let truncated = total > spec.limit;
    results.truncate(spec.limit);

    CodeQueryReport {
        schema: "code_passenger.query.v1".to_string(),
        graph_schema: graph.schema.clone(),
        root: graph.root.clone(),
        root_exists: graph.root_exists,
        max_depth: graph.max_depth,
        query: spec,
        result_count: total,
        truncated,
        results,
        summary: graph.summary.clone(),
        findings,
    }
}

pub(crate) fn query_text_matches<T: AsRef<str>>(spec: &CodeQuerySpec, haystacks: &[T]) -> bool {
    if let Some(text) = &spec.text_contains {
        any_contains(&text.to_lowercase(), haystacks)
    } else {
        true
    }
}

pub(crate) fn name_filter_matches(spec: &CodeQuerySpec, name: &str) -> bool {
    if let Some(want) = &spec.name_contains {
        name.to_lowercase().contains(&want.to_lowercase())
    } else {
        true
    }
}

pub(crate) fn path_filter_matches(spec: &CodeQuerySpec, path: Option<&String>) -> bool {
    if let Some(want) = &spec.path_contains {
        path.map(|p| p.to_lowercase().contains(&want.to_lowercase())).unwrap_or(false)
    } else {
        true
    }
}

pub(crate) fn dep_filter_matches(spec: &CodeQuerySpec, dep: &str) -> bool {
    if let Some(want) = &spec.dep {
        norm_dep(dep) == norm_dep(want) || dep.to_lowercase().contains(&want.to_lowercase())
    } else {
        true
    }
}

pub(crate) fn dep_list_filter_matches(spec: &CodeQuerySpec, deps: &[String]) -> bool {
    if let Some(want) = &spec.dep {
        deps.iter().any(|dep| dep_filter_matches(spec, dep) || norm_dep(dep) == norm_dep(want))
    } else {
        true
    }
}

pub(crate) fn feature_filter_matches(spec: &CodeQuerySpec, feature: &str) -> bool {
    if let Some(want) = &spec.feature {
        feature == want || feature.to_lowercase().contains(&want.to_lowercase())
    } else {
        true
    }
}

pub(crate) fn feature_list_filter_matches(spec: &CodeQuerySpec, features: &[String]) -> bool {
    if let Some(want) = &spec.feature {
        features.iter().any(|feature| feature == want || feature.to_lowercase().contains(&want.to_lowercase()))
    } else {
        true
    }
}

pub(crate) fn origin_filter_matches(spec: &CodeQuerySpec, origin: &str) -> bool {
    if let Some(want) = &spec.origin {
        origin.to_lowercase().contains(&want.to_lowercase())
    } else {
        true
    }
}

/// `--item`: the full path is the item, or ends with `::item`.
pub(crate) fn item_filter_matches(spec: &CodeQuerySpec, full_path: &str) -> bool {
    match &spec.item {
        Some(want) => path_names_item(full_path, want),
        None => true,
    }
}

/// One rule for "this path names that item", used by `--item` and the readings.
pub(crate) fn path_names_item(full_path: &str, item: &str) -> bool {
    let item = item.trim().trim_start_matches("::");
    full_path == item || full_path.ends_with(&format!("::{item}"))
}

pub(crate) fn scope_filter_matches(spec: &CodeQuerySpec, scope: &str) -> bool {
    if let Some(want) = &spec.scope_contains {
        scope.to_lowercase().contains(&want.to_lowercase())
    } else {
        true
    }
}

pub(crate) fn visibility_filter_matches(spec: &CodeQuerySpec, is_public: bool) -> bool {
    if spec.public_only && !is_public {
        return false;
    }
    if spec.private_only && is_public {
        return false;
    }
    true
}

pub(crate) fn signature_filter_matches(spec: &CodeQuerySpec, signature: &str) -> bool {
    let lower = signature.to_lowercase();
    if let Some(want) = &spec.returns_contains {
        if !lower.contains(&want.to_lowercase()) {
            return false;
        }
    }
    if let Some(want) = &spec.takes_contains {
        if !lower.contains(&want.to_lowercase()) {
            return false;
        }
    }
    true
}

pub fn render_code_query(report: &CodeQueryReport) -> String {
    let mut out = String::new();
    out.push_str("CODEPASSENGER :: CODE QUERY\n");
    out.push_str("────────────────────────────\n");
    out.push_str(&format!("root: {}\n", report.root));
    out.push_str(&format!("schema: {}\n", report.schema));
    out.push_str(&format!("graph_schema: {}\n", report.graph_schema));
    out.push_str(&format!("exists: {}\n", report.root_exists));
    out.push_str(&format!("max_depth: {}\n", report.max_depth));
    out.push('\n');

    out.push_str("query:\n");
    out.push_str(&format!("  kind: {}\n", report.query.kind));
    out.push_str(&format!("  raw_args: {}\n", if report.query.raw_args.is_empty() { "-".to_string() } else { report.query.raw_args.join(" ") }));
    out.push_str(&format!("  name: {}\n", opt_dash(report.query.name_contains.as_deref())));
    out.push_str(&format!("  contains: {}\n", opt_dash(report.query.text_contains.as_deref())));
    out.push_str(&format!("  path: {}\n", opt_dash(report.query.path_contains.as_deref())));
    out.push_str(&format!("  dep: {}\n", opt_dash(report.query.dep.as_deref())));
    out.push_str(&format!("  feature: {}\n", opt_dash(report.query.feature.as_deref())));
    out.push_str(&format!("  origin: {}\n", opt_dash(report.query.origin.as_deref())));
    out.push_str(&format!("  scope: {}\n", opt_dash(report.query.scope_contains.as_deref())));
    out.push_str(&format!("  returns: {}\n", opt_dash(report.query.returns_contains.as_deref())));
    out.push_str(&format!("  takes: {}\n", opt_dash(report.query.takes_contains.as_deref())));
    out.push_str(&format!("  public_only: {}\n", report.query.public_only));
    out.push_str(&format!("  private_only: {}\n", report.query.private_only));
    out.push_str(&format!("  limit: {}\n", report.query.limit));
    out.push('\n');

    out.push_str("summary:\n");
    out.push_str(&format!("  crates: {}\n", report.summary.crate_count));
    out.push_str(&format!("  files: {}\n", report.summary.file_count));
    out.push_str(&format!("  features: {}\n", report.summary.feature_count));
    out.push_str(&format!("  deps: {} (optional: {})\n", report.summary.dependency_count, report.summary.optional_dependency_count));
    out.push_str(&format!("  functions: {}\n", report.summary.function_count));
    out.push_str(&format!("  imports: {}\n", report.summary.import_count));
    out.push_str(&format!("  use_sites: {}\n", report.summary.use_site_count));
    out.push_str(&format!("  findings: {}\n", report.summary.finding_count));
    out.push('\n');

    if !report.root_exists {
        out.push_str("status:\n");
        out.push_str("  MISS root path does not exist; query results are intentionally empty\n");
        for finding in &report.findings {
            out.push_str(&format!("  FIND {} {} {}\n", finding.severity, finding.code, finding.message));
        }
        out.push('\n');
        out.push_str("safe next commands:\n");
        out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", report.root, report.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", report.root, report.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- code-query-json {} {} {}\n", report.root, report.max_depth, shell_join(&report.query.raw_args)));
        return out;
    }

    out.push_str("results:\n");
    out.push_str(&format!("  count: {}\n", report.result_count));
    out.push_str(&format!("  shown: {}\n", report.results.len()));
    out.push_str(&format!("  truncated: {}\n", report.truncated));
    if report.results.is_empty() {
        out.push_str("  NONE no graph regions matched the query\n");
    }

    for (idx, result) in report.results.iter().enumerate() {
        let code = query_region_code(&result.kind);
        let location = match (&result.path, result.line, result.column) {
            (Some(path), Some(line), Some(column)) => format!("{}:{}:{}", path, line, column),
            (Some(path), Some(line), None) => format!("{}:{}", path, line),
            (Some(path), None, _) => path.clone(),
            _ => "-".to_string(),
        };
        let crate_name = result.crate_name.as_deref().unwrap_or("-");
        out.push_str(&format!(
            "  [{:03}] {:<5} {:<28} {}\n",
            idx + 1,
            code,
            truncate(&result.label, 28),
            location,
        ));
        out.push_str(&format!("        id={} crate={} kind={}\n", result.id, crate_name, result.kind));
        if !result.score_reason.is_empty() {
            out.push_str(&format!("        reason={}\n", result.score_reason.join(",")));
        }
        for (key, value) in &result.attributes {
            out.push_str(&format!("        {}={}\n", key, truncate(value, 100)));
        }
    }

    out.push('\n');
    out.push_str("safe next commands:\n");
    out.push_str(&format!("  cargo run -p custodian -- code-query-json {} {} {}\n", report.root, report.max_depth, shell_join(&report.query.raw_args)));
    out.push_str(&format!("  cargo run -p custodian -- code-query {} {} {}\n", report.root, report.max_depth, shell_join(&report.query.raw_args)));
    out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", report.root, report.max_depth));
    out
}

pub(crate) fn build_pin_set_from_query_report(report: &CodeQueryReport) -> CodePinSet {
    let pins = report
        .results
        .iter()
        .enumerate()
        .map(|(idx, result)| code_pin_from_query_result(idx + 1, result))
        .collect::<Vec<_>>();

    CodePinSet {
        schema: "code_passenger.pin_set.v1".to_string(),
        graph_schema: report.graph_schema.clone(),
        root: report.root.clone(),
        root_exists: report.root_exists,
        max_depth: report.max_depth,
        source_query: report.query.clone(),
        pin_count: pins.len(),
        truncated: report.truncated,
        pins,
        summary: report.summary.clone(),
        findings: report.findings.clone(),
    }
}

pub(crate) fn code_pin_from_query_result(index: usize, result: &CodeQueryResult) -> CodePin {
    let range = match (&result.path, result.line, result.column) {
        (Some(path), Some(line), Some(column)) => Some(CodePinRange {
            path: path.clone(),
            start_line: line,
            start_column: column,
            end_line: line,
            end_column: column.saturating_add(1),
            range_kind: "point".to_string(),
        }),
        _ => None,
    };
    let copy_hint = match &range {
        Some(r) => format!("{}:{}:{}", r.path, r.start_line, r.start_column),
        None => "metadata-only pin; no source range available".to_string(),
    };

    CodePin {
        id: format!("pin_{index:03}"),
        source_result_id: result.id.clone(),
        kind: result.kind.clone(),
        label: result.label.clone(),
        crate_name: result.crate_name.clone(),
        path: result.path.clone(),
        line: result.line,
        column: result.column,
        range,
        copy_hint,
        attributes: result.attributes.clone(),
    }
}

pub fn render_pin_query(pin_set: &CodePinSet) -> String {
    let mut out = String::new();
    out.push_str("CODEPASSENGER :: PIN QUERY\n");
    out.push_str("──────────────────────────\n");
    out.push_str(&format!("root: {}\n", pin_set.root));
    out.push_str(&format!("schema: {}\n", pin_set.schema));
    out.push_str(&format!("graph_schema: {}\n", pin_set.graph_schema));
    out.push_str(&format!("exists: {}\n", pin_set.root_exists));
    out.push_str(&format!("max_depth: {}\n", pin_set.max_depth));
    out.push('\n');

    out.push_str("source query:\n");
    out.push_str(&format!("  kind: {}\n", pin_set.source_query.kind));
    out.push_str(&format!("  raw_args: {}\n", if pin_set.source_query.raw_args.is_empty() { "-".to_string() } else { pin_set.source_query.raw_args.join(" ") }));
    out.push_str(&format!("  dep: {}\n", opt_dash(pin_set.source_query.dep.as_deref())));
    out.push_str(&format!("  feature: {}\n", opt_dash(pin_set.source_query.feature.as_deref())));
    out.push_str(&format!("  scope: {}\n", opt_dash(pin_set.source_query.scope_contains.as_deref())));
    out.push_str(&format!("  limit: {}\n", pin_set.source_query.limit));
    out.push('\n');

    out.push_str("summary:\n");
    out.push_str(&format!("  crates: {}\n", pin_set.summary.crate_count));
    out.push_str(&format!("  files: {}\n", pin_set.summary.file_count));
    out.push_str(&format!("  features: {}\n", pin_set.summary.feature_count));
    out.push_str(&format!("  deps: {} (optional: {})\n", pin_set.summary.dependency_count, pin_set.summary.optional_dependency_count));
    out.push_str(&format!("  functions: {}\n", pin_set.summary.function_count));
    out.push_str(&format!("  imports: {}\n", pin_set.summary.import_count));
    out.push_str(&format!("  use_sites: {}\n", pin_set.summary.use_site_count));
    out.push_str(&format!("  findings: {}\n", pin_set.summary.finding_count));
    out.push('\n');

    if !pin_set.root_exists {
        out.push_str("status:\n");
        out.push_str("  MISS root path does not exist; pin set is intentionally empty\n");
        for finding in &pin_set.findings {
            out.push_str(&format!("  FIND {} {} {}\n", finding.severity, finding.code, finding.message));
        }
        out.push('\n');
        out.push_str("safe next commands:\n");
        out.push_str(&format!("  cargo run -p custodian -- code-query-json {} {} {}\n", pin_set.root, pin_set.max_depth, shell_join(&pin_set.source_query.raw_args)));
        out.push_str(&format!("  cargo run -p custodian -- pin-query-json {} {} {}\n", pin_set.root, pin_set.max_depth, shell_join(&pin_set.source_query.raw_args)));
        return out;
    }

    out.push_str("pins:\n");
    out.push_str(&format!("  count: {}\n", pin_set.pin_count));
    out.push_str(&format!("  truncated_source_results: {}\n", pin_set.truncated));
    if pin_set.pins.is_empty() {
        out.push_str("  NONE no query results were pinned\n");
    }

    for pin in &pin_set.pins {
        let code = query_region_code(&pin.kind);
        let location = match (&pin.path, pin.line, pin.column) {
            (Some(path), Some(line), Some(column)) => format!("{}:{}:{}", path, line, column),
            (Some(path), Some(line), None) => format!("{}:{}", path, line),
            (Some(path), None, _) => path.clone(),
            _ => "-".to_string(),
        };
        let crate_name = pin.crate_name.as_deref().unwrap_or("-");
        out.push_str(&format!(
            "  {} {:<5} {:<28} {}\n",
            pin.id,
            code,
            truncate(&pin.label, 28),
            location,
        ));
        out.push_str(&format!("        source_result_id={} crate={} kind={}\n", pin.source_result_id, crate_name, pin.kind));
        out.push_str(&format!("        copy_hint={}\n", pin.copy_hint));
        if let Some(range) = &pin.range {
            out.push_str(&format!("        range={}:{}:{}-{}:{} kind={}\n", range.path, range.start_line, range.start_column, range.end_line, range.end_column, range.range_kind));
        }
        for (key, value) in &pin.attributes {
            out.push_str(&format!("        {}={}\n", key, truncate(value, 100)));
        }
    }

    out.push('\n');
    out.push_str("safe next commands:\n");
    out.push_str(&format!("  cargo run -p custodian -- code-query {} {} {}\n", pin_set.root, pin_set.max_depth, shell_join(&pin_set.source_query.raw_args)));
    out.push_str(&format!("  cargo run -p custodian -- pin-query-json {} {} {} > /tmp/code_passenger_pins.json\n", pin_set.root, pin_set.max_depth, shell_join(&pin_set.source_query.raw_args)));
    out.push_str(&format!("  cargo run -p custodian -- pin-query {} {} {}\n", pin_set.root, pin_set.max_depth, shell_join(&pin_set.source_query.raw_args)));
    out
}

pub(crate) fn query_region_code(kind: &str) -> &'static str {
    match kind {
        "crate" => "CRTE",
        "file" => "FILE",
        "feature" => "FEAT",
        "dep" => "DEP",
        "fn" => "FN",
        "import" => "USE",
        "use-site" => "SITE",
        "finding" => "FIND",
        _ => "REGN",
    }
}

pub(crate) fn opt_dash(value: Option<&str>) -> String {
    value.filter(|s| !s.trim().is_empty()).unwrap_or("-").to_string()
}

pub(crate) fn shell_join(args: &[String]) -> String {
    if args.is_empty() {
        String::new()
    } else {
        args.iter().map(|s| shell_quote(s)).collect::<Vec<_>>().join(" ")
    }
}
