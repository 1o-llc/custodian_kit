//! Dry-run edit plans anchored to pins and context windows.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_code_edit_plan_args(args: &[String]) -> (CodeEditPlanOptions, Vec<String>) {
    let mut options = CodeEditPlanOptions::default();
    let mut passthrough = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--intent" => {
                if let Some(value) = args.get(idx + 1) {
                    options.intent = value.clone();
                    idx += 2;
                    continue;
                }
            }
            "--operation" | "--edit-operation" => {
                if let Some(value) = args.get(idx + 1) {
                    options.operation = normalize_edit_operation(value);
                    idx += 2;
                    continue;
                }
            }
            "--gate-feature" | "--cfg-feature" | "--target-feature" => {
                if let Some(value) = args.get(idx + 1) {
                    options.target_feature = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--risk" | "--risk-budget" => {
                if let Some(value) = args.get(idx + 1) {
                    options.risk_budget = normalize_risk_budget(value);
                    idx += 2;
                    continue;
                }
            }
            "--allow-write" | "--write" | "--apply" => {
                options.write_requested = true;
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

pub(crate) fn build_edit_plan_from_context(context: &CodeContextReport, options: CodeEditPlanOptions) -> CodeEditPlan {
    let operation = if options.operation == "context_review" {
        infer_edit_operation(context, &options)
    } else {
        options.operation.clone()
    };
    let policy = CodeEditPlanPolicy {
        mode: "dry_run_plan_only".to_string(),
        intent: options.intent.clone(),
        operation: operation.clone(),
        target_feature: options.target_feature.clone(),
        risk_budget: options.risk_budget.clone(),
        write_requested: options.write_requested,
        writes_allowed: false,
        requires_ack_before_apply: true,
        notes: vec![
            "edit-plan-json is a planning receipt only".to_string(),
            "no files are modified by this command".to_string(),
            "future edit-apply must require explicit check/write separation and an ack".to_string(),
            "candidate changes are anchored to Step 9 pins and Step 10 context windows".to_string(),
        ],
    };

    let mut unsupported_targets = Vec::<String>::new();
    let mut candidates = Vec::<CodeEditCandidate>::new();
    for ctx in &context.contexts {
        if ctx.path.is_empty() {
            unsupported_targets.push(format!("{} has no path", ctx.pin_id));
            continue;
        }
        candidates.push(build_edit_candidate(ctx, &operation, &options, candidates.len() + 1));
    }

    let mut findings = context.findings.clone();
    if options.write_requested {
        findings.push(FindingNode {
            severity: "warn".to_string(),
            code: "CPE001".to_string(),
            message: "write/apply flag was requested but Step 12 edit-plan-json never enables writes".to_string(),
            path: None,
            line: None,
            column: None,
        });
    }

    let safety = CodeEditSafety {
        writes: false,
        applies_patches: false,
        shell_execution: false,
        requires_human_review: true,
        unsupported_targets,
        notes: vec![
            "safe for CI because the output is JSON only".to_string(),
            "proposed_preview is not applied and may need a parser-aware later pass".to_string(),
            "real writes should happen only after reviewing this plan and running a guarded check pass".to_string(),
        ],
    };

    CodeEditPlan {
        schema: "code_passenger.edit_plan.v1".to_string(),
        graph_schema: context.graph_schema.clone(),
        root: context.root.clone(),
        root_exists: context.root_exists,
        max_depth: context.max_depth,
        source_query: context.source_query.clone(),
        context_options: context.context_options.clone(),
        policy,
        safety,
        candidate_count: candidates.len(),
        candidates,
        summary: context.summary.clone(),
        findings,
    }
}

pub(crate) fn build_edit_candidate(ctx: &CodeContextWindow, operation: &str, options: &CodeEditPlanOptions, index: usize) -> CodeEditCandidate {
    let target_feature = options.target_feature.clone().or_else(|| first_non_dash(ctx.attributes.get("feature_candidates")));
    let dep = ctx.attributes.get("dep").filter(|s| !s.is_empty() && s.as_str() != "-").cloned();
    let scope = ctx.attributes.get("scope").filter(|s| !s.is_empty() && s.as_str() != "-").cloned();
    let feature_candidates = ctx.attributes.get("feature_candidates")
        .map(|raw| raw.split(',').map(str::trim).filter(|s| !s.is_empty() && *s != "-").map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();
    let current_preview = anchor_preview_line(&ctx.content, ctx.anchor_line);
    let proposed_preview = proposed_preview_for_operation(operation, &current_preview, target_feature.as_deref(), dep.as_deref());
    let patch_kind = match operation {
        "cfg_gate" => "insert_cfg_guard_or_wrap_scope",
        "dependency_review" => "manifest_or_usage_review",
        "rename_review" => "symbol_rename_review",
        _ => "manual_review",
    }.to_string();
    let rationale = rationale_for_operation(operation, target_feature.as_deref(), dep.as_deref());

    CodeEditCandidate {
        id: format!("edit_{:03}", index),
        pin_id: ctx.pin_id.clone(),
        source_result_id: ctx.source_result_id.clone(),
        kind: ctx.kind.clone(),
        label: ctx.label.clone(),
        crate_name: ctx.crate_name.clone(),
        path: ctx.path.clone(),
        anchor_line: ctx.anchor_line,
        anchor_column: ctx.anchor_column,
        scope,
        dep,
        feature_candidates,
        operation: operation.to_string(),
        rationale,
        preconditions: vec![
            "review context content and confirm the anchor still matches the current file".to_string(),
            "run cargo test before any future write pass".to_string(),
            "do not apply if the candidate spans macro-generated or ambiguous code without parser support".to_string(),
        ],
        expected_effects: expected_effects_for_operation(operation, target_feature.as_deref()),
        context: CodeEditCandidateContext {
            context_id: ctx.id.clone(),
            copy_hint: ctx.copy_hint.clone(),
            byte_count: ctx.byte_count,
            estimated_tokens: ctx.estimated_tokens,
            content: ctx.content.clone(),
        },
        proposed_change: CodeProposedChange {
            id: format!("change_{:03}", index),
            path: ctx.path.clone(),
            start_line: ctx.anchor_line,
            end_line: ctx.anchor_line,
            patch_kind,
            current_preview,
            proposed_preview,
            apply_command: "not_available_in_step_12_use_future_guarded_edit_apply".to_string(),
            write_allowed: false,
        },
    }
}

pub(crate) fn infer_edit_operation(context: &CodeContextReport, options: &CodeEditPlanOptions) -> String {
    if options.target_feature.is_some() || context.source_query.feature.is_some() {
        "cfg_gate".to_string()
    } else if context.source_query.dep.is_some() {
        "dependency_review".to_string()
    } else if context.source_query.name_contains.is_some() {
        "rename_review".to_string()
    } else {
        "context_review".to_string()
    }
}

pub(crate) fn proposed_preview_for_operation(operation: &str, current: &str, target_feature: Option<&str>, dep: Option<&str>) -> String {
    match operation {
        "cfg_gate" => {
            let feature = target_feature.or(dep).unwrap_or("feature_name");
            if current.trim().is_empty() {
                format!("#[cfg(feature = \"{}\")]\n<insert guarded code here>", feature)
            } else {
                format!("#[cfg(feature = \"{}\")]\n{}", feature, current)
            }
        }
        "dependency_review" => {
            let dep = dep.unwrap_or("dependency");
            format!("// review dependency `{}` usage before editing: {}", dep, current)
        }
        "rename_review" => format!("// review rename candidate before parser-aware edit: {}", current),
        _ => format!("// manual review candidate: {}", current),
    }
}

pub(crate) fn rationale_for_operation(operation: &str, feature: Option<&str>, dep: Option<&str>) -> String {
    match operation {
        "cfg_gate" => format!("candidate may need a cfg feature guard{}{}",
            feature.map(|f| format!(" for feature `{}`", f)).unwrap_or_default(),
            dep.map(|d| format!(" around dependency `{}`", d)).unwrap_or_default()),
        "dependency_review" => format!("candidate is tied to dependency `{}` and should be reviewed before manifest or gate changes", dep.unwrap_or("unknown")),
        "rename_review" => "candidate is tied to a symbol/name query and needs parser-aware rename support before writes".to_string(),
        _ => "candidate was selected by query/context and requires manual review before any edit".to_string(),
    }
}

pub(crate) fn expected_effects_for_operation(operation: &str, feature: Option<&str>) -> Vec<String> {
    match operation {
        "cfg_gate" => vec![
            format!("would restrict the candidate to cfg(feature = \"{}\") if applied later", feature.unwrap_or("feature_name")),
            "would require follow-up cargo check/test with and without the feature".to_string(),
        ],
        "dependency_review" => vec![
            "would identify dependency-coupled code before any manifest or feature edit".to_string(),
            "would require feature-map/feature-probe review before applying changes".to_string(),
        ],
        "rename_review" => vec![
            "would prepare a parser-aware rename plan, not a text replacement".to_string(),
        ],
        _ => vec!["would preserve a human-readable review target for a future edit pass".to_string()],
    }
}

pub(crate) fn anchor_preview_line(content: &str, anchor_line: u32) -> String {
    let prefix = format!("{:>5} |", anchor_line);
    content.lines()
        .find_map(|line| line.strip_prefix(&prefix).map(|rest| rest.trim_start().to_string()))
        .or_else(|| content.lines().next().map(|line| line.to_string()))
        .unwrap_or_default()
}

pub(crate) fn first_non_dash(value: Option<&String>) -> Option<String> {
    value.and_then(|raw| raw.split(',').map(str::trim).find(|s| !s.is_empty() && *s != "-").map(str::to_string))
}

pub(crate) fn normalize_edit_operation(value: &str) -> String {
    match value.to_lowercase().replace('_', "-").as_str() {
        "cfg" | "cfg-gate" | "feature-gate" | "gate" => "cfg_gate".to_string(),
        "dependency" | "dep" | "dependency-review" | "dep-review" => "dependency_review".to_string(),
        "rename" | "rename-review" => "rename_review".to_string(),
        "context" | "context-review" | "manual" | "manual-review" => "context_review".to_string(),
        _ => "context_review".to_string(),
    }
}

pub(crate) fn normalize_risk_budget(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "none" => "none".to_string(),
        "low" => "low".to_string(),
        "medium" => "medium".to_string(),
        "high" => "high".to_string(),
        _ => "low".to_string(),
    }
}
