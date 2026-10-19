//! Opens stores for the command line.

pub fn open(name: &str) -> store::Store {
    let s = store::Store::new(name);
    let _ = s.log_line();
    s
}

pub fn sized(name: &str, size: usize) -> store::Store {
    store::Store { name: name.to_string(), size }
}
