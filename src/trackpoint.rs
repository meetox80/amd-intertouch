use crate::error::{Result, fail};
use crate::exec::{Action, Plan};
use crate::hw;
use crate::runner::Cmd;
use crate::sys::Sys;

pub const RULE: &str = "/etc/udev/rules.d/70-amd-intertouch-trackpoint.rules";
pub const LEGACY_RULE: &str = "/etc/udev/rules.d/70-thinkpad-pointer.rules";
pub const RATES: &[u32] = &[10, 20, 40, 60, 80, 100, 200];
pub const DEFAULT_RATE: u32 = 200;
pub const DEFAULT_SENSITIVITY: u32 = 200;

pub fn validate(rate: u32, sensitivity: u32) -> Result<()> {
    if !RATES.contains(&rate) {
        let list: Vec<String> = RATES.iter().map(u32::to_string).collect();
        return fail(format!(
            "rate must be one of {}, not {rate}",
            list.join(", ")
        ));
    }
    if !(1..=255).contains(&sensitivity) {
        return fail(format!("sensitivity must be 1-255, not {sensitivity}"));
    }
    Ok(())
}

pub fn rule_text(rate: u32, sensitivity: u32) -> String {
    format!(
        "ACTION==\"bind\", SUBSYSTEM==\"serio\", DRIVER==\"psmouse\", ATTR{{protocol}}==\"TPPS/2\", ATTR{{rate}}=\"{rate}\", ATTR{{sensitivity}}=\"{sensitivity}\"\n"
    )
}

pub fn apply_plan(sys: &Sys, rate: u32, sensitivity: u32) -> Result<Plan> {
    validate(rate, sensitivity)?;
    let mut plan = Plan::new();
    plan.push(Action::write(RULE, rule_text(rate, sensitivity), 0o644));
    plan.push(Action::Run(
        Cmd::new("udevadm").args(["control", "--reload"]),
    ));
    if let Some(tp) = hw::trackpoint(sys) {
        if tp.rate.is_some() {
            plan.push(Action::attr(&tp.attr("rate"), rate));
        }
        if tp.sensitivity.is_some() {
            plan.push(Action::attr(&tp.attr("sensitivity"), sensitivity));
        }
    }
    Ok(plan)
}

pub fn remove_plan() -> Plan {
    let mut plan = Plan::new();
    plan.push(Action::Remove(RULE.to_string()));
    plan.push(Action::Run(
        Cmd::new("udevadm").args(["control", "--reload"]),
    ));
    plan
}

pub fn legacy_rule_present(sys: &Sys) -> bool {
    sys.exists(LEGACY_RULE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert!(validate(200, 200).is_ok());
        assert!(validate(10, 1).is_ok());
        assert!(validate(100, 255).is_ok());
        assert_eq!(
            validate(150, 200).unwrap_err().0,
            "rate must be one of 10, 20, 40, 60, 80, 100, 200, not 150"
        );
        assert_eq!(
            validate(200, 0).unwrap_err().0,
            "sensitivity must be 1-255, not 0"
        );
        assert_eq!(
            validate(200, 256).unwrap_err().0,
            "sensitivity must be 1-255, not 256"
        );
    }

    #[test]
    fn rule() {
        assert_eq!(
            rule_text(200, 180),
            "ACTION==\"bind\", SUBSYSTEM==\"serio\", DRIVER==\"psmouse\", ATTR{protocol}==\"TPPS/2\", ATTR{rate}=\"200\", ATTR{sensitivity}=\"180\"\n"
        );
    }

    #[test]
    fn removal() {
        assert_eq!(
            remove_plan().lines(),
            vec![
                "remove  /etc/udev/rules.d/70-amd-intertouch-trackpoint.rules",
                "run     udevadm control --reload",
            ]
        );
    }

    #[test]
    fn apply_errors() {
        let sys = Sys::new("/nonexistent-root");
        assert!(apply_plan(&sys, 7, 200).is_err());
        let plan = apply_plan(&sys, 100, 128).unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert!(!legacy_rule_present(&sys));
    }
}
