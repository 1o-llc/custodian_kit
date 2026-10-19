pub fn user(x: u32) -> u32 {
    layered::uses_a(x) + layered::plain(x)
}

pub fn bystander(x: u32) -> u32 {
    layered::plain(x) - 1
}
