// Copyright © 2026 1o. All rights reserved.

//! The toolkit's shapes: every simple_impl 0.1.11 toolkit piece ([`crate::change::PIECES`]) called once on
//! placeholder arguments, expanded, and kept as the catalog the `patterns` reading matches function bodies against.
//!
//! How the catalog is made ([`probe`]): a probe crate holds one call of each piece, its arguments placeholders
//! named by the kind of argument the macro takes (`__si_condition` an expression, `__SiTy` a type, `__sid_field` an
//! identifier, ...), each call written by the change vocabulary's own renderer (`change::render_step`,
//! `change::render_fn`), so the calls are the ones a change spec writes. klepto scans the probe crate and expands
//! the calls with the same expander it reads the workspace with (`macro_rules!` of dependencies from cargo's
//! registry; std's macros left as written, as they are in hand-written code), and `cargo expand` expands it as
//! rustc does. A shape is cargo expand's expansion of its call, unless that holds a std macro cargo expand opens
//! into std's insides (`vec!`, `format!`, `write!`, `matches!`), when it is klepto's; each shape records which, and
//! whether klepto's agrees. Its placeholders are the holes a body's code fills.
//!
//! How a body is matched ([`normal`], [`matcher`]): both sides are written in one normal form (parentheses around
//! an atomic expression dropped, the prelude's names written short, a block statement holding plain statements
//! opened, a semicolon after an `if` statement dropped, every non-block match arm ending in a comma). A statement
//! matches a shape when its tokens are the shape's tokens with each hole filled; the filling is then proved by
//! putting it back into the raw expansion (an expression argument as rustc does, an invisible group) and comparing
//! the two normal forms: equal is **exact** (the call expands to what was written). When only a looser normal form
//! is equal (a `return` without its `;`, a match arm in a block, `a != b` written for the macro's `!(a == b)`, a
//! builder method without the `#[inline]` the builder writes), the match is **near** and the rule that made it equal
//! is the difference.

pub mod frame;
pub mod matcher;
pub mod normal;
pub mod probe;

use crate::change::Family;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub const SHAPES_SCHEMA: &str = "custodian.toolkit_shapes.v1";

/// Where a shape stands in a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    /// A statement of a body.
    Step,
    /// An expression giving a value: a body's last expression, or what a `let` binds.
    Value,
    /// A whole function in an impl (a method generator, a builder line).
    Function,
    /// The one function of a trait impl the piece writes.
    ImplFn,
}

/// What a hole takes, as the macro's fragment does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoleKind {
    /// `$x:expr`.
    Expr,
    /// `$x:ty`.
    Type,
    /// `$x:ident`.
    Ident,
    /// `$first:ident $(.$rest:ident)*`: a field, or a path of fields.
    FieldPath,
    /// `$x:pat_param`.
    Pat,
    /// `$x:path`.
    Path,
    /// `($($args:tt)*)`: a function's parameters as written.
    Args,
    /// A name the expansion binds itself (`Some(value) => value`): any one name, the same each time.
    Binder,
}

impl HoleKind {
    /// The kind a placeholder's spelling declares.
    pub fn of(placeholder: &str) -> Option<HoleKind> {
        let k = if placeholder.starts_with("__sib_") {
            HoleKind::Binder
        } else if placeholder.starts_with("__sid_") {
            HoleKind::Ident
        } else if placeholder.starts_with("__sif_") {
            HoleKind::FieldPath
        } else if placeholder.starts_with("__sip_") {
            HoleKind::Pat
        } else if placeholder.starts_with("__sit_") {
            HoleKind::Path
        } else if placeholder.starts_with("__sia_") {
            HoleKind::Args
        } else if placeholder.starts_with("__si_") {
            HoleKind::Expr
        } else if placeholder.starts_with("__Si") {
            HoleKind::Type
        } else {
            return None;
        };
        Some(k)
    }
}

/// One hole of a shape: its placeholder, the piece's slot it fills (`conditions` for each of `__si_conditions_0`,
/// `__si_conditions_1`), and what it takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hole {
    pub placeholder: String,
    pub slot: String,
    pub kind: HoleKind,
}

/// One piece called once, expanded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolkitShape {
    pub piece: String,
    pub family: Family,
    /// Which call of the piece this is (`2 conditions`, `const`, `with a value`).
    pub variant: String,
    pub position: Position,
    /// The call as a change spec writes it, on the placeholders.
    pub call: String,
    /// What klepto expanded the call to, its markers taken off: the tokens the holes are put back into.
    pub expansion: String,
    /// The expansion in the normal form, its own bindings named `__sib_<n>`: what a body's code is matched against.
    pub pattern: String,
    pub holes: Vec<Hole>,
    /// A binder's placeholder and the name the expansion gives it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binders: Vec<(String, String)>,
    /// An impl piece's trait (`From`, `Display`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trait_name: Option<String>,
    /// The pattern's tokens that are not holes: the more, the more particular the shape.
    pub specificity: usize,
    /// What expanded the call: `cargo expand` (rustc's own expansion), or klepto when the expansion holds a std
    /// macro cargo expand opens into std's insides (hand-written code holds the macro).
    pub expanded_by: String,
    /// For a shape cargo expand made: whether klepto, which the reading reads the workspace with, expands the call
    /// to the same normal form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub klepto_same: Option<bool>,
}

/// The catalog: every shape of the toolkit, and the pieces left out with why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: String,
    /// `simple_impl 0.1.11`.
    pub toolkit: String,
    pub shapes: Vec<ToolkitShape>,
    /// A piece with no shape here, and why (`ensure_all`: several statements, matched one `ensure` at a time).
    pub not_matched: Vec<(String, String)>,
}

/// The catalog shipped with this crate (made by `examples/toolkit_shapes.rs`; `toolkit::tests` holds it equal to
/// a fresh expansion of the probe crate).
pub const CATALOG_JSON: &str = include_str!("../../schema/toolkit/simple_impl-0.1.11.shapes.json");

/// The shipped catalog, read once.
pub fn catalog() -> &'static Catalog {
    static C: OnceLock<Catalog> = OnceLock::new();
    C.get_or_init(|| serde_json::from_str(CATALOG_JSON).expect("the shipped toolkit catalog reads"))
}

impl Catalog {
    /// The shapes of one position.
    pub fn at(&self, position: Position) -> impl Iterator<Item = &ToolkitShape> {
        self.shapes.iter().filter(move |s| s.position == position)
    }
}

#[cfg(test)]
mod tests;
