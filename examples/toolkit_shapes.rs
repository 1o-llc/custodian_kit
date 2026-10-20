//! Makes the toolkit's shape catalog (`schema/toolkit/simple_impl-0.1.12.shapes.json`): writes the probe crate
//! (one call of each toolkit piece on placeholders) under DIR, expands it with klepto (in rounds) and with
//! `cargo expand` (a declared line), and writes the catalog.
//!
//!     cargo run --example toolkit_shapes -- <DIR, a new directory>
use custodian_kit::toolkit::{probe, Position};

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("a new directory for the probe crate"));
    let cat = probe::build_catalog(&dir).expect("the probe expands");
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/toolkit/simple_impl-0.1.12.shapes.json");
    std::fs::write(&out, serde_json::to_string_pretty(&cat).unwrap() + "\n").unwrap();
    let by = |p: Position| cat.shapes.iter().filter(|s| s.position == p).count();
    let cargo = cat.shapes.iter().filter(|s| s.expanded_by == "cargo expand").count();
    let differs: Vec<String> = cat.shapes.iter().filter(|s| s.klepto_same == Some(false)).map(|s| format!("{} {}", s.piece, s.variant).trim().to_string()).collect();
    println!(
        "{} shapes ({} statements, {} values, {} functions, {} impl functions); {cargo} from cargo expand, {} from klepto; klepto expands {} of cargo's differently: {}\nwritten {}",
        cat.shapes.len(),
        by(Position::Step),
        by(Position::Value),
        by(Position::Function),
        by(Position::ImplFn),
        cat.shapes.len() - cargo,
        differs.len(),
        differs.join(", "),
        out.display()
    );
}
