//! The facts page of one reading: counts first, then one line per site,
//! grouped by what the site is, then what klepto could not resolve, then
//! the crates named with the binaries that link them, then what was not read.

use super::ReadingReport;
use crate::CodeQueryResult;

/// Attributes shown in the line itself, not after it.
const IN_LINE: [&str; 2] = ["context", "binaries"];

fn bins_text(report: &ReadingReport, crate_name: Option<&str>) -> String {
    let Some(c) = crate_name else { return String::new() };
    match report.binaries.get(c).map(Vec::as_slice) {
        None | Some([]) => " · no binary".to_string(),
        Some(b) if b.len() <= 4 => format!(" · {}", b.join(", ")),
        Some(b) => format!(" · {} binaries ({}, …; all under binaries)", b.len(), b[..3].join(", ")),
    }
}

fn line(report: &ReadingReport, r: &CodeQueryResult) -> String {
    let at = match (&r.path, r.line) {
        (Some(p), Some(l)) => format!("`{p}:{l}` · "),
        (Some(p), None) => format!("`{p}` · "),
        _ => String::new(),
    };
    let krate = r.crate_name.as_deref().map(|c| format!(" · {c}")).unwrap_or_default();
    let mut s = format!("- {at}**{}**{krate}{}", r.label, bins_text(report, r.crate_name.as_deref()));
    let extra: Vec<String> = r
        .attributes
        .iter()
        .filter(|(k, v)| !IN_LINE.contains(&k.as_str()) && !v.is_empty() && *v != "-")
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if !extra.is_empty() {
        s.push_str(&format!(" · {}", extra.join(" · ")));
    }
    if let Some(c) = r.attributes.get("context").filter(|c| !c.is_empty()) {
        s.push_str(&format!(" — `{}`", c.replace('`', "'")));
    }
    s.push('\n');
    s
}

fn grouped(report: &ReadingReport, rows: &[CodeQueryResult], out: &mut String) {
    let mut kinds: Vec<&str> = Vec::new();
    for r in rows {
        if !kinds.contains(&r.kind.as_str()) {
            kinds.push(&r.kind);
        }
    }
    for k in kinds {
        let these: Vec<&CodeQueryResult> = rows.iter().filter(|r| r.kind == k).collect();
        out.push_str(&format!("\n## {k} ({})\n\n", these.len()));
        for r in these {
            out.push_str(&line(report, r));
        }
    }
}

pub fn page(report: &ReadingReport) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {}\n\n", report.name));
    s.push_str(&format!("{}\n\n", report.question));
    s.push_str(&format!(
        "- kind: `{}` · target: `{}` · spec: `{}`\n- read: {} UTC · root: `{}`\n- receipt: {} (`{}`) inside {}\n",
        report.kind,
        report.target,
        report.spec,
        report.stamp,
        report.root,
        report.query.schema,
        report.query.query.raw_args.join(" "),
        report.schema
    ));
    s.push_str("\n## counts\n\n");
    for c in &report.counts {
        s.push_str(&format!("- {}: {}\n", c.what, c.n));
    }
    if let Some(r) = &report.requirements {
        s.push_str(&super::requirements::render::page(r));
        return s;
    }
    if report.query.results.is_empty() {
        s.push_str("\nno sites.\n");
    }
    grouped(report, &report.query.results, &mut s);
    if !report.unresolved.is_empty() {
        s.push_str("\n# unresolved, as klepto reports them\n");
        grouped(report, &report.unresolved, &mut s);
    }
    if !report.binaries.is_empty() {
        s.push_str("\n## binaries\n\n");
        for (c, b) in &report.binaries {
            let b = if b.is_empty() { "no binary".to_string() } else { b.join(", ") };
            s.push_str(&format!("- {c}: {b}\n"));
        }
    }
    if !report.not_read.is_empty() {
        s.push_str("\n## not read\n\n");
        for n in &report.not_read {
            s.push_str(&format!("- {n}\n"));
        }
    }
    s
}
