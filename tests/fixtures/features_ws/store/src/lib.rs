//! A store whose code is built under two features.

pub struct Store {
    pub name: String,
    pub size: usize,
}

impl Store {
    pub fn new(name: &str) -> Self {
        Store { name: name.to_string(), size: 0 }
    }

    #[cfg(feature = "log")]
    pub fn log_line(&self) -> String {
        format!("store {}", self.name)
    }
}

#[cfg(feature = "metrics")]
pub fn metrics(s: &Store) -> usize {
    s.size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_store_is_empty() {
        assert_eq!(Store::new("a").size, 0);
    }
}
