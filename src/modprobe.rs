use crate::error::{Result, fail};
use crate::exec::{Action, Plan};
use crate::runner::Cmd;
use crate::sys::Sys;

pub const CONF: &str = "/etc/modprobe.d/amd-intertouch.conf";
pub const PACKAGE_CONF: &str = "/usr/lib/modprobe.d/amd-intertouch.conf";
pub const NOSLEEP_CONF: &str = "/etc/modprobe.d/amd-intertouch-nosleep.conf";
pub const NOSLEEP_PARAM: &str = "/sys/module/psmouse/parameters/synaptics_rmi_nosleep";

pub fn conf_text() -> &'static str {
    "softdep i2c_piix4 post: rmi_smbus\noptions psmouse synaptics_intertouch=1\n"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoSleep {
    Auto,
    On,
    Off,
}

impl NoSleep {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(NoSleep::Auto),
            "on" => Ok(NoSleep::On),
            "off" => Ok(NoSleep::Off),
            other => fail(format!(
                "nosleep mode must be auto, on or off, not `{other}`"
            )),
        }
    }

    pub fn value(self) -> i32 {
        match self {
            NoSleep::Auto => -1,
            NoSleep::On => 1,
            NoSleep::Off => 0,
        }
    }

    pub fn from_value(v: &str) -> Option<Self> {
        match v.trim() {
            "-1" => Some(NoSleep::Auto),
            "1" => Some(NoSleep::On),
            "0" => Some(NoSleep::Off),
            _ => None,
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            NoSleep::Auto => "auto (on for known pads: LEN2064)",
            NoSleep::On => "on",
            NoSleep::Off => "off",
        }
    }

    pub fn conf_text(self) -> Option<String> {
        match self {
            NoSleep::Auto => None,
            mode => Some(format!(
                "options psmouse synaptics_rmi_nosleep={}\n",
                mode.value()
            )),
        }
    }
}

pub fn nosleep_actions(mode: NoSleep) -> Vec<Action> {
    match mode.conf_text() {
        Some(text) => vec![Action::write(NOSLEEP_CONF, text, 0o644)],
        None => vec![Action::Remove(NOSLEEP_CONF.to_string())],
    }
}

pub fn nosleep_plan(sys: &Sys, mode: NoSleep, now: bool) -> Plan {
    let mut plan = Plan::new();
    plan.extend(nosleep_actions(mode));
    if sys.exists(NOSLEEP_PARAM) {
        plan.push(Action::attr(NOSLEEP_PARAM, mode.value()));
    }
    if now {
        plan.push(Action::Run(Cmd::new("rmmod").arg("psmouse")));
        plan.push(Action::Run(Cmd::new("modprobe").arg("psmouse")));
        plan.rollback
            .push(Action::TryRun(Cmd::new("modprobe").arg("psmouse")));
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        assert_eq!(NoSleep::parse("auto").unwrap(), NoSleep::Auto);
        assert_eq!(NoSleep::parse("on").unwrap(), NoSleep::On);
        assert_eq!(NoSleep::parse("off").unwrap(), NoSleep::Off);
        assert_eq!(
            NoSleep::parse("yes").unwrap_err().0,
            "nosleep mode must be auto, on or off, not `yes`"
        );
    }

    #[test]
    fn values() {
        for m in [NoSleep::Auto, NoSleep::On, NoSleep::Off] {
            assert_eq!(NoSleep::from_value(&format!("{}\n", m.value())), Some(m));
        }
        assert_eq!(NoSleep::from_value("7"), None);
    }

    #[test]
    fn conf() {
        assert_eq!(NoSleep::Auto.conf_text(), None);
        assert_eq!(
            NoSleep::On.conf_text().unwrap(),
            "options psmouse synaptics_rmi_nosleep=1\n"
        );
        assert_eq!(
            NoSleep::Off.conf_text().unwrap(),
            "options psmouse synaptics_rmi_nosleep=0\n"
        );
        assert!(NoSleep::Auto.describe().contains("LEN2064"));
        assert_eq!(NoSleep::On.describe(), "on");
        assert_eq!(NoSleep::Off.describe(), "off");
    }

    #[test]
    fn module_conf() {
        assert_eq!(
            conf_text(),
            "softdep i2c_piix4 post: rmi_smbus\noptions psmouse synaptics_intertouch=1\n"
        );
    }

    #[test]
    fn packaged_conf() {
        let packaged = include_str!("../packaging/amd-intertouch.conf");
        assert_eq!(packaged, conf_text());
    }

    #[test]
    fn plans() {
        let sys = Sys::new("/nonexistent-root");
        let plan = nosleep_plan(&sys, NoSleep::On, false);
        assert_eq!(
            plan.lines(),
            vec!["write   /etc/modprobe.d/amd-intertouch-nosleep.conf  (40 bytes, mode 644)"]
        );
        let plan = nosleep_plan(&sys, NoSleep::Auto, true);
        assert_eq!(
            plan.lines(),
            vec![
                "remove  /etc/modprobe.d/amd-intertouch-nosleep.conf",
                "run     rmmod psmouse",
                "run     modprobe psmouse",
                "on failure:",
                "  run     modprobe psmouse  (failure ignored)",
            ]
        );
    }
}
