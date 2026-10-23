//! The fixture for w1a's findings: two `From` impls on one type with swappable bodies (two impls, one path), a
//! whole `From` impl a toolkit piece writes (an impl, no swap), and two functions whose routes agree past 48
//! characters (a spec name cut there collides).

pub struct Level(pub u32);

impl From<i64> for Level {
    fn from(v: i64) -> Self {
        if v < 0 {
            return Level(0);
        }
        Level(v as u32)
    }
}

impl From<i32> for Level {
    fn from(v: i32) -> Self {
        if v < 0 {
            return Level(1);
        }
        Level(v as u32)
    }
}

pub struct Wrapper(pub String);

impl From<String> for Wrapper {
    fn from(inner: String) -> Self {
        Self(inner)
    }
}

pub fn a_function_whose_route_runs_well_past_forty_eight_characters_one(x: i32) -> i32 {
    if x < 0 {
        return 0;
    }
    x
}

pub fn a_function_whose_route_runs_well_past_forty_eight_characters_two(x: i32) -> i32 {
    if x < 0 {
        return 1;
    }
    x
}
