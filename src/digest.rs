//! Every digest custodian makes, one way: `stable_digest::StableDigest`
//! (SHA-256, lowercase hex) over a canonical byte form declared in receipt_history. A
//! serialized value's canonical form is its JSON with object keys sorted
//! (serde_json's maps are ordered), so the same value always has the same
//! digest; what is digested never holds a timestamp of the digesting.
//! The functions are receipt_history's, so a receipt, a count and a reading
//! digest the same bytes the same way in every crate that keeps them.

pub use receipt_history::digest::{canonical, of, of_value, short, tagged, ALGORITHM};
