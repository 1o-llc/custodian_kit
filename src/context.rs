//! Bounded source context windows around pins.

#![allow(unused_imports)]
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn parse_code_context_args(args: &[String]) -> (CodeContextOptions, Vec<String>) {
    let mut options = CodeContextOptions::default();
    let mut query_args = Vec::<String>::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--context-before" | "--before" => {
                if let Some(value) = args.get(idx + 1).and_then(|s| s.parse::<usize>().ok()) {
                    options.before_lines = value;
                    idx += 2;
                    continue;
                }
            }
            "--context-after" | "--after" => {
                if let Some(value) = args.get(idx + 1).and_then(|s| s.parse::<usize>().ok()) {
                    options.after_lines = value;
                    idx += 2;
                    continue;
                }
            }
            "--context-lines" | "--lines" => {
                if let Some(value) = args.get(idx + 1).and_then(|s| s.parse::<usize>().ok()) {
                    options.before_lines = value;
                    options.after_lines = value;
                    idx += 2;
                    continue;
                }
            }
            "--max-window-bytes" => {
                if let Some(value) = args.get(idx + 1).and_then(|s| s.parse::<usize>().ok()) {
                    options.max_window_bytes = value.max(1);
                    idx += 2;
                    continue;
                }
            }
            "--max-total-bytes" => {
                if let Some(value) = args.get(idx + 1).and_then(|s| s.parse::<usize>().ok()) {
                    options.max_total_bytes = value.max(1);
                    idx += 2;
                    continue;
                }
            }
            _ => {}
        }
        query_args.push(args[idx].clone());
        idx += 1;
    }
    (options, query_args)
}

pub(crate) fn build_context_report_from_pin_set(pin_set: &CodePinSet, options: CodeContextOptions) -> CodeContextReport {
    let mut contexts = Vec::<CodeContextWindow>::new();
    let mut findings = pin_set.findings.clone();
    let mut total_bytes = 0usize;
    let mut total_estimated_tokens = 0usize;
    let mut truncated = pin_set.truncated;

    if pin_set.root_exists {
        for pin in &pin_set.pins {
            let Some(range) = &pin.range else {
                continue;
            };
            if total_bytes >= options.max_total_bytes {
                truncated = true;
                break;
            }

            match fs::read_to_string(&range.path) {
                Ok(source) => {
                    let lines = source.lines().collect::<Vec<_>>();
                    if lines.is_empty() {
                        continue;
                    }
                    let anchor = range.start_line.max(1) as usize;
                    let start = anchor.saturating_sub(options.before_lines).max(1);
                    let end = (anchor + options.after_lines).min(lines.len());
                    let mut content = String::new();
                    for line_no in start..=end {
                        let text = lines.get(line_no - 1).copied().unwrap_or("");
                        content.push_str(&format!("{:>5} | {}\n", line_no, text));
                    }

                    let remaining_total = options.max_total_bytes.saturating_sub(total_bytes);
                    let window_cap = options.max_window_bytes.min(remaining_total).max(1);
                    let (content, window_truncated) = truncate_to_byte_limit(&content, window_cap);
                    let byte_count = content.len();
                    let estimated_tokens = estimate_tokens(byte_count);
                    total_bytes += byte_count;
                    total_estimated_tokens += estimated_tokens;
                    if window_truncated {
                        truncated = true;
                    }

                    contexts.push(CodeContextWindow {
                        id: format!("ctx_{:03}", contexts.len() + 1),
                        pin_id: pin.id.clone(),
                        source_result_id: pin.source_result_id.clone(),
                        kind: pin.kind.clone(),
                        label: pin.label.clone(),
                        crate_name: pin.crate_name.clone(),
                        path: range.path.clone(),
                        anchor_line: range.start_line,
                        anchor_column: range.start_column,
                        start_line: start as u32,
                        end_line: end as u32,
                        byte_count,
                        estimated_tokens,
                        truncated: window_truncated,
                        copy_hint: pin.copy_hint.clone(),
                        content,
                        attributes: pin.attributes.clone(),
                    });
                }
                Err(err) => {
                    findings.push(FindingNode {
                        severity: "warn".to_string(),
                        code: "CPG010".to_string(),
                        message: format!("could not read pinned source context: {err}"),
                        path: Some(range.path.clone()),
                        line: Some(range.start_line),
                        column: Some(range.start_column),
                    });
                }
            }
        }
    }

    CodeContextReport {
        schema: "code_passenger.context.v1".to_string(),
        graph_schema: pin_set.graph_schema.clone(),
        root: pin_set.root.clone(),
        root_exists: pin_set.root_exists,
        max_depth: pin_set.max_depth,
        source_query: pin_set.source_query.clone(),
        context_options: options,
        pin_count: pin_set.pin_count,
        context_count: contexts.len(),
        truncated,
        total_bytes,
        total_estimated_tokens,
        pins: pin_set.pins.clone(),
        contexts,
        summary: pin_set.summary.clone(),
        findings,
    }
}

pub(crate) fn truncate_to_byte_limit(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    let mut out = String::new();
    for ch in value.chars() {
        if out.len() + ch.len_utf8() > max_bytes {
            break;
        }
        out.push(ch);
    }
    (out, true)
}

pub(crate) fn estimate_tokens(byte_count: usize) -> usize {
    (byte_count + 3) / 4
}
