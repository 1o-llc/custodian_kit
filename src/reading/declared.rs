//! declared_in: the names TOML declaration files hold at a key path, with
//! the file and line each one is written on (toml_edit keeps the spans).

use super::refusal::ReadingRefusal;
use super::spec::{DeclaredRole, DeclaredSource};
use crate::Result;
use crate::paths::{PathsTable, Resolving};
use std::ops::Range;
use std::path::Path;
use toml_edit::{ImDocument, Item, Table, TableLike, Value};

/// One name a declaration file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// Relative to the project root, `/`-separated.
    pub file: String,
    pub line: u32,
    /// The entry's own name (its `entry` key, else the file stem).
    pub entry: String,
    /// The key path as it matched, with array indexes (`nodes[2].answers.X`).
    pub at: String,
    pub role: DeclaredRole,
    pub value: String,
}

#[derive(Debug, Clone)]
enum Seg {
    Field { name: String, each: bool },
    Keys,
}

fn parse_key_path(key: &str) -> Result<Vec<Seg>> {
    let parts: Vec<&str> = key.split('.').map(str::trim).collect();
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        if *p == "*" {
            if i + 1 != parts.len() {
                return Err(ReadingRefusal::KeyPath.refuse(format!("`{key}`: `*` (a table's keys) comes last")));
            }
            out.push(Seg::Keys);
            continue;
        }
        let (name, each) = match p.strip_suffix("[]") {
            Some(n) => (n, true),
            None => (*p, false),
        };
        if name.is_empty() || name.contains(['[', ']', '*']) {
            return Err(ReadingRefusal::KeyPath.refuse(format!("`{key}`: segment `{p}`")));
        }
        out.push(Seg::Field { name: name.to_string(), each });
    }
    Ok(out)
}

/// A node of the document: an item, a value inside an array, or a table of
/// an array of tables.
#[derive(Clone, Copy)]
enum Node<'a> {
    I(&'a Item),
    V(&'a Value),
    T(&'a Table),
}

impl<'a> Node<'a> {
    fn table(self) -> Option<&'a dyn TableLike> {
        match self {
            Node::I(i) => i.as_table_like(),
            Node::V(v) => v.as_inline_table().map(|t| t as &dyn TableLike),
            Node::T(t) => Some(t as &dyn TableLike),
        }
    }

    /// Elements of an array or an array of tables.
    fn elements(self) -> Vec<Node<'a>> {
        match self {
            Node::I(Item::ArrayOfTables(a)) => a.iter().map(Node::T).collect(),
            Node::I(Item::Value(Value::Array(a))) | Node::V(Value::Array(a)) => a.iter().map(Node::V).collect(),
            _ => Vec::new(),
        }
    }

    /// The strings this node holds, with their spans.
    fn strings(self) -> Vec<(String, Option<Range<usize>>)> {
        let v = match self {
            Node::I(Item::Value(v)) | Node::V(v) => v,
            _ => return Vec::new(),
        };
        match v {
            Value::String(s) => vec![(s.value().clone(), s.span())],
            Value::Array(a) => a.iter().flat_map(|e| Node::V(e).strings()).collect(),
            _ => Vec::new(),
        }
    }
}

struct Hit {
    at: String,
    value: String,
    span: Option<Range<usize>>,
}

fn walk(node: Node<'_>, segs: &[Seg], at: &str, out: &mut Vec<Hit>) {
    let Some((seg, rest)) = segs.split_first() else {
        for (value, span) in node.strings() {
            out.push(Hit { at: at.to_string(), value, span });
        }
        return;
    };
    match seg {
        Seg::Keys => {
            let Some(t) = node.table() else { return };
            for (k, _) in t.iter() {
                let span = t.get_key_value(k).and_then(|(key, item)| key.span().or_else(|| item.span()));
                out.push(Hit { at: join(at, k), value: k.to_string(), span });
            }
        }
        Seg::Field { name, each } => {
            let Some(child) = node.table().and_then(|t| t.get(name)) else { return };
            let here = join(at, name);
            if *each {
                for (i, e) in Node::I(child).elements().into_iter().enumerate() {
                    walk(e, rest, &format!("{here}[{i}]"), out);
                }
            } else {
                walk(Node::I(child), rest, &here, out);
            }
        }
    }
}

fn join(at: &str, k: &str) -> String {
    if at.is_empty() { k.to_string() } else { format!("{at}.{k}") }
}

fn line_of(text: &str, offset: usize) -> u32 {
    text.as_bytes()[..offset.min(text.len())].iter().filter(|b| **b == b'\n').count() as u32 + 1
}

/// Every name the sources declare, sorted by file, line, key path. Each
/// source's files are routes (`decl:<name>`, or in free mode a raw glob),
/// resolved through the declared table.
pub fn read_declared(root: &Path, sources: &[DeclaredSource], table: &PathsTable, how: Resolving) -> Result<Vec<Declared>> {
    let mut out = Vec::new();
    for src in sources {
        let segs = parse_key_path(&src.key)?;
        let entry_segs = src.entry.as_deref().map(parse_key_path).transpose()?;
        let mut files = Vec::new();
        for f in &src.files {
            files.extend(table.resolve(f, how)?.files(root)?);
        }
        files.sort();
        files.dedup();
        for path in files {
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            let text = std::fs::read_to_string(&path).map_err(|e| ReadingRefusal::DeclaredFile.refuse(format!("{rel}: {e}")))?;
            let doc = ImDocument::parse(text.as_str()).map_err(|e| ReadingRefusal::DeclaredFile.refuse(format!("{rel}: {}", e.to_string().trim())))?;
            let top = Node::T(doc.as_table());
            let entry = match &entry_segs {
                Some(es) => {
                    let mut hits = Vec::new();
                    walk(top, es, "", &mut hits);
                    hits.into_iter().next().map(|h| h.value)
                }
                None => None,
            }
            .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            let mut hits = Vec::new();
            walk(top, &segs, "", &mut hits);
            for h in hits {
                out.push(Declared {
                    file: rel.clone(),
                    line: h.span.map(|s| line_of(&text, s.start)).unwrap_or(0),
                    entry: entry.clone(),
                    at: h.at,
                    role: src.role,
                    value: h.value,
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.file, a.line, &a.at, &a.value).cmp(&(&b.file, b.line, &b.at, &b.value)));
    out.dedup();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_paths_reach_fields_arrays_and_table_keys() {
        let dir = std::env::temp_dir().join(format!("custodian_declared_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("cat")).unwrap();
        std::fs::write(
            dir.join("cat/a.toml"),
            "id = \"one\"\nbinary = \"oned\"\nbinds = [\"/run/one.sock\", \"/run/two.sock\"]\n\n[env.ONE_KEY]\ndefault = \"x\"\n\n[[nodes]]\nnode = \"n1\"\n[nodes.answers]\nONE_ANSWER = \"y\"\n",
        )
        .unwrap();
        let src = |key: &str, role| DeclaredSource { files: vec!["cat/*.toml".into()], key: key.into(), role, entry: Some("id".into()) };
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"declared_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[lib]\npath = \"lib.rs\"\n").unwrap();
        std::fs::write(dir.join("lib.rs"), "").unwrap();
        let table = PathsTable::load(&dir, None).unwrap();
        let got = read_declared(
            &dir,
            &[src("binary", DeclaredRole::Binary), src("binds[]", DeclaredRole::Socket), src("env.*", DeclaredRole::Env), src("nodes[].answers.*", DeclaredRole::Env)],
            &table,
            Resolving::Free,
        )
        .unwrap();
        let flat: Vec<(String, u32, String, String)> = got.iter().map(|d| (d.entry.clone(), d.line, d.at.clone(), d.value.clone())).collect();
        assert_eq!(
            flat,
            vec![
                ("one".into(), 2, "binary".into(), "oned".into()),
                ("one".into(), 3, "binds[0]".into(), "/run/one.sock".into()),
                ("one".into(), 3, "binds[1]".into(), "/run/two.sock".into()),
                ("one".into(), 5, "env.ONE_KEY".into(), "ONE_KEY".into()),
                ("one".into(), 11, "nodes[0].answers.ONE_ANSWER".into(), "ONE_ANSWER".into()),
            ]
        );
        assert!(parse_key_path("env.*.x").is_err());
    }
}
