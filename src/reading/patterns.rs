// Copyright © 2026 1o. All rights reserved.

//! `patterns`: every function body in scope against the simple_impl toolkit.
//!
//! For each function: its shape (klepto's, `klepto::patterns`, the callees level's hash and token count; and the
//! frames of its statements, `toolkit::frame`, which klepto's whole-function shapes are too coarse for), the toolkit
//! pieces whose expansion it holds (`toolkit::matcher`: exact, or near with the difference named), and for a
//! function with an exact match the replace_body spec that swaps what matches for the calls. Then the toolkit's
//! pieces by how many functions hold them, the shapes that repeat across crates and no piece writes (ranked by count
//! times size, each with a body that has it and a proposed macro), and the counts by crate.

use super::facts::Facts;
use super::kinds::{named, site};
use super::spec::ReadingSpec;
use super::Answer;
use crate::toolkit::matcher::{self, Context, FnMatch, FnParts, Source};
use crate::toolkit::normal::Level;
use crate::toolkit::frame;
use crate::Result;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use syn::spanned::Spanned;

/// One function, read.
#[derive(Debug, Clone)]
struct FnRow {
    id: String,
    package: String,
    file: PathBuf,
    line: u32,
    column: u32,
    /// `exact`, `near`, `catch-all` (only `simple_method`), `none`.
    status: &'static str,
    /// Each piece matched, with how near (`ensure`, `require_some (near)`).
    pieces: Vec<(String, Level)>,
    also: Vec<String>,
    differences: Vec<String>,
    spec: Option<String>,
    uses_toolkit: bool,
    /// The frames of what no piece matched, each with the statement's text and line.
    frames: Vec<(frame::Frame, String, usize)>,
    /// The function's call for each exact piece (an example per piece).
    calls: Vec<(String, String)>,
}

/// A file to read: where, whose, its text, and klepto's functions in it by their place.
struct FileJob {
    path: PathBuf,
    package: String,
    source: String,
    fns: HashMap<(u32, u32), String>,
}

/// One function matched into its row.
fn row(src: &Source, job: &FileJob, at: proc_macro2::Span, parts: FnParts, ctx: &Context) -> Option<FnRow> {
    let FnParts { sig, body: block, .. } = parts;
    let s = at.start();
    let id = job.fns.get(&(s.line as u32, s.column as u32))?;
    let m: FnMatch = matcher::match_fn(src, parts, ctx);
    let spec = matcher::spec_for(&m, src, id, parts, false).or_else(|| matcher::spec_for(&m, src, id, parts, true));
    let mut pieces: Vec<(String, Level)> = Vec::new();
    let mut calls: Vec<(String, String)> = Vec::new();
    if let Some(w) = &m.whole {
        pieces.push((w.shape().piece.clone(), w.level));
        if w.level == Level::Exact {
            if let Some(f) = spec.as_ref().and_then(|s| s.function.as_ref()) {
                calls.push((w.shape().piece.clone(), crate::change::render_fn(f, "").unwrap_or_default().trim().to_string()));
            }
        }
    }
    for st in &m.steps {
        let level = if st.comment { Level::Near } else { st.matched.level };
        pieces.push((st.matched.shape().piece.clone(), level));
        if level == Level::Exact {
            if let Ok(c) = matcher::call_text(st, src) {
                calls.push((st.matched.shape().piece.clone(), c));
            }
        }
    }
    let status = if m.exact() {
        "exact"
    } else if m.near() {
        "near"
    } else if m.catch_all.as_ref().is_some_and(|c| c.level == Level::Exact) {
        "catch-all"
    } else {
        "none"
    };
    if status == "catch-all" {
        pieces.push(("simple_method".into(), Level::Exact));
    }
    let mut frames: Vec<(frame::Frame, String, usize)> = m.unmatched.iter().map(|u| (u.frame.clone(), u.text.clone(), u.line)).collect();
    // a small method no generator writes (the catch-all aside) is a whole-method frame
    if m.whole.is_none() {
        if let Some(f) = &m.method_frame {
            let text = src.range(block.span()).map(|(a, b)| format!("{} {}", quote::ToTokens::to_token_stream(sig), &src.text[a..b])).unwrap_or_default();
            frames.push((f.clone(), text, s.line));
        }
    }
    Some(FnRow {
        id: id.clone(),
        package: job.package.clone(),
        file: job.path.clone(),
        line: s.line as u32,
        column: s.column as u32,
        status,
        also: m.whole_also.iter().chain(m.steps.iter().flat_map(|x| x.also.iter())).cloned().collect::<BTreeSet<_>>().into_iter().collect(),
        differences: m.differences(),
        pieces,
        spec: spec.as_ref().and_then(|x| serde_json::to_string(x).ok()),
        uses_toolkit: m.uses_toolkit,
        frames,
        calls,
    })
}

/// One file's functions read (on the thread that holds the file's tokens).
fn read_file(job: &FileJob) -> std::result::Result<Vec<FnRow>, String> {
    let file = syn::parse_file(&job.source).map_err(|e| format!("{}: syn cannot parse it: {e}", job.path.display()))?;
    let src = Source::new(&job.source);
    let mut out = Vec::new();
    matcher::each_fn(&file, |at, parts, ctx| out.extend(row(&src, job, at, parts, ctx)));
    Ok(out)
}

/// The jobs spread over the machine's threads, each thread its own files.
fn read_all(jobs: Vec<FileJob>) -> (Vec<FnRow>, Vec<String>) {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
    let mut buckets: Vec<Vec<FileJob>> = (0..n).map(|_| Vec::new()).collect();
    // biggest first, round robin, so the threads end together
    let mut jobs = jobs;
    jobs.sort_by_key(|j| std::cmp::Reverse(j.source.len()));
    for (i, j) in jobs.into_iter().enumerate() {
        buckets[i % n].push(j);
    }
    let mut rows = Vec::new();
    let mut not_read = Vec::new();
    std::thread::scope(|s| {
        let handles: Vec<_> = buckets
            .into_iter()
            .map(|b| {
                s.spawn(move || {
                    let mut rows = Vec::new();
                    let mut not = Vec::new();
                    for j in &b {
                        match read_file(j) {
                            Ok(r) => rows.extend(r),
                            Err(e) => not.push(e),
                        }
                    }
                    (rows, not)
                })
            })
            .collect();
        for h in handles {
            if let Ok((r, e)) = h.join() {
                rows.extend(r);
                not_read.extend(e);
            } else {
                not_read.push("a reading thread stopped".into());
            }
        }
    });
    rows.sort_by(|a, b| (&a.file, a.line, a.column).cmp(&(&b.file, b.line, b.column)));
    (rows, not_read)
}

/// A frame repeated: how often, in which crates, one body that has it.
#[derive(Debug, Clone)]
struct Repeated {
    frame: frame::Frame,
    count: usize,
    crates: BTreeSet<String>,
    example: (String, PathBuf, usize, String),
}

pub(crate) fn answer(f: &Facts, spec: &ReadingSpec) -> Result<Answer> {
    let o = &spec.patterns;
    let only: Option<BTreeSet<String>> = (spec.target.trim() != "*").then(|| spec.target.split(',').map(|s| s.trim().replace('-', "_")).collect());
    let in_scope = |p: &str| only.as_ref().is_none_or(|o| o.contains(p));
    // (1) klepto's shapes, at the callees level
    let mined = klepto::patterns::mine(&f.k, &klepto::MiningOptions { min_tokens: o.min_tokens, min_members: 2 });
    let klepto_shape: HashMap<&str, (String, usize)> = mined
        .shapes
        .iter()
        .map(|s| (s.id.as_str(), (s.hashes.get(&klepto::ShapeLevel::Callees).map(|h| h.chars().take(12).collect()).unwrap_or_default(), s.tokens)))
        .collect();
    // klepto's functions, by file and place
    let mut by_file: HashMap<PathBuf, HashMap<(u32, u32), String>> = HashMap::new();
    for c in f.k.functions.iter().filter(|c| c.generated.is_none()) {
        let (Some(l), Some(col)) = (c.location.line, c.location.column) else { continue };
        by_file.entry(c.location.path.clone()).or_default().insert((l, col), c.fq_name.clone());
    }
    let jobs: Vec<FileJob> = f
        .k
        .files
        .iter()
        .filter_map(|pf| {
            let package = f.package_of(&pf.path)?.to_string();
            if !in_scope(&package) {
                return None;
            }
            let fns = by_file.remove(&pf.path)?;
            Some(FileJob { path: pf.path.clone(), package, source: pf.source.clone(), fns })
        })
        .collect();
    let (rows, not_read) = read_all(jobs);

    // (2) the toolkit's pieces by the functions that hold them
    #[derive(Default)]
    struct PieceCount {
        functions: BTreeSet<String>,
        exact: BTreeSet<String>,
        near: BTreeSet<String>,
        crates: BTreeSet<String>,
        statements: usize,
        example: Option<(String, String)>,
    }
    let mut by_piece: BTreeMap<String, PieceCount> = BTreeMap::new();
    for r in &rows {
        for (p, level) in &r.pieces {
            let e = by_piece.entry(p.clone()).or_default();
            e.functions.insert(r.id.clone());
            e.crates.insert(r.package.clone());
            e.statements += 1;
            match level {
                Level::Exact => e.exact.insert(r.id.clone()),
                Level::Near => e.near.insert(r.id.clone()),
            };
        }
        for (p, call) in &r.calls {
            let e = by_piece.entry(p.clone()).or_default();
            if e.example.is_none() {
                e.example = Some((r.id.clone(), call.clone()));
            }
        }
    }

    // (3) frames no piece writes, repeated across crates
    let mut repeated: HashMap<String, Repeated> = HashMap::new();
    for r in &rows {
        for (fr, text, line) in &r.frames {
            if !fr.structural || fr.fixed < o.min_size {
                continue;
            }
            let e = repeated.entry(fr.text.clone()).or_insert_with(|| Repeated { frame: fr.clone(), count: 0, crates: BTreeSet::new(), example: (r.id.clone(), r.file.clone(), *line, text.clone()) });
            e.count += 1;
            e.crates.insert(r.package.clone());
        }
    }
    let mut candidates: Vec<Repeated> = repeated.into_values().filter(|c| c.count >= o.min_repeats && c.crates.len() >= o.min_crates).collect();
    candidates.sort_by(|a, b| (std::cmp::Reverse(a.count * a.frame.fixed), &a.frame.text).cmp(&(std::cmp::Reverse(b.count * b.frame.fixed), &b.frame.text)));
    let candidate_texts: HashMap<&str, usize> = candidates.iter().enumerate().map(|(i, c)| (c.frame.text.as_str(), i)).collect();

    // (4) by crate
    #[derive(Default)]
    struct CrateCount {
        functions: usize,
        exact: usize,
        near: usize,
        catch_all: usize,
        uses_toolkit: usize,
        candidate_statements: usize,
    }
    let mut by_crate: BTreeMap<String, CrateCount> = BTreeMap::new();
    for r in &rows {
        let c = by_crate.entry(r.package.clone()).or_default();
        c.functions += 1;
        match r.status {
            "exact" => c.exact += 1,
            "near" => c.near += 1,
            "catch-all" => c.catch_all += 1,
            _ => {}
        }
        if r.uses_toolkit {
            c.uses_toolkit += 1;
        }
        c.candidate_statements += r.frames.iter().filter(|(fr, ..)| candidate_texts.contains_key(fr.text.as_str())).count();
    }

    let mut ans = Answer::default();
    let n_exact = rows.iter().filter(|r| r.status == "exact").count();
    let n_near = rows.iter().filter(|r| r.status == "near").count();
    ans.count("functions shaped", rows.len());
    ans.count("functions with an exact toolkit match (the swap is the same tokens after expansion)", n_exact);
    ans.count("functions with only near matches (the difference named)", n_near);
    ans.count("functions only the catch-all simple_method writes (one expression)", rows.iter().filter(|r| r.status == "catch-all").count());
    ans.count("functions already calling the toolkit", rows.iter().filter(|r| r.uses_toolkit).count());
    ans.count("toolkit pieces matched somewhere", by_piece.len());
    ans.count(format!("candidate shapes (no piece writes them; at least {} times in at least {} crates)", o.min_repeats, o.min_crates), candidates.len());
    ans.count("crates", by_crate.len());

    // rows: the pieces by functions, the candidates, the crates, then every function
    let mut piece_rows: Vec<(usize, crate::CodeQueryResult)> = by_piece
        .iter()
        .map(|(p, c)| {
            let mut attrs = vec![
                ("functions", c.functions.len().to_string()),
                ("exact", c.exact.len().to_string()),
                ("near", c.near.len().to_string()),
                ("crates", c.crates.len().to_string()),
                ("places", c.statements.to_string()),
            ];
            if let Some((id, call)) = &c.example {
                attrs.push(("example", format!("{id}: {}", call.replace('\n', " "))));
            }
            (c.functions.len(), named("toolkit-piece", p.clone(), None, "the toolkit piece whose expansion these bodies hold", attrs))
        })
        .collect();
    piece_rows.sort_by(|(x, a), (y, b)| y.cmp(x).then(a.label.cmp(&b.label)));
    ans.headline = piece_rows.iter().take(3).map(|(_, r)| format!("{}-shaped: {} functions in {} crates, {} exact", r.label, r.attributes["functions"], r.attributes["crates"], r.attributes["exact"])).collect();
    if let Some(c) = candidates.first() {
        ans.headline.push(format!("most repeated shape no piece writes: {} ({} times in {} crates)", frame::proposed_name(&c.frame), c.count, c.crates.len()));
    }
    ans.rows.extend(piece_rows.into_iter().map(|(_, r)| r));
    let mut names_used: BTreeMap<String, usize> = BTreeMap::new();
    for (i, c) in candidates.iter().take(o.candidates).enumerate() {
        // one name per shape: a second shape that would take a name gets it numbered
        let base = frame::proposed_name(&c.frame);
        let n = names_used.entry(base.clone()).or_default();
        *n += 1;
        let name = if *n == 1 { base } else { format!("{base}_{n}") };
        let args = frame::proposed_args(&c.frame);
        let (id, file, line, text) = &c.example;
        let mut r = site(
            f,
            "candidate-shape",
            format!("{}. {name}", i + 1),
            file,
            *line as u32,
            None,
            Some(id),
            "a shape no toolkit piece writes, repeated across crates",
            vec![
                ("count", c.count.to_string()),
                ("crates", c.crates.len().to_string()),
                ("size", c.frame.fixed.to_string()),
                ("score", (c.count * c.frame.fixed).to_string()),
                ("frame", c.frame.text.clone()),
                ("proposed", format!("{name}!({})", args.join(", "))),
                ("example", text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(400).collect()),
            ],
        );
        r.crate_name = f.package_of(file).map(str::to_string);
        ans.rows.push(r);
    }
    for (name, c) in &by_crate {
        ans.rows.push(named(
            "crate",
            name.clone(),
            Some(name.clone()),
            "the crate's functions against the toolkit",
            vec![
                ("functions", c.functions.to_string()),
                ("exact", c.exact.to_string()),
                ("near", c.near.to_string()),
                ("catch_all", c.catch_all.to_string()),
                ("calls_the_toolkit", c.uses_toolkit.to_string()),
                ("candidate_statements", c.candidate_statements.to_string()),
            ],
        ));
    }
    for r in &rows {
        let pieces: Vec<String> = r.pieces.iter().map(|(p, l)| if *l == Level::Near { format!("{p} (near)") } else { p.clone() }).collect();
        let mut attrs = vec![("status", r.status.to_string())];
        if let Some((h, t)) = klepto_shape.get(r.id.as_str()) {
            attrs.push(("shape", format!("{h} ({t} tokens)")));
        }
        if !pieces.is_empty() {
            attrs.push(("pieces", pieces.join(", ")));
        }
        if !r.also.is_empty() {
            attrs.push(("also", r.also.join(", ")));
        }
        if !r.differences.is_empty() {
            attrs.push(("differences", r.differences.join("; ")));
        }
        let cand: Vec<String> = r.frames.iter().filter_map(|(fr, ..)| candidate_texts.get(fr.text.as_str()).map(|i| format!("#{}", i + 1))).collect::<BTreeSet<_>>().into_iter().collect();
        if !cand.is_empty() {
            attrs.push(("candidates", cand.join(", ")));
        }
        if let Some(s) = &r.spec {
            attrs.push(("spec", s.clone()));
        }
        let mut row = site(f, "function", r.id.clone(), &r.file, r.line, Some(r.column), Some(&r.id), "a function body against the toolkit", attrs);
        row.attributes.remove("context");
        ans.rows.push(row);
    }
    ans.not_read = not_read;
    Ok(ans)
}
