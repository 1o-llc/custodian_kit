pub fn checksum(bytes: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for b in bytes {
        sum = sum.wrapping_mul(31).wrapping_add(*b as u32);
    }
    sum ^ (sum >> 16)
}
