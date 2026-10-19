//! The declared reading line's body as a program of its own, for tests that
//! run a job through the line runner without the custodian binary:
//! `exec_reading_line exec-reading --spec S --root R --out O --stamp T [--job J]`.
//! It prints the reading's summary as its last line, or exits 1 with the refusal.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    if args.first().map(String::as_str) != Some("exec-reading") {
        eprintln!("usage: exec_reading_line exec-reading --spec S --root R --out O --stamp T [--job J]");
        std::process::exit(2);
    }
    let (Some(spec), Some(root), Some(out), Some(stamp)) = (get("--spec"), get("--root"), get("--out"), get("--stamp")) else {
        eprintln!("--spec, --root, --out and --stamp are required");
        std::process::exit(2);
    };
    match custodian_kit::reading::exec_reading(spec.as_ref(), root.as_ref(), &out, &stamp, get("--job").is_some()) {
        Ok(summary) => println!("{}", serde_json::to_string(&summary).unwrap_or_default()),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
