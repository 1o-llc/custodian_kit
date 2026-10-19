//! Prints the vocabulary tables the README carries (a test holds them equal):
//! `cargo run --example vocabulary`.

use custodian_kit::api::{DeclaredRole, ReadingKind};

fn main() {
    println!("{}", ReadingKind::doc_table());
    println!("{}", DeclaredRole::doc_table());
}
