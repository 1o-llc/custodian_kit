//! Prints the vocabulary tables the README carries (a test holds them equal):
//! `cargo run --example vocabulary`.

use custodian_kit::api::{DeclaredRole, Depth, ReadingKind};
use custodian_kit::reading::requirements::{Basis, Check, Face, ReadFrom, Severity, Status};

fn main() {
    println!("{}", ReadingKind::doc_table());
    println!("{}", DeclaredRole::doc_table());
    println!("{}", Depth::doc_table());
    println!("{}", Face::doc_table());
    println!("{}", Basis::doc_table());
    println!("{}", Status::doc_table());
    println!("{}", Severity::doc_table());
    println!("{}", ReadFrom::doc_table());
    println!("{}", Check::doc_table());
}
