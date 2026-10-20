// Copyright © 2026 1o. All rights reserved.

//! James's rule for a laptop: no cargo line starts on a dying battery. Before custodian or code_passenger starts a
//! cargo line, the battery is read (`pmset -g batt`, a declared line); under [`MIN_PERCENT`] and not charging, the
//! line is refused with its own code ([`ReadingRefusal::LowBattery`], `CUST0042`) before it starts, and the job it
//! belongs to stays open. A machine with no battery to read (no `pmset`, no internal battery: a desktop, a Linux
//! box) has nothing to refuse. On AC power the battery is not dying, charging or not.
//!
//! The read can be given instead of made: [`BATTERY_ENV`] holds the text `pmset -g batt` would print, so a test
//! states the battery it runs under.

use crate::reading::refusal::ReadingRefusal;
use crate::Result;

/// Under this charge, and not charging, no cargo line starts.
pub const MIN_PERCENT: u8 = 20;

/// A job's floor instead of [`MIN_PERCENT`]: its budget's `battery=<percent>` (custodian sets it for the lines a job
/// runs, and code_passenger, started by them, inherits it).
pub const BATTERY_MIN_ENV: &str = "CUSTODIAN_BATTERY_MIN";

/// The floor now: [`BATTERY_MIN_ENV`] when it is set, else [`MIN_PERCENT`].
pub fn min_percent() -> u8 {
    std::env::var(BATTERY_MIN_ENV).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(MIN_PERCENT)
}

/// The battery read given instead of made: the text `pmset -g batt` prints (a test's battery).
pub const BATTERY_ENV: &str = "CUSTODIAN_BATTERY";

/// The battery as `pmset -g batt` reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    /// Charging, charged, or drawing from AC power: not running down.
    pub powered: bool,
}

impl Battery {
    /// What `pmset -g batt` printed: `Now drawing from 'Battery Power'` then ` -InternalBattery-0 (id=…)\t13%;
    /// discharging; …`. `None` when it names no internal battery.
    pub fn parse(text: &str) -> Option<Battery> {
        let line = text.lines().find(|l| l.contains("InternalBattery"))?;
        let after = line.split_once(')').map(|(_, r)| r).unwrap_or(line);
        let percent: u8 = after.split('%').next()?.trim().rsplit(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
        let state = after.split(';').nth(1).map(str::trim).unwrap_or_default();
        let ac = text.contains("'AC Power'");
        let powered = ac || state == "charging" || state == "charged" || state == "finishing charge";
        Some(Battery { percent, powered })
    }

    /// Whether a cargo line may start under this battery, at the floor [`MIN_PERCENT`].
    pub fn allows_cargo(&self) -> bool {
        self.allows_cargo_at(MIN_PERCENT)
    }

    /// Whether a cargo line may start under this battery, at the floor `min`.
    pub fn allows_cargo_at(&self, min: u8) -> bool {
        self.powered || self.percent >= min
    }

    /// The battery in words: `13%, not charging`.
    pub fn said(&self) -> String {
        format!("{}%, {}", self.percent, if self.powered { "charging" } else { "not charging" })
    }
}

/// Whether `program` is cargo (`cargo`, or a path to it).
pub fn is_cargo(program: impl AsRef<std::ffi::OsStr>) -> bool {
    std::path::Path::new(program.as_ref()).file_name().is_some_and(|n| n == "cargo")
}

/// The battery now: [`BATTERY_ENV`] when it is set, else `pmset -g batt` (a declared line, walked with no journal
/// receipt of its own: it is the check before a line, not a line of the job). `None`: no battery to read.
pub fn read() -> Option<Battery> {
    if let Some(given) = std::env::var_os(BATTERY_ENV) {
        return Battery::parse(&given.to_string_lossy());
    }
    use crate::exec::{line, Plan, Watch};
    let walked = Plan::new("battery").then(line("pmset", &crate::exec::Pmset::get("batt")).ok()?).walk(Watch::Quiet, |_, _| {}).ok()?;
    walked.into_iter().next().filter(|w| w.code == Some(0)).and_then(|w| Battery::parse(&w.stdout))
}

/// The rule, for `battery`: refused under [`min_percent`] and not charging.
pub fn check(battery: Option<Battery>) -> Result<()> {
    let min = min_percent();
    match battery {
        Some(b) if !b.allows_cargo_at(min) => Err(ReadingRefusal::LowBattery.refuse(format!("the battery is at {} (the floor is {min}%)", b.said()))),
        _ => Ok(()),
    }
}

/// The rule, now: every cargo line custodian or code_passenger starts asks this first.
pub fn cargo_may_start() -> Result<()> {
    check(read())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOW: &str = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=21299299)\t13%; discharging; 0:41 remaining present: true\n";
    const LOW_CHARGING: &str = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=21299299)\t13%; charging; 2:29 remaining present: true\n";
    const FULL: &str = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=21299299)\t86%; discharging; 6:02 remaining present: true\n";

    #[test]
    fn under_twenty_percent_and_not_charging_no_cargo_line_starts() {
        assert_eq!(Battery::parse(LOW), Some(Battery { percent: 13, powered: false }));
        assert_eq!(Battery::parse(LOW_CHARGING), Some(Battery { percent: 13, powered: true }));
        assert_eq!(Battery::parse(FULL), Some(Battery { percent: 86, powered: false }));
        assert_eq!(Battery::parse("Now drawing from 'AC Power'\n"), None, "no internal battery: nothing to refuse");
        let refused = check(Battery::parse(LOW)).unwrap_err().to_string();
        assert!(refused.starts_with("[CUST0042]") && refused.contains("13%, not charging"), "{refused}");
        assert!(check(Battery::parse(LOW_CHARGING)).is_ok());
        assert!(check(Battery::parse(FULL)).is_ok());
        assert!(check(None).is_ok());
        let at = |p: u8| Battery { percent: p, powered: false };
        assert!(check(Some(at(19))).is_err() && check(Some(at(20))).is_ok());
        assert!(is_cargo("cargo") && is_cargo("/Users/x/.cargo/bin/cargo") && !is_cargo("cargo-expand") && !is_cargo("git"));
    }
}
