// Copyright © 2026 1o. All rights reserved.

//! The change spec: one bounded operation on the code, declared as a document
//! (1o modifier `change`, `v1.<name>.1o.change`; `<name>.toml` is its twin),
//! the contract between the agent that asks for a change and the writer that
//! makes it.
//!
//! A spec names the objective, the operation (the vocabulary below, declared
//! once here: the preflight reads it, code_passenger implements it), the
//! target, what the operation makes (a struct, an enum, a function, a field,
//! a variant, a module, a dependency, a test), the scope it is authorized to
//! write, the feature configurations that must still build, the validations,
//! the postconditions, and the decisions it leaves open.
//!
//! A function's body is not free text. It is a declared sequence of
//! simple_impl 0.1.12 toolkit pieces (`ensure`, `require_some`, `push_if`,
//! ...), each by name with its arguments ([`PIECES`]), rendered by
//! [`render_fn`] into `simple_impl::<piece>!(...)` lines; three structural
//! steps (`let`, `value`, `raw`) carry what no piece does, and `raw` (and any
//! piece taking a code block) is flagged by the preflight. Whole functions can
//! come from the toolkit's method generators (`getter`, `setter`,
//! `simple_method`, `checked_method`, ...) and trait impls from its impl
//! generators (`impl_display_via`, `impl_from_newtype`, ...), so the code
//! written is lines klepto expands and reads.
//!
//! A valid spec is never an authorization: the preflight decides that
//! (`crate::preflight`), and a code-changing step runs only on a preflight
//! receipt of the same spec over the same code.

use crate::reading::refusal::ReadingRefusal;
use crate::reading::spec::vocabulary;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

vocabulary! {
    /// What a change spec asks the writer to do.
    Operation, "operation" {
        MakeStruct = "make_struct" : "a new struct in a module (target: crate:<name> or crate:<name>/src/<file>.rs): name, fields with types and visibility, derives, attributes, docs",
        MakeEnum = "make_enum" : "a new enum in a module: name, variants (unit, tuple or named fields, discriminants), derives, attributes, docs",
        MakeImpl = "make_impl" : "a new inherent impl block for a type (target: the type), with its functions",
        AddMethod = "add_method" : "a function added to a type's impl (target: the type): its signature and body, or a toolkit method generator",
        MakeFn = "make_fn" : "a free function in a module: its signature and body",
        AddField = "add_field" : "a field added to a struct (target: the struct); with a default, every struct literal gets it too",
        AddVariant = "add_variant" : "a variant added to an enum (target: the enum); the matches without a catch-all arm are listed, never edited",
        AddDerive = "add_derive" : "derives added to a type (target: the type), merged into its #[derive(..)]",
        AddAttribute = "add_attribute" : "one attribute line placed on an item (target: the item)",
        AddModule = "add_module" : "a new module: its own file and its `mod` line in the parent (target: the parent route)",
        AddDependency = "add_dependency" : "a dependency line in a crate's Cargo.toml (target: crate:<name>); `optional = true` makes it optional, and the lock file cargo writes is written with it",
        AddTest = "add_test" : "a test function (target: the function it proves): in the file's tests module (`to`: another inline test module of the file, by name; one the file does not have is made, `#[cfg(test)]`), its body from toolkit pieces",
        AddConstructor = "add_constructor" : "a constructor for a type (target: the type): `Self { field: param, .. }`, or a toolkit constructor generator",
        AddTraitImpl = "add_trait_impl" : "`impl Trait for Type` (target: the type): a toolkit impl generator, or the trait's functions",
        ChangeSignature = "change_signature" : "a function's signature changed (target: the function): a new parameter with the expression its call sites pass (inside a macro's arguments too; refused while a call site is unresolved, unless `call_sites.allow_unresolved`), a new name, a new return type; with `function.body`, its body written in the same step (a body that compiles only against the new signature)",
        Rename = "rename" : "an item renamed (target: the item) at its declaration and every use klepto resolves to it",
        Move = "move" : "an item moved to another module (target: the item, `to`: the module route), re-exported where it was so its paths still resolve",
        Extract = "extract" : "items moved out of a file into a new module of their own (`items`, `module`), re-exported where they were",
        Gate = "gate" : "an item put behind a crate feature the crate declares (target: the item, `feature`); an item already gated by it is refused. Code that compiles only with the feature on gets its gate as it is made, in the same step: `#[cfg(feature = \"..\")]` in the function's `attrs` (add_method, make_fn, add_test)",
        Anchor = "anchor" : "a linkage record's comment anchor placed above an item (target: the item, `linkage`: the record's name)",
        ReplaceBody = "replace_body" : "a function's body replaced (target: the function, `function`: its name and the new body as toolkit pieces); its signature, docs and attributes kept, so its callers are untouched; with a toolkit method generator (`function.generator`) the whole function is written as the call, its signature, docs and attributes inside it",
        ChangeDependency = "change_dependency" : "a dependency a crate already names, changed (target: crate:<name>, or `*`: every crate of the workspace naming it; `dependency`, or several in `dependencies` so a family moves together): its source becomes what the spec names (a version alone is the registry's, the path dropped; a path, with or without a version), features given replace its features; `optional` true or false, alone (its source kept) or with the rest",
        Inline = "inline" : "a function's body put in place of every call to it (target: a free function, or an associated one without self): each call becomes a block that binds the parameters to the call's arguments with `let`, then the body; the function is removed when it is not `pub` and every use of it was a call",
        Wrap = "wrap" : "a function's body wrapped (target: the function, `function`: its name and the pieces that run first): the pieces, then the body as it was, as one block; its signature, docs and attributes kept, so its callers are untouched",
        MakeLib = "make_lib" : "a crate's library root, src/lib.rs, made beside its binary (target: crate:<name>, a crate with no library; `module.doc`: the file's inner doc): cargo builds it as the crate's library, and the binary reaches it by the crate's name",
        RemoveDependency = "remove_dependency" : "a dependency a crate names, dropped from its manifest (target: crate:<name>; `dependency`: its name): refused while a feature names it or a source file of the crate still names its paths (each place said), then proved by the compile",
        RenamePaths = "rename_paths" : "every path through one crate in a crate's sources made a path through another (target: crate:<name>; `dependency.name`: the crate the paths name now, `new_name`: the crate they name after): `old::` becomes `new::`, `use old` and `extern crate old` with it, in every source file of the crate; proved by the compile",
        DeclareFeature = "declare_feature" : "a crate feature declared in its Cargo.toml (target: crate:<name>, `feature`: the name, `enables`: what it turns on, each `dep:<name>` (an optional dependency), `<dependency>/<feature>` or another feature of the crate); a feature the crate already declares gets the list given",
    }
}

vocabulary! {
    /// What a change keeps of the code that uses what it changes.
    Compatibility, "compatibility" {
        Preserve = "preserve" : "keep every existing use compiling: a new field needs its default, a moved item its re-export, a new parameter the expression its call sites pass",
        Break = "break" : "existing uses may stop compiling; the preflight lists them and the cargo check over the target and its dependents says which",
    }
}

impl Operation {
    /// The spec fields the operation needs besides its target.
    pub fn needs(self) -> &'static [&'static str] {
        use Operation::*;
        match self {
            MakeStruct | MakeEnum => &["item"],
            MakeImpl => &["functions"],
            AddMethod | MakeFn | AddTest | AddConstructor => &["function"],
            AddField => &["field"],
            AddVariant => &["variant"],
            AddDerive => &["derive"],
            AddAttribute => &["attribute"],
            AddModule => &["module"],
            AddDependency | ChangeDependency => &["dependency"],
            AddTraitImpl => &["trait"],
            ChangeSignature => &["function"],
            Rename => &["new_name"],
            Move => &["to"],
            Extract => &["items", "module"],
            Gate | DeclareFeature => &["feature"],
            Anchor => &["linkage"],
            ReplaceBody | Wrap => &["function"],
            Inline | MakeLib => &[],
            RemoveDependency => &["dependency"],
            RenamePaths => &["dependency", "new_name"],
        }
    }

    /// Whether the operation's target is an existing item (else a place: a module route, a crate).
    pub fn targets_an_item(self) -> bool {
        use Operation::*;
        !matches!(self, MakeStruct | MakeEnum | MakeFn | AddModule | AddDependency | ChangeDependency | DeclareFeature | MakeLib | Extract | RemoveDependency | RenamePaths)
    }

    /// The name the operation makes, which must be free where it lands.
    pub fn makes(self, spec: &ChangeSpec) -> Option<String> {
        use Operation::*;
        match self {
            MakeStruct | MakeEnum => spec.item.as_ref().map(|i| i.name.clone()),
            AddMethod | MakeFn | AddTest | AddConstructor => spec.function.as_ref().map(|f| f.name.clone()),
            AddField => spec.field.as_ref().map(|f| f.name.clone()),
            AddVariant => spec.variant.as_ref().map(|v| v.name.clone()),
            AddModule => spec.module.as_ref().map(|m| m.name.clone()),
            Extract => spec.module.as_ref().map(|m| m.name.clone()),
            Rename => spec.new_name.clone(),
            _ => None,
        }
    }
}

/// A function's parameter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Param {
    pub name: String,
    /// The type; empty only where the generator takes a bare name (`with_extend`).
    #[serde(default)]
    pub ty: String,
}

/// One toolkit piece (or a structural step: `let`, `value`, `raw`) with its
/// arguments. Which arguments a piece takes is [`PIECES`]'s to say; every
/// argument is an expression, a pattern, a type or a name as the piece's
/// macro takes it, never a statement (a `raw` step, or a `cleanup`/`block`
/// argument, is the one place statements go, and the preflight flags it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Piece {
    pub piece: String,
    /// `let <bind> = <piece>`: for a piece that gives a value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    /// A code block (`{ ... }`): statements, flagged as raw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub this: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    /// A field path (`port`, `inner.qos`) for the accessor and setter generators.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_ty: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrapper: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// A code block (`{ ... }`): statements, flagged as raw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// `raw`: statements written as they are. Flagged by the preflight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// `builder_new_default`'s visibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vis: Option<String>,
}

/// A function: its signature, its attributes and docs, and its body as
/// pieces, or one toolkit method generator that makes the whole function.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FnSpec {
    pub name: String,
    /// `pub`, `pub(crate)`; private when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vis: Option<String>,
    /// `&self`, `&mut self`, `self`, `mut self`: a method's receiver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    /// The return type.
    #[serde(default, rename = "returns", skip_serializing_if = "Option::is_none")]
    pub ret: Option<String>,
    /// `<T: Clone>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generics: Option<String>,
    /// The where clause, without `where`.
    #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<String>,
    #[serde(default, rename = "async", skip_serializing_if = "std::ops::Not::not")]
    pub is_async: bool,
    #[serde(default, rename = "const", skip_serializing_if = "std::ops::Not::not")]
    pub is_const: bool,
    #[serde(default, rename = "unsafe", skip_serializing_if = "std::ops::Not::not")]
    pub is_unsafe: bool,
    /// Attribute lines, `#[...]` each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
    /// The doc line(s), without `///`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// The body, in order; empty is `todo!()` with the doc line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<Piece>,
    /// A toolkit method generator that makes the whole function from the signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<Piece>,
}

/// A field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    pub name: String,
    pub ty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
    /// The expression every existing struct literal gets for it (`add_field`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// A variant: unit, tuple (`tuple` types) or named (`fields`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tuple: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discriminant: Option<String>,
}

/// A struct or an enum to make.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// `<T>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generics: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derives: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
    /// A struct's fields (none: a unit struct, unless `tuple`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldSpec>,
    /// A tuple struct's types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tuple: Vec<String>,
    /// An enum's variants.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<VariantSpec>,
    /// A toolkit item generator (`shell_command_builder`, `define_shell_builder`) in place of a declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<Piece>,
}

/// A module to make.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// A dependency line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencySpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// `optional = true`: the dependency is optional, turned on by a feature that names it `dep:<name>`
    /// (`declare_feature`); `false` makes it required again. Unset leaves it as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
}

/// What a change may read and write: routes (`crate:<name>`, `crate:<name>/src/<file>.rs`, `decl:<name>`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundaries {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read: Vec<String>,
    /// Empty: the target's own file only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write: Vec<String>,
}

/// A decision the change cannot make itself: the question, the options, why it matters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub question: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// What a signature change does to its call sites.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallSites {
    /// The expression each call passes for the new parameter (the last one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass: Option<String>,
    /// `true`: the change goes ahead with call sites klepto could not resolve, listed for a person; unset, the
    /// preflight refuses while any is unresolved (`unresolved: N`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_unresolved: bool,
}

/// One change spec.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSpec {
    /// The file's name; a receipt names the spec by it.
    pub name: String,
    /// What the change is for, in the asker's words.
    pub objective: String,
    pub operation: Option<Operation>,
    /// An item path (`core_lib::Config`) or a route (`crate:core_lib`, `crate:core_lib/src/store.rs`).
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FnSpec>,
    /// A new impl's functions (`make_impl`), a trait's (`add_trait_impl`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<FnSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<FieldSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<VariantSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derive: Vec<String>,
    /// `add_attribute`: the attribute line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    /// `add_trait_impl`: the trait (`Display`, `std::fmt::Display`).
    #[serde(default, rename = "trait", skip_serializing_if = "Option::is_none")]
    pub trait_name: Option<String>,
    /// `add_trait_impl`'s associated items, one line each (`type Output = Self;`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assoc: Vec<String>,
    /// A toolkit impl generator for `add_trait_impl` (`impl_display_via`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<Piece>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<ModuleSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency: Option<DependencySpec>,
    /// `change_dependency`: several dependencies changed in one spec (a family that must move together).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<DependencySpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_name: Option<String>,
    /// `move`: the module route the item goes to; `add_test`: the inline test module's name (`tests` when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// `extract`: the items moved out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    /// `gate`: the crate feature; `declare_feature`: the feature declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    /// `declare_feature`: what the feature turns on (`dep:chrono`, `serde/derive`, another feature); empty, it turns
    /// on nothing but itself (a feature that only gates code).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enables: Vec<String>,
    /// `anchor`: the linkage record's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linkage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_sites: Option<CallSites>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<Compatibility>,
    #[serde(default)]
    pub scope: Boundaries,
    /// Feature configurations that must still build: `default`, `none`, `all`, or a feature's name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// Validations after the change: `check` (cargo check over the check matrix, always), `affected_tests`, `test:<name>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validate: Vec<String>,
    /// What must hold after: `exists <item path>`, `resolves <path>`, `absent <item path>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postconditions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Question>,
    /// The change keeps what the function compiles to (a body swapped for the toolkit calls whose expansion it already
    /// is): `cargo expand` of the target before and after, the same tokens in the normal form, is a postcondition of
    /// the transform.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub same_expansion: bool,
}

/// The change spec's schema.
pub const CHANGE_SCHEMA: &str = "custodian.change.v1";

impl ChangeSpec {
    pub fn from_toml(text: &str) -> Result<Self> {
        let s: ChangeSpec = toml::from_str(text).map_err(|e| ReadingRefusal::ChangeInvalid.refuse(e.to_string().trim()))?;
        s.checked()
    }

    pub fn from_1o(text: &str) -> Result<Self> {
        let s: ChangeSpec = crate::oneo::parse(crate::oneo::Doc::Change, "change", text)?;
        s.checked()
    }

    /// `v1.<name>.1o.change` in 1o, else TOML.
    pub fn from_path(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| ReadingRefusal::SpecUnreadable.refuse(format!("{}: {e}", path.display())))?;
        match crate::oneo::Doc::of_file(path) {
            Some((crate::oneo::Doc::Change, _)) => Self::from_1o(&text),
            _ => Self::from_toml(&text),
        }
    }

    /// The spec as a 1o document, the form every writer of a change spec writes.
    pub fn to_1o(&self) -> Result<String> {
        crate::oneo::render(crate::oneo::Doc::Change, self)
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(self).map_err(|e| ReadingRefusal::Write.refuse(e))
    }

    /// The spec's digest over its canonical form: what a preflight and a transform receipt are bound to.
    pub fn digest(&self) -> String {
        crate::digest::of_value(&(CHANGE_SCHEMA, self))
    }

    /// The operation (a spec is refused without one).
    pub fn op(&self) -> Operation {
        self.operation.unwrap_or(Operation::MakeFn)
    }

    fn has(&self, field: &str) -> bool {
        match field {
            "item" => self.item.is_some(),
            "function" => self.function.is_some(),
            "functions" => !self.functions.is_empty() || self.generator.is_some(),
            "field" => self.field.is_some(),
            "variant" => self.variant.is_some(),
            "derive" => !self.derive.is_empty(),
            "attribute" => self.attribute.is_some(),
            "module" => self.module.is_some(),
            "dependency" => self.dependency.is_some() || !self.dependencies.is_empty(),
            "trait" => self.trait_name.is_some() || self.generator.is_some(),
            "new_name" => self.new_name.is_some(),
            "to" => self.to.is_some(),
            "items" => !self.items.is_empty(),
            "feature" => self.feature.is_some(),
            "linkage" => self.linkage.is_some(),
            _ => false,
        }
    }

    fn checked(self) -> Result<Self> {
        let refuse = |why: String| Err(ReadingRefusal::ChangeInvalid.refuse(format!("{}: {why}", self.name)));
        if self.name.trim().is_empty() || self.name.contains('/') || self.name.starts_with('.') {
            return refuse("the name must be one file name".into());
        }
        let Some(op) = self.operation else { return refuse("no operation".into()) };
        if self.target.trim().is_empty() {
            return refuse("no target".into());
        }
        let missing: Vec<&str> = op.needs().iter().copied().filter(|f| !self.has(f)).collect();
        if !missing.is_empty() {
            return refuse(format!("{} needs {}", op.as_str(), missing.join(", ")));
        }
        for p in self.pieces() {
            check_piece(p).map_err(|e| ReadingRefusal::ChangeInvalid.refuse(format!("{}: {e}", self.name)))?;
        }
        Ok(self)
    }

    /// The dependencies the spec names: `dependency`, then `dependencies`.
    pub fn dependencies_named(&self) -> Vec<&DependencySpec> {
        self.dependency.iter().chain(self.dependencies.iter()).collect()
    }

    /// Every piece the spec writes: bodies, generators.
    pub fn pieces(&self) -> Vec<&Piece> {
        let mut out: Vec<&Piece> = Vec::new();
        for f in self.function.iter().chain(self.functions.iter()) {
            out.extend(f.body.iter());
            out.extend(f.generator.iter());
        }
        out.extend(self.generator.iter());
        if let Some(i) = &self.item {
            out.extend(i.generator.iter());
        }
        out
    }

    /// The pieces that carry statements written as they are: `raw` steps and code-block arguments.
    pub fn raw(&self) -> Vec<String> {
        let mut out = Vec::new();
        for p in self.pieces() {
            if p.piece == "raw" {
                out.push(format!("raw: {}", p.code.clone().unwrap_or_default()));
            }
            if let Some(c) = &p.cleanup {
                out.push(format!("{} cleanup block: {c}", p.piece));
            }
            if let Some(b) = &p.block {
                out.push(format!("{} block: {b}", p.piece));
            }
            // a `value` or `let` step is an expression the tool writes as given: one that carries a closure or a
            // macro call holds code the pieces do not name, so it is said here (flagged, never refused)
            if matches!(p.piece.as_str(), "value" | "let") {
                if let Some(v) = &p.value {
                    let (closures, macros) = code_in_value(v);
                    if closures > 0 {
                        out.push(format!("{} with a closure: {v}", p.piece));
                    }
                    if !macros.is_empty() {
                        out.push(format!("{} with a macro call ({}): {v}", p.piece, macros.iter().map(|m| format!("{m}!")).collect::<Vec<_>>().join(", ")));
                    }
                }
            }
        }
        out
    }
}

/// The closures and the macro calls (by name) in an expression as written; none when it does not parse.
pub fn code_in_value(value: &str) -> (usize, Vec<String>) {
    use syn::visit::Visit;
    #[derive(Default)]
    struct Seen {
        closures: usize,
        macros: Vec<String>,
    }
    impl<'a> Visit<'a> for Seen {
        fn visit_expr_closure(&mut self, c: &'a syn::ExprClosure) {
            self.closures += 1;
            syn::visit::visit_expr_closure(self, c);
        }
        fn visit_macro(&mut self, m: &'a syn::Macro) {
            if let Some(s) = m.path.segments.last() {
                self.macros.push(s.ident.to_string());
            }
            // a macro's arguments are tokens: an expression-like one is read too (`vec![|x| x]`)
            if let Ok(args) = m.parse_body_with(syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated) {
                for a in &args {
                    self.visit_expr(a);
                }
            }
        }
    }
    let mut seen = Seen::default();
    if let Ok(e) = syn::parse_str::<syn::Expr>(value) {
        seen.visit_expr(&e);
    }
    (seen.closures, seen.macros)
}

// ---------------------------------------------------------------------------
// the toolkit pieces, declared once

/// Where a piece goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// A value: `let x = ...;`, an argument, the body's last value.
    Expression,
    /// A statement that may return early (`ensure`, `guard`, `bail`).
    Guard,
    /// Extraction and early control flow (`require_some`, `return_if`, `continue_if`).
    Flow,
    /// Conditional collection mutation (`push_if`, `insert_some`).
    Collection,
    /// Argument-vector assembly inside a command builder's `build` (`sc_*`).
    Argv,
    /// A whole function from its signature (`getter`, `setter`, `simple_method`, `checked_method`).
    Method,
    /// One builder method line inside an impl (`builder_*`).
    Builder,
    /// A trait impl (`impl_display_via`, `impl_from_newtype`).
    Impl,
    /// A whole item (`shell_command_builder`, `define_shell_builder`).
    Item,
    /// `let`, `value`, `raw`: not a toolkit macro.
    Step,
}

/// One toolkit piece: its macro's name, where it goes, its arguments as a template, and whether it gives a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PieceDecl {
    pub name: &'static str,
    pub family: Family,
    /// `{slot}` one argument, `{slot*,}` a list joined by `, `, `{slot*|}` by ` | `, `{a=>b*}` pairs `a => b`
    /// joined by `, `, `{a=>b;*}` pairs each ending `;`, `[...]` written only when its slots are given, `{sig}`
    /// the function's signature, `{ret}` its return type.
    pub template: &'static str,
    /// Gives a value (`let <bind> = ...`).
    pub binds: bool,
}

macro_rules! pieces {
    ($($name:literal => $family:ident, $binds:literal, $template:literal;)*) => {
        /// Every piece a spec may name: the toolkit of simple_impl 0.1.12 (each macro by its name) and the three
        /// structural steps.
        pub const PIECES: &[PieceDecl] = &[$(PieceDecl { name: $name, family: Family::$family, template: $template, binds: $binds },)*];
    };
}

pieces! {
    // pure checks: values
    "count_true" => Expression, true, "{conditions*,}";
    "all" => Expression, true, "{conditions*,}";
    "any" => Expression, true, "{conditions*,}";
    "exactly_one" => Expression, true, "{conditions*,}";
    "at_most_one" => Expression, true, "{conditions*,}";
    "implies" => Expression, true, "{left} => {right}";
    "excludes" => Expression, true, "{left}, {right}";
    "in_range" => Expression, true, "{value}, {minimum}, {maximum}";
    "nonempty" => Expression, true, "{value}";
    "is_one_of" => Expression, true, "{value}; {candidates*|}";
    // guards
    "guard" => Guard, false, "{condition} => {return_value}";
    "ensure" => Guard, false, "{condition}, {error}";
    "bail" => Guard, false, "{error}";
    "guard_with" => Guard, false, "{condition}, {cleanup} => {return_value}";
    "ensure_with" => Guard, false, "{condition}, {cleanup}, {error}";
    "bail_with" => Guard, false, "{cleanup}, {error}";
    "ensure_eq" => Guard, false, "{left}, {right}, {error}";
    "ensure_ne" => Guard, false, "{left}, {right}, {error}";
    "ensure_nonempty" => Guard, false, "{value}, {error}";
    "ensure_empty" => Guard, false, "{value}, {error}";
    "ensure_len" => Guard, false, "{value}, {expected}, {error}";
    "ensure_min_len" => Guard, false, "{value}, {minimum}, {error}";
    "ensure_max_len" => Guard, false, "{value}, {maximum}, {error}";
    "ensure_range" => Guard, false, "{value}, {minimum}, {maximum}, {error}";
    "ensure_all" => Guard, false, "{conditions=>errors*}";
    "ensure_any" => Guard, false, "{error}; {conditions*,}";
    "ensure_exactly_one" => Guard, false, "{error}; {conditions*,}";
    "ensure_at_most_one" => Guard, false, "{error}; {conditions*,}";
    "ensure_implies" => Guard, false, "{left} => {right}, {error}";
    "ensure_excludes" => Guard, false, "{left}, {right}, {error}";
    "ensure_valid" => Guard, false, "{value}[, {map_error}]";
    // extraction and early flow
    "require_some" => Flow, true, "{option}, {error}";
    "require_ok" => Flow, true, "{result}, {map_error}";
    "some_or_return" => Flow, true, "{option} => {return_value}";
    "ok_or_return" => Flow, true, "{result} => {return_value}";
    "return_if" => Flow, false, "{condition} => {return_value}";
    "return_unless" => Flow, false, "{condition} => {return_value}";
    "none_if" => Flow, false, "{condition}";
    "break_if" => Flow, false, "{condition}[ => {value}]";
    "continue_if" => Flow, false, "{condition}";
    "some_or_continue" => Flow, true, "{option}";
    "some_or_break" => Flow, true, "{option}";
    "require_ok_into" => Flow, true, "{result}, {map_error}";
    "require_some_into" => Flow, true, "{option}, {error}";
    "ok_or_else_return" => Flow, true, "{result}, {item} => {return_value}";
    "return_if_err" => Flow, false, "{result}, {item} => {return_value}";
    // values chosen
    "if_else" => Expression, true, "{condition} => {left}, {right}";
    "table" => Expression, true, "{value}; {candidates=>args*}";
    // collections
    "push_if" => Collection, false, "{collection}, {condition} => {value}";
    "push_some" => Collection, false, "{collection}, {option}";
    "extend_if" => Collection, false, "{collection}, {condition} => {values}";
    "extend_some" => Collection, false, "{collection}, {option}";
    "insert_if" => Collection, false, "{map}, {condition} => {key}, {value}";
    "insert_some" => Collection, false, "{map}, {key} => {option}";
    "assign_if" => Collection, false, "{condition}; {place} => {value}";
    "assign_if_some" => Collection, false, "{place}, {option}";
    "push_unique" => Collection, false, "{collection}, {value}";
    "extend_unique" => Collection, false, "{collection}, {values}";
    // argument vectors (inside a command builder's build)
    "sc_cmd" => Argv, true, "{value}";
    "sc_flags" => Argv, false, "{parts}, {this}, {fields=>flags*}";
    "sc_flags_into" => Argv, false, "{parts}, {this}, {fields=>flags*}";
    "sc_opt_kv" => Argv, false, "{parts}, {this}, {fields=>flags*}";
    "sc_args" => Argv, false, "{parts}, {args*,}";
    "sc_if_some" => Argv, false, "{parts}, {opt}, {map}";
    "sc_if_some_display" => Argv, false, "{parts}, {opt}";
    "sc_if_some_into" => Argv, false, "{parts}, {opt}";
    "sc_if_some_clone" => Argv, false, "{parts}, {opt}";
    "sc_if_some_prefix" => Argv, false, "{parts}, {opt}, {prefix}";
    "sc_each" => Argv, false, "{parts}, {iter}, {map}";
    "sc_each_display" => Argv, false, "{parts}, {iter}";
    "sc_if_some_flag" => Argv, false, "{vec}, {opt}, {flag}";
    // whole functions from the signature
    "simple_method" => Method, false, "{sig}[ -> {ret}] => {value}";
    "checked_method" => Method, false, "{sig} -> {ret}, {error_ty}; {conditions=>errors;*} => {value}";
    "checked_constructor" => Method, false, "{sig}, {error_ty}; {conditions=>errors;*} => {value}";
    "getter" => Method, false, "{sig} -> {ret} => {field}";
    "getter_ref" => Method, false, "{sig} -> {ret} => {field}";
    "getter_mut" => Method, false, "{sig} -> {ret} => {field}";
    "getter_option_ref" => Method, false, "{sig} -> {ret} => {field}";
    "getter_option_deref" => Method, false, "{sig} -> {ret} => {field}";
    "getter_slice" => Method, false, "{sig} -> {ret} => {field}";
    "len_field" => Method, false, "{sig} => {field}";
    "is_empty_field" => Method, false, "{sig} => {field}";
    "bool_field" => Method, false, "{sig} => {field}";
    "contains_field" => Method, false, "{sig} => {field}";
    "find_by" => Method, false, "{sig} -> {ret} => {field}, {item} => {condition}";
    "delegate" => Method, false, "{sig} -> {ret} => {field}.{method}([{pass}])";
    "delegate_mut" => Method, false, "{sig} -> {ret} => {field}.{method}([{pass}])";
    "newtype_value" => Method, false, "{sig} -> {ret}";
    "newtype_ref" => Method, false, "{sig} -> {ret}";
    "setter" => Method, false, "{sig} => {field}";
    "setter_into" => Method, false, "{sig} => {field}";
    "setter_some" => Method, false, "{sig} => {field}";
    "setter_some_into" => Method, false, "{sig} => {field}";
    "clear_collection" => Method, false, "{sig} => {field}";
    "take_field" => Method, false, "{sig} -> {ret} => {field}";
    "enable_field" => Method, false, "{sig} => {field}";
    "disable_field" => Method, false, "{sig} => {field}";
    "toggle_field" => Method, false, "{sig} => {field}";
    "with_field" => Method, false, "{sig} -> Self => {field}";
    "with_into" => Method, false, "{sig} -> Self => {field}";
    "with_some" => Method, false, "{sig} -> Self => {field}";
    "with_some_into" => Method, false, "{sig} -> Self => {field}";
    "with_push" => Method, false, "{sig} -> Self => {field}";
    "with_push_into" => Method, false, "{sig} -> Self => {field}";
    "with_extend" => Method, false, "{sig} -> Self => {field}: {elem}";
    "with_flag" => Method, false, "{sig} -> Self => {field}";
    "without_flag" => Method, false, "{sig} -> Self => {field}";
    "with_unique_push" => Method, false, "{sig} -> Self => {field}";
    "newtype_constructor" => Method, false, "{sig} -> Self";
    "forward_trait_method" => Method, false, "{sig} -> {ret} => {target}";
    // builder lines inside an impl
    "builder_new_default" => Builder, false, "[{vis}]";
    "builder_opt" => Builder, false, "{name} => {field}: {ty}";
    "builder_opt_into" => Builder, false, "{name} => {field}: {ty}";
    "builder_set" => Builder, false, "{name} => {field}: {ty}";
    "builder_vec_into" => Builder, false, "{name} => {field}: {elem}";
    "builder_vec_iter" => Builder, false, "{name} => {field}: {elem}";
    "builder_push" => Builder, false, "{name} => {field}: {elem}";
    "builder_flag" => Builder, false, "{name} => {field}";
    // trait impls
    "impl_default_via_new" => Impl, false, "{ty}";
    "impl_from_newtype" => Impl, false, "{wrapper}, {inner}";
    "impl_as_ref_newtype" => Impl, false, "{wrapper}, {inner}";
    "impl_deref_newtype" => Impl, false, "{wrapper}, {target}";
    "impl_deref_mut_newtype" => Impl, false, "{wrapper}";
    "impl_display_via" => Impl, false, "{ty}, {method}";
    "impl_debug_redacted" => Impl, false, "{ty}, {text}";
    "impl_std_error" => Impl, false, "{ty}";
    "impl_partial_ord_via_ord" => Impl, false, "{ty}";
    "impl_from_variant" => Impl, false, "{source} => {target}, {variant}";
    // whole items
    "shell_command_builder" => Item, false, "{name}, {command}";
    "define_shell_builder" => Item, false, "{name}, command = {command}, fields = {{ {fields*,} }}, build_parts = |{this}, {parts}| {block}";
    // structural steps (no macro)
    "let" => Step, false, "let {bind} = {value};";
    "value" => Step, false, "{value}";
    "raw" => Step, false, "{code}";
}

/// The crate the toolkit's macros come from, and the version the pieces are declared against.
pub const TOOLKIT: (&str, &str) = ("simple_impl", "0.1.12");

/// A piece's declaration by name.
pub fn piece(name: &str) -> Option<&'static PieceDecl> {
    PIECES.iter().find(|p| p.name == name)
}

impl Piece {
    /// A slot's value(s) by its name in a template.
    pub fn slot(&self, name: &str) -> Option<Vec<String>> {
        let one = |v: &Option<String>| v.clone().map(|x| vec![x]);
        let many = |v: &Vec<String>| (!v.is_empty()).then(|| v.clone());
        match name {
            "bind" => one(&self.bind),
            "condition" => one(&self.condition),
            "conditions" => many(&self.conditions),
            "error" => one(&self.error),
            "errors" => many(&self.errors),
            "value" => one(&self.value),
            "values" => one(&self.values),
            "option" => one(&self.option),
            "result" => one(&self.result),
            "map_error" => one(&self.map_error),
            "return_value" => one(&self.return_value),
            "left" => one(&self.left),
            "right" => one(&self.right),
            "expected" => one(&self.expected),
            "minimum" => one(&self.minimum),
            "maximum" => one(&self.maximum),
            "collection" => one(&self.collection),
            "map" => one(&self.map),
            "key" => one(&self.key),
            "place" => one(&self.place),
            "cleanup" => one(&self.cleanup),
            "candidates" => many(&self.candidates),
            "parts" => one(&self.parts),
            "this" => one(&self.this),
            "fields" => many(&self.fields),
            "flags" => many(&self.flags),
            "args" => many(&self.args),
            "opt" => one(&self.opt),
            "prefix" => one(&self.prefix),
            "iter" => one(&self.iter),
            "vec" => one(&self.vec),
            "flag" => one(&self.flag),
            "field" => one(&self.field),
            "method" => one(&self.method),
            "pass" => one(&self.pass),
            "item" => one(&self.item),
            "target" => one(&self.target),
            "elem" => one(&self.elem),
            "error_ty" => one(&self.error_ty),
            "ty" => one(&self.ty),
            "wrapper" => one(&self.wrapper),
            "inner" => one(&self.inner),
            "text" => one(&self.text),
            "source" => one(&self.source),
            "variant" => one(&self.variant),
            "name" => one(&self.name),
            "command" => one(&self.command),
            "block" => one(&self.block),
            "code" => one(&self.code),
            "vis" => one(&self.vis),
            _ => None,
        }
    }

    /// Every slot the piece was given.
    pub fn given(&self) -> Vec<&'static str> {
        SLOTS.iter().copied().filter(|s| self.slot(s).is_some()).collect()
    }
}

/// Every slot a piece can carry, in [`Piece`]'s order.
pub const SLOTS: &[&str] = &[
    "bind", "condition", "conditions", "error", "errors", "value", "values", "option", "result", "map_error", "return_value", "left", "right", "expected", "minimum", "maximum", "collection", "map", "key", "place", "cleanup", "candidates", "parts", "this", "fields", "flags", "args", "opt", "prefix", "iter", "vec", "flag", "field", "method", "pass", "item", "target", "elem", "error_ty", "ty", "wrapper", "inner", "text", "source", "variant", "name", "command", "block", "code", "vis",
];

/// A template's slots: (name, written only inside an optional group).
fn template_slots(t: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut optional = 0;
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '[' => optional += 1,
            ']' => optional -= 1,
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
            }
            '{' => {
                let mut tok = String::new();
                for d in chars.by_ref() {
                    if d == '}' {
                        break;
                    }
                    tok.push(d);
                }
                let base = tok.trim_end_matches(['*', ',', '|', ';']);
                for part in base.split("=>") {
                    if !part.is_empty() && part != "sig" && part != "ret" {
                        out.push((part.to_string(), optional > 0));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The slots a piece's macro takes (its template's), by the piece's name; none for a name that is not a piece.
pub fn slots_of(name: &str) -> Vec<String> {
    piece(name).map(|d| template_slots(d.template).into_iter().map(|(s, _)| s).collect()).unwrap_or_default()
}

/// A piece checked against its declaration: known, its required slots given, nothing it does not take.
pub fn check_piece(p: &Piece) -> std::result::Result<&'static PieceDecl, String> {
    let d = piece(&p.piece).ok_or_else(|| format!("`{}` is not a toolkit piece (simple_impl {}) or a step (let, value, raw)", p.piece, TOOLKIT.1))?;
    let slots = template_slots(d.template);
    for (s, optional) in &slots {
        if !optional && p.slot(s).is_none() {
            return Err(format!("{} needs `{s}`", d.name));
        }
    }
    let takes: Vec<&str> = slots.iter().map(|(s, _)| s.as_str()).chain(d.binds.then_some("bind")).chain((d.name == "let").then_some("bind")).collect();
    let extra: Vec<&str> = p.given().into_iter().filter(|g| !takes.contains(g)).collect();
    if !extra.is_empty() {
        return Err(format!("{} does not take {}", d.name, extra.join(", ")));
    }
    // paired lists pair up
    for (a, b) in [("conditions", "errors"), ("fields", "flags")] {
        if d.template.contains(&format!("{{{a}=>{b}")) && p.slot(a).map(|v| v.len()) != p.slot(b).map(|v| v.len()) {
            return Err(format!("{}: `{a}` and `{b}` pair up, one each", d.name));
        }
    }
    Ok(d)
}

/// A template filled from a piece's slots (and the function's signature and return type for a method piece).
fn fill(template: &str, p: &Piece, sig: &str, ret: Option<&str>) -> String {
    fn group(t: &str, p: &Piece, sig: &str, ret: Option<&str>) -> Option<String> {
        let mut out = String::new();
        let mut chars = t.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '[' => {
                    let mut depth = 1;
                    let mut inner = String::new();
                    for d in chars.by_ref() {
                        match d {
                            '[' => depth += 1,
                            ']' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        inner.push(d);
                    }
                    if let Some(g) = group(&inner, p, sig, ret) {
                        out.push_str(&g);
                    }
                }
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    out.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    out.push('}');
                }
                '{' => {
                    let mut tok = String::new();
                    for d in chars.by_ref() {
                        if d == '}' {
                            break;
                        }
                        tok.push(d);
                    }
                    if tok == "sig" {
                        out.push_str(sig);
                    } else if tok == "ret" {
                        out.push_str(ret?);
                    } else if let Some((pair, rest)) = tok.split_once('*').map(|(a, b)| (a.to_string(), b.to_string())) {
                        match pair.split_once("=>") {
                            Some((a, b)) => {
                                let (xs, ys) = (p.slot(a)?, p.slot(b.trim_end_matches(';'))?);
                                let each_ends = pair.ends_with(';');
                                let items: Vec<String> = xs.iter().zip(ys.iter()).map(|(x, y)| format!("{x} => {y}")).collect();
                                if each_ends {
                                    out.push_str(&items.iter().map(|i| format!("{i};")).collect::<Vec<_>>().join(" "));
                                } else {
                                    out.push_str(&items.join(", "));
                                }
                            }
                            None => {
                                let sep = if rest.contains('|') { " | " } else { ", " };
                                out.push_str(&p.slot(&pair)?.join(sep));
                            }
                        }
                    } else {
                        out.push_str(&p.slot(&tok)?.join(", "));
                    }
                }
                c => out.push(c),
            }
        }
        Some(out)
    }
    group(template, p, sig, ret).unwrap_or_default()
}

/// The macro path a piece is written with.
fn mac(name: &str) -> String {
    format!("{}::{name}!", TOOLKIT.0)
}

/// A body step as one line (no indentation).
pub fn render_step(p: &Piece) -> std::result::Result<String, String> {
    let d = check_piece(p)?;
    let args = fill(d.template, p, "", None);
    Ok(match (d.family, d.name) {
        (Family::Step, _) => args,
        (Family::Method | Family::Builder | Family::Impl | Family::Item, _) => return Err(format!("{} makes a whole {}, not a body step", d.name, match d.family {
            Family::Method => "function",
            Family::Builder => "builder method",
            Family::Impl => "impl",
            _ => "item",
        })),
        _ => match &p.bind {
            Some(b) if d.binds => format!("let {b} = {}({args});", mac(d.name)),
            None if d.binds && d.family == Family::Expression => format!("{}({args})", mac(d.name)),
            _ => format!("{}({args});", mac(d.name)),
        },
    })
}

fn doc_lines(doc: Option<&str>, indent: &str) -> String {
    doc.map(|d| d.lines().map(|l| if l.trim().is_empty() { format!("{indent}///\n") } else { format!("{indent}/// {}\n", l.trim_end()) }).collect()).unwrap_or_default()
}

fn vis(v: Option<&str>) -> String {
    v.filter(|v| !v.trim().is_empty()).map(|v| format!("{} ", v.trim())).unwrap_or_default()
}

/// A function's signature: `pub async fn name<T>(&self, a: A) -> R where ...`.
pub fn signature(f: &FnSpec) -> String {
    let mut args: Vec<String> = Vec::new();
    if let Some(r) = &f.receiver {
        args.push(r.clone());
    }
    args.extend(f.params.iter().map(|p| if p.ty.is_empty() { p.name.clone() } else { format!("{}: {}", p.name, p.ty) }));
    let mut s = format!("{}{}{}{}fn {}{}({})", vis(f.vis.as_deref()), if f.is_const { "const " } else { "" }, if f.is_async { "async " } else { "" }, if f.is_unsafe { "unsafe " } else { "" }, f.name, f.generics.clone().unwrap_or_default(), args.join(", "));
    if let Some(r) = &f.ret {
        s.push_str(&format!(" -> {r}"));
    }
    if let Some(w) = &f.where_clause {
        s.push_str(&format!(" where {w}"));
    }
    s
}

/// A function as source, indented by `indent`: docs, attributes, then the generator's line or the signature with
/// the body's steps (`todo!()` when the body is empty).
pub fn render_fn(f: &FnSpec, indent: &str) -> std::result::Result<String, String> {
    if let Some(g) = &f.generator {
        let d = check_piece(g)?;
        let text = match d.family {
            Family::Method => {
                // the macro takes the signature without its return type: the template writes `-> R` where the macro wants it
                let mut sig_only = f.clone();
                sig_only.ret = None;
                sig_only.doc = None;
                // forward_trait_method writes no visibility (a trait's method): a function with one is refused, never
                // written private
                if d.name == "forward_trait_method" && f.vis.as_deref().is_some_and(|v| !v.trim().is_empty()) {
                    return Err(format!("forward_trait_method writes a trait's method, with no visibility: `{} fn {}` is kept as written", f.vis.as_deref().unwrap_or_default(), f.name));
                }
                let sig = if d.name == "forward_trait_method" { signature(&FnSpec { vis: None, ..sig_only }) } else { signature(&sig_only) };
                // the docs go inside the call, as the attributes they stand for, onto the function the macro writes: a
                // `///` above a macro call documents nothing (rustc's unused doc comment)
                let docs: String = f.doc.as_deref().map(|d| d.lines().map(|l| format!("#[doc = {:?}] ", format!(" {l}").trim_end())).collect()).unwrap_or_default();
                let attrs: String = f.attrs.iter().map(|a| format!("{a} ")).collect();
                let attrs = format!("{docs}{attrs}");
                format!("{indent}{}({attrs}{});\n", mac(d.name), fill(d.template, g, &sig, f.ret.as_deref()))
            }
            // a builder line takes no attributes: its docs stay above it
            Family::Builder => format!("{}{indent}{}({});\n", doc_lines(f.doc.as_deref(), indent), mac(d.name), fill(d.template, g, "", None)),
            _ => return Err(format!("{} does not make a function", d.name)),
        };
        return Ok(text);
    }
    let mut out = doc_lines(f.doc.as_deref(), indent);
    for a in &f.attrs {
        out.push_str(&format!("{indent}{a}\n"));
    }
    out.push_str(&format!("{indent}{} {}\n", signature(f), render_body(f, indent)?));
    Ok(out)
}

/// A function's body as source, from its opening brace to its closing one (no newline after it), the steps
/// indented one level inside `indent`: what `replace_body` puts in place of a body, and what [`render_fn`] writes
/// after the signature.
pub fn render_body(f: &FnSpec, indent: &str) -> std::result::Result<String, String> {
    let mut out = String::from("{\n");
    let inner = format!("{indent}    ");
    if f.body.is_empty() {
        out.push_str(&format!("{inner}todo!({:?})\n", f.doc.clone().unwrap_or_else(|| f.name.clone())));
    }
    let last = f.body.len().saturating_sub(1);
    for (i, step) in f.body.iter().enumerate() {
        let tail = step.piece == "value" || (piece(&step.piece).is_some_and(|d| d.family == Family::Expression) && step.bind.is_none());
        if tail && i != last {
            return Err(format!("{}: `{}` gives the body's value, so it is the last step (or `bind` it)", f.name, step.piece));
        }
        let line = render_step(step)?;
        for l in line.lines() {
            out.push_str(&format!("{inner}{l}\n"));
        }
    }
    out.push_str(&format!("{indent}}}"));
    Ok(out)
}

fn field_line(fl: &FieldSpec, indent: &str) -> String {
    let mut s = doc_lines(fl.doc.as_deref(), indent);
    for a in &fl.attrs {
        s.push_str(&format!("{indent}{a}\n"));
    }
    s.push_str(&format!("{indent}{}{}: {},\n", vis(fl.vis.as_deref()), fl.name, fl.ty));
    s
}

/// A field as source (`add_field`), indented.
pub fn render_field(fl: &FieldSpec, indent: &str) -> String {
    field_line(fl, indent)
}

/// A variant as source (`add_variant`), indented.
pub fn render_variant(v: &VariantSpec, indent: &str) -> String {
    let mut s = doc_lines(v.doc.as_deref(), indent);
    for a in &v.attrs {
        s.push_str(&format!("{indent}{a}\n"));
    }
    let body = if !v.fields.is_empty() {
        let inner = format!("{indent}    ");
        format!(" {{\n{}{indent}}}", v.fields.iter().map(|f| field_line(&FieldSpec { vis: None, ..f.clone() }, &inner)).collect::<String>())
    } else if !v.tuple.is_empty() {
        format!("({})", v.tuple.join(", "))
    } else {
        String::new()
    };
    let disc = v.discriminant.as_ref().map(|d| format!(" = {d}")).unwrap_or_default();
    s.push_str(&format!("{indent}{}{body}{disc},\n", v.name));
    s
}

fn derive_line(derives: &[String], indent: &str) -> String {
    if derives.is_empty() {
        String::new()
    } else {
        format!("{indent}#[derive({})]\n", derives.join(", "))
    }
}

/// A struct as source (`make_struct`).
pub fn render_struct(i: &ItemSpec) -> std::result::Result<String, String> {
    if let Some(g) = &i.generator {
        let d = check_piece(g)?;
        if d.family != Family::Item {
            return Err(format!("{} does not make an item", d.name));
        }
        return Ok(format!("{}{}({});\n", doc_lines(i.doc.as_deref(), ""), mac(d.name), fill(d.template, g, "", None)));
    }
    let mut s = doc_lines(i.doc.as_deref(), "");
    s.push_str(&derive_line(&i.derives, ""));
    for a in &i.attrs {
        s.push_str(&format!("{a}\n"));
    }
    let head = format!("{}struct {}{}", vis(i.vis.as_deref()), i.name, i.generics.clone().unwrap_or_default());
    if !i.fields.is_empty() {
        s.push_str(&format!("{head} {{\n{}}}\n", i.fields.iter().map(|f| field_line(f, "    ")).collect::<String>()));
    } else if !i.tuple.is_empty() {
        s.push_str(&format!("{head}({});\n", i.tuple.join(", ")));
    } else {
        s.push_str(&format!("{head};\n"));
    }
    Ok(s)
}

/// An enum as source (`make_enum`).
pub fn render_enum(i: &ItemSpec) -> String {
    let mut s = doc_lines(i.doc.as_deref(), "");
    s.push_str(&derive_line(&i.derives, ""));
    for a in &i.attrs {
        s.push_str(&format!("{a}\n"));
    }
    s.push_str(&format!("{}enum {}{} {{\n{}}}\n", vis(i.vis.as_deref()), i.name, i.generics.clone().unwrap_or_default(), i.variants.iter().map(|v| render_variant(v, "    ")).collect::<String>()));
    s
}

/// An impl block as source: `impl Type { fns }` or `impl Trait for Type { assoc; fns }`.
pub fn render_impl(ty: &str, trait_name: Option<&str>, assoc: &[String], fns: &[FnSpec], generics: Option<&str>) -> std::result::Result<String, String> {
    let g = generics.unwrap_or("");
    let mut s = match trait_name {
        Some(t) => format!("impl{g} {t} for {ty} {{\n"),
        None => format!("impl{g} {ty} {{\n"),
    };
    for a in assoc {
        s.push_str(&format!("    {}\n", a.trim()));
    }
    for (i, f) in fns.iter().enumerate() {
        if i > 0 || !assoc.is_empty() {
            s.push('\n');
        }
        s.push_str(&render_fn(f, "    ")?);
    }
    s.push_str("}\n");
    Ok(s)
}

/// A trait impl from a toolkit impl generator.
pub fn render_impl_piece(g: &Piece) -> std::result::Result<String, String> {
    let d = check_piece(g)?;
    if d.family != Family::Impl {
        return Err(format!("{} does not make an impl", d.name));
    }
    Ok(format!("{}({});\n", mac(d.name), fill(d.template, g, "", None)))
}

/// Whether the source a spec writes calls the toolkit (so the crate needs the dependency).
pub fn uses_toolkit(spec: &ChangeSpec) -> bool {
    spec.pieces().iter().any(|p| piece(&p.piece).is_some_and(|d| d.family != Family::Step))
}

/// A body wrapped (`wrap`): the spec's pieces first, then the body as it was (`old`, from its opening brace to its
/// closing one) as the block's value, the pieces indented one level inside `indent`. None of the pieces may give the
/// body's value: the old body does.
pub fn render_wrapped(f: &FnSpec, indent: &str, old: &str) -> std::result::Result<String, String> {
    if f.body.is_empty() {
        return Err(format!("{}: wrap needs the pieces that run first", f.name));
    }
    let mut out = String::from("{\n");
    let inner = format!("{indent}    ");
    for step in &f.body {
        if step.piece == "value" || (piece(&step.piece).is_some_and(|d| d.family == Family::Expression) && step.bind.is_none()) {
            return Err(format!("{}: `{}` gives a value, and a wrapped body's value is the body it wraps (`bind` it)", f.name, step.piece));
        }
        for l in render_step(step)?.lines() {
            out.push_str(&format!("{inner}{l}\n"));
        }
    }
    out.push_str(&format!("{inner}{}\n{indent}}}", old.trim()));
    Ok(out)
}

/// A call inlined (`inline`): a block that binds each parameter (`pattern`, `type`) to the call's argument with
/// `let`, then the function's body without its braces (`body`, from its opening brace to its closing one).
pub fn render_inlined(params: &[(String, String)], args: &[String], body: &str) -> std::result::Result<String, String> {
    if params.len() != args.len() {
        return Err(format!("the call passes {} arguments for {} parameters", args.len(), params.len()));
    }
    let inner = body.trim().strip_prefix('{').and_then(|b| b.strip_suffix('}')).ok_or("the body is not a block")?;
    let mut out = String::from("{ ");
    for ((pat, ty), arg) in params.iter().zip(args) {
        out.push_str(&format!("let {}: {} = {}; ", pat.trim(), ty.trim(), arg.trim()));
    }
    out.push_str(inner.trim());
    out.push_str(" }");
    Ok(out)
}

/// Whether one of klepto's spellings of an argument (`&self`, `&'a mut self`, `self: Box<Self>`, `x: u32`) is the
/// receiver: klepto's args hold `self` beside the parameters a call passes.
pub fn is_receiver(arg: &str) -> bool {
    let n = arg.split(':').next().unwrap_or("").trim().trim_start_matches('&').trim_start_matches("mut ").trim();
    n == "self" || n.ends_with(" self")
}

/// What stops a function from being inlined, one line each (none: it can be). The one rule the preflight and the
/// writer read: a free function or an associated one without `self`, not generic, not `async` or `unsafe`, with a
/// body; no `return` and no `?` in its body (in the caller they would leave the caller), no `Self` (in the caller it
/// is the caller's); not calling itself; every use of it a call (a function named as a value has no call to put the
/// body in); and every call in the function's own file, where the names its body uses resolve as they do in it.
pub fn inline_blockers(k: &klepto::Klepto, item: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(f) = k.functions.iter().find(|x| x.fq_name == item) else { return vec![format!("{item}: not a function the scan captured")] };
    if f.args.iter().any(|a| is_receiver(a)) {
        out.push(format!("{item} takes self: only a free function or an associated one without self is inlined"));
    }
    if f.is_generic {
        out.push(format!("{item} is generic: its type parameters would not exist at the call"));
    }
    if f.is_async || f.is_unsafe {
        out.push(format!("{item} is {}: its body does not mean the same inside its callers", if f.is_async { "async" } else { "unsafe" }));
    }
    match std::fs::read_to_string(&f.location.path).ok().and_then(|t| syn::parse_file(&t).ok()) {
        Some(file) => {
            let line = f.location.line.unwrap_or(0) as usize;
            let mut v = BodyOf { name: f.name.clone(), line, found: None };
            syn::visit::Visit::visit_file(&mut v, &file);
            match v.found {
                Some(b) => {
                    let mut e = Escapes::default();
                    syn::visit::Visit::visit_block(&mut e, &b);
                    if e.ret {
                        out.push(format!("{item}'s body returns early (`return`): in a caller it would leave the caller"));
                    }
                    if e.question {
                        out.push(format!("{item}'s body uses `?`: in a caller it would leave the caller"));
                    }
                    if e.self_type {
                        out.push(format!("{item}'s body names `Self`: in a caller it is the caller's"));
                    }
                }
                None => out.push(format!("{item}: no body at {}:{line}", f.location.path.display())),
            }
        }
        None => out.push(format!("{item}: {} cannot be read", f.location.path.display())),
    }
    let names = |r: &Option<klepto::Resolution>| matches!(r, Some(klepto::Resolution::Local(id) | klepto::Resolution::External(id)) if id == item);
    if k.calls.iter().any(|c| names(&c.resolution) && c.enclosing_fn.as_deref() == Some(item)) {
        out.push(format!("{item} calls itself"));
    }
    let calls: std::collections::BTreeSet<(std::path::PathBuf, u32)> = k.calls.iter().filter(|c| names(&c.resolution)).filter_map(|c| Some((c.location.path.clone(), c.location.line?))).collect();
    let home = f.location.path.canonicalize().unwrap_or_else(|_| f.location.path.clone());
    for (path, line) in &calls {
        if path.canonicalize().unwrap_or_else(|_| path.clone()) != home {
            out.push(format!("{item} is called from another file, {}:{line}: the names its body uses do not resolve there as they do in its own", path.display()));
        }
    }
    for p in k.paths.iter().filter(|p| names(&p.resolution)) {
        if let Some(l) = p.location.line {
            if !calls.contains(&(p.location.path.clone(), l)) {
                out.push(format!("{item} is named as a value at {}:{l}, not called", p.location.path.display()));
            }
        }
    }
    out
}

/// The body of the function named `name` whose item starts at `line`.
struct BodyOf {
    name: String,
    line: usize,
    found: Option<syn::Block>,
}

impl<'ast> syn::visit::Visit<'ast> for BodyOf {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        use syn::spanned::Spanned;
        if f.sig.ident == self.name && (f.span().start().line..=f.span().end().line).contains(&self.line) {
            self.found = Some((*f.block).clone());
        }
        syn::visit::visit_item_fn(self, f);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        use syn::spanned::Spanned;
        if f.sig.ident == self.name && (f.span().start().line..=f.span().end().line).contains(&self.line) {
            self.found = Some(f.block.clone());
        }
        syn::visit::visit_impl_item_fn(self, f);
    }
}

/// `return` and `?` in a body, outside the closures, async blocks and items nested in it (theirs leave only them),
/// and `Self` anywhere in it.
#[derive(Default)]
struct Escapes {
    ret: bool,
    question: bool,
    self_type: bool,
}

impl<'ast> syn::visit::Visit<'ast> for Escapes {
    fn visit_expr_return(&mut self, e: &'ast syn::ExprReturn) {
        self.ret = true;
        syn::visit::visit_expr_return(self, e);
    }
    fn visit_expr_try(&mut self, e: &'ast syn::ExprTry) {
        self.question = true;
        syn::visit::visit_expr_try(self, e);
    }
    fn visit_path(&mut self, p: &'ast syn::Path) {
        if p.segments.first().is_some_and(|s| s.ident == "Self") {
            self.self_type = true;
        }
        syn::visit::visit_path(self, p);
    }
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

/// The attributes the compiler and its tools own (and `derive`, whose macros only add): none of them rewrites the
/// item it sits on.
pub const INERT_ATTRIBUTES: &[&str] = &[
    "derive", "cfg", "cfg_attr", "doc", "allow", "warn", "deny", "forbid", "expect", "must_use", "non_exhaustive", "repr", "deprecated", "inline", "cold", "track_caller", "automatically_derived", "path", "macro_export", "macro_use", "test", "ignore", "should_panic", "no_mangle", "export_name", "link", "link_name", "link_section", "used", "rustfmt", "clippy", "diagnostic", "target_feature", "global_allocator",
];

/// The attribute macros on an item, as `#[name] (path)`: attributes whose path resolves to a macro of a crate
/// (`#[bitpack]` through `use big_bad_bool::bitpack`), which rewrite the item before the compiler sees it. A
/// derive's helper attribute (`#[serde(..)]`, `#[bits(15)]`) resolves to nothing and is not one.
pub fn attribute_macros(k: &klepto::Klepto, item: &str) -> Vec<String> {
    let mut out: Vec<String> = k
        .attrs
        .iter()
        .filter(|a| a.enclosing_item.as_deref() == Some(item) && !INERT_ATTRIBUTES.contains(&a.path.split("::").next().unwrap_or("")))
        .filter_map(|a| match &a.resolution {
            Some(klepto::Resolution::Local(p) | klepto::Resolution::External(p)) => Some(format!("#[{}] ({p})", a.path)),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Why `op` cannot be written on `item` as it is declared: the attribute macros that rewrite its fields, for an
/// operation that writes the fields as declared (a new field, a constructor that names every field). `None` when it
/// can. The one rule the preflight and the writer read; its refusal is [`ReadingRefusal::MacroRewrites`].
pub fn rewritten_by(k: &klepto::Klepto, op: Operation, item: &str) -> Option<String> {
    if !matches!(op, Operation::AddField | Operation::AddConstructor) {
        return None;
    }
    let macros = attribute_macros(k, item);
    (!macros.is_empty()).then(|| ReadingRefusal::MacroRewrites.refuse(format!("{item} is under {}: {} writes the fields as declared, and the macro lays them out its own way", macros.join(", "), op.as_str())).to_string())
}

// ---------------------------------------------------------------------------
// where an operation lands, one rule for the preflight and the writer

/// Where in a file an operation writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum At {
    /// The end of the module's file (before its `#[cfg(test)]` module when it has one).
    EndOfFile,
    /// Inside the item at this line (a struct's fields, an enum's variants, an impl's functions, a test module).
    Inside { line: u32 },
    /// After the item at this line.
    After { line: u32 },
    /// Above the item at this line (its attributes).
    Above { line: u32 },
    /// At the item at this line (its name, its signature).
    At { line: u32 },
    /// A new file, its `mod` line in `parent` (empty: a crate root, which no file declares).
    NewFile { parent: String },
    /// The crate's manifest.
    Manifest,
}

/// Where an operation lands: the file, its module, its crate, the item it works on, the place in the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Landing {
    /// Relative to the root.
    pub file: String,
    /// The module path (`core_lib::store`).
    pub module: String,
    pub package: String,
    /// The item the operation works on, when it targets one.
    pub item: Option<String>,
    pub at: At,
}

impl Landing {
    /// One line a person reads: `inside core_lib::Config at crates/core_lib/src/lib.rs:47`.
    pub fn line(&self) -> String {
        match &self.at {
            At::EndOfFile => format!("the end of {} (module {})", self.file, self.module),
            At::Inside { line } => format!("inside {} at {}:{line}", self.item.as_deref().unwrap_or(&self.module), self.file),
            At::After { line } => format!("after {} at {}:{line}", self.item.as_deref().unwrap_or(&self.module), self.file),
            At::Above { line } => format!("above {} at {}:{line}", self.item.as_deref().unwrap_or(&self.module), self.file),
            At::At { line } => format!("{} at {}:{line}", self.item.as_deref().unwrap_or(&self.module), self.file),
            At::NewFile { parent } if parent.is_empty() => format!("a new file {}, the root of {}", self.file, self.package),
            At::NewFile { parent } => format!("a new file {} (module {}), its mod line in {parent}", self.file, self.module),
            At::Manifest => format!("{} ({})", self.file, self.package),
        }
    }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// The module whose own file is `file` (not an inline one).
pub fn module_of_file<'k>(k: &'k klepto::Klepto, file: &Path) -> Option<&'k klepto::ModuleInfo> {
    let want = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    k.modules.modules.values().find(|m| !m.inline && m.file.canonicalize().unwrap_or_else(|_| m.file.clone()) == want)
}

/// The package a crate root belongs to: a binary is a crate root of its own, named after its target (`stored`),
/// and its package (`cli`) is what cargo checks and what its manifest, dependencies and features are declared on.
pub fn package_of(k: &klepto::Klepto, crate_root: &str) -> String {
    k.crate_roots.get(crate_root).cloned().unwrap_or_else(|| crate_root.to_string())
}

/// One package a transform's proof compiles, and whether its tests compile too.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProofPackage {
    pub package: String,
    pub tests: bool,
    /// Why it is checked: `lands`, `direct dependent`, `reads <item>`.
    pub why: String,
}

/// Whether an operation changes what code elsewhere reads of its target (its signature, its name, its place, its
/// fields or variants, whether it is built): the readers of the target are checked too. Every other operation
/// changes the target's own crate only, and its direct dependents are the furthest a mistake can show.
pub fn reaches_readers(op: Operation) -> bool {
    use Operation::*;
    matches!(op, ChangeSignature | Rename | Move | Extract | Gate | AddField | AddVariant | Inline | ChangeDependency | DeclareFeature)
}

/// The packages a transform's proof compiles: the one it lands in (with its tests), each package that depends on
/// it directly, and, for an operation that changes what others read of its target, each package holding an occurrence
/// klepto resolves to the target. Never the whole closure of dependents: a body or an addition in one crate compiles
/// in a dependent's dependent only if it does in the direct one.
pub fn proof_packages(k: &klepto::Klepto, spec: &ChangeSpec, land: &Landing) -> Vec<ProofPackage> {
    let mut out: BTreeMap<String, ProofPackage> = BTreeMap::new();
    let mut add = |p: &str, tests: bool, why: String| {
        if k.features.contains_key(p) {
            out.entry(p.to_string()).or_insert(ProofPackage { package: p.to_string(), tests, why });
        }
    };
    add(&land.package, true, "lands".into());
    for (dependent, deps) in &k.package_deps {
        if deps.contains(&land.package) {
            add(dependent, false, "direct dependent".into());
        }
    }
    if reaches_readers(spec.op()) {
        if let Some(item) = land.item.as_deref() {
            let names_it = |r: &Option<klepto::Resolution>| matches!(r, Some(klepto::Resolution::Local(x) | klepto::Resolution::External(x)) if x == item || x.starts_with(&format!("{item}::")));
            let roots = k.paths.iter().filter(|o| names_it(&o.resolution)).map(|o| o.module_path.first().cloned()).chain(k.calls.iter().filter(|o| names_it(&o.resolution)).map(|o| o.module_path.first().cloned()));
            for root in roots.flatten() {
                add(&package_of(k, &root), false, format!("reads {item}"));
            }
        }
    }
    out.into_values().collect()
}

/// The item a target names: its id exactly, else by its trailing segments.
pub fn item_named<'k>(k: &'k klepto::Klepto, target: &str) -> Option<&'k klepto::ItemDecl> {
    let t = target.trim();
    k.items.iter().find(|d| d.id == t).or_else(|| k.items.iter().find(|d| crate::path_names_item(&d.id, t)))
}

/// A route's file: a crate's library (else its first binary), or the file a `crate:<name>/<file>` route names.
pub fn route_file(k: &klepto::Klepto, root: &Path, route: &str) -> Option<(String, std::path::PathBuf)> {
    let rest = route.trim().strip_prefix("crate:")?;
    let (name, file) = match rest.split_once('/') {
        Some((n, f)) => (n, Some(f)),
        None => (rest, None),
    };
    let p = k.packages.get(&name.replace('-', "_"))?;
    let path = match file {
        Some(f) => p.root.join(f),
        None => p.targets.iter().find(|t| t.kind == "lib").or_else(|| p.targets.iter().find(|t| t.kind == "bin")).map(|t| t.src_path.clone())?,
    };
    let _ = root;
    path.is_file().then(|| (p.name.clone(), path))
}

/// The route that names every crate of the workspace (`change_dependency`'s `*`).
pub const WORKSPACE: &str = "*";

/// One dependency line a `change_dependency` spec changes: the crate, its manifest (relative to the root), the
/// dependency's name as the manifest's key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyChange {
    pub package: String,
    pub manifest: String,
    pub name: String,
}

/// Every line a `change_dependency` spec changes, one rule for the preflight and the writer: for a crate target,
/// each named dependency of that crate (refused when the crate does not name one); for `*`, each crate of the
/// workspace naming one of them, in the scan's package order.
pub fn dependency_changes(k: &klepto::Klepto, root: &Path, spec: &ChangeSpec) -> std::result::Result<Vec<DependencyChange>, String> {
    let names: Vec<String> = spec.dependencies_named().iter().map(|d| d.name.replace('-', "_")).collect();
    let names_it = |pkg: &str, name: &str| k.features.get(pkg).is_some_and(|m| m.dependencies.iter().any(|d| d.name.replace('-', "_") == name || d.package.replace('-', "_") == name));
    let manifest = |pkg: &str| k.packages.get(pkg).map(|p| rel(root, &p.manifest_path));
    let target = spec.target.trim();
    let packages: Vec<String> = if target == WORKSPACE {
        k.packages.keys().cloned().collect()
    } else {
        let name = target.strip_prefix("crate:").filter(|n| !n.contains('/')).ok_or_else(|| format!("`{target}` is not a crate:<name> route or `*`"))?;
        let p = k.packages.get(&name.replace('-', "_")).ok_or_else(|| format!("`{target}` is no crate of the workspace"))?;
        if let Some(n) = names.iter().find(|n| !names_it(&p.name, n)) {
            return Err(format!("{} does not name the dependency {n}: add_dependency adds it", p.name));
        }
        vec![p.name.clone()]
    };
    let mut out = Vec::new();
    for pkg in &packages {
        for n in &names {
            if names_it(pkg, n) {
                if let Some(m) = manifest(pkg) {
                    out.push(DependencyChange { package: pkg.clone(), manifest: m, name: n.clone() });
                }
            }
        }
    }
    Ok(out)
}

/// A feature `declare_feature` writes, read against the crate's manifest as klepto reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDeclaration {
    pub package: String,
    pub feature: String,
    pub enables: Vec<String>,
    /// The list the crate declares the feature with now (`None`: not declared).
    pub was: Option<Vec<String>>,
    /// The required dependencies a `dep:<name>` in the list makes optional: cargo takes `dep:` only for an
    /// optional dependency, so declaring the feature is what makes it one.
    pub made_optional: Vec<String>,
    /// Why it cannot be declared as asked; empty when it can.
    pub problems: Vec<String>,
    /// What turning it on brings, through the whole feature graph (klepto's closure over the model the declaration
    /// leaves): `a:x (a/x in p)`, `c:z (c/z in a)`, `the dependency b (dep:b in p)`.
    #[serde(default)]
    pub brings: Vec<String>,
    /// Its weak entries (`b?/y`) whose dependency nothing it turns on turns on: they bring the feature only when
    /// something else turns the dependency on.
    #[serde(default)]
    pub weak: Vec<String>,
    /// Entries whose dependency's features are not known here (not a member, and cargo metadata not read).
    #[serde(default)]
    pub unchecked: Vec<String>,
}

/// Whether `name` is a feature name cargo takes: letters, digits, `_`, `-`, `+`, `.`, starting with a letter, a
/// digit or `_`.
fn feature_name_ok(name: &str) -> bool {
    let mut c = name.chars();
    c.next().is_some_and(|f| f.is_alphanumeric() || f == '_') && name.chars().all(|x| x.is_alphanumeric() || "_-+.".contains(x))
}

/// The feature `spec` declares (`declare_feature`): its list checked one entry at a time against the crate's
/// dependencies and features, the required dependencies its `dep:` entries make optional, and the list it had.
pub fn feature_declaration(k: &klepto::Klepto, root: &Path, spec: &ChangeSpec) -> std::result::Result<FeatureDeclaration, String> {
    let (package, _) = route_file(k, root, &spec.target).ok_or_else(|| format!("`{}` is not a crate:<name> route of the workspace", spec.target))?;
    let m = k.features.get(&package).ok_or_else(|| format!("{package}: klepto has no feature model for it"))?;
    let feature = spec.feature.clone().unwrap_or_default();
    let mut d = FeatureDeclaration { package: package.clone(), feature: feature.clone(), enables: spec.enables.clone(), ..Default::default() };
    if !feature_name_ok(&feature) {
        d.problems.push(format!("`{feature}` is not a feature name cargo takes"));
    }
    d.was = m.features.get(&feature).filter(|i| !i.implicit).map(|i| i.enables.iter().map(enable_text).collect());
    let dep = |n: &str| m.dependencies.iter().find(|x| x.name == n.replace('-', "_") && x.kind != klepto::DepKind::Dev);
    for e in &spec.enables {
        if let Some(n) = e.strip_prefix("dep:") {
            match dep(n) {
                Some(x) if !x.optional => d.made_optional.push(x.name.clone()),
                Some(_) => {}
                None => d.problems.push(format!("`{e}`: {package} has no dependency {n} (add_dependency adds it)")),
            }
        } else if let Some((n, f)) = e.split_once('/') {
            let n = n.trim_end_matches('?');
            if dep(n).is_none() {
                d.problems.push(format!("`{e}`: {package} has no dependency {n}"));
            } else if f.is_empty() {
                d.problems.push(format!("`{e}` names no feature of {n}"));
            } else {
                match k.dependency_feature_names(&package, n) {
                    Some(have) if !have.contains(f) => d.problems.push(format!("`{e}`: {n} has no feature {f} (its features: {})", if have.is_empty() { "none".to_string() } else { have.iter().cloned().collect::<Vec<_>>().join(", ") })),
                    Some(_) => {}
                    None => d.unchecked.push(format!("`{e}`: {n}'s features are not known here (not a member, and its cargo metadata not read)")),
                }
            }
        } else if e == &feature {
            d.problems.push(format!("`{e}`: a feature cannot turn itself on"));
        } else if !m.features.get(e.as_str()).is_some_and(|i| !i.implicit) {
            d.problems.push(format!("`{e}`: {package} declares no feature {e} (declare it first, or write dep:{e} for the dependency)"));
        }
    }
    d.made_optional.sort();
    d.made_optional.dedup();
    // what it brings, read through the whole feature graph as the declaration leaves it
    if d.problems.is_empty() {
        let asked = klepto::PackageSpec { features: [(feature.clone(), spec.enables.clone())].into_iter().collect(), optional: d.made_optional.iter().cloned().collect(), default: None };
        let map = k.feature_items_with(&m.apply_spec(&asked), &feature);
        for e in map.entries.iter().skip(1) {
            let what = match &e.key.feature {
                Some(f) => format!("{}:{f}", e.key.package),
                None => format!("the dependency {}", e.key.package),
            };
            let by = e.enabled_by.as_ref().map(|b| format!(" ({} in {}{})", enable_text(&b.entry), b.package, if b.weak { ", weak" } else { "" })).unwrap_or_default();
            d.brings.push(format!("{what}{by}"));
        }
        for w in &map.weak_pending {
            d.weak.push(format!("{} in {}: brings it only when {} is on", enable_text(&w.entry), w.package, match &w.entry {
                klepto::FeatureEnable::DepFeature { dep, .. } => dep.clone(),
                other => enable_text(other),
            }));
        }
    }
    if d.was.as_ref() == Some(&spec.enables) {
        d.problems.push(format!("{package} already declares {feature} = {:?}: nothing to change", spec.enables));
    }
    Ok(d)
}

fn enable_text(e: &klepto::FeatureEnable) -> String {
    match e {
        klepto::FeatureEnable::Feature(f) => f.clone(),
        klepto::FeatureEnable::Dep(d) => format!("dep:{d}"),
        klepto::FeatureEnable::DepFeature { dep, feature, weak } => format!("{dep}{}/{feature}", if *weak { "?" } else { "" }),
    }
}

/// A package's feature model as `spec` leaves its manifest: `declare_feature`'s feature and the dependencies it makes
/// optional, and `optional` on `add_dependency`/`change_dependency` (cargo's implicit features read again); the
/// model as it is for any other operation. `None` when the scan has no model of the package. The one rule the
/// preflight counts configurations by and the writer checks them by.
pub fn model_after(k: &klepto::Klepto, root: &Path, spec: &ChangeSpec, package: &str) -> Option<klepto::FeatureModel> {
    let mut m = k.features.get(package)?.clone();
    let mut asked = klepto::PackageSpec::default();
    match spec.op() {
        Operation::DeclareFeature => {
            if let Some(d) = feature_declaration(k, root, spec).ok().filter(|d| d.package == package && d.problems.is_empty()) {
                asked.features.insert(d.feature.clone(), d.enables.clone());
                asked.optional.extend(d.made_optional.iter().cloned());
            }
        }
        Operation::AddDependency | Operation::ChangeDependency => {
            for d in spec.dependencies_named() {
                if let Some(o) = d.optional {
                    for x in m.dependencies.iter_mut().filter(|x| x.name == d.name.replace('-', "_")) {
                        x.optional = o;
                    }
                }
            }
        }
        _ => return Some(m),
    }
    m = m.apply_spec(&asked);
    Some(m)
}

// ---------------------------------------------------------------------------
// calls inside a macro's arguments, one rule for the preflight and the writer

/// A call of a function written inside a macro's arguments, where klepto records the paths the tokens name and
/// resolves none: the invocation's place, and the paths in its tokens that resolve to the function from the
/// invocation's module (an item of the module, an import of it, a glob import's module, `crate::`, `self::`,
/// `super::`, `Self::`, a crate's name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroCall {
    /// The file, as klepto records it.
    pub file: std::path::PathBuf,
    pub line: u32,
    pub column: Option<u32>,
    /// The macro as written (`simple_impl::ensure_eq`).
    pub macro_path: String,
    /// The paths in its tokens that name the function (`shopping_list`, `kitchen::shopping_list`).
    pub paths: Vec<String>,
}

/// Every macro invocation whose tokens name `item` by a path that resolves to it, one per place: a macro klepto
/// expanded into another (`ensure_eq!` into `ensure!`) is the same place, its paths merged.
pub fn macro_calls(k: &klepto::Klepto, item: &str) -> Vec<MacroCall> {
    macro_paths(k, item).0
}

/// Every macro invocation whose tokens name `item`'s last segment by a path that resolves to nothing from its
/// module: a call of it or not, which nothing in the tokens tells. A path that resolves to another item is not it.
pub fn unresolved_macro_paths(k: &klepto::Klepto, item: &str) -> Vec<MacroCall> {
    macro_paths(k, item).1
}

/// The macro invocations naming `item`'s last segment: those whose path resolves to it, and those whose path
/// resolves to nothing.
fn macro_paths(k: &klepto::Klepto, item: &str) -> (Vec<MacroCall>, Vec<MacroCall>) {
    let tail = item.rsplit("::").next().unwrap_or(item);
    let (mut calls, mut unknown): (Vec<MacroCall>, Vec<MacroCall>) = (Vec::new(), Vec::new());
    let add = |out: &mut Vec<MacroCall>, m: &klepto::MacroInvocation, line: u32, paths: Vec<String>| match out.iter_mut().find(|c| c.file == m.location.path && c.line == line && c.column == m.location.column) {
        Some(c) => {
            for p in paths {
                if !c.paths.contains(&p) {
                    c.paths.push(p);
                }
            }
        }
        None => out.push(MacroCall { file: m.location.path.clone(), line, column: m.location.column, macro_path: m.path.clone().unwrap_or_else(|| m.name.clone()), paths }),
    };
    // a method with a receiver can be called by method syntax (`x.count()`), which no token says the type of
    let receiver = k.functions.iter().find(|f| f.fq_name == item).is_some_and(|f| f.args.iter().any(|a| is_receiver(a)));
    for m in &k.macros_inv {
        let Some(line) = m.location.line else { continue };
        if !m.token_paths.iter().any(|p| p.rsplit("::").next() == Some(tail)) {
            continue;
        }
        let module = module_of_invocation(k, &m.location.path, &m.module_path);
        let (mut to_it, mut to_none) = (Vec::new(), Vec::new());
        for named in names_in_tokens(&m.tokens, tail) {
            match named {
                // a declaration (`fn new` in a generator's tokens), a value, a field: no call
                Named::Other => {}
                Named::Method if receiver => to_none.push(format!(".{tail}")),
                Named::Method => {}
                Named::Call(p) => match module.as_deref().and_then(|md| resolve_in(k, md, &p, m.enclosing_fn.as_deref(), m.enclosing_item.as_deref())) {
                    Some(id) if id == item => to_it.push(p),
                    Some(_) => {}
                    None => to_none.push(p),
                },
            }
        }
        to_it.dedup();
        to_none.dedup();
        if !to_it.is_empty() {
            add(&mut calls, m, line, to_it);
        }
        if !to_none.is_empty() {
            add(&mut unknown, m, line, to_none);
        }
    }
    // a place one of its paths names it is a call: the others there are the same call's
    unknown.retain(|u| !calls.iter().any(|c| c.file == u.file && c.line == u.line && c.column == u.column));
    (calls, unknown)
}

/// What a name is where a macro's tokens write it.
enum Named {
    /// A path followed by its argument list: `list(1)`, `crate::list(3)`, `Type::new(..)`.
    Call(String),
    /// A method call: `.name(..)`.
    Method,
    /// A declaration (`fn name`), a value, a field: not a call.
    Other,
}

/// Each place `tail` is written in a macro's tokens (its text as klepto keeps it), and what it is there.
fn names_in_tokens(tokens: &str, tail: &str) -> Vec<Named> {
    use proc_macro2::TokenTree;
    fn walk(ts: proc_macro2::TokenStream, tail: &str, out: &mut Vec<Named>) {
        let tts: Vec<TokenTree> = ts.into_iter().collect();
        let punct = |i: usize, c: char| matches!(tts.get(i), Some(TokenTree::Punct(p)) if p.as_char() == c);
        for (i, t) in tts.iter().enumerate() {
            match t {
                TokenTree::Group(g) => walk(g.stream(), tail, out),
                TokenTree::Ident(id) if id == tail => {
                    // the path it ends: the segments before it, `a::b::tail`
                    let mut start = i;
                    while start >= 3 && punct(start - 1, ':') && punct(start - 2, ':') && matches!(tts.get(start - 3), Some(TokenTree::Ident(_))) {
                        start -= 3;
                    }
                    let path: Vec<String> = tts[start..=i].iter().filter_map(|t| match t { TokenTree::Ident(x) => Some(x.to_string()), _ => None }).collect();
                    let before = start.checked_sub(1).and_then(|j| tts.get(j));
                    // its arguments, after a turbofish when it has one
                    let mut j = i + 1;
                    if punct(j, ':') && punct(j + 1, ':') && punct(j + 2, '<') {
                        let mut depth = 0;
                        j += 2;
                        while j < tts.len() {
                            if punct(j, '<') {
                                depth += 1;
                            } else if punct(j, '>') {
                                depth -= 1;
                                if depth == 0 {
                                    j += 1;
                                    break;
                                }
                            }
                            j += 1;
                        }
                    }
                    let called = matches!(tts.get(j), Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Parenthesis);
                    out.push(match before {
                        Some(TokenTree::Ident(x)) if x == "fn" => Named::Other,
                        Some(TokenTree::Punct(p)) if p.as_char() == '.' => {
                            if called {
                                Named::Method
                            } else {
                                Named::Other
                            }
                        }
                        _ if called => Named::Call(path.join("::")),
                        _ => Named::Other,
                    });
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    if let Ok(ts) = tokens.parse::<proc_macro2::TokenStream>() {
        walk(ts, tail, &mut out);
    }
    out
}

/// The module an invocation sits in: its file's crate and the invocation's module path (`pantry::kitchen::tests`).
fn module_of_invocation(k: &klepto::Klepto, file: &Path, module_path: &[String]) -> Option<String> {
    let krate = k.files.iter().find(|f| f.path == file).and_then(|f| f.module.as_deref()).and_then(|m| m.split("::").next())?.to_string();
    let module = std::iter::once(krate).chain(module_path.iter().cloned()).collect::<Vec<_>>().join("::");
    k.modules.modules.contains_key(&module).then_some(module)
}

fn resolution_id(r: &Option<klepto::Resolution>) -> Option<&str> {
    match r {
        Some(klepto::Resolution::Local(id) | klepto::Resolution::External(id)) => Some(id),
        _ => None,
    }
}

/// What one name means in a module: its own item, an import naming it (by its last segment or its alias), an item of
/// a glob-imported module, a crate.
fn name_in(k: &klepto::Klepto, module: &str, name: &str, enclosing_fn: Option<&str>) -> Option<String> {
    if k.modules.modules.get(module).is_some_and(|m| m.items.contains_key(name)) {
        return Some(format!("{module}::{name}"));
    }
    let krate = module.split("::").next().unwrap_or(module);
    let segs: Vec<&str> = module.split("::").skip(1).collect();
    let here = |i: &klepto::StolenPath| {
        i.module_path.iter().map(String::as_str).eq(segs.iter().copied())
            && (i.enclosing_fn.is_none() || i.enclosing_fn.as_deref() == enclosing_fn)
            && k.files.iter().find(|f| f.path == i.location.path).and_then(|f| f.module.as_deref()).and_then(|m| m.split("::").next()) == Some(krate)
    };
    let mut glob = None;
    for i in k.imports.iter().filter(|i| here(i)) {
        let Some(id) = resolution_id(&i.resolution) else { continue };
        match &i.kind {
            klepto::UseKind::Name if i.segments.last().map(String::as_str) == Some(name) => return Some(id.to_string()),
            klepto::UseKind::Rename { alias } if alias == name => return Some(id.to_string()),
            klepto::UseKind::Glob if glob.is_none() && k.modules.modules.get(id).is_some_and(|m| m.items.contains_key(name)) => glob = Some(format!("{id}::{name}")),
            _ => {}
        }
    }
    glob.or_else(|| k.modules.modules.contains_key(name).then(|| name.to_string()))
}

/// A path written in `module` (inside `enclosing_fn`, in `enclosing_item`), resolved as the compiler would for the
/// forms [`MacroCall`] names; `None` when it does not resolve that way.
fn resolve_in(k: &klepto::Klepto, module: &str, path: &str, enclosing_fn: Option<&str>, enclosing_item: Option<&str>) -> Option<String> {
    let segs: Vec<&str> = path.split("::").map(str::trim).filter(|s| !s.is_empty()).collect();
    let (first, rest) = segs.split_first()?;
    let parent = |m: &str| m.rsplit_once("::").map(|(p, _)| p.to_string());
    let mut at = match *first {
        "crate" => module.split("::").next()?.to_string(),
        "self" => module.to_string(),
        "super" => parent(module)?,
        "Self" => {
            let e = enclosing_item?;
            let ty = parent(e)?;
            k.type_decl(&ty).map(|_| ty).or_else(|| k.type_decl(e).map(|_| e.to_string()))?
        }
        name => name_in(k, module, name, enclosing_fn)?,
    };
    for s in rest {
        at = if *s == "super" { parent(&at)? } else { format!("{at}::{s}") };
    }
    Some(at)
}

// ---------------------------------------------------------------------------
// optional dependencies the code a spec writes names, one rule for the preflight and the writer

/// An optional dependency the code a spec writes names by path (`chrono::NaiveDate`): the features that turn it on,
/// and whether the code is built only when one of them is on (a `#[cfg(feature = ..)]` among the spec's attributes,
/// a `cfg_attr` around the path, or the item or module it lands in gated so already).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionalUse {
    pub dependency: String,
    pub package: String,
    pub features: Vec<String>,
    pub gated: bool,
}

impl OptionalUse {
    /// The attribute that builds the code only with the dependency on: `#[cfg(feature = "chrono")]`.
    pub fn attr(&self) -> String {
        format!("#[cfg(feature = \"{}\")]", self.features.first().cloned().unwrap_or_else(|| self.dependency.clone()))
    }

    /// The fix said for a person: what to add to the spec.
    pub fn fix(&self) -> String {
        format!(
            "{} is an optional dependency of {} (turned on by {}): give the code `attrs = ['{}']`, or gate the place it lands",
            self.dependency,
            self.package,
            if self.features.is_empty() { "no feature".to_string() } else { self.features.join(", ") },
            self.attr()
        )
    }
}

/// Whether a cfg predicate holds only when `feature` is on.
fn requires_feature(p: &klepto::CfgPredicate, feature: &str) -> bool {
    match p {
        klepto::CfgPredicate::Feature(f) => f == feature,
        klepto::CfgPredicate::All(v) => v.iter().any(|c| requires_feature(c, feature)),
        _ => false,
    }
}

/// The heads of the paths written in `ts` (`chrono` of `chrono::NaiveDate`), each with the cfg predicates around it:
/// a `#[cfg(..)]` before it at the top level of the chunk, a `#[cfg_attr(PRED, ..)]` it sits inside.
fn path_heads(ts: proc_macro2::TokenStream, around: &[klepto::CfgPredicate], out: &mut Vec<(String, Vec<klepto::CfgPredicate>)>) {
    use proc_macro2::{Delimiter, TokenTree};
    let tts: Vec<TokenTree> = ts.into_iter().collect();
    let mut chunk_cfg: Vec<klepto::CfgPredicate> = around.to_vec();
    let mut i = 0;
    while i < tts.len() {
        match &tts[i] {
            TokenTree::Punct(p) if p.as_char() == '#' => {
                if let Some(TokenTree::Group(g)) = tts.get(i + 1).filter(|t| matches!(t, TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket)) {
                    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                    match (inner.first(), inner.get(1)) {
                        (Some(TokenTree::Ident(id)), Some(TokenTree::Group(args))) if id == "cfg" => {
                            chunk_cfg.push(klepto::CfgPredicate::parse_tokens(&args.stream()));
                            i += 2;
                            continue;
                        }
                        (Some(TokenTree::Ident(id)), Some(TokenTree::Group(args))) if id == "cfg_attr" => {
                            let parts: Vec<TokenTree> = args.stream().into_iter().collect();
                            let comma = parts.iter().position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ','));
                            let (pred, rest) = match comma {
                                Some(c) => (parts[..c].iter().cloned().collect::<proc_macro2::TokenStream>(), parts[c + 1..].iter().cloned().collect::<proc_macro2::TokenStream>()),
                                None => (args.stream(), proc_macro2::TokenStream::new()),
                            };
                            let mut inside = chunk_cfg.clone();
                            inside.push(klepto::CfgPredicate::parse_tokens(&pred));
                            path_heads(rest, &inside, out);
                            i += 2;
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            TokenTree::Ident(id) => {
                let before = i.checked_sub(1).and_then(|j| tts.get(j));
                let after_colons = matches!((tts.get(i + 1), tts.get(i + 2)), (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b))) if a.as_char() == ':' && b.as_char() == ':');
                let led = matches!(before, Some(TokenTree::Punct(p)) if p.as_char() == ':' || p.as_char() == '.');
                if after_colons && !led {
                    out.push((id.to_string(), chunk_cfg.clone()));
                }
            }
            TokenTree::Group(g) => path_heads(g.stream(), &chunk_cfg, out),
            _ => {}
        }
        i += 1;
    }
}

/// The code `spec` writes, as text: its function or functions, its item, field or variant, its derives, its
/// attribute line, its associated items.
fn written_code(spec: &ChangeSpec) -> Vec<String> {
    let mut out = Vec::new();
    out.extend(spec.function.iter().chain(&spec.functions).filter_map(|f| render_fn(f, "").ok()));
    if let Some(i) = &spec.item {
        out.extend(if i.variants.is_empty() { render_struct(i).ok() } else { Some(render_enum(i)) });
    }
    out.extend(spec.field.iter().map(|f| render_field(f, "")));
    out.extend(spec.variant.iter().map(|v| render_variant(v, "")));
    out.extend(spec.derive.iter().map(|d| format!("#[derive({d})] struct X;")));
    out.extend(spec.attribute.iter().map(|a| format!("{a} struct X;")));
    out.extend(spec.assoc.iter().cloned());
    out
}

/// Every optional dependency of the landing crate the code `spec` writes names by path, and whether that code is
/// gated by a feature that turns it on. A dependency with a declaration that is not optional is always there and
/// is not one.
pub fn optional_uses(k: &klepto::Klepto, spec: &ChangeSpec, land: &Landing) -> Vec<OptionalUse> {
    let Some(m) = k.features.get(&land.package) else { return Vec::new() };
    let mut heads = Vec::new();
    for code in written_code(spec) {
        if let Ok(ts) = code.parse::<proc_macro2::TokenStream>() {
            path_heads(ts, &[], &mut heads);
        }
    }
    // the place it lands: the item it works on, else the module
    let mut place: Vec<klepto::CfgPredicate> = Vec::new();
    if let Some(d) = land.item.as_deref().and_then(|i| item_named(k, i)) {
        place.push(d.cfg.clone());
    }
    if let Some(md) = k.modules.modules.get(&land.module) {
        place.push(md.cfg.clone());
    }
    let mut out: Vec<OptionalUse> = Vec::new();
    for (head, around) in heads {
        let decls: Vec<&klepto::DepInfo> = m.dependencies.iter().filter(|d| d.name == head && d.kind != klepto::DepKind::Dev).collect();
        if decls.is_empty() || decls.iter().any(|d| !d.optional) {
            continue;
        }
        let features: Vec<String> = m.features_enabling(&head).into_iter().collect();
        let gated = around.iter().chain(&place).any(|p| features.iter().any(|f| requires_feature(p, f)));
        match out.iter_mut().find(|u| u.dependency == head) {
            Some(u) => u.gated &= gated,
            None => out.push(OptionalUse { dependency: head, package: land.package.clone(), features, gated }),
        }
    }
    out
}

/// Where `spec`'s operation lands in the scan, or why it cannot.
pub fn landing(k: &klepto::Klepto, root: &Path, spec: &ChangeSpec) -> std::result::Result<Landing, String> {
    use Operation::*;
    let op = spec.op();
    let at_item = |d: &klepto::ItemDecl, at: fn(u32) -> At| -> Landing {
        let line = d.location.line.unwrap_or(1);
        Landing { file: rel(root, &d.location.path), module: d.module.clone(), package: package_of(k, &d.package), item: Some(d.id.clone()), at: at(line) }
    };
    match op {
        MakeLib => {
            let name = spec.target.trim().strip_prefix("crate:").filter(|n| !n.contains('/')).ok_or_else(|| format!("`{}` is not a crate:<name> route", spec.target))?;
            let p = k.packages.get(&name.replace('-', "_")).ok_or_else(|| format!("`{}` is no crate of the workspace", spec.target))?;
            Ok(Landing { file: rel(root, &p.root.join("src/lib.rs")), module: p.name.replace('-', "_"), package: p.name.clone(), item: None, at: At::NewFile { parent: String::new() } })
        }
        ChangeDependency => {
            let changes = dependency_changes(k, root, spec)?;
            let one = changes.first().ok_or_else(|| format!("no crate of `{}` names {}", spec.target, spec.dependencies_named().iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", ")))?;
            let every = spec.target.trim() == WORKSPACE;
            Ok(Landing {
                file: one.manifest.clone(),
                module: if every { WORKSPACE.into() } else { one.package.clone() },
                package: if every { WORKSPACE.into() } else { one.package.clone() },
                item: None,
                at: At::Manifest,
            })
        }
        MakeStruct | MakeEnum | MakeFn | AddModule | Extract | AddDependency | DeclareFeature | RemoveDependency | RenamePaths => {
            let (package, file) = route_file(k, root, &spec.target).ok_or_else(|| format!("`{}` is not a crate:<name> or crate:<name>/<file>.rs route of the workspace", spec.target))?;
            if matches!(op, AddDependency | DeclareFeature | RemoveDependency | RenamePaths) {
                let m = k.packages.get(&package).map(|p| rel(root, &p.manifest_path)).unwrap_or_default();
                return Ok(Landing { file: m, module: package.clone(), package, item: None, at: At::Manifest });
            }
            let module = module_of_file(k, &file).map(|m| m.path.clone()).unwrap_or_else(|| package.clone());
            if matches!(op, AddModule | Extract) {
                let name = spec.module.as_ref().map(|m| m.name.clone()).unwrap_or_default();
                let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
                let new = if ["lib", "main", "mod"].contains(&stem.as_str()) { dir.join(format!("{name}.rs")) } else { dir.join(&stem).join(format!("{name}.rs")) };
                return Ok(Landing { file: rel(root, &new), module: format!("{module}::{name}"), package, item: None, at: At::NewFile { parent: rel(root, &file) } });
            }
            Ok(Landing { file: rel(root, &file), module, package, item: None, at: At::EndOfFile })
        }
        _ => {
            // a function a dependency's macro writes (a toolkit generator) is seen only when the scan read that
            // macro: say why it could not
            // a route that names its impl (`path[impl From<i64> for Level]`, two impls on one path): the declaration
            // at that function's place, and the route kept as the item, so the writer finds the same one
            if crate::route::split(&spec.target).1.is_some() {
                let f = crate::route::find_fn(&k.functions, &spec.target).map_err(|e| e.to_string())?;
                let (path, _) = crate::route::split(&spec.target);
                let d = k.items.iter().find(|d| d.id == path && d.location.line == f.location.line && d.location.path == f.location.path).ok_or_else(|| format!("`{}` names no item of the scan", spec.target))?;
                let mut l = at_item(d, |l| At::At { line: l });
                l.item = Some(spec.target.trim().to_string());
                return Ok(l);
            }
            let d = item_named(k, &spec.target).ok_or_else(|| match &k.dep_macro_note {
                Some(note) => format!("`{}` names no item of the scan ({note}: a function a dependency's macro writes is not seen until the lock file names the dependency; cargo writes it, and a transform that changes a manifest writes it too)", spec.target),
                None => format!("`{}` names no item of the scan", spec.target),
            })?;
            Ok(match op {
                AddField | AddVariant => at_item(d, |l| At::Inside { line: l }),
                AddMethod | AddConstructor => match k.impls_of(&d.id).into_iter().find(|i| i.trait_.is_none()) {
                    Some(i) => Landing { file: rel(root, &i.location.path), module: i.module.clone(), package: package_of(k, &i.package), item: Some(d.id.clone()), at: At::Inside { line: i.location.line.unwrap_or(1) } },
                    None => at_item(d, |l| At::After { line: l }),
                },
                MakeImpl | AddTraitImpl => at_item(d, |l| At::After { line: l }),
                AddDerive | AddAttribute | Gate | Anchor => at_item(d, |l| At::Above { line: l }),
                ChangeSignature | Rename | Move => at_item(d, |l| At::At { line: l }),
                AddTest => at_item(d, |l| At::After { line: l }),
                ReplaceBody | Inline | Wrap => at_item(d, |l| At::At { line: l }),
                _ => unreachable!("the place-targeting operations are matched above"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(name: &str) -> Piece {
        Piece { piece: name.into(), ..Piece::default() }
    }

    #[test]
    fn every_piece_is_a_toolkit_macro_or_a_step_and_its_template_reads() {
        let mut names: Vec<&str> = PIECES.iter().map(|p| p.name).collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), n, "one declaration per piece");
        for p in PIECES {
            for (s, _) in template_slots(p.template) {
                assert!(SLOTS.contains(&s.as_str()), "{}: slot {s} is not a field of Piece", p.name);
            }
        }
        assert_eq!(PIECES.iter().filter(|p| p.family != Family::Step).count(), 130);
    }

    /// Every toolkit piece is a `macro_rules!` of simple_impl 0.1.12 by that name, and every macro there is a piece:
    /// the vocabulary loses none and invents none. The source is the dev-dependency's, in cargo's registry.
    #[test]
    fn the_pieces_are_the_toolkit_exactly() {
        let home = std::env::var_os("CARGO_HOME").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cargo"));
        let src = std::fs::read_dir(home.join("registry/src"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path().join(format!("{}-{}", TOOLKIT.0, TOOLKIT.1)))
            .find(|p| p.is_dir())
            .expect("simple_impl 0.1.12 in cargo's registry (a dev-dependency)");
        let mut macros = Vec::new();
        for f in ["src/macros/mod.rs", "src/macros/toolkit.rs"] {
            let text = std::fs::read_to_string(src.join(f)).unwrap();
            for l in text.lines() {
                if let Some(rest) = l.strip_prefix("macro_rules! ") {
                    macros.push(rest.trim_end_matches(" {").trim().to_string());
                }
            }
        }
        macros.sort();
        let mut pieces: Vec<String> = PIECES.iter().filter(|p| p.family != Family::Step).map(|p| p.name.to_string()).collect();
        pieces.sort();
        assert_eq!(pieces, macros);
    }

    #[test]
    fn a_body_of_pieces_renders_as_toolkit_lines() {
        let f = FnSpec {
            name: "free".into(),
            vis: Some("pub".into()),
            receiver: Some("&self".into()),
            ret: Some("Result<usize, &'static str>".into()),
            doc: Some("Slots left in the pool.".into()),
            body: vec![
                Piece { condition: Some("self.size > 0".into()), error: Some("\"empty pool\"".into()), ..step("ensure") },
                Piece { option: Some("self.limit".into()), error: Some("\"no limit\"".into()), bind: Some("limit".into()), ..step("require_some") },
                Piece { conditions: vec!["self.used <= self.size".into(), "limit > 0".into()], errors: vec!["\"over\"".into(), "\"zero\"".into()], ..step("ensure_all") },
                Piece { value: Some("Ok(self.size - self.used)".into()), ..step("value") },
            ],
            ..FnSpec::default()
        };
        let text = render_fn(&f, "    ").unwrap();
        assert_eq!(
            text,
            "    /// Slots left in the pool.\n    pub fn free(&self) -> Result<usize, &'static str> {\n        simple_impl::ensure!(self.size > 0, \"empty pool\");\n        let limit = simple_impl::require_some!(self.limit, \"no limit\");\n        simple_impl::ensure_all!(self.used <= self.size => \"over\", limit > 0 => \"zero\");\n        Ok(self.size - self.used)\n    }\n"
        );
    }

    #[test]
    fn a_method_generator_makes_the_whole_function() {
        let f = FnSpec { name: "port".into(), vis: Some("pub".into()), receiver: Some("&self".into()), ret: Some("u16".into()), generator: Some(Piece { field: Some("port".into()), ..step("getter") }), ..FnSpec::default() };
        assert_eq!(render_fn(&f, "").unwrap(), "simple_impl::getter!(pub fn port(&self) -> u16 => port);\n");
        let c = FnSpec {
            name: "free".into(),
            vis: Some("pub".into()),
            receiver: Some("&self".into()),
            ret: Some("usize".into()),
            generator: Some(Piece { error_ty: Some("&'static str".into()), conditions: vec!["self.size > 0".into()], errors: vec!["\"empty\"".into()], value: Some("self.size - self.used".into()), ..step("checked_method") }),
            ..FnSpec::default()
        };
        assert_eq!(render_fn(&c, "").unwrap(), "simple_impl::checked_method!(pub fn free(&self) -> usize, &'static str; self.size > 0 => \"empty\"; => self.size - self.used);\n");
        let empty = FnSpec { name: "later".into(), doc: Some("Not written yet.".into()), ..FnSpec::default() };
        assert_eq!(render_fn(&empty, "").unwrap(), "/// Not written yet.\nfn later() {\n    todo!(\"Not written yet.\")\n}\n");
    }

    #[test]
    fn a_piece_is_refused_for_a_slot_it_lacks_or_one_it_does_not_take() {
        assert!(check_piece(&Piece { condition: Some("x".into()), ..step("ensure") }).unwrap_err().contains("needs `error`"));
        assert!(check_piece(&Piece { condition: Some("x".into()), error: Some("e".into()), field: Some("f".into()), ..step("ensure") }).unwrap_err().contains("does not take field"));
        assert!(check_piece(&step("shout")).unwrap_err().contains("not a toolkit piece"));
        assert!(check_piece(&Piece { conditions: vec!["a".into()], errors: vec![], ..step("ensure_all") }).is_err());
        assert_eq!(render_step(&Piece { condition: Some("done".into()), ..step("break_if") }).unwrap(), "simple_impl::break_if!(done);");
        assert_eq!(render_step(&Piece { condition: Some("done".into()), value: Some("1".into()), ..step("break_if") }).unwrap(), "simple_impl::break_if!(done => 1);");
        assert_eq!(render_step(&Piece { value: Some("x".into()), candidates: vec!["A".into(), "B".into()], bind: Some("ok".into()), ..step("is_one_of") }).unwrap(), "let ok = simple_impl::is_one_of!(x; A | B);");
    }

    #[test]
    fn a_spec_is_refused_without_what_its_operation_needs() {
        let e = ChangeSpec::from_toml("name = \"x\"\nobjective = \"o\"\noperation = \"add_field\"\ntarget = \"core_lib::Config\"\n").unwrap_err();
        assert!(e.to_string().contains("add_field needs field"), "{e}");
        let ok = ChangeSpec::from_toml("name = \"x\"\nobjective = \"o\"\noperation = \"add_field\"\ntarget = \"core_lib::Config\"\n[field]\nname = \"digest\"\nty = \"String\"\ndefault = \"String::new()\"\n").unwrap();
        assert_eq!(ok.op(), Operation::AddField);
        let text = ok.to_1o().unwrap_or_default();
        if !text.is_empty() {
            assert_eq!(ChangeSpec::from_1o(&text).unwrap(), ok);
        }
    }
}
