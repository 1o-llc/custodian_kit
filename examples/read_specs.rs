//! Answers spec files over one scan of a workspace, as a job would (declared
//! routes only), and prints each reading's counts and its first rows of the
//! kinds named: `cargo run --release --example read_specs -- <root> <spec>... [--rows kind,kind]`.
//! It writes nothing.

use custodian_kit::api::{self, ReadingSpec, Resolving, Workspace};
use std::path::PathBuf;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let rows: Vec<String> = match args.iter().position(|a| a == "--rows") {
        Some(i) => {
            let v = args.remove(i + 1);
            args.remove(i);
            v.split(',').map(str::to_string).collect()
        }
        None => Vec::new(),
    };
    let root = PathBuf::from(args.remove(0));
    let specs: Vec<ReadingSpec> = args.iter().map(|p| ReadingSpec::from_path(p.as_ref()).unwrap_or_else(|e| panic!("{p}: {e}"))).collect();
    let t = std::time::Instant::now();
    let ws = Workspace::open_for(&root, &specs[0], Resolving::Job).unwrap_or_else(|e| panic!("{e}"));
    println!("scan: {} files, {:.1}s", ws.klepto().files.len(), t.elapsed().as_secs_f64());
    for s in &specs {
        let t = std::time::Instant::now();
        match api::run(&ws, s) {
            Ok(r) => {
                println!("\n## {} ({}, {}) {:.1}s", s.name, s.kind.as_str(), s.target, t.elapsed().as_secs_f64());
                for c in r.counts() {
                    println!("- {}: {}", c.what, c.n);
                }
                println!("- unresolved: {}", r.unresolved().len());
                for k in &rows {
                    for x in r.rows().iter().filter(|x| &x.kind == k).take(40) {
                        println!("  {k}: {} {}", x.label, x.attributes.iter().filter(|(a, _)| !matches!(a.as_str(), "context" | "header")).map(|(a, v)| format!("{a}={v}")).collect::<Vec<_>>().join(" "));
                    }
                }
            }
            Err(e) => println!("\n## {}: refused {e}", s.name),
        }
    }
}
