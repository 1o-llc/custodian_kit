pub fn bracket(s: &str) -> String {
    helper_c::tag(s)
}

pub fn twice(s: &str) -> String {
    helper_c::tag(&bracket(s))
}
