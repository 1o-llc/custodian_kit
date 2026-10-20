// Copyright © 2026 1o. All rights reserved.

//! Frames: the shape a statement or a small method has once what a macro would take as arguments is taken out,
//! for finding the shapes that repeat and that no toolkit macro writes.
//!
//! klepto's shapes (`klepto::patterns`) are whole functions, signature and body, at five levels that replace
//! locals, literals, types and callees one token at a time; field names stay. Two `ensure`-like statements with
//! different conditions are different shapes there, since the condition is many tokens. A frame keeps a
//! statement's control (`if`, `match`, `let`, `return`, `for`, `?`), the prelude's constructors (`Some`, `Err`,
//! ...), the methods called at its top (`.push(..)`, `.insert(..)`), `self`, `true`/`false`, and makes every
//! operand one hole: a condition, a scrutinee, what is returned, pushed or assigned. A small method's frame keeps
//! its receiver, its parameter count, the shape of its return type, and its body's chain of fields and methods on
//! `self` (`self.$f.as_str()`), each field a hole.

use quote::ToTokens;

/// What a hole stands for, for naming a proposed macro's arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Condition,
    Option,
    Result,
    Value,
    Error,
    ReturnValue,
    Collection,
    MapError,
    Place,
    Iter,
    Args,
    Field,
    Arg,
    Ty,
    Expr,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Condition => "condition",
            Role::Option => "option",
            Role::Result => "result",
            Role::Value => "value",
            Role::Error => "error",
            Role::ReturnValue => "return_value",
            Role::Collection => "collection",
            Role::MapError => "map_error",
            Role::Place => "place",
            Role::Iter => "iter",
            Role::Args => "args",
            Role::Field => "field",
            Role::Arg => "arg",
            Role::Ty => "ty",
            Role::Expr => "expr",
        }
    }
}

/// A frame: its text (holes written `$`, `$b` for a name it binds, `$f` a field), its holes' roles in order, and how
/// many of its tokens are not holes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Frame {
    pub text: String,
    pub roles: Vec<Role>,
    pub fixed: usize,
    /// Whether it holds control (`if`, `match`, `for`, `while`, `loop`, `return`, `break`, `continue`, `?`) or is a
    /// whole method: what can be a macro of its own.
    pub structural: bool,
}

#[derive(Default)]
struct W {
    out: Vec<String>,
    roles: Vec<Role>,
    fixed: usize,
    control: bool,
}

impl W {
    fn t(&mut self, s: &str) {
        self.out.push(s.to_string());
        self.fixed += 1;
    }
    fn hole(&mut self, r: Role) {
        self.out.push("$".into());
        self.roles.push(r);
    }
    fn binder(&mut self) {
        self.out.push("$b".into());
    }
    fn ctl(&mut self, s: &str) {
        self.control = true;
        self.t(s);
    }
}

const PRELUDE_CTORS: &[&str] = &["Some", "Ok", "Err"];

fn path_name(e: &syn::Expr) -> Option<String> {
    match e {
        syn::Expr::Path(p) if p.qself.is_none() => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

fn is_self(e: &syn::Expr) -> bool {
    matches!(e, syn::Expr::Path(p) if p.path.is_ident("self"))
}

/// An operand: a hole, unless it is control itself, a prelude constructor, `None`, `self`, `true`/`false`, `()`,
/// a reference to an operand, or `x.m()` with no arguments.
fn operand(w: &mut W, e: &syn::Expr, role: Role) {
    match e {
        syn::Expr::Call(c) if path_name(&c.func).is_some_and(|n| PRELUDE_CTORS.contains(&n.as_str())) && c.args.len() == 1 => {
            let n = path_name(&c.func).unwrap_or_default();
            w.t(&n);
            w.t("(");
            let inner = match n.as_str() {
                "Err" => Role::Error,
                _ => Role::Value,
            };
            operand(w, &c.args[0], inner);
            w.t(")");
        }
        syn::Expr::Path(p) if p.path.is_ident("None") => w.t("None"),
        _ if is_self(e) => w.t("self"),
        syn::Expr::Lit(l) if matches!(l.lit, syn::Lit::Bool(_)) => w.t(&l.lit.to_token_stream().to_string()),
        syn::Expr::Tuple(t) if t.elems.is_empty() => w.t("()"),
        syn::Expr::Reference(r) => {
            w.t(if r.mutability.is_some() { "&mut" } else { "&" });
            operand(w, &r.expr, role);
        }
        syn::Expr::MethodCall(m) if m.args.is_empty() && m.turbofish.is_none() => {
            operand(w, &m.receiver, role);
            w.t(".");
            w.t(&m.method.to_string());
            w.t("()");
        }
        syn::Expr::Try(t) => {
            operand(w, &t.expr, role);
            w.ctl("?");
        }
        syn::Expr::Return(_) | syn::Expr::Break(_) | syn::Expr::Continue(_) | syn::Expr::Block(_) | syn::Expr::If(_) | syn::Expr::Match(_) => expr(w, e),
        _ => w.hole(role),
    }
}

fn pat(w: &mut W, p: &syn::Pat) {
    match p {
        syn::Pat::Ident(i) if i.subpat.is_none() => {
            if i.by_ref.is_some() {
                w.t("ref");
            }
            if i.mutability.is_some() {
                w.t("mut");
            }
            w.binder();
        }
        syn::Pat::Wild(_) => w.t("_"),
        syn::Pat::TupleStruct(ts) if ts.path.segments.last().is_some_and(|s| PRELUDE_CTORS.contains(&s.ident.to_string().as_str())) => {
            w.t(&ts.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default());
            w.t("(");
            for (i, e) in ts.elems.iter().enumerate() {
                if i > 0 {
                    w.t(",");
                }
                pat(w, e);
            }
            w.t(")");
        }
        syn::Pat::Path(pp) if pp.path.is_ident("None") => w.t("None"),
        syn::Pat::Reference(r) => {
            w.t("&");
            pat(w, &r.pat);
        }
        syn::Pat::Type(t) => {
            pat(w, &t.pat);
            w.t(":");
            w.hole(Role::Ty);
        }
        _ => w.out.push("$p".into()),
    }
}

fn block(w: &mut W, b: &syn::Block) {
    w.t("{");
    for s in &b.stmts {
        stmt_into(w, s);
    }
    w.t("}");
}

fn cond(w: &mut W, c: &syn::Expr) {
    match c {
        syn::Expr::Let(l) => {
            w.t("let");
            pat(w, &l.pat);
            w.t("=");
            operand(w, &l.expr, Role::Option);
        }
        syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Not(_)) => {
            w.t("!");
            w.hole(Role::Condition);
        }
        _ => w.hole(Role::Condition),
    }
}

/// An expression standing as a statement (or an arm's body).
fn expr(w: &mut W, e: &syn::Expr) {
    match e {
        syn::Expr::If(i) => {
            w.ctl("if");
            cond(w, &i.cond);
            block(w, &i.then_branch);
            if let Some((_, els)) = &i.else_branch {
                w.t("else");
                expr(w, els);
            }
        }
        syn::Expr::Match(m) => {
            w.ctl("match");
            let ctors: Vec<String> = m.arms.iter().filter_map(|a| match &a.pat {
                syn::Pat::TupleStruct(ts) => ts.path.segments.last().map(|s| s.ident.to_string()),
                syn::Pat::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
                _ => None,
            }).collect();
            let role = if ctors.iter().any(|c| c == "Some" || c == "None") {
                Role::Option
            } else if ctors.iter().any(|c| c == "Ok" || c == "Err") {
                Role::Result
            } else {
                Role::Value
            };
            operand(w, &m.expr, role);
            w.t("{");
            for a in &m.arms {
                pat(w, &a.pat);
                if a.guard.is_some() {
                    w.t("if");
                    w.hole(Role::Condition);
                }
                w.t("=>");
                arm_body(w, &a.body);
                w.t(",");
            }
            w.t("}");
        }
        syn::Expr::ForLoop(f) => {
            w.ctl("for");
            pat(w, &f.pat);
            w.t("in");
            operand(w, &f.expr, Role::Iter);
            block(w, &f.body);
        }
        syn::Expr::While(x) => {
            w.ctl("while");
            cond(w, &x.cond);
            block(w, &x.body);
        }
        syn::Expr::Loop(l) => {
            w.ctl("loop");
            block(w, &l.body);
        }
        syn::Expr::Return(r) => {
            w.ctl("return");
            if let Some(x) = &r.expr {
                operand(w, x, Role::ReturnValue);
            }
        }
        syn::Expr::Break(b) => {
            w.ctl("break");
            if let Some(x) = &b.expr {
                operand(w, x, Role::Value);
            }
        }
        syn::Expr::Continue(_) => w.ctl("continue"),
        syn::Expr::Block(b) => block(w, &b.block),
        syn::Expr::Assign(a) => {
            place(w, &a.left);
            w.t("=");
            operand(w, &a.right, Role::Value);
        }
        syn::Expr::Binary(b) if is_compound_assign(&b.op) => {
            place(w, &b.left);
            w.t(&b.op.to_token_stream().to_string());
            operand(w, &b.right, Role::Value);
        }
        syn::Expr::MethodCall(m) => {
            let method = m.method.to_string();
            let (recv, arg) = method_roles(&method);
            operand(w, &m.receiver, recv);
            w.t(".");
            w.t(&method);
            w.t("(");
            for (i, a) in m.args.iter().enumerate() {
                if i > 0 {
                    w.t(",");
                }
                operand(w, a, arg);
            }
            w.t(")");
        }
        syn::Expr::Try(t) => {
            expr(w, &t.expr);
            w.ctl("?");
        }
        _ => operand(w, e, Role::Expr),
    }
}

/// What a method's receiver and arguments stand for, by the method.
fn method_roles(method: &str) -> (Role, Role) {
    match method {
        "push" | "push_str" | "extend" | "insert" | "remove" | "retain" | "entry" | "append" | "truncate" => (Role::Collection, Role::Value),
        "map_err" => (Role::Result, Role::MapError),
        "ok_or" | "ok_or_else" => (Role::Option, Role::Error),
        "unwrap_or" | "unwrap_or_else" | "unwrap_or_default" | "expect" => (Role::Option, Role::Value),
        _ => (Role::Expr, Role::Value),
    }
}

fn is_compound_assign(op: &syn::BinOp) -> bool {
    use syn::BinOp::*;
    matches!(op, AddAssign(_) | SubAssign(_) | MulAssign(_) | DivAssign(_) | RemAssign(_) | BitXorAssign(_) | BitAndAssign(_) | BitOrAssign(_) | ShlAssign(_) | ShrAssign(_))
}

fn place(w: &mut W, e: &syn::Expr) {
    match e {
        syn::Expr::Field(f) if is_self(&f.base) => {
            w.t("self");
            w.t(".");
            w.out.push("$f".into());
            w.roles.push(Role::Field);
        }
        syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => {
            w.t("*");
            w.hole(Role::Place);
        }
        _ => w.hole(Role::Place),
    }
}

fn arm_body(w: &mut W, e: &syn::Expr) {
    match e {
        syn::Expr::Path(_) | syn::Expr::Lit(_) | syn::Expr::Call(_) | syn::Expr::Return(_) | syn::Expr::Break(_) | syn::Expr::Continue(_) | syn::Expr::Block(_) | syn::Expr::Tuple(_) => expr(w, e),
        _ => w.hole(Role::Value),
    }
}

fn stmt_into(w: &mut W, s: &syn::Stmt) {
    match s {
        syn::Stmt::Local(l) => {
            w.t("let");
            pat(w, &l.pat);
            if let Some(init) = &l.init {
                w.t("=");
                match &*init.expr {
                    syn::Expr::Match(_) | syn::Expr::If(_) | syn::Expr::Try(_) => expr(w, &init.expr),
                    other => operand(w, other, Role::Value),
                }
                if let Some((_, els)) = &init.diverge {
                    w.t("else");
                    expr(w, els);
                }
            }
            w.t(";");
        }
        syn::Stmt::Expr(e, semi) => {
            expr(w, e);
            if semi.is_some() {
                w.t(";");
            }
        }
        syn::Stmt::Macro(m) => {
            w.t(&m.mac.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default());
            w.t("!(..)");
            if m.semi_token.is_some() {
                w.t(";");
            }
        }
        syn::Stmt::Item(_) => w.out.push("$item".into()),
    }
}

fn done(w: W) -> Frame {
    Frame { text: w.out.join(" "), roles: w.roles, fixed: w.fixed, structural: w.control }
}

/// A statement's frame.
pub fn of_stmt(s: &syn::Stmt) -> Frame {
    let mut w = W::default();
    stmt_into(&mut w, s);
    done(w)
}

/// A type's frame: references, slices, `Option`/`Result`/`Vec`/`Box` and the primitive names kept, any other type
/// a hole.
fn ty(w: &mut W, t: &syn::Type) {
    match t {
        syn::Type::Reference(r) => {
            w.t(if r.mutability.is_some() { "&mut" } else { "&" });
            ty(w, &r.elem);
        }
        syn::Type::Slice(s) => {
            w.t("[");
            ty(w, &s.elem);
            w.t("]");
        }
        syn::Type::Tuple(t) if t.elems.is_empty() => w.t("()"),
        syn::Type::Path(p) if p.qself.is_none() => {
            let Some(last) = p.path.segments.last() else { return w.hole(Role::Ty) };
            let name = last.ident.to_string();
            const KEPT: &[&str] = &["Self", "bool", "usize", "u8", "u16", "u32", "u64", "i32", "i64", "f64", "str", "String", "Option", "Result", "Vec", "Box"];
            if !KEPT.contains(&name.as_str()) {
                return w.hole(Role::Ty);
            }
            w.t(&name);
            if let syn::PathArguments::AngleBracketed(a) = &last.arguments {
                w.t("<");
                for (i, g) in a.args.iter().enumerate() {
                    if i > 0 {
                        w.t(",");
                    }
                    match g {
                        syn::GenericArgument::Type(t) => ty(w, t),
                        _ => w.hole(Role::Ty),
                    }
                }
                w.t(">");
            }
        }
        _ => w.hole(Role::Ty),
    }
}

/// The body of a small method, finer than a statement's frame: fields and methods on `self` kept as a chain,
/// each field a hole; a parameter a hole; a literal a hole.
fn fine(w: &mut W, e: &syn::Expr, params: &[String]) {
    match e {
        syn::Expr::Field(f) => {
            fine(w, &f.base, params);
            w.t(".");
            match &f.member {
                syn::Member::Named(_) => {
                    w.out.push("$f".into());
                    w.roles.push(Role::Field);
                }
                syn::Member::Unnamed(i) => w.t(&i.index.to_string()),
            }
        }
        syn::Expr::MethodCall(m) => {
            fine(w, &m.receiver, params);
            w.t(".");
            w.t(&m.method.to_string());
            w.t("(");
            for (i, a) in m.args.iter().enumerate() {
                if i > 0 {
                    w.t(",");
                }
                fine(w, a, params);
            }
            w.t(")");
        }
        syn::Expr::Reference(r) => {
            w.t(if r.mutability.is_some() { "&mut" } else { "&" });
            fine(w, &r.expr, params);
        }
        syn::Expr::Unary(u) => {
            w.t(&u.op.to_token_stream().to_string());
            fine(w, &u.expr, params);
        }
        syn::Expr::Call(c) if path_name(&c.func).is_some_and(|n| PRELUDE_CTORS.contains(&n.as_str()) || n == "Self") => {
            w.t(&c.func.to_token_stream().to_string());
            w.t("(");
            for (i, a) in c.args.iter().enumerate() {
                if i > 0 {
                    w.t(",");
                }
                fine(w, a, params);
            }
            w.t(")");
        }
        syn::Expr::Path(p) if p.path.is_ident("self") || p.path.is_ident("None") || p.path.is_ident("Self") => w.t(&p.path.to_token_stream().to_string()),
        syn::Expr::Path(p) if p.path.get_ident().is_some_and(|i| params.contains(&i.to_string())) => w.hole(Role::Arg),
        syn::Expr::Lit(l) if matches!(l.lit, syn::Lit::Bool(_)) => w.t(&l.lit.to_token_stream().to_string()),
        syn::Expr::Assign(a) => {
            fine(w, &a.left, params);
            w.t("=");
            fine(w, &a.right, params);
        }
        syn::Expr::Paren(p) => {
            w.t("(");
            fine(w, &p.expr, params);
            w.t(")");
        }
        _ => w.hole(Role::Expr),
    }
}

/// A small method's frame (at most three statements, no generics): `None` for a function that is not one.
pub fn of_method(sig: &syn::Signature, body: &syn::Block) -> Option<Frame> {
    if body.stmts.len() > 3 || !sig.generics.params.is_empty() || sig.asyncness.is_some() || sig.unsafety.is_some() {
        return None;
    }
    let mut w = W { control: true, ..W::default() };
    w.t("fn");
    w.t("(");
    let mut params = Vec::new();
    for (i, a) in sig.inputs.iter().enumerate() {
        if i > 0 {
            w.t(",");
        }
        match a {
            syn::FnArg::Receiver(r) => w.t(match (&r.reference, r.mutability.is_some()) {
                (Some(_), true) => "&mut self",
                (Some(_), false) => "&self",
                (None, true) => "mut self",
                (None, false) => "self",
            }),
            syn::FnArg::Typed(t) => {
                if let syn::Pat::Ident(i) = &*t.pat {
                    params.push(i.ident.to_string());
                }
                w.out.push("$a".into());
                w.roles.push(Role::Arg);
                w.t(":");
                ty(&mut w, &t.ty);
            }
        }
    }
    w.t(")");
    if let syn::ReturnType::Type(_, t) = &sig.output {
        w.t("->");
        ty(&mut w, t);
    }
    w.t("{");
    for s in &body.stmts {
        match s {
            syn::Stmt::Expr(e, semi) => {
                fine(&mut w, e, &params);
                if semi.is_some() {
                    w.t(";");
                }
            }
            other => stmt_into(&mut w, other),
        }
    }
    w.t("}");
    let f = done(w);
    // a body that is one hole is any one-expression method: simple_method's, not a shape of its own
    if f.text.ends_with("{ $ }") {
        return None;
    }
    Some(f)
}

/// A proposed macro's name from a frame's words: its control words, receivers and methods, in order, at most six;
/// never a keyword alone.
pub fn proposed_name(f: &Frame) -> String {
    let mut words: Vec<String> = Vec::new();
    for t in f.text.split_whitespace() {
        let w = match t {
            "!" => "not".to_string(),
            "?" => "try".to_string(),
            "&self" => "ref".to_string(),
            "&mut" | "&mut self" => "mut".to_string(),
            "mut self" => "owned".to_string(),
            "=>" | "=" | "." | "," | ";" | "(" | ")" | "{" | "}" | "<" | ">" | "->" | ":" | "&" | "[" | "]" | "()" | "!(..)" => continue,
            _ if t.starts_with('$') => continue,
            _ if t.chars().all(|c| c.is_alphanumeric() || c == '_') && !t.is_empty() => t.to_lowercase(),
            _ => continue,
        };
        if ["fn", "in", "_"].contains(&w.as_str()) || w.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if words.last() != Some(&w) {
            words.push(w);
        }
        if words.len() == 6 {
            break;
        }
    }
    const KEYWORDS: &[&str] = &["if", "match", "let", "for", "while", "loop", "return", "break", "continue", "self", "ref", "mut", "str", "bool", "else", "true", "false", "some", "none", "ok", "err", "string", "usize", "fn", "impl", "in", "as", "move", "type", "use", "where", "async", "await", "dyn", "const", "static", "struct", "enum", "trait", "mod", "pub", "crate", "super", "unsafe", "extern"];
    match words.as_slice() {
        [] => "shape".into(),
        [one] if KEYWORDS.contains(&one.as_str()) => format!("{one}_shape"),
        _ => words.join("_"),
    }
}

/// A proposed macro's arguments: one per hole, named by its role, numbered when a role repeats.
pub fn proposed_args(f: &Frame) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in &f.roles {
        let base = r.name();
        let n = out.iter().filter(|a| a.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_') == base).count();
        out.push(if n == 0 { base.to_string() } else { format!("{base}_{}", n + 1) });
    }
    out
}
