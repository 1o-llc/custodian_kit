//! Prints what klepto recorded in one file of a workspace: paths, calls, attributes, edges.
//!
//!     cargo run --example probe -- <workspace root> <file suffix> [tests]
fn main() {
    let mut a = std::env::args().skip(1);
    let root = std::path::PathBuf::from(a.next().unwrap_or_else(|| ".".into()));
    let suffix = a.next().unwrap_or_default();
    let tests = a.next().is_some();
    let opts = custodian_kit::ScanOptions { tests, ..custodian_kit::ScanOptions::graph() };
    let k = custodian_kit::klepto_scan(&root.canonicalize().unwrap(), &opts).unwrap();
    for f in k.files.iter().filter(|f| f.path.to_string_lossy().ends_with(&suffix)) {
        println!("file {} module {:?}", f.path.display(), f.module);
    }
    for p in k.paths.iter().filter(|p| p.location.path.to_string_lossy().ends_with(&suffix)) {
        println!("path {} {:?} {:?} {:?}", p.path, p.location.line, p.resolution, p.enclosing_item);
    }
    for at in k.attrs.iter().filter(|p| p.location.path.to_string_lossy().ends_with(&suffix)) {
        println!("attr {} {:?} item {:?}", at.path, at.location.line, at.enclosing_item);
    }
    let in_file: std::collections::BTreeSet<&str> = k.items.iter().filter(|d| d.location.path.to_string_lossy().ends_with(&suffix)).map(|d| d.id.as_str()).collect();
    for e in k.graph.edges.iter().filter(|e| in_file.contains(e.from.as_str())) {
        println!("edge {} -> {} {:?}", e.from, e.to, e.kind);
    }
    println!("roots {:?}", k.crate_roots);
}
