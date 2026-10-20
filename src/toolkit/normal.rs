// Copyright © 2026 1o. All rights reserved.

//! The normal form a body and a toolkit expansion are compared in.
//!
//! [`Level::Exact`] changes nothing rustc compiles: parentheses around an atomic expression (a path, a literal, a
//! call, a method call, a field, an index, a macro, a tuple, an array, a `?`) dropped, as are the invisible groups
//! a macro argument sits in; the prelude's names written short (`::core::result::Result::Err` is `Err`,
//! `::core::option::Option<T>` is `Option<T>`); a block statement holding only plain statements opened into the
//! block around it; the `;` after an `if` without `else`, a `for` or a `while` dropped; every match arm whose body
//! is not a block ending in a comma, every block arm without one; an empty statement (`;;`) and a where clause's
//! trailing comma gone; a type's invisible group or parentheses dropped.
//!
//! [`Level::Near`] adds what reads the same but is not the same tokens, each named when it is what made two forms
//! equal: a `return`, `break` or `continue` at the end of a block written without its `;`; a match arm whose body
//! is a block holding one `return`, `break` or `continue`; a negated comparison (`!(a == b)`) written as the
//! comparison it negates (`a != b`).

use super::Position;
use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use syn::parse::Parser;
use syn::visit_mut::VisitMut;

/// How loose the normal form is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Exact,
    Near,
}

/// A node in the normal form: its tokens, their text, and the near rules that changed it.
#[derive(Debug, Clone)]
pub struct Normal {
    pub tokens: TokenStream,
    pub text: String,
    pub fired: Vec<&'static str>,
}

pub const RULE_SEMI: &str = "a `return`, `break` or `continue` written without its `;`";
pub const RULE_ARM_BLOCK: &str = "a match arm's `return`, `break` or `continue` written inside a block";
pub const RULE_NEGATION: &str = "a comparison written as the comparison it negates (`a != b` for the macro's `!(a == b)`)";

/// The prelude's names as they are written in full, and their short name.
const PRELUDE: &[(&[&str], &str)] = &[
    (&["option", "Option", "Some"], "Some"),
    (&["option", "Option", "None"], "None"),
    (&["result", "Result", "Ok"], "Ok"),
    (&["result", "Result", "Err"], "Err"),
    (&["Option", "Some"], "Some"),
    (&["Option", "None"], "None"),
    (&["Result", "Ok"], "Ok"),
    (&["Result", "Err"], "Err"),
    (&["option", "Option"], "Option"),
    (&["result", "Result"], "Result"),
    (&["convert", "Into"], "Into"),
    (&["convert", "From"], "From"),
    (&["convert", "AsRef"], "AsRef"),
    (&["convert", "AsMut"], "AsMut"),
    (&["convert", "TryFrom"], "TryFrom"),
    (&["convert", "TryInto"], "TryInto"),
    (&["iter", "IntoIterator"], "IntoIterator"),
    (&["iter", "Iterator"], "Iterator"),
    (&["iter", "Extend"], "Extend"),
    (&["default", "Default"], "Default"),
    (&["clone", "Clone"], "Clone"),
    (&["cmp", "PartialEq"], "PartialEq"),
    (&["cmp", "PartialOrd"], "PartialOrd"),
    (&["cmp", "Eq"], "Eq"),
    (&["cmp", "Ord"], "Ord"),
    (&["string", "String"], "String"),
    (&["string", "ToString"], "ToString"),
    (&["vec", "Vec"], "Vec"),
    (&["boxed", "Box"], "Box"),
    (&["matches"], "matches"),
];

const ROOTS: &[&str] = &["core", "std", "alloc"];

/// The short name of a path written in full, if it is one of the prelude's.
fn prelude_name(idents: &[String], leading: bool) -> Option<&'static str> {
    let rest: &[String] = match idents.first() {
        Some(r) if ROOTS.contains(&r.as_str()) => &idents[1..],
        _ if leading => return None,
        _ => idents,
    };
    let rooted = rest.len() < idents.len();
    PRELUDE.iter().find_map(|(full, short)| {
        let names: Vec<&str> = rest.iter().map(String::as_str).collect();
        // `Option::Some` reads short without a root; `option::Option` only with one
        let needs_root = full[0].chars().next().is_some_and(char::is_lowercase);
        (names == *full && (rooted || !needs_root)).then_some(*short)
    })
}

fn is_atomic(e: &syn::Expr) -> bool {
    use syn::Expr::*;
    matches!(e, Path(_) | Lit(_) | Field(_) | MethodCall(_) | Call(_) | Index(_) | Macro(_) | Paren(_) | Tuple(_) | Array(_) | Try(_) | Await(_) | Group(_))
}

fn diverges(e: &syn::Expr) -> bool {
    matches!(e, syn::Expr::Return(_) | syn::Expr::Break(_) | syn::Expr::Continue(_))
}

/// Statements that open into the block around them: no `let`, no item.
fn plain(stmts: &[syn::Stmt]) -> bool {
    !stmts.is_empty() && stmts.iter().all(|s| matches!(s, syn::Stmt::Expr(..) | syn::Stmt::Macro(_)))
}

fn negated(op: &syn::BinOp) -> Option<syn::BinOp> {
    use syn::BinOp::*;
    Some(match op {
        Eq(t) => Ne(syn::Token![!=](t.spans[0])),
        Ne(t) => Eq(syn::Token![==](t.spans[0])),
        Lt(t) => Ge(syn::Token![>=](t.spans[0])),
        Gt(t) => Le(syn::Token![<=](t.spans[0])),
        Le(t) => Gt(syn::Token![>](t.spans[0])),
        Ge(t) => Lt(syn::Token![<](t.spans[0])),
        _ => return None,
    })
}

struct Norm {
    level: Level,
    fired: Vec<&'static str>,
}

impl Norm {
    fn fire(&mut self, rule: &'static str) {
        if !self.fired.contains(&rule) {
            self.fired.push(rule);
        }
    }
}

impl VisitMut for Norm {
    fn visit_path_mut(&mut self, p: &mut syn::Path) {
        syn::visit_mut::visit_path_mut(self, p);
        if p.segments.len() < 2 && p.leading_colon.is_none() {
            return;
        }
        // a path whose segments before the last carry generic arguments is not one of these
        if p.segments.iter().rev().skip(1).any(|s| !s.arguments.is_none()) {
            return;
        }
        let idents: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
        let Some(short) = prelude_name(&idents, p.leading_colon.is_some()) else { return };
        let first = p.leading_colon.map(|c| c.spans[0]).unwrap_or_else(|| p.segments[0].ident.span());
        let last = p.segments.last().expect("two segments").clone();
        let span = first.join(last.ident.span()).unwrap_or(last.ident.span());
        let mut seg = last;
        seg.ident = syn::Ident::new(short, span);
        p.leading_colon = None;
        p.segments = std::iter::once(seg).collect();
    }

    fn visit_expr_mut(&mut self, e: &mut syn::Expr) {
        syn::visit_mut::visit_expr_mut(self, e);
        loop {
            match e {
                syn::Expr::Group(g) => *e = (*g.expr).clone(),
                syn::Expr::Paren(p) if p.attrs.is_empty() && is_atomic(&p.expr) => *e = (*p.expr).clone(),
                _ => break,
            }
        }
        if self.level == Level::Near {
            if let syn::Expr::Unary(u) = e {
                if let (syn::UnOp::Not(_), syn::Expr::Paren(p)) = (&u.op, &*u.expr) {
                    if let syn::Expr::Binary(b) = &*p.expr {
                        if let Some(op) = negated(&b.op) {
                            let mut b = b.clone();
                            b.op = op;
                            *e = syn::Expr::Binary(b);
                            self.fire(RULE_NEGATION);
                        }
                    }
                }
            }
        }
    }

    fn visit_where_clause_mut(&mut self, w: &mut syn::WhereClause) {
        syn::visit_mut::visit_where_clause_mut(self, w);
        // a trailing comma after the last predicate
        if let Some(last) = w.predicates.pop() {
            w.predicates.push(last.into_value());
        }
    }

    fn visit_type_mut(&mut self, t: &mut syn::Type) {
        syn::visit_mut::visit_type_mut(self, t);
        loop {
            match t {
                syn::Type::Group(g) => *t = (*g.elem).clone(),
                syn::Type::Paren(p) => *t = (*p.elem).clone(),
                _ => break,
            }
        }
    }

    fn visit_arm_mut(&mut self, a: &mut syn::Arm) {
        syn::visit_mut::visit_arm_mut(self, a);
        if self.level == Level::Near {
            if let syn::Expr::Block(b) = &*a.body {
                if b.attrs.is_empty() && b.label.is_none() && b.block.stmts.len() == 1 {
                    if let syn::Stmt::Expr(inner, _) = &b.block.stmts[0] {
                        if diverges(inner) {
                            a.body = Box::new(inner.clone());
                            self.fire(RULE_ARM_BLOCK);
                        }
                    }
                }
            }
        }
        a.comma = match &*a.body {
            syn::Expr::Block(_) => None,
            _ => Some(syn::Token![,](Span::call_site())),
        };
    }

    fn visit_block_mut(&mut self, b: &mut syn::Block) {
        syn::visit_mut::visit_block_mut(self, b);
        let mut out: Vec<syn::Stmt> = Vec::with_capacity(b.stmts.len());
        for s in std::mem::take(&mut b.stmts) {
            match s {
                // an empty statement (`;;`): nothing
                syn::Stmt::Expr(syn::Expr::Verbatim(ts), _) if ts.is_empty() => {}
                syn::Stmt::Expr(syn::Expr::Block(eb), _) if eb.attrs.is_empty() && eb.label.is_none() && plain(&eb.block.stmts) => out.extend(eb.block.stmts),
                syn::Stmt::Expr(e @ (syn::Expr::If(_) | syn::Expr::ForLoop(_) | syn::Expr::While(_)), Some(_)) if !matches!(&e, syn::Expr::If(i) if i.else_branch.is_some()) => {
                    out.push(syn::Stmt::Expr(e, None))
                }
                other => out.push(other),
            }
        }
        if self.level == Level::Near {
            if let Some(syn::Stmt::Expr(e, semi @ None)) = out.last_mut() {
                if diverges(e) {
                    *semi = Some(syn::Token![;](Span::call_site()));
                    self.fire(RULE_SEMI);
                }
            }
        }
        b.stmts = out;
    }
}

/// Statements in the normal form (a block's, opened as the normal form opens them).
pub fn normal_stmts(stmts: &[syn::Stmt], level: Level) -> Normal {
    let mut b = syn::Block { brace_token: Default::default(), stmts: stmts.to_vec() };
    let mut n = Norm { level, fired: Vec::new() };
    n.visit_block_mut(&mut b);
    let mut tokens = TokenStream::new();
    for s in &b.stmts {
        s.to_tokens(&mut tokens);
    }
    Normal { text: tokens.to_string(), tokens, fired: n.fired }
}

/// An expression in the normal form.
pub fn normal_expr(e: &syn::Expr, level: Level) -> Normal {
    let mut e = e.clone();
    let mut n = Norm { level, fired: Vec::new() };
    n.visit_expr_mut(&mut e);
    let tokens = e.to_token_stream();
    Normal { text: tokens.to_string(), tokens, fired: n.fired }
}

/// A function in the normal form.
pub fn normal_fn(f: &syn::ImplItemFn, level: Level) -> Normal {
    let mut f = f.clone();
    let mut n = Norm { level, fired: Vec::new() };
    n.visit_impl_item_fn_mut(&mut f);
    let tokens = f.to_token_stream();
    Normal { text: tokens.to_string(), tokens, fired: n.fired }
}

/// Tokens of a position (statements, an expression, a function), parsed and put in the normal form.
pub fn normal_tokens(ts: TokenStream, position: Position, level: Level) -> Result<Normal, String> {
    match position {
        Position::Step => {
            let stmts = syn::Block::parse_within.parse2(ts).map_err(|e| format!("not statements: {e}"))?;
            Ok(normal_stmts(&stmts, level))
        }
        Position::Value => {
            let e: syn::Expr = syn::parse2(ts).map_err(|e| format!("not an expression: {e}"))?;
            Ok(normal_expr(&e, level))
        }
        Position::Function | Position::ImplFn => {
            let f: syn::ImplItemFn = syn::parse2(ts).map_err(|e| format!("not a function: {e}"))?;
            Ok(normal_fn(&f, level))
        }
    }
}

// ---------------------------------------------------------------------------
// the names a function binds, written by where they are bound

/// Every name a function binds (its parameters, a `let`, a match arm, a closure's parameters, a `for`, an
/// `if let`) renamed `__b<n>` in the order they are bound, scope by scope, and every use of it with it: two
/// functions that differ only in what their locals are called read the same. A macro's own names
/// (`Some(value) => value` in `require_some!`) are hygienic, so its expansion and the hand-written body it
/// replaces (`Some(port) => port`) differ only so. A name starting with a capital (a unit variant, a constant) is
/// left as it is; a name inside a macro's tokens is renamed where it is a token of its own (not inside a string).
struct Alpha {
    scopes: Vec<std::collections::HashMap<String, String>>,
    n: usize,
}

impl Alpha {
    fn bind(&mut self, i: &mut syn::Ident) {
        let name = i.to_string();
        if name.chars().next().is_some_and(char::is_uppercase) || name == "self" {
            return;
        }
        let c = format!("__b{}", self.n);
        self.n += 1;
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name, c.clone());
        }
        *i = syn::Ident::new(&c, i.span());
    }

    fn lookup(&self, name: &str) -> Option<String> {
        self.scopes.iter().rev().find_map(|s| s.get(name).cloned())
    }

    fn scoped(&mut self, f: impl FnOnce(&mut Self)) {
        self.scopes.push(Default::default());
        f(self);
        self.scopes.pop();
    }

    fn rename_tokens(&self, ts: TokenStream) -> TokenStream {
        ts.into_iter()
            .map(|t| match t {
                proc_macro2::TokenTree::Ident(i) => match self.lookup(&i.to_string()) {
                    Some(c) => proc_macro2::TokenTree::Ident(proc_macro2::Ident::new(&c, i.span())),
                    None => proc_macro2::TokenTree::Ident(i),
                },
                proc_macro2::TokenTree::Group(g) => {
                    let mut ng = proc_macro2::Group::new(g.delimiter(), self.rename_tokens(g.stream()));
                    ng.set_span(g.span());
                    proc_macro2::TokenTree::Group(ng)
                }
                other => other,
            })
            .collect()
    }
}

impl VisitMut for Alpha {
    fn visit_pat_ident_mut(&mut self, p: &mut syn::PatIdent) {
        if let Some((_, sub)) = &mut p.subpat {
            self.visit_pat_mut(sub);
        }
        self.bind(&mut p.ident);
    }

    fn visit_expr_path_mut(&mut self, e: &mut syn::ExprPath) {
        if e.qself.is_none() && e.path.segments.len() == 1 && e.path.leading_colon.is_none() {
            let seg = &mut e.path.segments[0];
            if let Some(c) = self.lookup(&seg.ident.to_string()) {
                seg.ident = syn::Ident::new(&c, seg.ident.span());
                return;
            }
        }
        syn::visit_mut::visit_expr_path_mut(self, e);
    }

    fn visit_macro_mut(&mut self, m: &mut syn::Macro) {
        m.tokens = self.rename_tokens(std::mem::take(&mut m.tokens));
    }

    fn visit_block_mut(&mut self, b: &mut syn::Block) {
        self.scoped(|s| syn::visit_mut::visit_block_mut(s, b));
    }

    fn visit_local_mut(&mut self, l: &mut syn::Local) {
        // what a `let` binds is not seen by its own value
        if let Some(init) = &mut l.init {
            self.visit_expr_mut(&mut init.expr);
            if let Some((_, d)) = &mut init.diverge {
                self.visit_expr_mut(d);
            }
        }
        self.visit_pat_mut(&mut l.pat);
    }

    fn visit_arm_mut(&mut self, a: &mut syn::Arm) {
        self.scoped(|s| {
            s.visit_pat_mut(&mut a.pat);
            if let Some((_, g)) = &mut a.guard {
                s.visit_expr_mut(g);
            }
            s.visit_expr_mut(&mut a.body);
        });
    }

    fn visit_expr_closure_mut(&mut self, c: &mut syn::ExprClosure) {
        self.scoped(|s| {
            for p in c.inputs.iter_mut() {
                s.visit_pat_mut(p);
            }
            s.visit_expr_mut(&mut c.body);
        });
    }

    fn visit_expr_for_loop_mut(&mut self, f: &mut syn::ExprForLoop) {
        self.visit_expr_mut(&mut f.expr);
        self.scoped(|s| {
            s.visit_pat_mut(&mut f.pat);
            s.visit_block_mut(&mut f.body);
        });
    }

    fn visit_expr_if_mut(&mut self, i: &mut syn::ExprIf) {
        if let syn::Expr::Let(l) = &mut *i.cond {
            self.visit_expr_mut(&mut l.expr);
            self.scoped(|s| {
                s.visit_pat_mut(&mut l.pat);
                s.visit_block_mut(&mut i.then_branch);
            });
        } else {
            self.visit_expr_mut(&mut i.cond);
            self.visit_block_mut(&mut i.then_branch);
        }
        if let Some((_, e)) = &mut i.else_branch {
            self.visit_expr_mut(e);
        }
    }

    fn visit_expr_while_mut(&mut self, w: &mut syn::ExprWhile) {
        if let syn::Expr::Let(l) = &mut *w.cond {
            self.visit_expr_mut(&mut l.expr);
            self.scoped(|s| {
                s.visit_pat_mut(&mut l.pat);
                s.visit_block_mut(&mut w.body);
            });
        } else {
            self.visit_expr_mut(&mut w.cond);
            self.visit_block_mut(&mut w.body);
        }
    }

    fn visit_impl_item_fn_mut(&mut self, f: &mut syn::ImplItemFn) {
        self.scoped(|s| {
            for a in f.sig.inputs.iter_mut() {
                if let syn::FnArg::Typed(t) = a {
                    s.visit_pat_mut(&mut t.pat);
                }
            }
            s.visit_block_mut(&mut f.block);
        });
    }
}

/// A function in the form the to-toolkit proof compares: its bindings renamed by where they are bound, then the
/// exact normal form.
pub fn proof_form(f: &syn::ImplItemFn) -> String {
    let mut f = f.clone();
    let mut a = Alpha { scopes: Vec::new(), n: 0 };
    a.visit_impl_item_fn_mut(&mut f);
    normal_fn(&f, Level::Exact).text
}
