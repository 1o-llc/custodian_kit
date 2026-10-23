//! The requirements page: the report's contract, section by section, for a person and an agent alike.

use super::{Basis, Face, Finding, RequirementsReport, Status};

fn at(f: &Finding) -> String {
    f.at.as_ref().map(|p| format!("`{}{}`{} · ", p.file, p.line.map(|l| format!(":{l}")).unwrap_or_default(), if p.section.is_empty() { String::new() } else { format!(" [{}] {}", p.section, p.field) })).unwrap_or_default()
}

fn finding(f: &Finding, s: &mut String) {
    s.push_str(&format!("- {}**{}** · {} · {} · `{}`{}: {}\n", at(f), f.severity, f.basis, f.from, f.check, f.krate.as_ref().map(|k| format!(" · {k}")).unwrap_or_default(), f.what));
    if f.requirement != "-" {
        s.push_str(&format!("  - requires: {}\n", f.requirement));
    }
    s.push_str(&format!("  - evidence: {}\n  - consequence: {}\n", f.evidence, f.consequence));
    if let Some(a) = &f.action {
        s.push_str(&format!("  - action: {a}\n"));
    }
}

/// The report as markdown.
pub fn page(r: &RequirementsReport) -> String {
    let mut s = String::new();
    s.push_str("\n## requested outcome and scope\n\n");
    s.push_str(&format!("- intent: {}\n- target: `{}` · faces: {}\n", r.intent.clone().unwrap_or_else(|| "none given (the workspace as it is)".into()), r.target, r.faces.join(", ")));
    for n in &r.needs {
        let mut parts = Vec::new();
        if let Some(d) = &n.dependency {
            parts.push(format!("`{d}`"));
        }
        if let Some(v) = &n.version {
            parts.push(format!("`{v}`"));
        }
        if !n.features.is_empty() {
            parts.push(format!("features {}", n.features.join(", ")));
        }
        if let Some(p) = &n.path {
            parts.push(format!("path `{p}`"));
        }
        if let Some(c) = &n.new_crate {
            parts.push(format!("new crate at `{c}`"));
        }
        s.push_str(&format!("- need: {} in `{}` ({})\n", if parts.is_empty() { "the crate itself".into() } else { parts.join(" ") }, n.member.clone().unwrap_or_else(|| "?".into()), n.said));
    }
    let t = &r.provenance.toolchain;
    s.push_str(&format!("- toolchain: {} · host: {}{}\n", t.release.clone().unwrap_or_else(|| "not read".into()), t.host.clone().unwrap_or_else(|| "not read".into()), t.file.as_ref().map(|f| format!(" · toolchain file {f}")).unwrap_or_default()));

    s.push_str("\n## current state\n\n");
    for root in &r.state.roots {
        s.push_str(&format!(
            "- workspace root `{}`{}: {} member(s); [workspace.dependencies] {} entr{}; [workspace.package] {}{}\n",
            root.manifest,
            if root.nested { " (nested, its own workspace)" } else { "" },
            root.members.len(),
            root.dependencies,
            if root.dependencies == 1 { "y" } else { "ies" },
            if root.package_fields.is_empty() { "none".to_string() } else { root.package_fields.join(", ") },
            if root.lints { "; [workspace.lints]" } else { "" }
        ));
    }
    s.push_str(&format!("- manifests read: {} · lock: {} · cargo run on: {}\n- cargo metadata: {}\n- the scanner and cargo: {}\n", r.state.manifests, r.state.lock.clone().unwrap_or_else(|| "none".into()), r.state.cargo_root.clone().unwrap_or_else(|| "-".into()), r.state.metadata, if r.state.agreement.is_empty() { "-".to_string() } else { r.state.agreement.clone() }));
    if r.state.crates.len() <= 12 {
        s.push_str(&format!("- crates: {}\n", r.state.crates.join(", ")));
    } else {
        s.push_str(&format!("- crates: {} ({}, …)\n", r.state.crates.len(), r.state.crates[..12].join(", ")));
    }

    s.push_str("\n## satisfied prerequisites\n\n");
    for (what, n) in &r.state.satisfied {
        s.push_str(&format!("- {what}: {n}\n"));
    }
    for f in r.findings.iter().filter(|f| f.status == Status::Satisfied) {
        finding(f, &mut s);
    }

    s.push_str("\n## unsatisfied prerequisites\n\n");
    let unsat: Vec<&Finding> = r.findings.iter().filter(|f| f.status == Status::Unsatisfied && f.face == Face::Prerequisites).collect();
    if unsat.is_empty() {
        s.push_str("none.\n");
    }
    for f in unsat {
        finding(f, &mut s);
    }

    s.push_str("\n## compatibility and constraints\n\n### rust versions\n\n");
    if r.rust.is_empty() {
        s.push_str("not read.\n");
    } else {
        s.push_str("| crate | edition (first Rust) | rust-version | the lock's highest |\n|---|---|---|---|\n");
        for row in &r.rust {
            s.push_str(&format!("| {} | {} ({}) | {} | {} |\n", row.krate, row.edition, row.edition_min, row.declared.clone().unwrap_or_else(|| "none".into()), row.lock_max.as_ref().map(|(v, who)| format!("{v} ({who})")).unwrap_or_else(|| "-".into())));
        }
    }
    for f in r.findings.iter().filter(|f| f.face == Face::Rust) {
        finding(f, &mut s);
    }
    s.push_str("\n### the lock's shape\n\n");
    if r.duplicates.is_empty() {
        s.push_str("no package in more than one version (or the lock face was not read).\n");
    }
    for d in &r.duplicates {
        s.push_str(&format!("- **{}**: {} version{}, compiles {} times · {}\n", d.name, d.versions.len(), if d.versions.len() == 1 { "" } else { "s" }, d.compiles, d.collapse));
        for v in &d.versions {
            let by: Vec<String> = v.forced_by.iter().map(|f| format!("{}{}{}", f.package, f.requirement.as_ref().map(|q| format!(" `{q}`")).unwrap_or_default(), f.at.as_ref().map(|p| format!(" ({}{})", p.file, p.line.map(|l| format!(":{l}")).unwrap_or_default())).unwrap_or_default())).collect();
            s.push_str(&format!(
                "  - {}{} ×{} ← {}{}{}\n",
                v.version,
                v.source.as_ref().map(|x| format!(" ({x})")).unwrap_or_default(),
                v.builds,
                if by.is_empty() { "unknown".into() } else { by.join(", ") },
                if v.pullers_from_metadata { " (from cargo metadata's resolve: cargo tree printed its pullers before, as `(*)`)" } else { "" },
                if v.through > 0 { format!(" (+{} through them)", v.through) } else { String::new() }
            ));
        }
    }
    for f in r.findings.iter().filter(|f| f.face == Face::Lock && f.basis == Basis::Unknown) {
        finding(f, &mut s);
    }

    s.push_str("\n## impact and reuse\n\n");
    for i in &r.impact {
        s.push_str(&format!("- `{}`: depends on {}; depended on directly by {}; reached by {} package(s) in all\n", i.krate, if i.depends_on.is_empty() { "-".into() } else { super::shorten(&i.depends_on, 10) }, if i.direct_dependents.is_empty() { "nothing".into() } else { super::shorten(&i.direct_dependents, 10) }, i.all_dependents.len()));
        for l in &i.readings {
            s.push_str(&format!("  - `{l}`\n"));
        }
    }
    for x in &r.reuse {
        s.push_str(&format!("- reuse: {x}\n"));
    }
    if r.impact.is_empty() && r.reuse.is_empty() {
        s.push_str("nothing asked of a crate (no intent or need names one).\n");
    }

    s.push_str("\n## suggested operation order\n\n");
    if r.steps.is_empty() {
        s.push_str("nothing to do.\n");
    }
    for st in &r.steps {
        s.push_str(&format!("{}. **{}** {}{} — verify: {}\n", st.n, if st.required { "required" } else { "recommended" }, st.action, st.at.as_ref().map(|p| format!(" (`{}{}`)", p.file, p.line.map(|l| format!(":{l}")).unwrap_or_default())).unwrap_or_default(), st.verify));
    }

    s.push_str("\n## unknowns and assumptions\n\n");
    for u in &r.unknowns {
        s.push_str(&format!("- {u}\n"));
    }
    let info: Vec<&Finding> = r.findings.iter().filter(|f| f.status == Status::Informational && f.face == Face::Prerequisites).collect();
    if !info.is_empty() {
        s.push_str("\n### informational\n\n");
        for f in info {
            finding(f, &mut s);
        }
    }

    s.push_str("\n## provenance\n\n");
    let p = &r.provenance;
    s.push_str(&format!("- inputs digest: `{}`\n- lock: {}\n- manifests: {} digested\n", p.inputs, p.lock.as_ref().map(|(f, d)| format!("`{f}` {}", crate::digest::short(d, 12))).unwrap_or_else(|| "none".into()), p.manifests.len()));
    for x in &p.ran {
        s.push_str(&format!("- ran `{}` in `{}`: exit {} ({} ms){}\n", x.line, x.dir, x.code.map(|c| c.to_string()).unwrap_or_else(|| "-".into()), x.millis, if x.said.is_empty() { String::new() } else { format!(" — {}", x.said) }));
    }
    s.push_str(&format!("- mutation performed: {}\n", p.mutation));
    s
}
