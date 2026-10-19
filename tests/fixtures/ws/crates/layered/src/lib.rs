//! One dependency per depth: helper_a in one fn (and the fn that calls it),
//! helper_b in every item of the module `wide`, helper_c in every item of
//! the file whole.rs.
pub mod whole;

pub fn plain(x: u32) -> u32 {
    x + 10
}

pub fn uses_a(x: u32) -> u32 {
    helper_a::base(x) * 3
}

pub fn calls_uses_a(x: u32) -> u32 {
    uses_a(x) + plain(x)
}

pub mod wide {
    pub fn w1(x: u32) -> u32 {
        helper_b::extra(x) + 7
    }

    pub fn w2(x: u32) -> u32 {
        w1(x) * 5
    }
}
