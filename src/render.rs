//! The terminal regions: code nav, feature map, feature sites, feature probe, code probe.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub fn render_code_nav(graph: &CodeGraph) -> String {
    let mut out = String::new();
    out.push_str("CODEPASSENGER :: CODE NAV\n");
    out.push_str("──────────────────────────\n");
    out.push_str(&format!("root: {}\n", graph.root));
    out.push_str(&format!("schema: {}\n", graph.schema));
    out.push_str(&format!("exists: {}\n", graph.root_exists));
    out.push_str(&format!("max_depth: {}\n", graph.max_depth));
    out.push('\n');
    out.push_str("summary:\n");
    out.push_str(&format!("  crates: {}\n", graph.summary.crate_count));
    out.push_str(&format!("  files: {}\n", graph.summary.file_count));
    out.push_str(&format!("  features: {}\n", graph.summary.feature_count));
    out.push_str(&format!("  deps: {} (optional: {})\n", graph.summary.dependency_count, graph.summary.optional_dependency_count));
    out.push_str(&format!("  functions: {}\n", graph.summary.function_count));
    out.push_str(&format!("  imports: {}\n", graph.summary.import_count));
    out.push_str(&format!("  use_sites: {}\n", graph.summary.use_site_count));
    out.push_str(&format!("  findings: {}\n", graph.summary.finding_count));

    out.push('\n');
    out.push_str("regions:\n");
    if !graph.root_exists {
        out.push_str("  MISS root path does not exist; no code regions available\n");
    }

    for krate in &graph.crates {
        out.push_str(&format!(
            "  CRTE {:<24} {}\n",
            truncate(&krate.name, 24),
            krate.crate_root
        ));
    }

    for feature in &graph.features {
        let deps = if feature.enables_deps.is_empty() {
            "-".to_string()
        } else {
            feature.enables_deps.join(",")
        };
        let forwards = if feature.forwards_dependency_features.is_empty() {
            "-".to_string()
        } else {
            feature
                .forwards_dependency_features
                .iter()
                .map(|fw| format!("{}/{}", fw.dep, fw.feature))
                .collect::<Vec<_>>()
                .join(",")
        };
        out.push_str(&format!(
            "  FEAT {:<24} crate={} deps={} forwards={}\n",
            truncate(&feature.name, 24),
            feature.crate_name,
            deps,
            forwards
        ));
    }

    for dep in &graph.deps {
        let opt = if dep.optional { "optional" } else { "normal" };
        let enabled = if dep.enabled_by_features.is_empty() {
            "-".to_string()
        } else {
            dep.enabled_by_features.join(",")
        };
        out.push_str(&format!(
            "  DEP  {:<24} crate={} {} via={} features={}\n",
            truncate(&dep.name, 24),
            dep.crate_name,
            opt,
            dep.source_table,
            enabled
        ));
    }

    for file in &graph.files {
        out.push_str(&format!(
            "  FILE {:<24} {} lines={} bytes={}\n",
            truncate(&file.crate_name, 24),
            file.path,
            file.line_count,
            file.byte_count
        ));
    }

    for func in &graph.functions {
        out.push_str(&format!(
            "  FN   {:<24} {}:{}:{} {}\n",
            truncate(&func.name, 24),
            func.path,
            func.line,
            func.column,
            truncate(&func.signature_preview, 72)
        ));
    }

    for import in &graph.imports {
        let pub_tag = if import.is_public_use { "pub" } else { "priv" };
        out.push_str(&format!(
            "  USE  {:<24} {}:{}:{} {:?} {} {}\n",
            truncate(&import.root, 24),
            import.path,
            import.line,
            import.column,
            import.origin,
            pub_tag,
            truncate(&import.full_path, 72)
        ));
    }

    for site in &graph.use_sites {
        let feat = if site.feature_candidates.is_empty() {
            "-".to_string()
        } else {
            site.feature_candidates.join(",")
        };
        let opt = if site.optional_dep { "optional" } else { "normal" };
        out.push_str(&format!(
            "  SITE {:<24} {}:{}:{} {:?} {} feature={} scope={} {}\n",
            truncate(&site.dep, 24),
            site.path,
            site.line,
            site.column,
            site.kind,
            opt,
            feat,
            truncate(&site.scope, 32),
            truncate(&site.full_path, 72)
        ));
    }

    for finding in &graph.findings {
        let loc = match (&finding.path, finding.line, finding.column) {
            (Some(path), Some(line), Some(col)) => format!("{path}:{line}:{col}"),
            (Some(path), _, _) => path.clone(),
            _ => "-".to_string(),
        };
        out.push_str(&format!(
            "  FIND {:<7} {:<8} {} {}\n",
            finding.severity,
            finding.code,
            loc,
            finding.message
        ));
    }

    out.push('\n');
    out.push_str("safe next commands:\n");
    out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", graph.root, graph.max_depth));
    out
}

pub fn render_feature_map(graph: &CodeGraph) -> String {
    let mut out = String::new();
    out.push_str("CODEPASSENGER :: FEATURE MAP\n");
    out.push_str("────────────────────────────\n");
    out.push_str(&format!("root: {}\n", graph.root));
    out.push_str(&format!("schema: {}\n", graph.schema));
    out.push_str(&format!("exists: {}\n", graph.root_exists));
    out.push_str(&format!("max_depth: {}\n", graph.max_depth));
    out.push('\n');
    out.push_str("summary:\n");
    out.push_str(&format!("  crates: {}\n", graph.summary.crate_count));
    out.push_str(&format!("  features: {}\n", graph.summary.feature_count));
    out.push_str(&format!("  deps: {} (optional: {})\n", graph.summary.dependency_count, graph.summary.optional_dependency_count));
    out.push_str(&format!("  use_sites: {}\n", graph.summary.use_site_count));
    out.push_str(&format!("  findings: {}\n", graph.summary.finding_count));
    out.push('\n');

    if !graph.root_exists {
        out.push_str("status:\n");
        out.push_str("  MISS root path does not exist; feature map is intentionally empty\n");
        for finding in &graph.findings {
            out.push_str(&format!("  FIND {} {} {}\n", finding.severity, finding.code, finding.message));
        }
        out.push('\n');
        out.push_str("safe next commands:\n");
        out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- feature-map {} {}\n", graph.root, graph.max_depth));
        return out;
    }

    out.push_str("crates:\n");
    if graph.crates.is_empty() {
        out.push_str("  NONE no package crates found under root\n");
    }

    for krate in &graph.crates {
        let features = graph.features.iter().filter(|f| f.crate_id == krate.id).collect::<Vec<_>>();
        let deps = graph.deps.iter().filter(|d| d.crate_id == krate.id).collect::<Vec<_>>();
        let optional_deps = deps.iter().copied().filter(|d| d.optional).collect::<Vec<_>>();
        let crate_sites = graph.use_sites.iter().filter(|s| s.crate_id == krate.id).collect::<Vec<_>>();

        out.push_str(&format!("\n  CRTE {}\n", krate.name));
        out.push_str(&format!("    manifest: {}\n", krate.manifest_path));
        out.push_str(&format!("    features: {}\n", features.len()));
        out.push_str(&format!("    optional_deps: {}\n", optional_deps.len()));
        out.push_str(&format!("    use_sites: {}\n", crate_sites.len()));

        if features.is_empty() && optional_deps.is_empty() {
            out.push_str("    note: no optional dependency features declared in this crate\n");
        }

        if !features.is_empty() {
            out.push_str("    feature declarations:\n");
            for feature in features {
                let forwards = if feature.forwards_dependency_features.is_empty() {
                    "-".to_string()
                } else {
                    feature
                        .forwards_dependency_features
                        .iter()
                        .map(|fw| format!("{}/{}", fw.dep, fw.feature))
                        .collect::<Vec<_>>()
                        .join(",")
                };
                out.push_str(&format!(
                    "      FEAT {:<24} members={} enables={} forwards={}\n",
                    truncate(&feature.name, 24),
                    dash_if_empty(&feature.members),
                    dash_if_empty(&feature.enables_deps),
                    forwards,
                ));

                let mut feature_dep_names = feature.enables_deps.clone();
                for fw in &feature.forwards_dependency_features {
                    if !feature_dep_names.iter().any(|d| norm_dep(d) == norm_dep(&fw.dep)) {
                        feature_dep_names.push(fw.dep.clone());
                    }
                }
                feature_dep_names.sort();
                feature_dep_names.dedup_by(|a, b| norm_dep(a) == norm_dep(b));

                for dep_name in feature_dep_names {
                    let sites = matching_dep_sites(graph, &krate.id, &dep_name);
                    let scopes = unique_site_scopes(&sites);
                    let files = unique_site_files(&sites);
                    out.push_str(&format!(
                        "        DEP {:<24} sites={} files={} scopes={}\n",
                        truncate(&dep_name, 24),
                        sites.len(),
                        files.len(),
                        dash_if_empty(&scopes),
                    ));
                    for site in sites.iter().take(8) {
                        out.push_str(&format!(
                            "          SITE {}:{}:{} {:?} scope={} {}\n",
                            site.path,
                            site.line,
                            site.column,
                            site.kind,
                            truncate(&site.scope, 32),
                            truncate(&site.full_path, 80),
                        ));
                    }
                    if sites.len() > 8 {
                        out.push_str(&format!("          ... {} more site(s) omitted\n", sites.len() - 8));
                    }
                }
            }
        }

        if !optional_deps.is_empty() {
            out.push_str("    optional dependency usage:\n");
            for dep in optional_deps {
                let sites = matching_dep_sites(graph, &krate.id, &dep.name);
                let scopes = unique_site_scopes(&sites);
                let files = unique_site_files(&sites);
                out.push_str(&format!(
                    "      DEP {:<24} features={} sites={} files={} scopes={}\n",
                    truncate(&dep.name, 24),
                    dash_if_empty(&dep.enabled_by_features),
                    sites.len(),
                    files.len(),
                    dash_if_empty(&scopes),
                ));
                if sites.is_empty() {
                    out.push_str("        note: optional dep declared but no direct use-sites found in scanned source\n");
                }
                for site in sites.iter().take(10) {
                    out.push_str(&format!(
                        "        SITE {}:{}:{} {:?} scope={} feature_candidates={} {}\n",
                        site.path,
                        site.line,
                        site.column,
                        site.kind,
                        truncate(&site.scope, 32),
                        dash_if_empty(&site.feature_candidates),
                        truncate(&site.full_path, 80),
                    ));
                }
                if sites.len() > 10 {
                    out.push_str(&format!("        ... {} more site(s) omitted\n", sites.len() - 10));
                }
            }
        }
    }

    out.push('\n');
    out.push_str("safe next commands:\n");
    out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- code-probe {} <dep-or-feature> {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- feature-map {} {}\n", graph.root, graph.max_depth));
    out
}

pub fn render_feature_sites(graph: &CodeGraph, target: &str) -> String {
    let target = target.trim();
    let q = target.to_lowercase();
    let target_norm = norm_dep(target);
    let mut out = String::new();
    out.push_str("CODEPASSENGER :: FEATURE SITES\n");
    out.push_str("──────────────────────────────\n");
    out.push_str(&format!("root: {}\n", graph.root));
    out.push_str(&format!("schema: {}\n", graph.schema));
    out.push_str(&format!("exists: {}\n", graph.root_exists));
    out.push_str(&format!("max_depth: {}\n", graph.max_depth));
    out.push_str(&format!("target: {}\n", if target.is_empty() { "<empty>" } else { target }));
    out.push('\n');
    out.push_str("summary:\n");
    out.push_str(&format!("  crates: {}\n", graph.summary.crate_count));
    out.push_str(&format!("  features: {}\n", graph.summary.feature_count));
    out.push_str(&format!("  deps: {} (optional: {})\n", graph.summary.dependency_count, graph.summary.optional_dependency_count));
    out.push_str(&format!("  use_sites: {}\n", graph.summary.use_site_count));
    out.push_str(&format!("  findings: {}\n", graph.summary.finding_count));
    out.push('\n');

    if !graph.root_exists {
        out.push_str("status:\n");
        out.push_str("  MISS root path does not exist; feature sites are intentionally empty\n");
        for finding in &graph.findings {
            out.push_str(&format!("  FIND {} {} {}\n", finding.severity, finding.code, finding.message));
        }
        out.push('\n');
        out.push_str("safe next commands:\n");
        out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- feature-map {} {}\n", graph.root, graph.max_depth));
        return out;
    }

    if target.is_empty() {
        out.push_str("status:\n");
        out.push_str("  NONE no target provided; pass a dependency or feature name\n");
        out.push('\n');
        out.push_str("safe next commands:\n");
        out.push_str(&format!("  cargo run -p custodian -- feature-map {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- feature-sites {} <dep-or-feature> {}\n", graph.root, graph.max_depth));
        return out;
    }

    out.push_str("matches:\n");
    let mut total_sites = 0usize;
    let mut any_crate_match = false;

    for krate in &graph.crates {
        let crate_features = graph.features.iter().filter(|f| f.crate_id == krate.id).collect::<Vec<_>>();
        let crate_deps = graph.deps.iter().filter(|d| d.crate_id == krate.id).collect::<Vec<_>>();

        let mut matched_features = crate_features
            .iter()
            .copied()
            .filter(|feature| {
                feature.name.to_lowercase() == q
                    || norm_dep(&feature.name) == target_norm
                    || feature.enables_deps.iter().any(|dep| norm_dep(dep) == target_norm)
                    || feature.forwards_dependency_features.iter().any(|fw| norm_dep(&fw.dep) == target_norm || fw.raw.to_lowercase() == q)
            })
            .collect::<Vec<_>>();
        matched_features.sort_by(|a, b| a.name.cmp(&b.name));
        matched_features.dedup_by(|a, b| a.id == b.id);

        let target_is_feature = matched_features
            .iter()
            .any(|feature| feature.name.to_lowercase() == q || norm_dep(&feature.name) == target_norm);

        let mut direct_target_deps = BTreeSet::<String>::new();
        for dep in &crate_deps {
            if norm_dep(&dep.name) == target_norm || norm_dep(&dep.cargo_key) == target_norm {
                direct_target_deps.insert(dep.name.clone());
            }
        }

        let mut target_deps = BTreeSet::<String>::new();
        if target_is_feature || direct_target_deps.is_empty() {
            for feature in &matched_features {
                for dep in &feature.enables_deps {
                    target_deps.insert(dep.clone());
                }
                for fw in &feature.forwards_dependency_features {
                    target_deps.insert(fw.dep.clone());
                }
            }
        }
        if !target_is_feature {
            target_deps.extend(direct_target_deps.iter().cloned());
        }

        let mut sites = graph
            .use_sites
            .iter()
            .filter(|site| {
                if site.crate_id != krate.id {
                    return false;
                }
                let dep_hit = target_deps.iter().any(|dep| norm_dep(dep) == norm_dep(&site.dep));
                let feature_hit = target_is_feature && matched_features.iter().any(|feature| {
                    site.feature_candidates.iter().any(|candidate| candidate == &feature.name)
                });
                let fallback_hit = target_deps.is_empty()
                    && matched_features.is_empty()
                    && (site.dep.to_lowercase().contains(&q)
                        || site.full_path.to_lowercase().contains(&q)
                        || site.scope.to_lowercase().contains(&q));
                dep_hit || feature_hit || fallback_hit
            })
            .collect::<Vec<_>>();
        sites.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)).then(a.column.cmp(&b.column)));

        let matched_deps = crate_deps
            .iter()
            .copied()
            .filter(|dep| target_deps.iter().any(|target_dep| norm_dep(target_dep) == norm_dep(&dep.name)))
            .collect::<Vec<_>>();

        if matched_features.is_empty() && matched_deps.is_empty() && sites.is_empty() {
            continue;
        }

        any_crate_match = true;
        total_sites += sites.len();
        let files = sites.iter().map(|site| site.path.clone()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
        let scopes = sites.iter().map(|site| site.scope.clone()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();

        out.push_str(&format!("\n  CRTE {}\n", krate.name));
        out.push_str(&format!("    manifest: {}\n", krate.manifest_path));
        out.push_str(&format!("    matched_features: {}\n", matched_features.len()));
        out.push_str(&format!("    matched_deps: {}\n", matched_deps.len()));
        out.push_str(&format!("    sites: {}\n", sites.len()));
        out.push_str(&format!("    files: {}\n", dash_if_empty(&files))); 
        out.push_str(&format!("    scopes: {}\n", dash_if_empty(&scopes))); 

        for feature in matched_features {
            let forwards = if feature.forwards_dependency_features.is_empty() {
                "-".to_string()
            } else {
                feature
                    .forwards_dependency_features
                    .iter()
                    .map(|fw| format!("{}/{}", fw.dep, fw.feature))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            out.push_str(&format!(
                "    FEAT {:<24} members={} enables={} forwards={}\n",
                truncate(&feature.name, 24),
                dash_if_empty(&feature.members),
                dash_if_empty(&feature.enables_deps),
                forwards,
            ));
        }

        for dep in matched_deps {
            out.push_str(&format!(
                "    DEP  {:<24} optional={} features={} via={}\n",
                truncate(&dep.name, 24),
                dep.optional,
                dash_if_empty(&dep.enabled_by_features),
                dep.source_table,
            ));
        }

        if sites.is_empty() {
            out.push_str("    note: target matched a feature/dependency declaration but no direct use-sites were found\n");
            continue;
        }

        let mut by_file = BTreeMap::<String, Vec<&UseSiteNode>>::new();
        for site in sites {
            by_file.entry(site.path.clone()).or_default().push(site);
        }

        out.push_str("    sites by file:\n");
        for (path, file_sites) in by_file {
            out.push_str(&format!("      FILE {} sites={}\n", path, file_sites.len()));
            for site in file_sites.iter().take(20) {
                out.push_str(&format!(
                    "        SITE {}:{} {:?} dep={} head={} optional_dep={} feature_candidates={} scope={} {}\n",
                    site.line,
                    site.column,
                    site.kind,
                    site.dep,
                    truncate(&site.head, 32),
                    site.optional_dep,
                    dash_if_empty(&site.feature_candidates),
                    truncate(&site.scope, 40),
                    truncate(&site.full_path, 96),
                ));
            }
            if file_sites.len() > 20 {
                out.push_str(&format!("        ... {} more site(s) omitted\n", file_sites.len() - 20));
            }
        }
    }

    if !any_crate_match {
        out.push_str("  NONE no feature/dependency/use-site matched the target\n");
    }
    out.push('\n');
    out.push_str(&format!("total_sites: {}\n", total_sites));
    out.push('\n');
    out.push_str("safe next commands:\n");
    out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- code-probe {} {} {}\n", graph.root, shell_quote(target), graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- feature-map {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- feature-sites {} {} {}\n", graph.root, shell_quote(target), graph.max_depth));
    out
}

pub fn render_feature_probe(graph: &CodeGraph, target: &str) -> String {
    let target = target.trim();
    let mut out = String::new();
    out.push_str("CODEPASSENGER :: FEATURE PROBE\n");
    out.push_str("──────────────────────────────\n");
    out.push_str(&format!("root: {}\n", graph.root));
    out.push_str(&format!("schema: {}\n", graph.schema));
    out.push_str(&format!("exists: {}\n", graph.root_exists));
    out.push_str(&format!("max_depth: {}\n", graph.max_depth));
    out.push_str(&format!("target: {}\n", if target.is_empty() { "<empty>" } else { target }));
    out.push('\n');
    out.push_str("summary:\n");
    out.push_str(&format!("  crates: {}\n", graph.summary.crate_count));
    out.push_str(&format!("  features: {}\n", graph.summary.feature_count));
    out.push_str(&format!("  deps: {} (optional: {})\n", graph.summary.dependency_count, graph.summary.optional_dependency_count));
    out.push_str(&format!("  use_sites: {}\n", graph.summary.use_site_count));
    out.push_str(&format!("  findings: {}\n", graph.summary.finding_count));
    out.push('\n');

    if !graph.root_exists {
        out.push_str("status:\n");
        out.push_str("  MISS root path does not exist; feature probe is intentionally empty\n");
        for finding in &graph.findings {
            out.push_str(&format!("  FIND {} {} {}\n", finding.severity, finding.code, finding.message));
        }
        out.push('\n');
        out.push_str("safe next commands:\n");
        out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- feature-map {} {}\n", graph.root, graph.max_depth));
        return out;
    }

    if target.is_empty() {
        out.push_str("status:\n");
        out.push_str("  NONE no target provided; pass a dependency or feature name\n");
        out.push('\n');
        out.push_str("safe next commands:\n");
        out.push_str(&format!("  cargo run -p custodian -- feature-map {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- feature-probe {} <dep-or-feature> {}\n", graph.root, graph.max_depth));
        return out;
    }

    let probes = collect_feature_probe_crates(graph, target);
    if probes.is_empty() {
        out.push_str("decision:\n");
        out.push_str("  status: no_match\n");
        out.push_str("  reason: no feature, dependency, or use-site matched the target\n");
        out.push_str("  writes: false\n");
        out.push_str("  recommendation: run feature-map or code-probe to discover available targets\n");
        out.push('\n');
        out.push_str("safe next commands:\n");
        out.push_str(&format!("  cargo run -p custodian -- feature-map {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- code-probe {} {} {}\n", graph.root, shell_quote(target), graph.max_depth));
        return out;
    }

    out.push_str("decision:\n");
    out.push_str("  writes: false\n");
    out.push_str("  edit_plan_emitted: false\n");
    out.push_str("  purpose: manual feature/dependency gate review\n");
    out.push('\n');

    for probe in probes {
        let optional_deps = probe.matched_deps.iter().filter(|dep| dep.optional).count();
        let normal_deps = probe.matched_deps.len().saturating_sub(optional_deps);
        let files = probe.sites.iter().map(|site| site.path.clone()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
        let scopes = probe.sites.iter().map(|site| site.scope.clone()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
        let related_deps = probe.related_deps();
        let public_imports = graph
            .imports
            .iter()
            .filter(|import| import.crate_id == probe.krate.id && import.is_public_use)
            .filter(|import| related_deps.iter().any(|dep| norm_dep(dep) == norm_dep(&import.root)))
            .count();
        let feature_names = probe.matched_features.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
        let enabled_features = probe
            .matched_deps
            .iter()
            .flat_map(|dep| dep.enabled_by_features.clone())
            .chain(feature_names.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let forwards = probe
            .matched_features
            .iter()
            .flat_map(|feature| feature.forwards_dependency_features.iter().map(|fw| fw.raw.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let scope_suggestion = scope_suggestion(&scopes, public_imports, probe.sites.len());
        let review_level = review_level(optional_deps, normal_deps, probe.matched_features.len(), probe.sites.len(), public_imports);

        out.push_str(&format!("  CRTE {}\n", probe.krate.name));
        out.push_str(&format!("    manifest: {}\n", probe.krate.manifest_path));
        out.push_str(&format!("    review_level: {}\n", review_level));
        out.push_str(&format!("    matched_features: {}\n", probe.matched_features.len()));
        out.push_str(&format!("    matched_deps: {}\n", probe.matched_deps.len()));
        out.push_str(&format!("    optional_deps: {}\n", optional_deps));
        out.push_str(&format!("    normal_deps: {}\n", normal_deps));
        out.push_str(&format!("    sites: {}\n", probe.sites.len()));
        out.push_str(&format!("    files: {}\n", dash_if_empty(&files)));
        out.push_str(&format!("    scopes: {}\n", dash_if_empty(&scopes)));
        out.push_str(&format!("    public_imports: {}\n", public_imports));
        out.push_str(&format!("    enabled_by_features: {}\n", dash_if_empty(&enabled_features)));
        out.push_str(&format!("    forwarded_dependency_features: {}\n", dash_if_empty(&forwards)));
        out.push_str(&format!("    scope_suggestion: {}\n", scope_suggestion));

        if probe.sites.is_empty() {
            out.push_str("    interpretation: declaration matched, but no scanned source use-sites were found\n");
        } else if optional_deps > 0 {
            out.push_str("    interpretation: optional dependency has source use-sites; review cfg scope before editing\n");
        } else if !probe.matched_features.is_empty() {
            out.push_str("    interpretation: feature matched; inspect enabled deps and forwarded dep features together\n");
        } else {
            out.push_str("    interpretation: normal dependency/source use detected; feature gate may not be required\n");
        }

        for feature in &probe.matched_features {
            let forwards = if feature.forwards_dependency_features.is_empty() {
                "-".to_string()
            } else {
                feature
                    .forwards_dependency_features
                    .iter()
                    .map(|fw| format!("{}/{}", fw.dep, fw.feature))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            out.push_str(&format!(
                "    FEAT {:<24} members={} enables={} forwards={}\n",
                truncate(&feature.name, 24),
                dash_if_empty(&feature.members),
                dash_if_empty(&feature.enables_deps),
                forwards,
            ));
        }

        for dep in &probe.matched_deps {
            out.push_str(&format!(
                "    DEP  {:<24} optional={} features={} via={}\n",
                truncate(&dep.name, 24),
                dep.optional,
                dash_if_empty(&dep.enabled_by_features),
                dep.source_table,
            ));
        }

        if !probe.sites.is_empty() {
            out.push_str("    representative sites:\n");
            for site in probe.sites.iter().take(12) {
                out.push_str(&format!(
                    "      SITE {}:{}:{} {:?} dep={} optional_dep={} feature_candidates={} scope={} {}\n",
                    site.path,
                    site.line,
                    site.column,
                    site.kind,
                    site.dep,
                    site.optional_dep,
                    dash_if_empty(&site.feature_candidates),
                    truncate(&site.scope, 40),
                    truncate(&site.full_path, 96),
                ));
            }
            if probe.sites.len() > 12 {
                out.push_str(&format!("      ... {} more site(s) omitted\n", probe.sites.len() - 12));
            }
        }

        out.push_str("    next_review_steps:\n");
        out.push_str("      - inspect representative sites and enclosing scopes\n");
        out.push_str("      - compare feature declarations against actual use-sites\n");
        out.push_str("      - defer edits until code-context/edit-plan passes exist\n");
    }

    out.push('\n');
    out.push_str("safe next commands:\n");
    out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- code-probe {} {} {}\n", graph.root, shell_quote(target), graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- feature-map {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- feature-sites {} {} {}\n", graph.root, shell_quote(target), graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- feature-probe {} {} {}\n", graph.root, shell_quote(target), graph.max_depth));
    out
}

#[derive(Debug, Clone)]
pub(crate) struct FeatureProbeCrate<'a> {
    pub(crate) krate: &'a CrateNode,
    pub(crate) matched_features: Vec<&'a FeatureNode>,
    pub(crate) matched_deps: Vec<&'a DependencyNode>,
    pub(crate) sites: Vec<&'a UseSiteNode>,
}

impl<'a> FeatureProbeCrate<'a> {
    pub(crate) fn related_deps(&self) -> BTreeSet<String> {
        let mut deps = BTreeSet::new();
        for dep in &self.matched_deps {
            deps.insert(dep.name.clone());
        }
        for feature in &self.matched_features {
            for dep in &feature.enables_deps {
                deps.insert(dep.clone());
            }
            for fw in &feature.forwards_dependency_features {
                deps.insert(fw.dep.clone());
            }
        }
        for site in &self.sites {
            deps.insert(site.dep.clone());
        }
        deps
    }
}

pub(crate) fn collect_feature_probe_crates<'a>(graph: &'a CodeGraph, target: &str) -> Vec<FeatureProbeCrate<'a>> {
    let q = target.trim().to_lowercase();
    let target_norm = norm_dep(target);
    let mut out = Vec::new();

    for krate in &graph.crates {
        let crate_features = graph.features.iter().filter(|f| f.crate_id == krate.id).collect::<Vec<_>>();
        let crate_deps = graph.deps.iter().filter(|d| d.crate_id == krate.id).collect::<Vec<_>>();

        let mut matched_features = crate_features
            .iter()
            .copied()
            .filter(|feature| {
                feature.name.to_lowercase() == q
                    || norm_dep(&feature.name) == target_norm
                    || feature.enables_deps.iter().any(|dep| norm_dep(dep) == target_norm)
                    || feature.forwards_dependency_features.iter().any(|fw| norm_dep(&fw.dep) == target_norm || fw.raw.to_lowercase() == q)
            })
            .collect::<Vec<_>>();
        matched_features.sort_by(|a, b| a.name.cmp(&b.name));
        matched_features.dedup_by(|a, b| a.id == b.id);

        let target_is_feature = matched_features
            .iter()
            .any(|feature| feature.name.to_lowercase() == q || norm_dep(&feature.name) == target_norm);

        let mut direct_target_deps = BTreeSet::<String>::new();
        for dep in &crate_deps {
            if norm_dep(&dep.name) == target_norm || norm_dep(&dep.cargo_key) == target_norm {
                direct_target_deps.insert(dep.name.clone());
            }
        }

        let mut target_deps = BTreeSet::<String>::new();
        if target_is_feature || direct_target_deps.is_empty() {
            for feature in &matched_features {
                for dep in &feature.enables_deps {
                    target_deps.insert(dep.clone());
                }
                for fw in &feature.forwards_dependency_features {
                    target_deps.insert(fw.dep.clone());
                }
            }
        }
        if !target_is_feature {
            target_deps.extend(direct_target_deps.iter().cloned());
        }

        let mut sites = graph
            .use_sites
            .iter()
            .filter(|site| {
                if site.crate_id != krate.id {
                    return false;
                }
                let dep_hit = target_deps.iter().any(|dep| norm_dep(dep) == norm_dep(&site.dep));
                let feature_hit = target_is_feature && matched_features.iter().any(|feature| {
                    site.feature_candidates.iter().any(|candidate| candidate == &feature.name)
                });
                let fallback_hit = target_deps.is_empty()
                    && matched_features.is_empty()
                    && (site.dep.to_lowercase().contains(&q)
                        || site.full_path.to_lowercase().contains(&q)
                        || site.scope.to_lowercase().contains(&q));
                dep_hit || feature_hit || fallback_hit
            })
            .collect::<Vec<_>>();
        sites.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)).then(a.column.cmp(&b.column)));

        let mut matched_deps = crate_deps
            .iter()
            .copied()
            .filter(|dep| target_deps.iter().any(|target_dep| norm_dep(target_dep) == norm_dep(&dep.name)))
            .collect::<Vec<_>>();
        matched_deps.sort_by(|a, b| a.name.cmp(&b.name));
        matched_deps.dedup_by(|a, b| a.id == b.id);

        if matched_features.is_empty() && matched_deps.is_empty() && sites.is_empty() {
            continue;
        }

        out.push(FeatureProbeCrate { krate, matched_features, matched_deps, sites });
    }

    out
}

pub(crate) fn scope_suggestion(scopes: &[String], public_imports: usize, site_count: usize) -> String {
    if site_count == 0 {
        return "declaration_only_no_source_scope".to_string();
    }
    if public_imports > 0 {
        return "public_api_or_reexport_review_required".to_string();
    }
    if scopes.len() == 1 {
        let only = scopes.first().map(String::as_str).unwrap_or("file");
        if only == "file" {
            "file_level_or_import_level_gate_candidate".to_string()
        } else if only.starts_with("fn ") {
            "function_level_gate_candidate".to_string()
        } else {
            format!("single_scope_gate_candidate:{only}")
        }
    } else if scopes.iter().all(|scope| scope.starts_with("fn ")) {
        "multi_function_gate_or_helper_extraction_candidate".to_string()
    } else {
        "mixed_scope_manual_review_required".to_string()
    }
}

pub(crate) fn review_level(optional_deps: usize, normal_deps: usize, matched_features: usize, sites: usize, public_imports: usize) -> String {
    if sites == 0 {
        "declaration_only".to_string()
    } else if public_imports > 0 {
        "public_api_gate_review".to_string()
    } else if optional_deps > 0 {
        "optional_dep_gate_review".to_string()
    } else if matched_features > 0 {
        "feature_group_review".to_string()
    } else if normal_deps > 0 {
        "normal_dependency_review".to_string()
    } else {
        "source_site_review".to_string()
    }
}

pub(crate) fn matching_dep_sites<'a>(graph: &'a CodeGraph, crate_id: &str, dep_name: &str) -> Vec<&'a UseSiteNode> {
    let dep_name = norm_dep(dep_name);
    graph
        .use_sites
        .iter()
        .filter(|site| site.crate_id == crate_id && norm_dep(&site.dep) == dep_name)
        .collect()
}

pub(crate) fn unique_site_scopes(sites: &[&UseSiteNode]) -> Vec<String> {
    sites.iter().map(|site| site.scope.clone()).collect::<BTreeSet<_>>().into_iter().collect()
}

pub(crate) fn unique_site_files(sites: &[&UseSiteNode]) -> Vec<String> {
    sites.iter().map(|site| site.path.clone()).collect::<BTreeSet<_>>().into_iter().collect()
}

#[derive(Debug, Clone)]
pub(crate) struct ProbeMatch {
    pub(crate) kind: &'static str,
    pub(crate) label: String,
    pub(crate) location: String,
    pub(crate) details: Vec<String>,
}

pub fn render_code_probe(graph: &CodeGraph, query: &str) -> String {
    let query = query.trim();
    let matches = collect_probe_matches(graph, query);
    let mut out = String::new();
    out.push_str("CODEPASSENGER :: CODE PROBE\n");
    out.push_str("────────────────────────────\n");
    out.push_str(&format!("root: {}\n", graph.root));
    out.push_str(&format!("schema: {}\n", graph.schema));
    out.push_str(&format!("exists: {}\n", graph.root_exists));
    out.push_str(&format!("max_depth: {}\n", graph.max_depth));
    out.push_str(&format!("query: {}\n", if query.is_empty() { "<summary>" } else { query }));
    out.push('\n');
    out.push_str("summary:\n");
    out.push_str(&format!("  crates: {}\n", graph.summary.crate_count));
    out.push_str(&format!("  files: {}\n", graph.summary.file_count));
    out.push_str(&format!("  features: {}\n", graph.summary.feature_count));
    out.push_str(&format!("  deps: {} (optional: {})\n", graph.summary.dependency_count, graph.summary.optional_dependency_count));
    out.push_str(&format!("  functions: {}\n", graph.summary.function_count));
    out.push_str(&format!("  imports: {}\n", graph.summary.import_count));
    out.push_str(&format!("  use_sites: {}\n", graph.summary.use_site_count));
    out.push_str(&format!("  findings: {}\n", graph.summary.finding_count));
    out.push('\n');

    if !graph.root_exists {
        out.push_str("status:\n");
        out.push_str("  MISS root path does not exist; probe is intentionally empty\n");
        for finding in &graph.findings {
            out.push_str(&format!("  FIND {} {} {}\n", finding.severity, finding.code, finding.message));
        }
        out.push('\n');
        out.push_str("safe next commands:\n");
        out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", graph.root, graph.max_depth));
        out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", graph.root, graph.max_depth));
        return out;
    }

    out.push_str("matches:\n");
    if matches.is_empty() {
        out.push_str("  NONE no graph regions matched the probe query\n");
    } else {
        out.push_str(&format!("  count: {}\n", matches.len()));
        for (idx, hit) in matches.iter().take(40).enumerate() {
            out.push_str(&format!(
                "  [{:03}] {:<5} {:<28} {}\n",
                idx + 1,
                hit.kind,
                truncate(&hit.label, 28),
                hit.location
            ));
            for detail in &hit.details {
                out.push_str(&format!("        - {}\n", detail));
            }
        }
        if matches.len() > 40 {
            out.push_str(&format!("  ... {} more match(es) omitted\n", matches.len() - 40));
        }
    }

    out.push('\n');
    out.push_str("safe next commands:\n");
    out.push_str(&format!("  cargo run -p custodian -- code-graph-json {} {}\n", graph.root, graph.max_depth));
    out.push_str(&format!("  cargo run -p custodian -- code-nav {} {}\n", graph.root, graph.max_depth));
    if !query.is_empty() {
        out.push_str(&format!("  cargo run -p custodian -- code-probe {} {} {}\n", graph.root, shell_quote(query), graph.max_depth));
    }
    out
}

pub(crate) fn collect_probe_matches(graph: &CodeGraph, query: &str) -> Vec<ProbeMatch> {
    let q = query.trim().to_lowercase();
    let summary_mode = q.is_empty() || q == "." || q == "summary" || q == "root" || q == "all";
    let mut out = Vec::new();

    for krate in &graph.crates {
        if summary_mode || any_contains(&q, &[&krate.id, &krate.name, &krate.crate_root, &krate.manifest_path]) {
            out.push(ProbeMatch {
                kind: "CRTE",
                label: krate.name.clone(),
                location: krate.crate_root.clone(),
                details: vec![
                    format!("manifest={}", krate.manifest_path),
                    format!("src_root={}", krate.src_root.clone().unwrap_or_else(|| "-".to_string())),
                ],
            });
        }
    }

    for feature in &graph.features {
        let forwards = feature.forwards_dependency_features.iter().map(|fw| fw.raw.clone()).collect::<Vec<_>>().join(",");
        if summary_mode || any_contains(&q, &[&feature.id, &feature.name, &feature.crate_name, &feature.members.join(","), &feature.enables_deps.join(","), &forwards]) {
            out.push(ProbeMatch {
                kind: "FEAT",
                label: format!("{}::{}", feature.crate_name, feature.name),
                location: feature.crate_name.clone(),
                details: vec![
                    format!("members={}", dash_if_empty(&feature.members)),
                    format!("enables_deps={}", dash_if_empty(&feature.enables_deps)),
                    format!("forwards={}", if forwards.is_empty() { "-".to_string() } else { forwards }),
                ],
            });
        }
    }

    for dep in &graph.deps {
        if summary_mode || any_contains(&q, &[&dep.id, &dep.name, &dep.cargo_key, &dep.crate_name, &dep.enabled_by_features.join(",")]) {
            let site_count = graph.use_sites.iter().filter(|site| site.crate_id == dep.crate_id && norm_dep(&site.dep) == norm_dep(&dep.name)).count();
            out.push(ProbeMatch {
                kind: "DEP",
                label: format!("{}::{}", dep.crate_name, dep.name),
                location: dep.source_table.clone(),
                details: vec![
                    format!("optional={}", dep.optional),
                    format!("cargo_key={}", dep.cargo_key),
                    format!("enabled_by_features={}", dash_if_empty(&dep.enabled_by_features)),
                    format!("matching_use_sites={site_count}"),
                ],
            });
        }
    }

    for file in &graph.files {
        if summary_mode || any_contains(&q, &[&file.id, &file.path, &file.relative_path, &file.crate_name]) {
            let fn_count = graph.functions.iter().filter(|f| f.path == file.path).count();
            let site_count = graph.use_sites.iter().filter(|s| s.path == file.path).count();
            out.push(ProbeMatch {
                kind: "FILE",
                label: file.relative_path.clone(),
                location: file.path.clone(),
                details: vec![
                    format!("crate={}", file.crate_name),
                    format!("lines={}", file.line_count),
                    format!("bytes={}", file.byte_count),
                    format!("functions={fn_count}"),
                    format!("use_sites={site_count}"),
                ],
            });
        }
    }

    for func in &graph.functions {
        if summary_mode || any_contains(&q, &[&func.id, &func.name, &func.path, &func.crate_name, &func.signature_preview]) {
            let related_sites = graph.use_sites.iter().filter(|site| site.path == func.path && site.scope == format!("fn {}", func.name)).count();
            out.push(ProbeMatch {
                kind: "FN",
                label: func.name.clone(),
                location: format!("{}:{}:{}", func.path, func.line, func.column),
                details: vec![
                    format!("crate={}", func.crate_name),
                    format!("visibility={}", func.visibility),
                    format!("signature={}", truncate(&func.signature_preview, 120)),
                    format!("scoped_use_sites={related_sites}"),
                ],
            });
        }
    }

    for import in &graph.imports {
        if summary_mode || any_contains(&q, &[&import.id, &import.root, &import.full_path, &import.path, &import.crate_name]) {
            out.push(ProbeMatch {
                kind: "USE",
                label: import.root.clone(),
                location: format!("{}:{}:{}", import.path, import.line, import.column),
                details: vec![
                    format!("crate={}", import.crate_name),
                    format!("origin={:?}", import.origin),
                    format!("public={}", import.is_public_use),
                    format!("full_path={}", import.full_path),
                ],
            });
        }
    }

    for site in &graph.use_sites {
        if summary_mode || any_contains(&q, &[&site.id, &site.dep, &site.full_path, &site.head, &site.path, &site.scope, &site.feature_candidates.join(",")]) {
            out.push(ProbeMatch {
                kind: "SITE",
                label: site.dep.clone(),
                location: format!("{}:{}:{}", site.path, site.line, site.column),
                details: vec![
                    format!("crate={}", site.crate_name),
                    format!("kind={:?}", site.kind),
                    format!("head={}", site.head),
                    format!("scope={}", site.scope),
                    format!("optional_dep={}", site.optional_dep),
                    format!("feature_candidates={}", dash_if_empty(&site.feature_candidates)),
                    format!("full_path={}", site.full_path),
                ],
            });
        }
    }

    for finding in &graph.findings {
        if summary_mode || any_contains(&q, &[finding.severity.as_str(), finding.code.as_str(), finding.message.as_str(), finding.path.as_deref().unwrap_or("")]) {
            let loc = match (&finding.path, finding.line, finding.column) {
                (Some(path), Some(line), Some(col)) => format!("{path}:{line}:{col}"),
                (Some(path), _, _) => path.clone(),
                _ => "-".to_string(),
            };
            out.push(ProbeMatch {
                kind: "FIND",
                label: finding.code.clone(),
                location: loc,
                details: vec![
                    format!("severity={}", finding.severity),
                    format!("message={}", finding.message),
                ],
            });
        }
    }

    out
}

pub(crate) fn any_contains<T: AsRef<str>>(q: &str, haystacks: &[T]) -> bool {
    if q.is_empty() {
        return true;
    }
    haystacks.iter().any(|h| h.as_ref().to_lowercase().contains(q))
}

pub(crate) fn dash_if_empty(xs: &[String]) -> String {
    if xs.is_empty() { "-".to_string() } else { xs.join(",") }
}

pub(crate) fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':')) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
