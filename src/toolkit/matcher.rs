// Copyright © 2026 1o. All rights reserved.

//! A function's body matched against the toolkit's shapes, and the replace_body spec that swaps what matches for
//! the calls.
//!
//! A statement matches a shape when its tokens in the normal form are the shape's with each hole filled (holes
//! filled shortest first, backtracking). The filling is then put back into the raw expansion, an expression or a
//! type inside an invisible group as rustc puts a `$x:expr` there, and the two normal forms compared: equal is
//! exact. Statements are matched at every depth of the body (a `continue_if` sits in a loop); a matched statement
//! is not looked inside. Whole functions are matched against the method generators (the function without its
//! attributes and visibility, which the generator passes through) and the builder lines (with them: a builder
//! writes `#[inline] pub` itself); a trait impl's one function against the impl generators of that trait.

use super::frame::{self, Frame};
use super::normal::{self, Level, Normal};
use super::{catalog, HoleKind, Position, ToolkitShape};
use crate::change::{ChangeSpec, Family, FnSpec, Operation, Param, Piece};
use proc_macro2::{Delimiter, Group, Spacing, TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::{BTreeMap, HashMap};
use syn::spanned::Spanned;
use syn::visit::Visit;

// ---------------------------------------------------------------------------
// the shapes, prepared once

/// A catalog shape ready to match: its pattern's tokens at both levels, its raw expansion's tokens, its holes.
pub struct Prepared {
    pub index: usize,
    pub exact: Vec<TokenTree>,
    pub near: Vec<TokenTree>,
    pub raw: TokenStream,
    pub kinds: HashMap<String, HoleKind>,
}

fn rename_binders(text: &str, shape: &ToolkitShape) -> String {
    let mut t = text.to_string();
    for (ph, name) in &shape.binders {
        t = super::probe::rename_word(&t, name, ph);
    }
    t.replace("__sia_args : __SiArgsTy", "__sia_args")
}

fn prepare(index: usize, shape: &ToolkitShape) -> Option<Prepared> {
    let raw: TokenStream = shape.expansion.parse().ok()?;
    // the raw expansion with `__sia_args` standing for the parameters parses as a function only once filled; the
    // patterns are made from the probe's form, where it was a parameter
    let probe_form: TokenStream = shape.expansion.replace("(__sia_args)", "(__sia_args : __SiArgsTy)").replace("( __sia_args )", "(__sia_args : __SiArgsTy)").parse().ok()?;
    let at = |level| normal::normal_tokens(probe_form.clone(), shape.position, level).ok().map(|n| rename_binders(&n.text, shape)).and_then(|t| t.parse::<TokenStream>().ok()).map(|ts| ts.into_iter().collect::<Vec<_>>());
    let exact = at(Level::Exact)?;
    let near = at(Level::Near)?;
    let mut kinds: HashMap<String, HoleKind> = shape.holes.iter().map(|h| (h.placeholder.clone(), h.kind)).collect();
    for (ph, _) in &shape.binders {
        kinds.insert(ph.clone(), HoleKind::Binder);
    }
    Some(Prepared { index, exact, near, raw, kinds })
}

fn build_prepared() -> BTreeMap<Position, Vec<Prepared>> {
    let c = catalog();
    let mut by: BTreeMap<Position, Vec<Prepared>> = BTreeMap::new();
    for (i, s) in c.shapes.iter().enumerate() {
        if let Some(p) = prepare(i, s) {
            by.entry(s.position).or_default().push(p);
        }
    }
    for v in by.values_mut() {
        v.sort_by_key(|p| (std::cmp::Reverse(c.shapes[p.index].specificity), p.index));
    }
    by
}

thread_local! {
    /// Every shape prepared (tokens live on one thread), the most particular first within each position.
    static PREPARED: BTreeMap<Position, Vec<Prepared>> = build_prepared();
}

/// `f` over the prepared shapes of this thread.
pub fn with_prepared<R>(f: impl FnOnce(&BTreeMap<Position, Vec<Prepared>>) -> R) -> R {
    PREPARED.with(|p| f(p))
}

// ---------------------------------------------------------------------------
// matching tokens with holes

/// What each hole was filled with: tokens of the candidate's normal form.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    pub map: BTreeMap<String, Vec<TokenTree>>,
}

fn text(ts: &[TokenTree]) -> String {
    ts.iter().cloned().collect::<TokenStream>().to_string()
}

fn same(a: &TokenTree, b: &TokenTree) -> bool {
    match (a, b) {
        (TokenTree::Ident(x), TokenTree::Ident(y)) => x == y,
        (TokenTree::Punct(x), TokenTree::Punct(y)) => x.as_char() == y.as_char() && (x.spacing() == y.spacing() || x.spacing() == Spacing::Alone && y.spacing() == Spacing::Alone),
        (TokenTree::Literal(x), TokenTree::Literal(y)) => x.to_string() == y.to_string(),
        (TokenTree::Group(x), TokenTree::Group(y)) => x.delimiter() == y.delimiter() && x.stream().to_string() == y.stream().to_string(),
        _ => false,
    }
}

fn hole_of(t: &TokenTree, kinds: &HashMap<String, HoleKind>) -> Option<(String, HoleKind)> {
    match t {
        TokenTree::Ident(i) => {
            let s = i.to_string();
            kinds.get(&s).map(|k| (s, *k))
        }
        _ => None,
    }
}

/// Whether tokens are what a hole of this kind takes, as the macro's fragment would read them (an expression that
/// is not a `let`, a type, a path, a pattern).
fn fits(kind: HoleKind, ts: &[TokenTree]) -> bool {
    let stream: TokenStream = ts.iter().cloned().collect();
    match kind {
        HoleKind::Expr => matches!(syn::parse2::<syn::Expr>(stream), Ok(e) if !matches!(e, syn::Expr::Let(_))),
        HoleKind::Type => syn::parse2::<syn::Type>(stream).is_ok(),
        HoleKind::Path => syn::parse2::<syn::Path>(stream).is_ok(),
        HoleKind::Pat => syn::parse::Parser::parse2(syn::Pat::parse_single, stream).is_ok(),
        HoleKind::Ident | HoleKind::Binder | HoleKind::FieldPath | HoleKind::Args => true,
    }
}

/// `p` matched against `c`, holes filled into `b`; `budget` bounds the search.
fn match_seq(p: &[TokenTree], c: &[TokenTree], kinds: &HashMap<String, HoleKind>, b: &mut Bindings, budget: &mut usize) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let Some(first) = p.first() else { return c.is_empty() };
    if let Some((name, kind)) = hole_of(first, kinds) {
        // a hole already filled must be filled the same here
        if let Some(bound) = b.map.get(&name).cloned() {
            let n = bound.len();
            return c.len() >= n && text(&c[..n]) == text(&bound) && match_seq(&p[1..], &c[n..], kinds, b, budget);
        }
        let lengths: Vec<usize> = match kind {
            HoleKind::Ident | HoleKind::Binder => {
                if matches!(c.first(), Some(TokenTree::Ident(_))) {
                    vec![1]
                } else {
                    vec![]
                }
            }
            HoleKind::FieldPath => {
                let mut v = Vec::new();
                let mut i = 0;
                while matches!(c.get(i), Some(TokenTree::Ident(_))) {
                    v.push(i + 1);
                    if matches!(c.get(i + 1), Some(TokenTree::Punct(pp)) if pp.as_char() == '.') && matches!(c.get(i + 2), Some(TokenTree::Ident(_))) {
                        i += 2;
                    } else {
                        break;
                    }
                }
                v
            }
            _ => {
                let min = if kind == HoleKind::Args { 0 } else { 1 };
                let next = p.get(1).filter(|t| hole_of(t, kinds).is_none());
                (min..=c.len()).filter(|&n| match next {
                    Some(TokenTree::Group(g)) => matches!(c.get(n), Some(TokenTree::Group(x)) if x.delimiter() == g.delimiter()),
                    Some(t) => c.get(n).is_some_and(|x| same(t, x)),
                    None => true,
                }).collect()
            }
        };
        for n in lengths {
            if !fits(kind, &c[..n]) {
                continue;
            }
            let mut trial = b.clone();
            trial.map.insert(name.clone(), c[..n].to_vec());
            if match_seq(&p[1..], &c[n..], kinds, &mut trial, budget) {
                *b = trial;
                return true;
            }
        }
        return false;
    }
    let Some(cf) = c.first() else { return false };
    match (first, cf) {
        (TokenTree::Group(pg), TokenTree::Group(cg)) if pg.delimiter() == cg.delimiter() => {
            let pi: Vec<TokenTree> = pg.stream().into_iter().collect();
            let ci: Vec<TokenTree> = cg.stream().into_iter().collect();
            let mut trial = b.clone();
            if match_seq(&pi, &ci, kinds, &mut trial, budget) && match_seq(&p[1..], &c[1..], kinds, &mut trial, budget) {
                *b = trial;
                return true;
            }
            false
        }
        (TokenTree::Group(_), _) | (_, TokenTree::Group(_)) => false,
        _ => same(first, cf) && match_seq(&p[1..], &c[1..], kinds, b, budget),
    }
}

/// The raw expansion with each hole filled: an expression, a type or a path in an invisible group, as rustc puts a
/// `$x:expr` there; a name, a field path, parameters or a pattern as they are.
fn instantiate(raw: &TokenStream, shape: &ToolkitShape, kinds: &HashMap<String, HoleKind>, b: &Bindings) -> TokenStream {
    let binder_of: HashMap<&str, &str> = shape.binders.iter().map(|(ph, name)| (name.as_str(), ph.as_str())).collect();
    fn walk(ts: TokenStream, kinds: &HashMap<String, HoleKind>, binder_of: &HashMap<&str, &str>, b: &Bindings) -> TokenStream {
        let mut out = TokenStream::new();
        for t in ts {
            match t {
                TokenTree::Group(g) => {
                    let mut ng = Group::new(g.delimiter(), walk(g.stream(), kinds, binder_of, b));
                    ng.set_span(g.span());
                    out.extend([TokenTree::Group(ng)]);
                }
                TokenTree::Ident(ref i) => {
                    let s = i.to_string();
                    let key = binder_of.get(s.as_str()).map(|x| x.to_string()).unwrap_or(s);
                    match (b.map.get(&key), kinds.get(&key)) {
                        (Some(fill), Some(HoleKind::Expr | HoleKind::Type | HoleKind::Path)) => out.extend([TokenTree::Group(Group::new(Delimiter::None, fill.iter().cloned().collect()))]),
                        (Some(fill), Some(_)) => out.extend(fill.iter().cloned()),
                        _ => out.extend([t]),
                    }
                }
                other => out.extend([other]),
            }
        }
        out
    }
    walk(raw.clone(), kinds, &binder_of, b)
}

/// One shape matched: which, how near, what made it near, and its holes' fillings.
#[derive(Debug, Clone)]
pub struct Matched {
    pub shape: usize,
    pub level: Level,
    pub differences: Vec<String>,
    pub bindings: Bindings,
}

impl Matched {
    pub fn shape(&self) -> &'static ToolkitShape {
        &catalog().shapes[self.shape]
    }
}

/// A candidate in both normal forms.
pub struct Cand {
    pub exact: Normal,
    pub near: Normal,
}

/// Every shape of `position` that `cand` matches, most particular first; the first exact one ends the search.
fn try_all(position: Position, cand: &Cand, filter: impl Fn(&ToolkitShape) -> bool) -> Vec<Matched> {
    let c = catalog();
    let ce: Vec<TokenTree> = cand.exact.tokens.clone().into_iter().collect();
    let cn: Vec<TokenTree> = cand.near.tokens.clone().into_iter().collect();
    let mut out = Vec::new();
    with_prepared(|all| {
    for p in all.get(&position).map(Vec::as_slice).unwrap_or(&[]) {
        let shape = &c.shapes[p.index];
        if !filter(shape) {
            continue;
        }
        // a quick refusal: the first tokens differ
        let first_ok = |pat: &[TokenTree], cc: &[TokenTree]| match (pat.first(), cc.first()) {
            (Some(x), Some(y)) if hole_of(x, &p.kinds).is_none() && !matches!(x, TokenTree::Group(_)) => same(x, y),
            _ => true,
        };
        let mut found = None;
        if first_ok(&p.exact, &ce) {
            let mut b = Bindings::default();
            let mut budget = 20_000;
            if match_seq(&p.exact, &ce, &p.kinds, &mut b, &mut budget) {
                let inst = instantiate(&p.raw, shape, &p.kinds, &b);
                if normal::normal_tokens(inst.clone(), position, Level::Exact).is_ok_and(|n| n.text == cand.exact.text) {
                    found = Some(Matched { shape: p.index, level: Level::Exact, differences: Vec::new(), bindings: b });
                } else if let Ok(n) = normal::normal_tokens(inst, position, Level::Near) {
                    if n.text == cand.near.text {
                        let mut d: Vec<String> = cand.near.fired.iter().chain(&n.fired).map(|s| s.to_string()).collect();
                        d.dedup();
                        found = Some(Matched { shape: p.index, level: Level::Near, differences: d, bindings: b });
                    }
                }
            }
        }
        let near_differs = cand.near.text != cand.exact.text || text(&p.near) != text(&p.exact);
        if found.is_none() && near_differs && first_ok(&p.near, &cn) {
            let mut b = Bindings::default();
            let mut budget = 20_000;
            if match_seq(&p.near, &cn, &p.kinds, &mut b, &mut budget) {
                let inst = instantiate(&p.raw, shape, &p.kinds, &b);
                if let Ok(n) = normal::normal_tokens(inst, position, Level::Near) {
                    if n.text == cand.near.text {
                        let mut d: Vec<String> = cand.near.fired.iter().chain(&n.fired).map(|s| s.to_string()).collect();
                        d.sort();
                        d.dedup();
                        found = Some(Matched { shape: p.index, level: Level::Near, differences: d, bindings: b });
                    }
                }
            }
        }
        if let Some(m) = found {
            let exact = m.level == Level::Exact;
            out.push(m);
            if exact {
                break;
            }
        }
    }
    });
    // exact ones first, then by how particular the shape is
    out.sort_by_key(|m| (m.level, std::cmp::Reverse(c.shapes[m.shape].specificity), m.shape));
    out
}

// ---------------------------------------------------------------------------
// the source: line and column to byte offsets, a filling's text as written

/// A file's text with its line starts, for taking spans' text.
pub struct Source<'s> {
    pub text: &'s str,
    starts: Vec<usize>,
}

impl<'s> Source<'s> {
    pub fn new(text: &'s str) -> Self {
        let mut starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Source { text, starts }
    }

    /// proc_macro2's line (1-based) and column (characters) as a byte offset.
    pub fn offset(&self, line: usize, column: usize) -> Option<usize> {
        let start = *self.starts.get(line.checked_sub(1)?)?;
        let rest = &self.text[start..];
        let mut off = start;
        for (n, ch) in rest.chars().enumerate() {
            if n == column {
                return Some(off);
            }
            if ch == '\n' {
                return Some(off);
            }
            off += ch.len_utf8();
        }
        Some(off)
    }

    /// A span's bytes, when it has a place in this file.
    pub fn range(&self, span: proc_macro2::Span) -> Option<(usize, usize)> {
        let (s, e) = (span.start(), span.end());
        // a span made here (call_site) is zero-length: it has no place in the file
        if s.line == 0 || (s.line, s.column) == (e.line, e.column) {
            return None;
        }
        Some((self.offset(s.line, s.column)?, self.offset(e.line, e.column)?))
    }

    /// The bytes covered by every leaf of `ts` that has a place.
    fn cover(&self, ts: &[TokenTree]) -> Option<(usize, usize)> {
        let mut lo = usize::MAX;
        let mut hi = 0;
        fn each(src: &Source, t: &TokenTree, lo: &mut usize, hi: &mut usize) {
            match t {
                TokenTree::Group(g) => {
                    if let Some((a, b)) = src.range(g.span()) {
                        *lo = (*lo).min(a);
                        *hi = (*hi).max(b);
                    } else {
                        for x in g.stream() {
                            each(src, &x, lo, hi);
                        }
                    }
                }
                other => {
                    if let Some((a, b)) = src.range(other.span()) {
                        *lo = (*lo).min(a);
                        *hi = (*hi).max(b);
                    }
                }
            }
        }
        for t in ts {
            each(self, t, &mut lo, &mut hi);
        }
        (lo < hi).then_some((lo, hi))
    }

    /// A filling as written in the source (one parenthesized group: what is inside it); its tokens' text when it
    /// has no place.
    pub fn filling(&self, ts: &[TokenTree]) -> String {
        let inner: Vec<TokenTree>;
        let ts = match ts {
            [TokenTree::Group(g)] if g.delimiter() == Delimiter::Parenthesis => {
                inner = g.stream().into_iter().collect();
                // a tuple's parentheses are the value (`(gate_note, gate_ok)`): only grouping ones are let go
                let tuple = inner.iter().any(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ','));
                if inner.is_empty() || tuple {
                    ts
                } else {
                    &inner[..]
                }
            }
            _ => ts,
        };
        match self.cover(ts) {
            Some((a, b)) => self.text[a..b].to_string(),
            None => text(ts),
        }
    }

    /// Whether a node's source holds a comment (characters no token of it covers).
    pub fn has_comment(&self, node: &impl ToTokens) -> bool {
        let ts: Vec<TokenTree> = node.to_token_stream().into_iter().collect();
        let Some((a, b)) = self.cover(&ts) else { return false };
        let written: usize = self.text[a..b].chars().filter(|c| !c.is_whitespace()).count();
        fn leaves(src: &Source, t: &TokenTree, n: &mut usize) {
            match t {
                TokenTree::Group(g) => {
                    *n += 2;
                    for x in g.stream() {
                        leaves(src, &x, n);
                    }
                }
                other => *n += src.range(other.span()).map(|(a, b)| src.text[a..b].chars().filter(|c| !c.is_whitespace()).count()).unwrap_or(0),
            }
        }
        let mut n = 0;
        for t in &ts {
            leaves(self, t, &mut n);
        }
        written > n
    }
}

// ---------------------------------------------------------------------------
// a match as a toolkit piece

/// A matched shape's piece, its slots filled with what the body has there.
pub fn piece_of(m: &Matched, src: &Source) -> Piece {
    let shape = m.shape();
    let mut lists: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for h in shape.holes.iter().filter(|h| h.kind != HoleKind::Binder) {
        let Some(fill) = m.bindings.map.get(&h.placeholder) else { continue };
        let filling = src.filling(fill);
        // a slot written around its placeholder (`|| __si_error`) takes the filling in its place
        let written = match super::probe::written_in(m.shape, &h.placeholder) {
            Some(text) => super::probe::rename_word(text, &h.placeholder, &filling),
            None => filling,
        };
        lists.entry(h.slot.clone()).or_default().push((h.placeholder.clone(), written));
    }
    let mut p = Piece { piece: shape.piece.clone(), ..Piece::default() };
    for (slot, mut values) in lists {
        values.sort();
        set_slot(&mut p, &slot, values.into_iter().map(|(_, v)| v).collect());
    }
    // the signature's parts (its name, its parameters' types) are the function's, not the call's
    let takes = crate::change::slots_of(&shape.piece);
    for s in crate::change::SLOTS {
        if !takes.iter().any(|t| t == s) && *s != "bind" {
            clear_slot(&mut p, s);
        }
    }
    p
}

fn clear_slot(p: &mut Piece, slot: &str) {
    let mut q = Piece { piece: p.piece.clone(), bind: p.bind.clone(), ..Piece::default() };
    for s in crate::change::SLOTS {
        if *s == slot || *s == "bind" {
            continue;
        }
        if let Some(v) = p.slot(s) {
            set_slot(&mut q, s, v);
        }
    }
    *p = q;
}

/// A slot set by its name (the inverse of `Piece::slot`).
pub fn set_slot(p: &mut Piece, slot: &str, v: Vec<String>) {
    let one = v.first().cloned();
    match slot {
        "condition" => p.condition = one,
        "conditions" => p.conditions = v,
        "error" => p.error = one,
        "errors" => p.errors = v,
        "value" => p.value = one,
        "values" => p.values = one,
        "option" => p.option = one,
        "result" => p.result = one,
        "map_error" => p.map_error = one,
        "return_value" => p.return_value = one,
        "left" => p.left = one,
        "right" => p.right = one,
        "expected" => p.expected = one,
        "minimum" => p.minimum = one,
        "maximum" => p.maximum = one,
        "collection" => p.collection = one,
        "map" => p.map = one,
        "key" => p.key = one,
        "place" => p.place = one,
        "cleanup" => p.cleanup = one,
        "candidates" => p.candidates = v,
        "parts" => p.parts = one,
        "this" => p.this = one,
        "fields" => p.fields = v,
        "flags" => p.flags = v,
        "args" => p.args = v,
        "opt" => p.opt = one,
        "prefix" => p.prefix = one,
        "iter" => p.iter = one,
        "vec" => p.vec = one,
        "flag" => p.flag = one,
        "field" => p.field = one,
        "method" => p.method = one,
        "pass" => p.pass = one,
        "item" => p.item = one,
        "target" => p.target = one,
        "elem" => p.elem = one,
        "error_ty" => p.error_ty = one,
        "ty" => p.ty = one,
        "wrapper" => p.wrapper = one,
        "inner" => p.inner = one,
        "text" => p.text = one,
        "source" => p.source = one,
        "variant" => p.variant = one,
        "name" => p.name = one,
        "command" => p.command = one,
        "block" => p.block = one,
        "code" => p.code = one,
        "vis" => p.vis = one,
        "bind" => p.bind = one,
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// one function

/// One statement or value of a body that matched.
#[derive(Debug, Clone)]
pub struct StepMatch {
    pub matched: Matched,
    /// The shapes it also matches, most particular first.
    pub also: Vec<String>,
    /// The statement's place in the file (bytes) and its first line.
    pub range: (usize, usize),
    pub line: usize,
    /// The index of the body's top-level statement it is, or `None` inside one.
    pub top: Option<usize>,
    /// A `let`'s pattern, when the match is what a `let` binds.
    pub bind: Option<String>,
    /// The body's last expression.
    pub tail: bool,
    /// A comment inside it the call would drop.
    pub comment: bool,
}

/// A statement no shape matched, by its frame.
#[derive(Debug, Clone)]
pub struct Unmatched {
    pub frame: Frame,
    pub text: String,
    pub line: usize,
}

/// Where a function sits.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The trait of the impl it is in (`From`), and that impl's self type and how many items it has.
    pub trait_name: Option<String>,
    pub self_ty: Option<String>,
    pub impl_items: usize,
    /// In an impl at all (a method generator writes an impl's function).
    pub in_impl: bool,
}

/// What a function's body matched.
#[derive(Debug, Clone, Default)]
pub struct FnMatch {
    /// The best whole-function match (not the catch-all `simple_method`).
    pub whole: Option<Matched>,
    pub whole_also: Vec<String>,
    /// `simple_method`, the catch-all: the body is one expression.
    pub catch_all: Option<Matched>,
    pub steps: Vec<StepMatch>,
    pub unmatched: Vec<Unmatched>,
    pub method_frame: Option<Frame>,
    /// The body already calls the toolkit.
    pub uses_toolkit: bool,
}

impl FnMatch {
    pub fn exact(&self) -> bool {
        self.whole.as_ref().is_some_and(|m| m.level == Level::Exact) || self.steps.iter().any(|s| s.matched.level == Level::Exact && !s.comment)
    }

    pub fn near(&self) -> bool {
        !self.exact() && (self.whole.is_some() || !self.steps.is_empty())
    }

    /// Every difference that keeps a match near, each once.
    pub fn differences(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut add = |s: String| {
            if !out.contains(&s) {
                out.push(s);
            }
        };
        if let Some(w) = &self.whole {
            for d in &w.differences {
                add(format!("{}: {d}", w.shape().piece));
            }
        }
        for s in &self.steps {
            for d in &s.matched.differences {
                add(format!("{}: {d}", s.matched.shape().piece));
            }
            if s.comment {
                add(format!("{}: a comment inside the statement the call would drop", s.matched.shape().piece));
            }
        }
        out
    }
}

fn cand_stmt(s: &syn::Stmt) -> Cand {
    Cand { exact: normal::normal_stmts(std::slice::from_ref(s), Level::Exact), near: normal::normal_stmts(std::slice::from_ref(s), Level::Near) }
}

fn cand_expr(e: &syn::Expr) -> Cand {
    Cand { exact: normal::normal_expr(e, Level::Exact), near: normal::normal_expr(e, Level::Near) }
}

fn cand_fn(f: &syn::ImplItemFn) -> Cand {
    Cand { exact: normal::normal_fn(f, Level::Exact), near: normal::normal_fn(f, Level::Near) }
}

fn is_doc(a: &syn::Attribute) -> bool {
    a.path().is_ident("doc")
}

/// Calls of the toolkit already in a body.
struct Calls(bool);

impl<'ast> Visit<'ast> for Calls {
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if m.path.segments.first().is_some_and(|s| s.ident == crate::change::TOOLKIT.0) {
            self.0 = true;
        }
        syn::visit::visit_macro(self, m);
    }
}

struct Walker<'a, 's> {
    src: &'a Source<'s>,
    steps: Vec<StepMatch>,
    unmatched: Vec<Unmatched>,
}

fn names(ms: &[Matched]) -> Vec<String> {
    let mut v: Vec<String> = ms.iter().map(|m| m.shape().piece.clone()).collect();
    v.dedup();
    v
}

impl<'a, 's> Walker<'a, 's> {
    fn place(&self, node: &impl Spanned) -> ((usize, usize), usize) {
        let sp = node.span();
        (self.src.range(sp).unwrap_or((0, 0)), sp.start().line)
    }

    fn block(&mut self, b: &syn::Block, is_body: bool) {
        let last = b.stmts.len().saturating_sub(1);
        for (i, s) in b.stmts.iter().enumerate() {
            let top = is_body.then_some(i);
            let tail = is_body && i == last && matches!(s, syn::Stmt::Expr(_, None));
            if !self.stmt(s, tail, top) {
                syn::visit::visit_stmt(self, s);
            }
        }
    }

    /// One statement: matched as a step, or its `let` value, or (the body's last) as a value. True when matched.
    fn stmt(&mut self, s: &syn::Stmt, tail: bool, top: Option<usize>) -> bool {
        let ((a, b), line) = self.place(s);
        if matches!(s, syn::Stmt::Expr(..) | syn::Stmt::Macro(_)) && !tail {
            let ms = try_all(Position::Step, &cand_stmt(s), |_| true);
            if let Some(m) = ms.first().cloned() {
                self.steps.push(StepMatch { also: names(&ms[1..]), matched: m, range: (a, b), line, top, bind: None, tail: false, comment: self.src.has_comment(s) });
                return true;
            }
        }
        if let syn::Stmt::Local(l) = s {
            if let (Some(init), true) = (&l.init, l.attrs.is_empty()) {
                if init.diverge.is_none() {
                    let ms = try_all(Position::Value, &cand_expr(&init.expr), |_| true);
                    if let Some(m) = ms.first().cloned() {
                        let bind = self.src.range(l.pat.span()).map(|(x, y)| self.src.text[x..y].to_string()).unwrap_or_else(|| l.pat.to_token_stream().to_string());
                        self.steps.push(StepMatch { also: names(&ms[1..]), matched: m, range: (a, b), line, top, bind: Some(bind), tail: false, comment: self.src.has_comment(s) });
                        return true;
                    }
                }
            }
        }
        if tail {
            if let syn::Stmt::Expr(e, None) = s {
                let ms = try_all(Position::Value, &cand_expr(e), |_| true);
                if let Some(m) = ms.first().cloned() {
                    self.steps.push(StepMatch { also: names(&ms[1..]), matched: m, range: (a, b), line, top, bind: None, tail: true, comment: self.src.has_comment(s) });
                    return true;
                }
                // a tail that is a statement shape (an `if` with no else, as the last thing)
                let ms = try_all(Position::Step, &cand_stmt(s), |_| true);
                if let Some(m) = ms.first().cloned() {
                    if m.shape().family != Family::Expression {
                        self.steps.push(StepMatch { also: names(&ms[1..]), matched: m, range: (a, b), line, top, bind: None, tail: false, comment: self.src.has_comment(s) });
                        return true;
                    }
                }
            }
        }
        let f = frame::of_stmt(s);
        if f.structural {
            let text = self.src.text.get(a..b).unwrap_or("").to_string();
            self.unmatched.push(Unmatched { frame: f, text, line });
        }
        false
    }
}

impl<'ast, 'a, 's> Visit<'ast> for Walker<'a, 's> {
    fn visit_block(&mut self, b: &'ast syn::Block) {
        self.block(b, false);
    }
    // an item inside a body is its own function, read on its own
    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

/// A function as syn reads it: its attributes, visibility, signature and body.
#[derive(Clone, Copy)]
pub struct FnParts<'a> {
    pub attrs: &'a [syn::Attribute],
    pub vis: &'a syn::Visibility,
    pub sig: &'a syn::Signature,
    pub body: &'a syn::Block,
}

/// A function matched against the toolkit.
pub fn match_fn(src: &Source, f: FnParts, ctx: &Context) -> FnMatch {
    let FnParts { attrs, vis, sig, body } = f;
    let mut out = FnMatch::default();
    let mut calls = Calls(false);
    calls.visit_block(body);
    out.uses_toolkit = calls.0;
    // whole functions: a method generator (attributes and visibility pass through), a builder line (with them)
    {
        let bare = syn::ImplItemFn { attrs: Vec::new(), vis: syn::Visibility::Inherited, defaultness: None, sig: sig.clone(), block: body.clone() };
        let mut ms = try_all(Position::Function, &cand_fn(&bare), |s| s.family == Family::Method);
        let kept: Vec<syn::Attribute> = attrs.iter().filter(|a| !is_doc(a)).cloned().collect();
        let dressed = syn::ImplItemFn { attrs: kept, vis: vis.clone(), defaultness: None, sig: sig.clone(), block: body.clone() };
        let builder_here = ctx.in_impl && ctx.trait_name.is_none();
        let mut bs = if builder_here { try_all(Position::Function, &cand_fn(&dressed), |s| s.family == Family::Builder) } else { Vec::new() };
        if bs.is_empty() && builder_here {
            // a builder line without its `#[inline] pub`, or with its docs: near
            let inline: syn::Attribute = syn::parse_quote!(#[inline]);
            let as_builder = syn::ImplItemFn { attrs: vec![inline], vis: syn::parse_quote!(pub), defaultness: None, sig: sig.clone(), block: body.clone() };
            for mut m in try_all(Position::Function, &cand_fn(&as_builder), |s| s.family == Family::Builder) {
                m.level = Level::Near;
                let mut d = Vec::new();
                if !attrs.iter().any(|a| a.path().is_ident("inline")) {
                    d.push("the builder line writes `#[inline]`, which this method does not have".to_string());
                }
                if !matches!(vis, syn::Visibility::Public(_)) {
                    d.push("the builder line writes `pub`, which this method is not".to_string());
                }
                if attrs.iter().any(is_doc) {
                    d.push("a builder line takes no docs: this method's would be dropped".to_string());
                }
                if attrs.iter().any(|a| !is_doc(a) && !a.path().is_ident("inline")) {
                    d.push("a builder line takes no attributes: this method's would be dropped".to_string());
                }
                m.differences = d;
                bs.push(m);
            }
        }
        ms.extend(bs);
        ms.sort_by_key(|m| (m.level, std::cmp::Reverse(m.shape().specificity), m.shape));
        let (catch, rest): (Vec<Matched>, Vec<Matched>) = ms.into_iter().partition(|m| m.shape().piece == "simple_method");
        out.catch_all = catch.into_iter().next();
        if let Some(first) = rest.first().cloned() {
            out.whole_also = names(&rest[1..]);
            out.whole = Some(first);
        }
    }
    if let (Some(tr), true) = (&ctx.trait_name, ctx.impl_items <= 2) {
        let bare = syn::ImplItemFn { attrs: Vec::new(), vis: syn::Visibility::Inherited, defaultness: None, sig: sig.clone(), block: body.clone() };
        let ms = try_all(Position::ImplFn, &cand_fn(&bare), |s| s.trait_name.as_deref() == Some(tr.as_str()));
        if let Some(first) = ms.first().cloned() {
            out.whole_also = names(&ms[1..]);
            out.whole = Some(first);
        }
    }
    out.method_frame = if ctx.in_impl { frame::of_method(sig, body) } else { None };
    if out.whole.is_none() {
        let mut w = Walker { src, steps: Vec::new(), unmatched: Vec::new() };
        w.block(body, true);
        out.steps = w.steps;
        out.unmatched = w.unmatched;
    }
    out
}

// ---------------------------------------------------------------------------
// the spec

/// Continuation lines of a statement taken out of the file lose the statement's own indentation (the renderer
/// puts the body's back), unless that changes a token (a string over several lines).
fn dedent(text: &str, indent: usize) -> String {
    let out: String = text
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                l.to_string()
            } else {
                let strip = l.chars().take(indent).take_while(|c| *c == ' ').count();
                l[strip..].to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let same = |a: &str, b: &str| match (syn::parse_str::<syn::Block>(&format!("{{{a}}}")), syn::parse_str::<syn::Block>(&format!("{{{b}}}"))) {
        (Ok(x), Ok(y)) => x.to_token_stream().to_string() == y.to_token_stream().to_string(),
        _ => a == b,
    };
    if same(text, &out) {
        out
    } else {
        text.to_string()
    }
}

fn column_of(src: &Source, at: usize) -> usize {
    let line_start = src.text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    src.text[line_start..at].chars().count()
}

/// The call a step match writes (as `change::render_step` writes it), without its `;` for a value.
pub fn call_text(m: &StepMatch, src: &Source) -> Result<String, String> {
    let mut p = piece_of(&m.matched, src);
    if m.bind.is_some() {
        p.bind = m.bind.clone();
    }
    let line = crate::change::render_step(&p)?;
    Ok(if m.tail { line.trim_end_matches(';').to_string() } else { line })
}

/// The longest swap spec name: its file (`v1.<name>.1o.change`, a member record `<name>.expansion.json`) stays under
/// 200 characters, well inside the file system's 255.
pub const SPEC_NAME_MAX: usize = 180;

/// A swap spec's name: `to-toolkit-<route>` with the route's stable digest at its end
/// ([`crate::reading::spec::route_name`]); a route too long for [`SPEC_NAME_MAX`] keeps its first words and the same
/// digest, so the name stays unique and stable, and the full route is the spec's target and objective.
pub fn spec_name_for(item: &str) -> String {
    let full = format!("to-toolkit-{}", crate::reading::spec::route_name(item));
    if full.len() <= SPEC_NAME_MAX {
        return full;
    }
    let (head, digest) = full.rsplit_once('-').unwrap_or((full.as_str(), ""));
    // route_name writes ASCII only: any byte is a boundary
    let keep = SPEC_NAME_MAX - digest.len() - 1;
    format!("{}-{digest}", head[..keep].trim_end_matches(['_', '-']))
}

/// The replace_body spec for a function: its exact matches swapped for the calls, the rest of the body as it is
/// (`let` and `value` steps, `raw` for any other statement and for comments between statements); `None` when
/// nothing matches exactly. `item` is the function's path (the spec's target).
pub fn spec_for(m: &FnMatch, src: &Source, item: &str, f: FnParts, near_too: bool) -> Option<ChangeSpec> {
    let FnParts { attrs, vis, sig, body } = f;
    let name = &sig.ident.to_string();
    let usable = |l: Level| l == Level::Exact || near_too;
    let base = ChangeSpec {
        name: spec_name_for(item),
        objective: format!("{item}: its body as simple_impl toolkit calls"),
        operation: Some(Operation::ReplaceBody),
        target: item.to_string(),
        // an exact swap compiles to what the body compiled to: the transform proves it by cargo expand
        same_expansion: !near_too,
        ..ChangeSpec::default()
    };
    // forward_trait_method writes a trait's method, with no visibility: a `pub fn` it would make private is left as
    // it is written
    let keeps_vis = |w: &&Matched| !(w.shape().piece == "forward_trait_method" && !matches!(vis, syn::Visibility::Inherited));
    if let Some(w) = m.whole.as_ref().filter(|w| usable(w.level) && w.shape().position == Position::Function).filter(keeps_vis) {
        let g = piece_of(w, src);
        let text_of = |n: &dyn Spanned| src.range(n.span()).map(|(a, b)| src.text[a..b].to_string()).unwrap_or_default();
        let mut fattrs: Vec<String> = Vec::new();
        for a in attrs {
            if w.shape().family == Family::Builder {
                continue;
            }
            if is_doc(a) {
                // a doc comment inside the call is the attribute it stands for
                let syn::Meta::NameValue(nv) = &a.meta else { continue };
                fattrs.push(format!("#[doc = {}]", nv.value.to_token_stream()));
            } else {
                fattrs.push(text_of(a));
            }
        }
        let mut params = Vec::new();
        let mut receiver = None;
        for a in &sig.inputs {
            match a {
                syn::FnArg::Receiver(r) => receiver = Some(text_of(r)),
                syn::FnArg::Typed(t) => params.push(Param { name: text_of(&t.pat), ty: text_of(&t.ty) }),
            }
        }
        if w.shape().piece == "with_extend" {
            for p in &mut params {
                p.ty = String::new();
            }
        }
        // checked_method writes `Result<$ok, $error_ty>` itself: its call takes the `Ok` type
        let ret = match (&sig.output, w.bindings.map.get("__SiOk")) {
            (_, Some(ok)) => Some(src.filling(ok)),
            (syn::ReturnType::Type(_, t), None) => Some(text_of(t)),
            (syn::ReturnType::Default, None) => None,
        };
        let f = FnSpec {
            name: name.to_string(),
            vis: (!matches!(vis, syn::Visibility::Inherited)).then(|| text_of(vis)),
            receiver,
            params,
            ret,
            is_const: sig.constness.is_some(),
            attrs: fattrs,
            generator: Some(if w.shape().family == Family::Builder { Piece { name: Some(name.to_string()), ..g } } else { g }),
            ..FnSpec::default()
        };
        return Some(ChangeSpec { function: Some(f), ..base });
    }
    let swapped: Vec<&StepMatch> = m.steps.iter().filter(|s| usable(s.matched.level) && !s.comment).collect();
    if swapped.is_empty() {
        return None;
    }
    let mut steps: Vec<Piece> = Vec::new();
    let raw = |code: String| Piece { piece: "raw".into(), code: Some(code), ..Piece::default() };
    let (open, close) = src.range(body.brace_token.span.join()).map(|(a, b)| (a + 1, b - 1)).unwrap_or((0, 0));
    let mut cursor = open;
    let last = body.stmts.len().saturating_sub(1);
    for (i, s) in body.stmts.iter().enumerate() {
        let (a, b) = src.range(s.span())?;
        // comments between statements stay, as their own raw step
        let gap = src.text[cursor..a].trim();
        if !gap.is_empty() {
            steps.push(raw(dedent(gap, 0)));
        }
        cursor = b;
        let indent = column_of(src, a);
        if let Some(sm) = swapped.iter().find(|x| x.top == Some(i)) {
            let mut p = piece_of(&sm.matched, src);
            if sm.bind.is_some() {
                p.bind = sm.bind.clone();
            }
            let d = crate::change::piece(&p.piece)?;
            if sm.tail && d.family != Family::Expression {
                steps.push(Piece { piece: "value".into(), value: Some(call_text(sm, src).ok()?), ..Piece::default() });
            } else {
                steps.push(p);
            }
            continue;
        }
        // the statement as written, its inner matches swapped
        let mut text = src.text[a..b].to_string();
        let mut inner: Vec<&&StepMatch> = swapped.iter().filter(|x| x.top.is_none() && x.range.0 >= a && x.range.1 <= b).collect();
        inner.sort_by_key(|x| std::cmp::Reverse(x.range.0));
        for x in inner {
            let call = call_text(x, src).ok()?;
            text.replace_range(x.range.0 - a..x.range.1 - a, &call);
        }
        let text = dedent(&text, indent);
        match s {
            syn::Stmt::Local(l) if l.attrs.is_empty() && l.init.as_ref().is_some_and(|i| i.diverge.is_none()) && !text.contains("//") => {
                let (pa, pb) = src.range(l.pat.span())?;
                let bind = src.text[pa..pb].to_string();
                let eq = text.find('=')?;
                let value = text[eq + 1..].trim().trim_end_matches(';').trim_end().to_string();
                steps.push(Piece { piece: "let".into(), bind: Some(bind), value: Some(value), ..Piece::default() });
            }
            syn::Stmt::Expr(_, None) if i == last => steps.push(Piece { piece: "value".into(), value: Some(text), ..Piece::default() }),
            _ => steps.push(raw(text)),
        }
    }
    let trailing = src.text.get(cursor..close).unwrap_or("").trim();
    if !trailing.is_empty() {
        let gives_value = |p: &Piece| p.piece == "value" || (p.bind.is_none() && crate::change::piece(&p.piece).is_some_and(|d| d.family == Family::Expression));
        match steps.last_mut() {
            Some(last) if gives_value(last) => {
                let v = if last.piece == "value" { last.value.take().unwrap_or_default() } else { crate::change::render_step(last).ok()? };
                *last = Piece { piece: "value".into(), value: Some(format!("{v}\n{trailing}")), ..Piece::default() };
            }
            _ => steps.push(raw(dedent(trailing, 0))),
        }
    }
    Some(ChangeSpec { function: Some(FnSpec { name: name.to_string(), body: steps, ..FnSpec::default() }), ..base })
}

// ---------------------------------------------------------------------------
// the functions of a file

type EachFn<'f> = dyn FnMut(proc_macro2::Span, FnParts, &Context) + 'f;

struct Each<'f, 'g> {
    ctx: Vec<Context>,
    f: &'g mut EachFn<'f>,
}

impl<'ast, 'f, 'g> Visit<'ast> for Each<'f, 'g> {
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let trait_name = i.trait_.as_ref().and_then(|(_, p, _)| p.segments.last()).map(|s| s.ident.to_string());
        self.ctx.push(Context { trait_name, self_ty: Some(i.self_ty.to_token_stream().to_string()), impl_items: i.items.len(), in_impl: true });
        syn::visit::visit_item_impl(self, i);
        self.ctx.pop();
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        let ctx = self.ctx.last().cloned().unwrap_or_default();
        (self.f)(f.span(), FnParts { attrs: &f.attrs, vis: &f.vis, sig: &f.sig, body: &f.block }, &ctx);
        // a function inside a body is its own, in no impl
        self.ctx.push(Context::default());
        syn::visit::visit_impl_item_fn(self, f);
        self.ctx.pop();
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        (self.f)(f.span(), FnParts { attrs: &f.attrs, vis: &f.vis, sig: &f.sig, body: &f.block }, &Context::default());
        self.ctx.push(Context::default());
        syn::visit::visit_item_fn(self, f);
        self.ctx.pop();
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        if let Some(b) = &f.default {
            (self.f)(f.span(), FnParts { attrs: &f.attrs, vis: &syn::Visibility::Inherited, sig: &f.sig, body: b }, &Context::default());
        }
        syn::visit::visit_trait_item_fn(self, f);
    }
}

/// Every function of a file with what it sits in: its place (its first token, attributes included: where klepto
/// records a function), attributes, visibility, signature, body and impl.
pub fn each_fn<'f>(file: &syn::File, mut f: impl FnMut(proc_macro2::Span, FnParts, &Context) + 'f) {
    let mut e = Each { ctx: Vec::new(), f: &mut f };
    e.visit_file(file);
}

/// One function of a file matched, with its spec: the exact matches swapped (`spec`), or, when it has only near
/// ones, the spec that would swap those (`near_spec`, which a transform refuses).
#[derive(Debug, Clone)]
pub struct FnAt {
    pub name: String,
    pub matched: FnMatch,
    pub spec: Option<ChangeSpec>,
    pub near_spec: Option<ChangeSpec>,
}

/// The function of `text` at klepto's place (`line`, `column`), matched; `item` is its path (the spec's target).
pub fn function_at(text: &str, line: u32, column: u32, item: &str) -> Option<FnAt> {
    let file = syn::parse_file(text).ok()?;
    let src = Source::new(text);
    let mut out = None;
    each_fn(&file, |at, parts, ctx| {
        let s = at.start();
        if out.is_some() || (s.line as u32, s.column as u32) != (line, column) {
            return;
        }
        let m = match_fn(&src, parts, ctx);
        let name = parts.sig.ident.to_string();
        let spec = spec_for(&m, &src, item, parts, false);
        let near_spec = spec_for(&m, &src, item, parts, true);
        out = Some(FnAt { name, matched: m, spec, near_spec });
    });
    out
}

#[cfg(test)]
mod spec_name_tests {
    use super::*;

    #[test]
    fn a_long_route_names_its_spec_under_200_characters_with_its_digest() {
        let short = "access_vocabulary::Capability::wire_name";
        assert_eq!(spec_name_for(short), format!("to-toolkit-{}", crate::reading::spec::route_name(short)), "a short route's name is as before");
        let long = format!("nodeplan_private_verification_protocol::{}::verify", "PrivateVerificationProtocolRecordWithAVeryLongName".repeat(6));
        let name = spec_name_for(&long);
        assert!(name.len() <= SPEC_NAME_MAX && crate::oneo::Doc::Change.file_name(&name).len() < 200, "{name}");
        assert!(name.ends_with(&crate::reading::spec::route_name(&long)[crate::reading::spec::route_name(&long).len() - 8..]), "{name}");
        assert_eq!(name, spec_name_for(&long), "stable");
        assert_ne!(name, spec_name_for(&format!("{long}2")), "another route, another name");
    }
}

