//! custodian's documents in 1o: the reading spec, the job file and the paths
//! table, each declared once as a 1o schema shipped with this crate
//! (`schema/<modifier>/v1.schema.json`), parsed by core-1o and checked against
//! its schema by core-1o's validator, then read into the same in-memory form
//! the TOML documents read into. One internal form: a spec's digest is over
//! that form, never over the text.
//!
//! A 1o document is `{ <modifier> { key value ... } }`; a block is a table and
//! a key written several times is a list (core-1o keeps repeated keys in
//! order). Which keys are lists is the Rust form's to say: a `Vec` field takes
//! every occurrence, a single field refuses a second one.
//!
//! A file is named the 1o way, `v1.<name>.1o.<modifier>`, so the language
//! server finds its schema: `custodian init` copies the schemas to
//! `.custodian/schema/` and lists that folder in the project's
//! `oneo.project.json`, where core-1o's workspace looks.

use crate::reading::refusal::ReadingRefusal;
use crate::Result;
use core_1o::engine::{validate_ast, Lexer, Node, NodeValue, Parser};
use serde::de::{self, DeserializeOwned, IntoDeserializer, Visitor};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// The documents custodian keeps in 1o, by their modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doc {
    Reading,
    Job,
    Paths,
    /// A change spec (`crate::change`).
    Change,
    /// A linkage record (`crate::linkage`).
    Linkage,
}

impl Doc {
    pub const ALL: &'static [Doc] = &[Doc::Reading, Doc::Job, Doc::Paths, Doc::Change, Doc::Linkage];

    /// The modifier: the file name's last part and the document's top key.
    pub fn modifier(self) -> &'static str {
        match self {
            Doc::Reading => "reading",
            Doc::Job => "job",
            Doc::Paths => "paths",
            Doc::Change => "change",
            Doc::Linkage => "linkage",
        }
    }

    /// The shipped schema, as JSON text.
    pub fn schema_text(self) -> &'static str {
        match self {
            Doc::Reading => include_str!("../schema/reading/v1.schema.json"),
            Doc::Job => include_str!("../schema/job/v1.schema.json"),
            Doc::Paths => include_str!("../schema/paths/v1.schema.json"),
            Doc::Change => include_str!("../schema/change/v1.schema.json"),
            Doc::Linkage => include_str!("../schema/linkage/v1.schema.json"),
        }
    }

    pub fn schema(self) -> serde_json::Value {
        serde_json::from_str(self.schema_text()).expect("a shipped schema is JSON")
    }

    /// `v1.<name>.1o.<modifier>`.
    pub fn file_name(self, name: &str) -> String {
        format!("{VERSION}.{name}.1o.{}", self.modifier())
    }

    /// The document a file name names, and the name in it.
    pub fn of_file(path: &Path) -> Option<(Doc, String)> {
        let rec = core_1o::utils::parse_filename(path.file_name()?.to_str()?)?;
        let doc = Doc::ALL.iter().copied().find(|d| d.modifier() == rec.modifier)?;
        (rec.version == VERSION).then_some((doc, rec.name))
    }
}

/// The schema version every custodian document is written at.
pub const VERSION: &str = "v1";

fn refuse(what: &str, text: &str, at: usize, message: impl std::fmt::Display) -> crate::CustodianError {
    // core-1o's spans count characters
    let before: String = text.chars().take(at).collect();
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
    ReadingRefusal::OneoInvalid.refuse(format!("{what}:{line}:{col}: {message}"))
}

/// A document's text read into `T`: parsed by core-1o, checked against the
/// shipped schema, then read into the Rust form. `what` names it in a refusal.
pub fn parse<T: DeserializeOwned>(doc: Doc, what: &str, text: &str) -> Result<T> {
    let nodes = Parser::new(Lexer::new(text).tokenize()).parse().map_err(|e| {
        let d = core_1o::engine::EngineDiagnostic::from(&e);
        refuse(what, text, d.span.start, d.message)
    })?;
    let mut diags = Vec::new();
    validate_ast(&nodes, &doc.schema(), &mut diags);
    if let Some(d) = diags.first() {
        let more = if diags.len() > 1 { format!(" (and {} more)", diags.len() - 1) } else { String::new() };
        return Err(refuse(what, text, d.span.start, format!("{}{more}", d.message)));
    }
    let top = nodes.iter().find(|n| n.key == doc.modifier()).ok_or_else(|| refuse(what, text, 0, format!("no `{}` block", doc.modifier())))?;
    let NodeValue::Block(body) = &top.value else {
        return Err(refuse(what, text, top.span.start, format!("`{}` is not a block", doc.modifier())));
    };
    T::deserialize(BlockDe(body)).map_err(|e| refuse(what, text, top.span.start, e))
}

/// `value` as a 1o document. A string holding a double quote is refused:
/// core-1o's strings have no escapes, so it could not be read back.
pub fn render<T: Serialize>(doc: Doc, value: &T) -> Result<String> {
    let v = serde_json::to_value(value).map_err(|e| ReadingRefusal::Write.refuse(e))?;
    let mut out = String::from("{\n");
    out.push_str(&format!("  {} {{\n", doc.modifier()));
    let serde_json::Value::Object(map) = v else {
        return Err(ReadingRefusal::Write.refuse("a 1o document is a block"));
    };
    body(&map, 2, &mut out)?;
    out.push_str("  }\n}\n");
    Ok(out)
}

fn body(map: &serde_json::Map<String, serde_json::Value>, depth: usize, out: &mut String) -> Result<()> {
    for (k, v) in map {
        entry(k, v, depth, out)?;
    }
    Ok(())
}

fn entry(key: &str, v: &serde_json::Value, depth: usize, out: &mut String) -> Result<()> {
    use serde_json::Value as J;
    let pad = "  ".repeat(depth);
    match v {
        J::Null => {}
        J::Array(items) => {
            for i in items {
                entry(key, i, depth, out)?;
            }
        }
        J::Object(m) => {
            out.push_str(&format!("{pad}{key} {{\n"));
            body(m, depth + 1, out)?;
            out.push_str(&format!("{pad}}}\n"));
        }
        J::String(s) => {
            if s.contains('"') {
                return Err(ReadingRefusal::OneoUnwritable.refuse(format!("{key}: {s}")));
            }
            out.push_str(&format!("{pad}{key} \"{s}\"\n"));
        }
        J::Bool(b) => out.push_str(&format!("{pad}{key} {b}\n")),
        J::Number(n) => {
            if n.as_i64().is_some_and(|i| i < 0) {
                return Err(ReadingRefusal::OneoUnwritable.refuse(format!("{key}: {n} (core-1o reads no negative numbers)")));
            }
            out.push_str(&format!("{pad}{key} {n}\n"));
        }
    }
    Ok(())
}

/// The shipped schemas copied to `<dir>/<modifier>/v1.schema.json`, the
/// layout core-1o's workspace reads. Returns the files written.
pub fn write_schemas(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for d in Doc::ALL {
        let p = dir.join(d.modifier()).join(format!("{VERSION}.schema.json"));
        crate::store::write_whole(&p, d.schema_text().as_bytes())?;
        out.push(p);
    }
    Ok(out)
}

/// The project's `oneo.project.json` with `rel` among its schema
/// directories: made when missing, its other keys kept when present.
pub fn declare_schema_dir(root: &Path, rel: &str) -> Result<PathBuf> {
    let p = root.join("oneo.project.json");
    let mut v: serde_json::Value = match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| ReadingRefusal::OneoInvalid.refuse(format!("{}: {e}", p.display())))?,
        Err(_) => serde_json::json!({ "formatVersion": 1, "schemaDirectories": [] }),
    };
    let obj = v.as_object_mut().ok_or_else(|| ReadingRefusal::OneoInvalid.refuse(format!("{}: not an object", p.display())))?;
    obj.entry("formatVersion").or_insert(serde_json::json!(1));
    let dirs = obj.entry("schemaDirectories").or_insert(serde_json::json!([]));
    let list = dirs.as_array_mut().ok_or_else(|| ReadingRefusal::OneoInvalid.refuse(format!("{}: schemaDirectories is not a list", p.display())))?;
    if !list.iter().any(|d| d.as_str() == Some(rel)) {
        list.push(serde_json::json!(rel));
    }
    crate::store::write_whole(&p, (serde_json::to_string_pretty(&v).map_err(|e| ReadingRefusal::Write.refuse(e))? + "\n").as_bytes())?;
    Ok(p)
}

// ---------------------------------------------------------------------------
// The Rust form from core-1o's nodes: a block is a map of its keys, each key
// the occurrences written for it.

type DeError = de::value::Error;

struct BlockDe<'a>(&'a [Node]);

/// One key's occurrences, in order.
struct Occurrences<'a> {
    key: &'a str,
    values: Vec<&'a NodeValue>,
}

struct ValueDe<'a>(&'a NodeValue);

fn grouped(nodes: &[Node]) -> Vec<Occurrences<'_>> {
    let mut out: Vec<Occurrences<'_>> = Vec::new();
    for n in nodes {
        match out.iter_mut().find(|o| o.key == n.key) {
            Some(o) => o.values.push(&n.value),
            None => out.push(Occurrences { key: &n.key, values: vec![&n.value] }),
        }
    }
    out
}

struct BlockAccess<'a> {
    groups: std::vec::IntoIter<Occurrences<'a>>,
    next: Option<Occurrences<'a>>,
}

impl<'de, 'a> de::MapAccess<'de> for BlockAccess<'a> {
    type Error = DeError;
    fn next_key_seed<K: de::DeserializeSeed<'de>>(&mut self, seed: K) -> std::result::Result<Option<K::Value>, DeError> {
        match self.groups.next() {
            Some(g) => {
                let k = seed.deserialize(g.key.to_string().into_deserializer())?;
                self.next = Some(g);
                Ok(Some(k))
            }
            None => Ok(None),
        }
    }
    fn next_value_seed<V: de::DeserializeSeed<'de>>(&mut self, seed: V) -> std::result::Result<V::Value, DeError> {
        seed.deserialize(self.next.take().ok_or_else(|| de::Error::custom("a value without its key"))?)
    }
}

impl<'de, 'a> de::Deserializer<'de> for BlockDe<'a> {
    type Error = DeError;
    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, DeError> {
        v.visit_map(BlockAccess { groups: grouped(self.0).into_iter(), next: None })
    }
    serde::forward_to_deserialize_any! { bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any }
}

impl<'a> Occurrences<'a> {
    fn one(&self) -> std::result::Result<ValueDe<'a>, DeError> {
        match self.values.as_slice() {
            [v] => Ok(ValueDe(v)),
            many => Err(de::Error::custom(format!("`{}` is written {} times; it takes one value", self.key, many.len()))),
        }
    }
}

struct Seq<'a>(std::vec::IntoIter<&'a NodeValue>);

impl<'de, 'a> de::SeqAccess<'de> for Seq<'a> {
    type Error = DeError;
    fn next_element_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> std::result::Result<Option<T::Value>, DeError> {
        self.0.next().map(|v| seed.deserialize(ValueDe(v))).transpose()
    }
}

macro_rules! one_then {
    ($($m:ident),*) => {
        $(fn $m<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, DeError> {
            self.one()?.$m(v)
        })*
    };
}

impl<'de, 'a> de::Deserializer<'de> for Occurrences<'a> {
    type Error = DeError;
    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, DeError> {
        if self.values.len() == 1 {
            ValueDe(self.values[0]).deserialize_any(v)
        } else {
            v.visit_seq(Seq(self.values.into_iter()))
        }
    }
    fn deserialize_seq<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, DeError> {
        v.visit_seq(Seq(self.values.into_iter()))
    }
    fn deserialize_option<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, DeError> {
        v.visit_some(self)
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, v: V) -> std::result::Result<V::Value, DeError> {
        v.visit_newtype_struct(self)
    }
    fn deserialize_struct<V: Visitor<'de>>(self, name: &'static str, fields: &'static [&'static str], v: V) -> std::result::Result<V::Value, DeError> {
        self.one()?.deserialize_struct(name, fields, v)
    }
    fn deserialize_enum<V: Visitor<'de>>(self, name: &'static str, variants: &'static [&'static str], v: V) -> std::result::Result<V::Value, DeError> {
        self.one()?.deserialize_enum(name, variants, v)
    }
    one_then! { deserialize_bool, deserialize_i8, deserialize_i16, deserialize_i32, deserialize_i64, deserialize_u8, deserialize_u16, deserialize_u32, deserialize_u64, deserialize_f32, deserialize_f64, deserialize_char, deserialize_str, deserialize_string, deserialize_map, deserialize_identifier }
    serde::forward_to_deserialize_any! { i128 u128 bytes byte_buf unit unit_struct tuple tuple_struct ignored_any }
}

impl<'de, 'a> de::Deserializer<'de> for ValueDe<'a> {
    type Error = DeError;
    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, DeError> {
        match self.0 {
            NodeValue::Block(nodes) => BlockDe(nodes).deserialize_any(v),
            NodeValue::String(s) => v.visit_str(s),
            NodeValue::Number(n) => v.visit_i64(*n),
            NodeValue::Float(f) => v.visit_f64(*f),
            NodeValue::Bool(b) => v.visit_bool(*b),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, DeError> {
        v.visit_some(self)
    }
    fn deserialize_seq<V: Visitor<'de>>(self, v: V) -> std::result::Result<V::Value, DeError> {
        // one value where a list is taken: a list of one
        v.visit_seq(Seq(vec![self.0].into_iter()))
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, v: V) -> std::result::Result<V::Value, DeError> {
        v.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(self, _name: &'static str, _variants: &'static [&'static str], v: V) -> std::result::Result<V::Value, DeError> {
        match self.0 {
            NodeValue::String(s) => v.visit_enum(s.as_str().into_deserializer()),
            _ => Err(de::Error::custom("expected a word")),
        }
    }
    serde::forward_to_deserialize_any! { bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit unit_struct tuple tuple_struct map struct identifier ignored_any }
}
