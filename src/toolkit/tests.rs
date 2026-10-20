// Copyright © 2026 1o. All rights reserved.

use super::matcher::{self, Context, Source};
use super::normal::Level;
use super::*;

/// Every function of `text` matched, by name: (exact pieces, near pieces with their differences, whole-function
/// piece, spec).
fn matched(text: &str) -> Vec<(String, matcher::FnMatch, Option<crate::change::ChangeSpec>)> {
    let file = syn::parse_file(text).unwrap();
    let src = Source::new(text);
    let mut out = Vec::new();
    for item in &file.items {
        match item {
            syn::Item::Fn(f) => {
                let parts = matcher::FnParts { attrs: &f.attrs, vis: &f.vis, sig: &f.sig, body: &f.block };
                let m = matcher::match_fn(&src, parts, &Context::default());
                let spec = matcher::spec_for(&m, &src, &format!("demo::{}", f.sig.ident), parts, false);
                out.push((f.sig.ident.to_string(), m, spec));
            }
            syn::Item::Impl(i) => {
                let trait_name = i.trait_.as_ref().and_then(|(_, p, _)| p.segments.last()).map(|s| s.ident.to_string());
                let ctx = Context { trait_name, self_ty: None, impl_items: i.items.len(), in_impl: true };
                for it in &i.items {
                    if let syn::ImplItem::Fn(f) = it {
                        let parts = matcher::FnParts { attrs: &f.attrs, vis: &f.vis, sig: &f.sig, body: &f.block };
                        let m = matcher::match_fn(&src, parts, &ctx);
                        let spec = matcher::spec_for(&m, &src, &format!("demo::T::{}", f.sig.ident), parts, false);
                        out.push((f.sig.ident.to_string(), m, spec));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn pieces(m: &matcher::FnMatch, level: Level) -> Vec<String> {
    let mut v: Vec<String> = m.steps.iter().filter(|s| s.matched.level == level).map(|s| s.matched.shape().piece.clone()).collect();
    if let Some(w) = m.whole.as_ref().filter(|w| w.level == level) {
        v.push(w.shape().piece.clone());
    }
    v
}

#[test]
fn every_piece_has_a_shape_or_a_reason() {
    let (missing, both) = probe::pieces_accounted();
    assert!(missing.is_empty(), "pieces with no shape and no reason: {missing:?}");
    assert!(both.is_empty(), "pieces with a shape and a reason: {both:?}");
    let c = catalog();
    assert_eq!(c.shapes.len(), probe::probe_count());
    for (i, s) in c.shapes.iter().enumerate() {
        assert_eq!(s.call, probe::call_of(i), "the call is the vocabulary's: regenerate with examples/toolkit_shapes.rs");
    }
}

/// The shipped catalog is what the probe expands to now (klepto in rounds, cargo expand as rustc does); skipped
/// where cargo-expand is not installed.
#[test]
fn the_shipped_catalog_is_the_probe_expanded() {
    if !crate::exec::on_path("cargo-expand") {
        eprintln!("cargo-expand is not on PATH: the catalog is not rebuilt here");
        return;
    }
    // one directory, written over each run (its build kept), so runs do not pile up
    let dir = std::env::temp_dir().join("custodian_toolkit_probe");
    let fresh = probe::build_catalog(&dir).unwrap();
    assert_eq!(&fresh, catalog(), "regenerate with `cargo run --example toolkit_shapes -- <new dir>`");
}

#[test]
fn hand_written_bodies_match_the_toolkit_exactly() {
    let text = r#"
fn qos(level: u8) -> Result<u8, &'static str> {
    if !(level <= 2) {
        return Err("qos is 0, 1 or 2");
    }
    Ok(level)
}

fn qos_next(level: u8) -> Result<u8, &'static str> {
    // the level asked for
    if !(level <= 2) {
        return Err("qos is 0, 1 or 2");
    }
    let next = level + 1;
    Ok(next)
}

fn first(o: Option<u32>) -> Result<u32, String> {
    let v = match o {
        Some(v) => v,
        None => return Err("none".to_string()),
    };
    Ok(v + 1)
}

fn collect(xs: Vec<Option<u32>>, out: &mut Vec<u32>) {
    for x in xs {
        if let Some(v) = x {
            out.push(v);
        }
    }
}

fn deref_collect(xs: &[Option<u32>], out: &mut Vec<u32>) {
    for x in xs {
        if let Some(v) = x {
            out.push(*v);
        }
    }
}

fn skip(xs: &[u32]) -> u32 {
    let mut n = 0;
    for x in xs {
        if *x == 0 {
            continue;
        }
        n += x;
    }
    n
}

struct T { port: u16, name: String, tags: Vec<String>, label: Option<String> }

impl T {
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
    pub fn has_tags(&self) -> bool {
        !self.tags.is_empty()
    }
}
"#;
    let all = matched(text);
    let by = |n: &str| all.iter().find(|(name, ..)| name == n).unwrap();
    assert_eq!(pieces(&by("qos").1, Level::Exact), ["checked_method"], "the whole function: {:?}", by("qos").1);
    assert_eq!(pieces(&by("qos_next").1, Level::Exact), ["ensure"]);
    assert_eq!(pieces(&by("first").1, Level::Exact), ["require_some"]);
    assert_eq!(pieces(&by("collect").1, Level::Exact), ["push_some"], "{:?}", by("collect").1.steps.iter().map(|s| s.matched.shape().piece.clone()).collect::<Vec<_>>());
    assert!(pieces(&by("deref_collect").1, Level::Exact).is_empty(), "a `let` is no `$condition:expr`: {:?}", by("deref_collect").1);
    assert_eq!(pieces(&by("skip").1, Level::Exact), ["continue_if"]);
    assert_eq!(pieces(&by("port").1, Level::Exact), ["getter"]);
    assert_eq!(pieces(&by("tags").1, Level::Exact), ["getter_slice"]);
    assert_eq!(pieces(&by("label").1, Level::Exact), ["getter_option_deref"]);
    assert_eq!(pieces(&by("with_port").1, Level::Exact), ["with_field"]);
    assert_eq!(pieces(&by("has_tags").1, Level::Exact), ["nonempty"], "a tail value");
    // the spec swaps the statement for the call and keeps the rest
    let spec = by("qos").2.clone().unwrap();
    assert_eq!(crate::change::render_fn(spec.function.as_ref().unwrap(), "").unwrap(), "simple_impl::checked_method!(fn qos(level: u8) -> u8, &'static str; level <= 2 => \"qos is 0, 1 or 2\"; => level);\n");
    let spec = by("qos_next").2.clone().unwrap();
    let body = &spec.function.as_ref().unwrap().body;
    assert_eq!(body[0].piece, "raw", "the comment above it stays");
    assert_eq!(body[1].piece, "ensure");
    assert_eq!(body[1].condition.as_deref(), Some("level <= 2"));
    assert_eq!(body[1].error.as_deref(), Some("\"qos is 0, 1 or 2\""));
    assert_eq!(body[2].piece, "let");
    assert_eq!(body[3].piece, "value");
    let rendered = crate::change::render_body(spec.function.as_ref().unwrap(), "").unwrap();
    assert_eq!(rendered, "{\n    // the level asked for\n    simple_impl::ensure!(level <= 2, \"qos is 0, 1 or 2\");\n    let next = level + 1;\n    Ok(next)\n}");
    let spec = by("first").2.clone().unwrap();
    assert_eq!(crate::change::render_body(spec.function.as_ref().unwrap(), "").unwrap(), "{\n    let v = simple_impl::require_some!(o, \"none\".to_string());\n    Ok(v + 1)\n}");
    let spec = by("skip").2.clone().unwrap();
    assert!(crate::change::render_body(spec.function.as_ref().unwrap(), "").unwrap().contains("simple_impl::continue_if!(*x == 0);"), "{:?}", spec);
    let spec = by("port").2.clone().unwrap();
    assert_eq!(crate::change::render_fn(spec.function.as_ref().unwrap(), "").unwrap(), "simple_impl::getter!(pub fn port(&self) -> u16 => port);\n");
}

#[test]
fn a_near_match_names_its_difference() {
    let text = r#"
fn ne(a: u8, b: u8) -> Result<(), &'static str> {
    if !(a == b) {
        return Err("differ")
    }
    Ok(())
}
"#;
    let all = matched(text);
    let m = &all[0].1;
    assert!(pieces(m, Level::Exact).is_empty(), "{:?}", m.steps.iter().map(|s| (s.matched.shape().piece.clone(), s.matched.level)).collect::<Vec<_>>());
    assert!(m.near(), "{m:?}");
    assert!(m.differences().iter().any(|d| d.contains("without its `;`")), "{:?}", m.differences());
    assert!(all[0].2.is_none(), "no spec swaps a near match");
}
