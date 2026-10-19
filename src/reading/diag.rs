//! liaise code tables, read from klepto's data: a `liaise_codes!` invocation's
//! tokens, and an enum deriving `LiaiseCodes` with its `#[liaise(..)]`
//! attributes on the enum and each variant. A code renders as liaise renders
//! it (`Liaise::code`: the prefix, then the number in four digits).

use super::facts::Facts;
use std::path::PathBuf;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Expr, Ident, Lit, LitStr, Meta, Token};

/// One declared code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// `PREFIX0003`, or `None` when the prefix or the number is a constant
    /// klepto's tokens do not give the value of.
    pub code: Option<String>,
    pub prefix: String,
    pub number: String,
    pub message: String,
    pub severity: Option<String>,
    /// The item that raises it: `crate::module::Code::Variant`.
    pub variant: String,
    pub file: PathBuf,
    pub line: u32,
    /// `liaise_codes!` or `LiaiseCodes`.
    pub declared_by: &'static str,
}

/// `liaise` renders a code as the prefix and the number in four digits.
pub fn render(prefix: &str, number: &str) -> Option<String> {
    let n: u16 = number.trim().parse().ok()?;
    Some(format!("{prefix}{n:04}"))
}

struct Entry {
    variant: Ident,
    number: String,
    message: String,
}

struct Table {
    prefix: String,
    name: Ident,
    entries: Vec<Entry>,
}

fn text_of(e: &Expr) -> String {
    match e {
        Expr::Lit(l) => match &l.lit {
            Lit::Str(s) => s.value(),
            Lit::Int(i) => i.base10_digits().to_string(),
            other => quote::ToTokens::to_token_stream(other).to_string(),
        },
        other => quote::ToTokens::to_token_stream(other).to_string(),
    }
}

impl Parse for Table {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        if key != "prefix" {
            return Err(syn::Error::new(key.span(), "a liaise_codes! table starts with `prefix:`"));
        }
        input.parse::<Token![:]>()?;
        let prefix: Expr = input.parse()?;
        let _ = input.parse::<Option<Token![,]>>()?;
        let _: syn::Visibility = input.parse()?;
        let name: Ident = input.parse()?;
        let body;
        syn::braced!(body in input);
        let mut entries = Vec::new();
        while !body.is_empty() {
            let variant: Ident = body.parse()?;
            body.parse::<Token![=]>()?;
            let number: Expr = body.parse()?;
            body.parse::<Token![=>]>()?;
            let message: LitStr = body.parse()?;
            entries.push(Entry { variant, number: text_of(&number), message: message.value() });
            let _ = body.parse::<Option<Token![,]>>()?;
        }
        Ok(Table { prefix: text_of(&prefix), name, entries })
    }
}

fn name_values(args: &str) -> Vec<(String, String)> {
    let Ok(metas) = Punctuated::<Meta, Token![,]>::parse_terminated.parse_str(args) else { return Vec::new() };
    metas
        .into_iter()
        .filter_map(|m| match m {
            Meta::NameValue(nv) => Some((nv.path.get_ident()?.to_string(), text_of(&nv.value))),
            _ => None,
        })
        .collect()
}

/// Every code table klepto saw, its codes in declaration order.
pub fn tables(f: &Facts) -> (Vec<Declared>, Vec<(PathBuf, u32, String)>) {
    let mut out = Vec::new();
    let mut unread = Vec::new();
    for m in &f.k.macros_inv {
        if m.name != "liaise_codes" {
            continue;
        }
        let Some(line) = m.location.line else { continue };
        match syn::parse_str::<Table>(&m.tokens) {
            Ok(t) => {
                let module = m.module_path.join("::");
                let package = f.package_of(&m.location.path).unwrap_or("").to_string();
                let base = [package.as_str(), module.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("::");
                for e in t.entries {
                    out.push(Declared {
                        code: render(&t.prefix, &e.number),
                        prefix: t.prefix.clone(),
                        number: e.number,
                        message: e.message,
                        severity: None,
                        variant: format!("{base}::{}::{}", t.name, e.variant),
                        file: m.location.path.clone(),
                        line,
                        declared_by: "liaise_codes!",
                    });
                }
            }
            Err(e) => unread.push((m.location.path.clone(), line, format!("liaise_codes! not read: {e}"))),
        }
    }
    let derives_codes = |r: &klepto::Resolution| {
        let text = match r {
            klepto::Resolution::Local(s) | klepto::Resolution::External(s) => s.as_str(),
            klepto::Resolution::Unresolved { raw, .. } => raw.as_str(),
        };
        text.rsplit("::").next() == Some("LiaiseCodes")
    };
    for t in f.k.types.iter().filter(|t| t.derives.iter().any(derives_codes)) {
        let enum_attr = f.k.attrs.iter().find(|a| a.path == "liaise" && a.enclosing_item.as_deref() == Some(t.id.as_str()));
        let kv = enum_attr.map(|a| name_values(&a.args)).unwrap_or_default();
        let prefix = kv.iter().find(|(k, _)| k == "prefix").map(|(_, v)| v.clone()).unwrap_or_default();
        for v in &t.variants {
            let Some((_, args)) = v.attrs.iter().find(|(p, _)| p == "liaise") else { continue };
            let kv = name_values(args);
            let get = |k: &str| kv.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
            let number = get("code").unwrap_or_default();
            out.push(Declared {
                code: render(&prefix, &number),
                prefix: prefix.clone(),
                number,
                message: get("msg").unwrap_or_default(),
                severity: get("severity"),
                variant: format!("{}::{}", t.id, v.name),
                file: v.location.as_ref().map(|l| l.path.clone()).unwrap_or_else(|| t.location.path.clone()),
                line: v.location.as_ref().and_then(|l| l.line).or(t.location.line).unwrap_or(0),
                declared_by: "LiaiseCodes",
            });
        }
    }
    (out, unread)
}

/// The codes that appear in a text (a journal, a log): every declared code
/// whose rendering occurs in it, in the order of first appearance.
pub fn codes_in<'d>(text: &str, declared: &'d [Declared]) -> Vec<&'d Declared> {
    let mut found: Vec<(usize, &Declared)> = declared.iter().filter_map(|d| Some((text.find(d.code.as_deref()?)?, d))).collect();
    found.sort_by_key(|(at, d)| (*at, d.variant.clone()));
    found.into_iter().map(|(_, d)| d).collect()
}
