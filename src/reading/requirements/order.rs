//! The suggested order: the fewest steps that answer the unsatisfied findings, in the order cargo needs them
//! (membership, then the root's declarations, the crate's, the features, cargo's word again, Rust, the lock), each
//! required (the change cannot work without it) or recommended, with how to see it done. Then the checks this reading
//! never runs. A step never carries a version the reading did not read.

use super::{Duplicate, Finding, Severity, Status, Step};

/// How a phase's step is seen done.
fn verify(phase: u8) -> &'static str {
    match phase {
        0 => "the manifest parses (custodian requirements reads it again)",
        1 => "cargo metadata lists the crate among the workspace's members",
        2 | 3 => "cargo metadata --frozen reads the workspace (custodian requirements runs it)",
        4 => "cargo metadata --frozen reads it; the crate compiles with the feature (cargo check: a compile, not run here)",
        5 => "custodian requirements again: cargo metadata reads the workspace",
        6 => "custodian requirements again: the rust face shows no disagreement",
        7 => "cargo tree -d no longer lists the package",
        _ => "custodian requirements again",
    }
}

/// `checked`: the crates the change lands in (the needs' members, else the target's), which the closing checks name.
pub fn steps(findings: &[Finding], duplicates: &[Duplicate], checked: &[String]) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    let mut open: Vec<&Finding> = findings.iter().filter(|f| f.status == Status::Unsatisfied && f.action.is_some()).collect();
    open.sort_by_key(|f| (f.check.rule().2, f.severity, f.at.as_ref().map(|p| (p.file.clone(), p.line))));
    for f in open {
        let action = f.action.clone().unwrap_or_default();
        if let Some(s) = steps.iter_mut().find(|s| s.action == action) {
            s.answers.push(f.id.clone());
            continue;
        }
        steps.push(Step { n: 0, required: f.severity == Severity::Error, action, at: f.at.clone(), verify: verify(f.check.rule().2).to_string(), answers: vec![f.id.clone()] });
    }
    let required = steps.iter().any(|s| s.required);
    if required {
        steps.push(Step { n: 0, required: true, action: "read the requirements again once the steps above are made: cargo metadata --frozen, then the faces, over the manifests as they then are".into(), at: None, verify: "no unsatisfied error left".into(), answers: Vec::new() });
    }
    for d in duplicates.iter().filter(|d| d.collapse.starts_with("one change")) {
        let at = d.versions.iter().flat_map(|v| v.forced_by.iter()).find(|f| f.local && f.at.is_some()).and_then(|f| f.at.clone());
        steps.push(Step { n: 0, required: false, action: format!("`{}`: {}", d.name, d.collapse), at, verify: verify(7).to_string(), answers: Vec::new() });
    }
    let named = checked;
    {
        let ps: Vec<String> = named.iter().take(8).map(|n| format!("-p {n}")).collect();
        if !ps.is_empty() {
            steps.push(Step { n: 0, required: false, action: format!("cargo check {} (and --no-default-features / --all-features where the change touches features): the compile this reading never runs", ps.join(" ")), at: None, verify: "it compiles".into(), answers: Vec::new() });
            steps.push(Step { n: 0, required: false, action: format!("custodian run crate:{} --kind affected_tests, then the test lines it names", named[0]), at: None, verify: "the tests the change reaches pass".into(), answers: Vec::new() });
        }
    }
    for (i, s) in steps.iter_mut().enumerate() {
        s.n = i + 1;
    }
    steps
}
