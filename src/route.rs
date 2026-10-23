// Copyright © 2026 1o. All rights reserved.

//! A function's route: the path klepto gives it (`package::module::Type::method`), and when two functions share that
//! path (two `From` impls on one type), the impl it sits in after it, `path[impl From<A> for Type]`. Every line that
//! names a function by its route finds it here, so a spec's target names one function or is refused with
//! [`ReadingRefusal::AmbiguousTarget`].

use crate::reading::refusal::ReadingRefusal;
use crate::Result;
use klepto::{CapturedFn, FnKind};

/// The route with the impl named, as the source writes its header (`path[impl From<i64> for Level]`, the trait's
/// generic arguments kept: klepto's model drops them, so two `From` impls on a type differ only there);
/// `path[impl Type]` for an inherent impl; the path alone for a function in no impl. When the source does not read,
/// klepto's trait and type and the function's signature.
pub fn impl_route(f: &CapturedFn) -> String {
    let FnKind::ImplMethod { self_ty, trait_ty } = &f.kind else { return f.fq_name.clone() };
    if let Some((tr, ty)) = f.location.line.and_then(|l| impl_header(&f.location.path, l as usize)) {
        return match tr {
            Some(t) => format!("{}[impl {t} for {ty}]", f.fq_name),
            None => format!("{}[impl {ty}]", f.fq_name),
        };
    }
    match trait_ty {
        Some(t) => format!("{}[impl {} for {}; {}]", f.fq_name, t.trim(), self_ty.trim(), f.signature.trim()),
        None => format!("{}[impl {}; {}]", f.fq_name, self_ty.trim(), f.signature.trim()),
    }
}

/// Tokens as one word: `From < i64 >` is `From<i64>`; a space kept only between two words (`dyn Trait`).
fn compact(t: &str) -> String {
    let mut out = String::new();
    for w in t.split_whitespace() {
        let joins = out.chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_') && w.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_');
        if joins {
            out.push(' ');
        }
        out.push_str(w);
    }
    out
}

/// The header of the impl that holds line `line` of the file at `path`: its trait (with its generic arguments) and
/// its type, as written.
fn impl_header(path: &std::path::Path, line: usize) -> Option<(Option<String>, String)> {
    use quote::ToTokens;
    use syn::spanned::Spanned;
    fn walk(items: &[syn::Item], line: usize) -> Option<(Option<String>, String)> {
        for i in items {
            let sp = i.span();
            if line < sp.start().line || line > sp.end().line {
                continue;
            }
            match i {
                syn::Item::Impl(imp) => return Some((imp.trait_.as_ref().map(|(_, p, _)| compact(&p.to_token_stream().to_string())), compact(&imp.self_ty.to_token_stream().to_string()))),
                syn::Item::Mod(m) => return m.content.as_ref().and_then(|(_, items)| walk(items, line)),
                _ => {}
            }
        }
        None
    }
    let text = std::fs::read_to_string(path).ok()?;
    walk(&syn::parse_file(&text).ok()?.items, line)
}

/// The route `f` is named by among `all`: its path when no other function written in the source shares it, else
/// [`impl_route`].
pub fn route_of(f: &CapturedFn, all: &[CapturedFn]) -> String {
    let shared = all.iter().filter(|x| x.generated.is_none() && x.fq_name == f.fq_name).count() > 1;
    if shared {
        impl_route(f)
    } else {
        f.fq_name.clone()
    }
}

/// A route split into its path and the impl it names (`[impl ...]`), when it names one.
pub fn split(route: &str) -> (&str, Option<&str>) {
    let r = route.trim();
    match r.strip_suffix(']').and_then(|x| x.rsplit_once("[impl ")) {
        Some((path, imp)) => (path, Some(imp)),
        None => (r, None),
    }
}

/// The function `route` names among `all`: the ones written in the source first (a macro's output only when none is);
/// refused when none has the route, and when more than one has it (two impls on one path) with the routes that tell
/// them apart.
pub fn find_fn<'a>(all: &'a [CapturedFn], route: &str) -> Result<&'a CapturedFn> {
    let (path, imp) = split(route);
    let by_path: Vec<&CapturedFn> = all.iter().filter(|f| f.fq_name == path).collect();
    let written: Vec<&CapturedFn> = by_path.iter().copied().filter(|f| f.generated.is_none()).collect();
    let pool = if written.is_empty() { by_path } else { written };
    let found: Vec<&CapturedFn> = match imp {
        Some(_) => pool.into_iter().filter(|f| same(&impl_route(f), route)).collect(),
        None => pool,
    };
    match found.as_slice() {
        [one] => Ok(one),
        [] => Err(ReadingRefusal::TargetUnknown.refuse(format!("{route}: not a function klepto captured"))),
        many => Err(ReadingRefusal::AmbiguousTarget.refuse(format!("{route} names {} functions: {}", many.len(), many.iter().map(|f| impl_route(f)).collect::<Vec<_>>().join(", ")))),
    }
}

/// Two routes the same, whitespace aside.
pub fn same(a: &str, b: &str) -> bool {
    a.split_whitespace().collect::<String>() == b.split_whitespace().collect::<String>()
}

/// The impl a route names (`From<i64> for Level` of `path[impl From<i64> for Level]`) as its trait and its type.
pub fn named_impl(route: &str) -> Option<(Option<String>, String)> {
    let (_, imp) = split(route);
    let imp = imp?.split(';').next()?.trim();
    Some(match imp.rsplit_once(" for ") {
        Some((t, ty)) => (Some(t.trim().to_string()), ty.trim().to_string()),
        None => (None, imp.to_string()),
    })
}
