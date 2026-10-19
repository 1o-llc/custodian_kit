fn main() {
    let c = core_lib::Config::new("app");
    let line = core_lib::serve_metrics(&core_lib::socket());
    println!("{} {}", c.name, line);
}
