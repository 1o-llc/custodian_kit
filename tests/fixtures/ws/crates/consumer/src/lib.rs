//! helper_a is used by every public fn; helper_b by one of five.
pub fn one(x: u32) -> u32 {
    helper_a::base(x)
}
pub fn two(x: u32) -> u32 {
    helper_a::base(x) + 2
}
pub fn three(x: u32) -> u32 {
    helper_a::base(x) + 3
}
pub fn four(x: u32) -> u32 {
    helper_a::base(x) + 4
}
pub fn five(x: u32) -> u32 {
    helper_a::base(helper_b::extra(x))
}
