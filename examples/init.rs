//! `Store::init` at a root, agent layout, as `custodian agent init` calls it,
//! printing what it made: `cargo run --example init -- <root>`.

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("a root"));
    let (store, made) = custodian_kit::store::Store::init(&root, custodian_kit::store::Mode::Agent).unwrap_or_else(|e| panic!("{e}"));
    for p in made {
        println!("{}", p.strip_prefix(&store.root).unwrap_or(&p).display());
    }
}
