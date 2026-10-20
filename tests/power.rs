// Copyright © 2026 1o. All rights reserved.

//! James's rule, where every cargo line the kit runs starts (`exec::Plan::walk`): under 20% and not charging the
//! line is refused before it starts, with its own code. The battery is given (`CUSTODIAN_BATTERY`), in a test
//! binary of its own, so no other test runs under it.

use custodian_kit::exec::{line, Cargo, Plan, Watch};

#[test]
fn a_cargo_line_is_refused_before_it_starts_on_a_dying_battery_and_runs_on_a_charging_one() {
    // a manifest that is not there: the line starts and cargo says so at once, nothing is built or written
    let manifest = std::env::temp_dir().join(format!("custodian-kit-power-{}", std::process::id())).join("Cargo.toml");
    let plan = || Plan::new("power").then(line("check", &Cargo::check(manifest.as_os_str()).offline()).unwrap());
    std::env::set_var(custodian_kit::power::BATTERY_ENV, "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t12%; discharging; 0:30 remaining present: true\n");
    let refused = plan().walk(Watch::Quiet, |_, _| {}).unwrap_err().to_string();
    assert!(refused.starts_with("[CUST0042]") && refused.contains("12%, not charging"), "{refused}");
    // not cargo: the rule is cargo's only
    let pmset = Plan::new("battery").then(line("pmset", &custodian_kit::exec::Pmset::get("batt")).unwrap());
    assert!(!pmset.walk(Watch::Quiet, |_, _| {}).is_err_and(|e| e.to_string().contains("CUST0042")));
    std::env::set_var(custodian_kit::power::BATTERY_ENV, "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t12%; charging; 2:00 remaining present: true\n");
    let walked = plan().walk(Watch::Quiet, |_, _| {}).unwrap();
    assert_eq!(walked.len(), 1, "charging: the line ran");
    assert!(walked[0].stderr.contains("Cargo.toml"), "{}", walked[0].stderr);
}
