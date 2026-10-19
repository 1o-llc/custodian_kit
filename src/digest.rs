//! Every digest custodian makes, one way: `stable_digest::StableDigest`
//! (SHA-256, lowercase hex) over a canonical byte form declared here. A
//! serialized value's canonical form is its JSON with object keys sorted
//! (serde_json's maps are ordered), so the same value always has the same
//! digest; what is digested never holds a timestamp of the digesting.

use serde::Serialize;
use stable_digest::StableDigest;

/// The algorithm every digest names.
pub const ALGORITHM: &str = "sha256";

/// The digest of bytes, lowercase hex.
pub fn of(bytes: impl AsRef<[u8]>) -> String {
    StableDigest::sha256(bytes).as_str().to_string()
}

/// `sha256:<hex>`, the form a receipt field carries.
pub fn tagged(bytes: impl AsRef<[u8]>) -> String {
    format!("{ALGORITHM}:{}", of(bytes))
}

/// A value's canonical bytes: its JSON, keys sorted.
pub fn canonical<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
    let v = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
    serde_json::to_vec(&v).unwrap_or_default()
}

/// `sha256:<hex>` of a value's canonical bytes.
pub fn of_value<T: Serialize + ?Sized>(value: &T) -> String {
    tagged(canonical(value))
}

/// The first `n` hex characters of a tagged or plain digest, for an id.
pub fn short(digest: &str, n: usize) -> String {
    let hex = digest.rsplit(':').next().unwrap_or(digest);
    hex.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_forms_do_not_depend_on_field_order() {
        let a: serde_json::Value = serde_json::from_str(r#"{"b":1,"a":[2,3]}"#).unwrap();
        let b: serde_json::Value = serde_json::from_str(r#"{"a":[2,3],"b":1}"#).unwrap();
        assert_eq!(of_value(&a), of_value(&b));
        assert_eq!(of_value(&a).len(), "sha256:".len() + 64);
        assert_eq!(of("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
