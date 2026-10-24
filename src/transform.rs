// Copyright © 2026 1o. All rights reserved.

//! A transform's record: what code_passenger wrote for one change spec, file
//! by file, with the digests before and after, the lines it wrote, the
//! passenger checkpoints around the write, the cargo checks that proved it
//! and the postconditions a fresh scan showed. code_passenger writes it
//! (`code-passenger transform --spec S --write`); the kit reads it at a
//! bounded job's close, where every line the job touched must be covered by
//! one ([`coverage`]).
//!
//! It is reversible: the before checkpoint restores every file it wrote
//! (`code-passenger transform --revert RECORD`, or `passenger checkout`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TRANSFORM_SCHEMA: &str = "custodian.transform.v1";

/// One file a transform wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileWrite {
    /// Relative to the root.
    pub path: String,
    /// The file's digest before (lowercase sha256 hex of its bytes; `none` when the transform made it).
    pub before: String,
    /// After.
    pub after: String,
    /// The lines written, 1-based inclusive ranges in the file after.
    #[serde(default)]
    pub lines: Vec<(u32, u32)>,
}

/// One cargo check of the proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checked {
    pub package: String,
    pub features: Vec<String>,
    pub no_default: bool,
    pub ok: bool,
    /// The first error, when it failed.
    #[serde(default)]
    pub error: Option<String>,
}

/// One transform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformRecord {
    pub schema: String,
    /// The change spec's name and file (relative to the root).
    pub spec_name: String,
    pub spec: String,
    /// `ChangeSpec::digest`: the spec version this ran.
    pub spec_digest: String,
    pub operation: String,
    pub target: String,
    /// The preflight receipt it ran on.
    #[serde(default)]
    pub preflight_seq: Option<i64>,
    /// The code digest the preflight read (the same one this ran over).
    #[serde(default)]
    pub code_digest: Option<String>,
    pub files: Vec<FileWrite>,
    /// The passenger checkpoints before and after the write.
    #[serde(default)]
    pub checkpoint_before: Option<String>,
    #[serde(default)]
    pub checkpoint_after: Option<String>,
    pub checks: Vec<Checked>,
    /// `(postcondition, held)` from a fresh klepto scan of the result.
    pub postconditions: Vec<(String, bool)>,
    /// The sites the operation lists for a person and does not edit (matches without a catch-all, literals with no default).
    #[serde(default)]
    pub listed: Vec<String>,
    /// The source it generated, as placed.
    #[serde(default)]
    pub generated: Vec<String>,
    /// Written to the tree (else proved on the scratch copy only).
    pub written: bool,
    /// For a revert: the record it undid.
    #[serde(default)]
    pub reverts: Option<String>,
}

/// A function's expansion before and after a change, in the toolkit's proof form (its locals named by where they are
/// bound, the prelude's names short): kept beside the transform's record (`<record>.expansion.json`), its verdict one of
/// the record's postconditions (`expands <function> the same`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expansion {
    pub function: String,
    pub package: String,
    pub before: String,
    pub after: String,
    pub same: bool,
    /// The `cargo expand` line, and how long the two expansions took.
    pub line: String,
    pub millis: i64,
}

impl TransformRecord {
    /// Every check and postcondition held.
    pub fn proved(&self) -> bool {
        self.checks.iter().all(|c| c.ok) && self.postconditions.iter().all(|(_, held)| *held)
    }
}

/// A file a bounded job touched that no transform of the job covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Uncovered {
    pub path: String,
    /// The digest the job's transforms leave it at (or its open digest): the state a hand edit started from.
    pub from: String,
    /// Its digest now (`none`: removed).
    pub now: String,
    /// The lines changed by hand, 1-based, when a snapshot of `from` is held; empty when not known.
    pub lines: Vec<u32>,
}

/// The files a job touched against what its transforms wrote: `open` and `close` are each file's digest at the
/// job's open and close counts (path → hex), `transforms` the job's records in the order they ran. A file is covered
/// when its transforms carry it from its open digest to its close digest, each one's `before` the last one's `after`.
/// Returns every touched file that is not, with the digest the hand edit began from.
pub fn coverage(open: &BTreeMap<String, String>, close: &BTreeMap<String, String>, transforms: &[TransformRecord]) -> Vec<Uncovered> {
    let mut paths: Vec<&String> = open.keys().chain(close.keys()).collect();
    paths.sort();
    paths.dedup();
    let mut out = Vec::new();
    for p in paths {
        let start = open.get(p).cloned().unwrap_or_else(|| "none".to_string());
        let end = close.get(p).cloned().unwrap_or_else(|| "none".to_string());
        if start == end && !transforms.iter().any(|t| t.files.iter().any(|f| &f.path == p)) {
            continue;
        }
        // each write taken once, in any order: a batch's members of one file are recorded in the list's order, not
        // the order their edits stack in, so the chain is followed until no write carries it further
        let writes: Vec<&FileWrite> = transforms.iter().filter(|t| t.written).flat_map(|t| t.files.iter().filter(|f| &f.path == p)).collect();
        let mut used = vec![false; writes.len()];
        let mut cur = start.clone();
        while let Some(i) = (0..writes.len()).find(|i| !used[*i] && writes[*i].before == cur && writes[*i].after != cur) {
            used[i] = true;
            cur = writes[i].after.clone();
        }
        if cur != end {
            out.push(Uncovered { path: p.clone(), from: cur, now: end, lines: Vec::new() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(writes: &[(&str, &str, &str)]) -> TransformRecord {
        TransformRecord {
            schema: TRANSFORM_SCHEMA.into(),
            spec_name: "s".into(),
            spec: "s.toml".into(),
            spec_digest: "d".into(),
            operation: "add_field".into(),
            target: "t".into(),
            preflight_seq: None,
            code_digest: None,
            files: writes.iter().map(|(p, b, a)| FileWrite { path: p.to_string(), before: b.to_string(), after: a.to_string(), lines: vec![] }).collect(),
            checkpoint_before: None,
            checkpoint_after: None,
            checks: vec![],
            postconditions: vec![],
            listed: vec![],
            generated: vec![],
            written: true,
            reverts: None,
        }
    }

    #[test]
    fn a_batch_whose_members_stack_in_another_order_than_recorded_still_covers_its_file() {
        let open: BTreeMap<String, String> = [("a.rs".into(), "a0".into())].into();
        let close: BTreeMap<String, String> = [("a.rs".into(), "a3".into())].into();
        // recorded in the list's order: the third edit first
        let ts = vec![rec(&[("a.rs", "a2", "a3")]), rec(&[("a.rs", "a0", "a1")]), rec(&[("a.rs", "a1", "a2")])];
        assert!(coverage(&open, &close, &ts).is_empty());
        let unwritten = TransformRecord { written: false, ..rec(&[("a.rs", "a0", "a3")]) };
        assert_eq!(coverage(&open, &close, &[unwritten]).len(), 1, "a record not written carries nothing");
    }

    #[test]
    fn a_file_is_covered_only_when_its_transforms_carry_it_open_to_close() {
        let open: BTreeMap<String, String> = [("a.rs".into(), "a0".into()), ("b.rs".into(), "b0".into()), ("c.rs".into(), "c0".into())].into();
        // a: two transforms in a row; b: a transform then a hand edit; c: a hand edit only; d: made by a transform
        let close: BTreeMap<String, String> = [("a.rs".into(), "a2".into()), ("b.rs".into(), "b9".into()), ("c.rs".into(), "c9".into()), ("d.rs".into(), "d1".into())].into();
        let ts = vec![rec(&[("a.rs", "a0", "a1"), ("b.rs", "b0", "b1")]), rec(&[("a.rs", "a1", "a2"), ("d.rs", "none", "d1")])];
        let u = coverage(&open, &close, &ts);
        let got: Vec<(&str, &str)> = u.iter().map(|x| (x.path.as_str(), x.from.as_str())).collect();
        assert_eq!(got, [("b.rs", "b1"), ("c.rs", "c0")]);
        // nothing touched, nothing to cover
        assert!(coverage(&open, &open, &[]).is_empty());
    }
}
