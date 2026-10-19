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
//! simple_impl 0.1.11 toolkit pieces (`ensure`, `require_some`, `push_if`,
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
        AddDependency = "add_dependency" : "a dependency line in a crate's Cargo.toml (target: crate:<name>)",
        AddTest = "add_test" : "a test function (target: the function it proves): in the file's tests module, its body from toolkit pieces",
        AddConstructor = "add_constructor" : "a constructor for a type (target: the type): `Self { field: param, .. }`, or a toolkit constructor generator",
        AddTraitImpl = "add_trait_impl" : "`impl Trait for Type` (target: the type): a toolkit impl generator, or the trait's functions",
        ChangeSignature = "change_signature" : "a function's signature changed (target: the function): a new parameter with the expression its call sites pass, or a new name",
        Rename = "rename" : "an item renamed (target: the item) at its declaration and every use klepto resolves to it",
        Move = "move" : "an item moved to another module (target: the item, `to`: the module route), re-exported where it was so its paths still resolve",
        Extract = "extract" : "items moved out of a file into a new module of their own (`items`, `module`), re-exported where they were",
        Gate = "gate" : "an item put behind a crate feature the crate declares (target: the item, `feature`)",
        Anchor = "anchor" : "a linkage record's comment anchor placed above an item (target: the item, `linkage`: the record's name)",
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
            AddDependency => &["dependency"],
            AddTraitImpl => &["trait"],
            ChangeSignature => &["function"],
            Rename => &["new_name"],
            Move => &["to"],
            Extract => &["items", "module"],
            Gate => &["feature"],
            Anchor => &["linkage"],
        }
    }

    /// Whether the operation's target is an existing item (else a place: a module route, a crate).
    pub fn targets_an_item(self) -> bool {
        use Operation::*;
        !matches!(self, MakeStruct | MakeEnum | MakeFn | AddModule | AddDependency | Extract)
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_name: Option<String>,
    /// `move`: the module route the item goes to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// `extract`: the items moved out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    /// `gate`: the crate feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
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

    /// The spec as a 1o document (refused when a value holds a double quote: core-1o has no escapes).
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
            "dependency" => self.dependency.is_some(),
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
        }
        out
    }
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
        /// Every piece a spec may name: the toolkit of simple_impl 0.1.11 (each macro by its name) and the three
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
pub const TOOLKIT: (&str, &str) = ("simple_impl", "0.1.11");

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
    let mut out = doc_lines(f.doc.as_deref(), indent);
    if let Some(g) = &f.generator {
        let d = check_piece(g)?;
        let text = match d.family {
            Family::Method => {
                // the macro takes the signature without its return type: the template writes `-> R` where the macro wants it
                let mut sig_only = f.clone();
                sig_only.ret = None;
                sig_only.doc = None;
                let sig = if d.name == "forward_trait_method" { signature(&FnSpec { vis: None, ..sig_only }) } else { signature(&sig_only) };
                let attrs: String = f.attrs.iter().map(|a| format!("{a} ")).collect();
                format!("{indent}{}({attrs}{});\n", mac(d.name), fill(d.template, g, &sig, f.ret.as_deref()))
            }
            Family::Builder => format!("{indent}{}({});\n", mac(d.name), fill(d.template, g, "", None)),
            _ => return Err(format!("{} does not make a function", d.name)),
        };
        out.push_str(&text);
        return Ok(out);
    }
    for a in &f.attrs {
        out.push_str(&format!("{indent}{a}\n"));
    }
    out.push_str(&format!("{indent}{} {{\n", signature(f)));
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
    out.push_str(&format!("{indent}}}\n"));
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
    /// A new file, its `mod` line in `parent`.
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

/// Where `spec`'s operation lands in the scan, or why it cannot.
pub fn landing(k: &klepto::Klepto, root: &Path, spec: &ChangeSpec) -> std::result::Result<Landing, String> {
    use Operation::*;
    let op = spec.op();
    let at_item = |d: &klepto::ItemDecl, at: fn(u32) -> At| -> Landing {
        let line = d.location.line.unwrap_or(1);
        Landing { file: rel(root, &d.location.path), module: d.module.clone(), package: d.package.clone(), item: Some(d.id.clone()), at: at(line) }
    };
    match op {
        MakeStruct | MakeEnum | MakeFn | AddModule | Extract | AddDependency => {
            let (package, file) = route_file(k, root, &spec.target).ok_or_else(|| format!("`{}` is not a crate:<name> or crate:<name>/<file>.rs route of the workspace", spec.target))?;
            if op == AddDependency {
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
            let d = item_named(k, &spec.target).ok_or_else(|| format!("`{}` names no item of the scan", spec.target))?;
            Ok(match op {
                AddField | AddVariant => at_item(d, |l| At::Inside { line: l }),
                AddMethod | AddConstructor => match k.impls_of(&d.id).into_iter().find(|i| i.trait_.is_none()) {
                    Some(i) => Landing { file: rel(root, &i.location.path), module: i.module.clone(), package: i.package.clone(), item: Some(d.id.clone()), at: At::Inside { line: i.location.line.unwrap_or(1) } },
                    None => at_item(d, |l| At::After { line: l }),
                },
                MakeImpl | AddTraitImpl => at_item(d, |l| At::After { line: l }),
                AddDerive | AddAttribute | Gate | Anchor => at_item(d, |l| At::Above { line: l }),
                ChangeSignature | Rename | Move => at_item(d, |l| At::At { line: l }),
                AddTest => at_item(d, |l| At::After { line: l }),
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
        assert_eq!(PIECES.iter().filter(|p| p.family != Family::Step).count(), 123);
    }

    /// Every toolkit piece is a `macro_rules!` of simple_impl 0.1.11 by that name, and every macro there is a piece:
    /// the vocabulary loses none and invents none. The source is the dev-dependency's, in cargo's registry.
    #[test]
    fn the_pieces_are_the_toolkit_exactly() {
        let home = std::env::var_os("CARGO_HOME").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cargo"));
        let src = std::fs::read_dir(home.join("registry/src"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path().join(format!("{}-{}", TOOLKIT.0, TOOLKIT.1)))
            .find(|p| p.is_dir())
            .expect("simple_impl 0.1.11 in cargo's registry (a dev-dependency)");
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
