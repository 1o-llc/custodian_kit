//! The reading diff: two readings of one spec compared row by row. A row is
//! the same row when its kind, label, file and line text are the same (a site
//! that only moved lines is the same site); what is in one and not the other
//! was added or removed, and every count is shown before and after.

use crate::reading::ReadingReport;
use crate::CodeQueryResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RowKey {
    pub kind: String,
    pub label: String,
    pub path: Option<String>,
    pub context: String,
    /// The line it is on in the reading it comes from.
    pub line: Option<u32>,
}

fn key(r: &CodeQueryResult) -> RowKey {
    RowKey { kind: r.kind.clone(), label: r.label.clone(), path: r.path.clone(), context: r.attributes.get("context").cloned().unwrap_or_default(), line: r.line }
}

fn same(k: &RowKey) -> (String, String, Option<String>, String) {
    (k.kind.clone(), k.label.clone(), k.path.clone(), k.context.clone())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountChange {
    pub what: String,
    pub before: Option<usize>,
    pub after: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingDiff {
    pub name: String,
    pub before: String,
    pub after: String,
    pub counts: Vec<CountChange>,
    pub added: Vec<RowKey>,
    pub removed: Vec<RowKey>,
}

impl ReadingDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.counts.iter().all(|c| c.before == c.after)
    }
}

fn rows(r: &ReadingReport) -> Vec<RowKey> {
    r.query.results.iter().chain(&r.unresolved).map(key).collect()
}

pub fn compare(before: &ReadingReport, after: &ReadingReport) -> ReadingDiff {
    let (b, a) = (rows(before), rows(after));
    let bs: BTreeSet<_> = b.iter().map(same).collect();
    let as_: BTreeSet<_> = a.iter().map(same).collect();
    let added = a.iter().filter(|k| !bs.contains(&same(k))).cloned().collect();
    let removed = b.iter().filter(|k| !as_.contains(&same(k))).cloned().collect();
    let mut whats: Vec<String> = before.counts.iter().map(|c| c.what.clone()).collect();
    for c in &after.counts {
        if !whats.contains(&c.what) {
            whats.push(c.what.clone());
        }
    }
    let counts = whats
        .into_iter()
        .map(|w| CountChange {
            before: before.counts.iter().find(|c| c.what == w).map(|c| c.n),
            after: after.counts.iter().find(|c| c.what == w).map(|c| c.n),
            what: w,
        })
        .collect();
    ReadingDiff { name: after.name.clone(), before: before.stamp.clone(), after: after.stamp.clone(), counts, added, removed }
}

fn n(v: Option<usize>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "-".into())
}

/// The diff as a page: counts that moved first, then rows added and removed,
/// by kind (a crate pair removed from a duplicates reading is a copy gone).
pub fn render(d: &ReadingDiff) -> String {
    let mut s = format!("# {} — what changed\n\n- before: {}\n- after: {}\n\n## counts\n\n", d.name, d.before, d.after);
    for c in &d.counts {
        let mark = if c.before == c.after { "" } else { " (changed)" };
        s.push_str(&format!("- {}: {} → {}{mark}\n", c.what, n(c.before), n(c.after)));
    }
    let list = |s: &mut String, title: &str, rows: &[RowKey]| {
        s.push_str(&format!("\n## {title} ({})\n\n", rows.len()));
        for r in rows {
            let at = match (&r.path, r.line) {
                (Some(p), Some(l)) => format!("`{p}:{l}` · "),
                (Some(p), None) => format!("`{p}` · "),
                _ => String::new(),
            };
            let gone = if title == "removed" && r.kind == "crate-pair" { " (copies gone)" } else { "" };
            s.push_str(&format!("- {at}{} **{}**{gone}\n", r.kind, r.label));
        }
    };
    list(&mut s, "added", &d.added);
    list(&mut s, "removed", &d.removed);
    if d.is_empty() {
        s.push_str("\nnothing changed.\n");
    }
    s
}
