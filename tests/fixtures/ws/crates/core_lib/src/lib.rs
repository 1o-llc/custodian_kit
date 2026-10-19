//! The fixture's library: env reads, a mode enum, a config type, a metrics server.

/// The key the socket path is read from.
pub const ENV_SOCKET: &str = "FIX_SOCKET";

pub fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

pub fn socket() -> String {
    env_or(ENV_SOCKET, "/run/fix/core.sock")
}

pub fn token() -> Option<String> {
    std::env::var("FIX_TOKEN").ok()
}

pub fn named_only() -> &'static str {
    "FIX_NAMED"
}

pub enum Mode {
    Serving,
    ReadOnly,
    Maintenance,
}

pub fn parse_mode(s: &str) -> Mode {
    match s {
        "read_only" => Mode::ReadOnly,
        "maintenance" => Mode::Maintenance,
        _ => Mode::Serving,
    }
}

pub fn serving(m: &Mode) -> bool {
    match m {
        Mode::Serving => true,
        _ => false,
    }
}

pub fn label(m: &Mode) -> &'static str {
    match m {
        Mode::Serving => "serving",
        Mode::ReadOnly => "read only",
        Mode::Maintenance => "maintenance",
    }
}

pub struct Config {
    pub name: String,
}

impl Config {
    pub fn new(name: &str) -> Self {
        Config { name: name.to_string() }
    }
}

pub fn serve_metrics(addr: &str) -> String {
    format!("serving metrics on {addr}")
}

pub fn vault_key() -> u64 {
    let k = 41;
    k + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_parse() {
        assert!(serving(&parse_mode("serving")));
    }

    #[test]
    fn config_names() {
        assert_eq!(Config::new("a").name, "a");
    }
}

/// A sentence naming a key is not a key.
pub fn note() -> &'static str {
    "FIX_TOKEN=set in tests"
}
