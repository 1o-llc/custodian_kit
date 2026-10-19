//! The reading diff: two readings of one spec compared row by row. A row is
//! the same row when its kind, label, file and line text are the same (a site
//! that only moved lines is the same site); what is in one and not the other
//! was added or removed, and every count is shown before and after.
//!
//! The comparison and its page are diff_history's; this module reads a
//! [`ReadingReport`] as diff_history reads a reading (its rows, then its
//! unresolved, each keyed by kind, label, file and line text; its counts).

use crate::reading::ReadingReport;
use crate::CodeQueryResult;

pub use diff_history::{render, CountChange, ReadingDiff, RowKey};

fn key(r: &CodeQueryResult) -> RowKey {
    RowKey { kind: r.kind.clone(), label: r.label.clone(), path: r.path.clone(), context: r.attributes.get("context").cloned().unwrap_or_default(), line: r.line }
}

/// A reading as diff_history reads it.
pub fn reading_of(r: &ReadingReport) -> diff_history::Reading {
    diff_history::Reading {
        name: r.name.clone(),
        stamp: r.stamp.clone(),
        counts: r.counts.iter().map(|c| diff_history::CountValue { what: c.what.clone(), n: c.n }).collect(),
        rows: r.query.results.iter().chain(&r.unresolved).map(key).collect(),
    }
}

pub fn compare(before: &ReadingReport, after: &ReadingReport) -> ReadingDiff {
    diff_history::compare(&reading_of(before), &reading_of(after))
}
