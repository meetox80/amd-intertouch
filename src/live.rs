use crate::assets::{self, ASSETS_HASH};
use crate::dkmsconf;
use crate::error::{Result, fail};
use crate::exec::{Action, Plan};
use crate::hw;
use crate::runner::Cmd;
use crate::sys::Sys;

pub const CACHE: &str = "/var/cache/amd-intertouch";

pub const MODULES: [(&str, &str); 3] = [
    ("i2c_piix4", "drivers/i2c/busses/i2c-piix4.ko"),
    ("rmi_smbus", "drivers/input/rmi4/rmi_smbus.ko"),
    ("psmouse", "drivers/input/mouse/psmouse.ko"),
];

pub const LOG_KEYS: &[&str] = &[
    "piix4",
    "asf",
    "host notify",
    "rmi4",
    "synaptics",
    "psmouse",
    "trackpoint",
    "smbus",
];

pub fn build_dir(kernel: &str) -> String {
    format!("{CACHE}/{kernel}")
}

pub fn stamp_path(kernel: &str) -> String {
    format!("{}/.stamp", build_dir(kernel))
}

pub fn stamp_text(kernel: &str) -> String {
    format!("{ASSETS_HASH:016x} {kernel}\n")
}

pub fn needs_build(sys: &Sys, kernel: &str) -> bool {
    let dir = build_dir(kernel);
    sys.read(stamp_path(kernel)).as_deref() != Some(stamp_text(kernel).as_str())
        || MODULES
            .iter()
            .any(|(_, ko)| !sys.exists(format!("{dir}/{ko}")))
}

pub fn build_plan(kernel: &str) -> Plan {
    let dir = build_dir(kernel);
    let mut plan = Plan::new();
    plan.push(Action::RemoveTree(dir.clone()));
    plan.extend(assets::write_actions(&dir));
    for p in dkmsconf::embedded().patches {
        plan.push(Action::Run(
            Cmd::new("patch")
                .args(["-p1", "--forward", "--no-backup-if-mismatch", "-s", "-i"])
                .arg(format!("patches/{p}"))
                .cwd(dir.clone()),
        ));
    }
    plan.push(Action::Run(
        Cmd::new("make")
            .arg("-C")
            .arg(format!("/usr/lib/modules/{kernel}/build"))
            .arg(format!("M={dir}"))
            .arg("modules"),
    ));
    plan.push(Action::write(
        &stamp_path(kernel),
        stamp_text(kernel),
        0o644,
    ));
    plan
}

pub fn ensure_swappable(sys: &Sys, kernel: &str) -> Result<()> {
    for (name, ko) in MODULES {
        let file = ko.rsplit('/').next().unwrap_or(ko);
        if hw::module_builtin(sys, kernel, file) {
            return fail(format!(
                "{name} is built into this kernel, so it cannot be swapped live"
            ));
        }
    }
    if !sys.is_dir(format!("/usr/lib/modules/{kernel}/build")) {
        return fail(format!("kernel headers for {kernel} are not installed"));
    }
    Ok(())
}

fn unload_actions(sys: &Sys) -> Vec<Action> {
    ["psmouse", "rmi_smbus", "i2c_piix4"]
        .into_iter()
        .filter(|m| hw::module_loaded(sys, m))
        .map(|m| Action::Run(Cmd::new("rmmod").arg(m)))
        .collect()
}

fn reload_actions() -> Vec<Action> {
    vec![
        Action::TryRun(Cmd::new("rmmod").arg("psmouse")),
        Action::TryRun(Cmd::new("rmmod").arg("rmi_smbus")),
        Action::TryRun(Cmd::new("rmmod").arg("i2c_piix4")),
        Action::Run(Cmd::new("modprobe").arg("i2c_piix4")),
        Action::Run(Cmd::new("modprobe").arg("rmi_smbus")),
        Action::Run(Cmd::new("modprobe").arg("psmouse")),
    ]
}

pub fn swap_plan(sys: &Sys, kernel: &str) -> Plan {
    let dir = build_dir(kernel);
    let mut plan = Plan::new();
    plan.push(Action::Run(Cmd::new("modprobe").arg("i2c_smbus")));
    plan.push(Action::Run(Cmd::new("modprobe").arg("rmi_core")));
    plan.extend(unload_actions(sys));
    for (name, ko) in MODULES {
        let mut cmd = Cmd::new("insmod").arg(format!("{dir}/{ko}"));
        if name == "psmouse" {
            cmd = cmd.arg("synaptics_intertouch=1");
        }
        plan.push(Action::Run(cmd));
    }
    plan.push(Action::Sleep(5000));
    plan.rollback = reload_actions();
    plan
}

pub fn restore_plan() -> Plan {
    let mut plan = Plan::new();
    plan.extend(reload_actions());
    plan.push(Action::Sleep(3000));
    plan
}

pub fn filter_log(log: &str) -> Vec<String> {
    log.lines()
        .filter(|l| {
            let lower = l.to_lowercase();
            LOG_KEYS.iter().any(|k| lower.contains(k))
        })
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(build_dir("k"), "/var/cache/amd-intertouch/k");
        assert_eq!(stamp_path("k"), "/var/cache/amd-intertouch/k/.stamp");
        assert!(stamp_text("k").ends_with(" k\n"));
        assert_eq!(stamp_text("k").len(), 16 + 3);
    }

    #[test]
    fn build_order() {
        let plan = build_plan("7.2.7-arch1-1");
        let lines = plan.lines();
        assert_eq!(
            lines[0],
            "remove  /var/cache/amd-intertouch/7.2.7-arch1-1  (recursive)"
        );
        let runs: Vec<&String> = lines.iter().filter(|l| l.starts_with("run")).collect();
        let patches = dkmsconf::embedded().patches;
        assert_eq!(runs.len(), patches.len() + 1);
        for (line, p) in runs.iter().zip(&patches) {
            assert!(line.ends_with(&format!("-i patches/{p})")), "{line}");
            assert!(line.contains("(cd /var/cache/amd-intertouch/7.2.7-arch1-1 && patch -p1"));
        }
        assert_eq!(
            runs.last().unwrap().as_str(),
            "run     make -C /usr/lib/modules/7.2.7-arch1-1/build M=/var/cache/amd-intertouch/7.2.7-arch1-1 modules"
        );
        assert!(
            lines
                .last()
                .unwrap()
                .starts_with("write   /var/cache/amd-intertouch/7.2.7-arch1-1/.stamp")
        );
        assert!(plan.rollback.is_empty());
    }

    #[test]
    fn restore_order() {
        assert_eq!(
            restore_plan().lines(),
            vec![
                "run     rmmod psmouse  (failure ignored)",
                "run     rmmod rmi_smbus  (failure ignored)",
                "run     rmmod i2c_piix4  (failure ignored)",
                "run     modprobe i2c_piix4",
                "run     modprobe rmi_smbus",
                "run     modprobe psmouse",
                "wait    3000 ms",
            ]
        );
    }

    #[test]
    fn log_filter() {
        let log = "usb 1-1: new device\npiix4_smbus 0000:00:14.0: ASF Host Notify on, IRQ 7\n\
                   rmi4_smbus 10-002c: registering SMbus-connected sensor\nwifi up\n";
        assert_eq!(filter_log(log).len(), 2);
    }
}
