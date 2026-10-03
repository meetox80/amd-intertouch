use crate::hw::{self, Transport};
use crate::install::{self, DkmsEntry};
use crate::modprobe::{self, NoSleep};
use crate::runner::{Cmd, Runner, query_ok};
use crate::sys::Sys;

pub const MODULES: [&str; 3] = ["i2c_piix4", "rmi_smbus", "psmouse"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostNotify {
    Enabled(String),
    Failed(String),
    Unknown,
}

impl HostNotify {
    pub fn describe(&self) -> String {
        match self {
            HostNotify::Enabled(irq) => format!("enabled, {irq}"),
            HostNotify::Failed(why) => format!("not enabled: {why}"),
            HostNotify::Unknown => "no ASF Host Notify messages this boot".to_string(),
        }
    }
}

const NOTIFY_ON: &str = "ASF Host Notify on, ";
const NOTIFY_OFF: &str = "ASF Host Notify off: ";

fn after(line: &str, marker: &str) -> Option<String> {
    line.find(marker)
        .map(|i| line[i + marker.len()..].trim().to_string())
}

pub fn parse_host_notify(log: &str) -> HostNotify {
    let mut state = HostNotify::Unknown;
    for line in log.lines() {
        if let Some(irq) = after(line, NOTIFY_ON) {
            state = HostNotify::Enabled(irq);
        } else if let Some(why) = after(line, NOTIFY_OFF) {
            state = HostNotify::Failed(why);
        }
    }
    state
}

pub fn kernel_log(runner: &dyn Runner) -> Option<String> {
    query_ok(
        runner,
        &Cmd::new("journalctl").args(["-k", "-b", "--no-pager", "-o", "cat"]),
    )
    .or_else(|| query_ok(runner, &Cmd::new("dmesg")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Patched,
    Stock,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loaded {
    No,
    Matches,
    Differs,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleState {
    pub name: String,
    pub origin: Origin,
    pub loaded: Loaded,
}

impl ModuleState {
    pub fn describe(&self) -> &'static str {
        match (self.origin, self.loaded) {
            (Origin::Patched, Loaded::Matches) => "patched build installed and loaded",
            (Origin::Patched, Loaded::Differs) => {
                "patched build installed, another build loaded (reboot or `restore` to switch)"
            }
            (Origin::Patched, Loaded::No) => "patched build installed, not loaded",
            (Origin::Stock, Loaded::Matches) => "stock",
            (Origin::Stock, Loaded::Differs) => {
                "stock installed, another build loaded (live `try`)"
            }
            (Origin::Stock, Loaded::No) => "stock, not loaded",
            (_, Loaded::Unknown) => "loaded, build unknown",
            (Origin::Unknown, _) => "module not found",
        }
    }
}

pub fn module_state(sys: &Sys, runner: &dyn Runner, name: &str) -> ModuleState {
    let file = query_ok(runner, &Cmd::new("modinfo").args(["-n", name]))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let origin = match &file {
        Some(f) if f.contains("/updates/") => Origin::Patched,
        Some(_) => Origin::Stock,
        None => Origin::Unknown,
    };
    let file_src = query_ok(
        runner,
        &Cmd::new("modinfo").args(["-F", "srcversion", name]),
    )
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());
    let loaded_src = sys
        .read_trim(format!("/sys/module/{name}/srcversion"))
        .filter(|s| !s.is_empty());
    let loaded = if !hw::module_loaded(sys, name) {
        Loaded::No
    } else {
        match (loaded_src, file_src) {
            (Some(l), Some(f)) if l == f => Loaded::Matches,
            (Some(_), Some(_)) => Loaded::Differs,
            _ => Loaded::Unknown,
        }
    };
    ModuleState {
        name: name.to_string(),
        origin,
        loaded,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub transport: Transport,
    pub host_notify: HostNotify,
    pub modules: Vec<ModuleState>,
    pub dkms: Vec<DkmsEntry>,
    pub nosleep: Option<NoSleep>,
    pub trackpoint: Option<hw::Trackpoint>,
}

pub fn collect(sys: &Sys, runner: &dyn Runner) -> Status {
    Status {
        transport: hw::touchpad_transport(&hw::input_devices(sys)),
        host_notify: kernel_log(runner)
            .map(|l| parse_host_notify(&l))
            .unwrap_or(HostNotify::Unknown),
        modules: MODULES
            .iter()
            .map(|m| module_state(sys, runner, m))
            .collect(),
        dkms: install::dkms_entries(runner),
        nosleep: sys
            .read(modprobe::NOSLEEP_PARAM)
            .and_then(|v| NoSleep::from_value(&v)),
        trackpoint: hw::trackpoint(sys),
    }
}

pub fn dkms_lines(entries: &[DkmsEntry]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    for e in entries {
        let key = (e.name.clone(), e.version.clone());
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let kernels: Vec<String> = entries
            .iter()
            .filter(|o| o.name == e.name && o.version == e.version)
            .map(|o| match &o.kernel {
                Some(k) => format!("{} on {k}", o.state),
                None => o.state.clone(),
            })
            .collect();
        lines.push(format!("{} {}: {}", e.name, e.version, kernels.join(", ")));
    }
    lines
}

pub fn relevant_dkms(entries: Vec<DkmsEntry>) -> Vec<DkmsEntry> {
    entries
        .into_iter()
        .filter(|e| e.name == crate::NAME || e.name == crate::LEGACY_DKMS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_states() {
        let ok = "x\npiix4_smbus 0000:00:14.0: ASF Host Notify on, IRQ 7\n";
        assert_eq!(parse_host_notify(ok), HostNotify::Enabled("IRQ 7".into()));
        let journal = "[40931.2] meow kernel: piix4_smbus 0000:00:14.0: ASF Host Notify on, IRQ 7";
        assert_eq!(
            parse_host_notify(journal),
            HostNotify::Enabled("IRQ 7".into())
        );
        let bad = "piix4_smbus 0000:00:14.0: ASF Host Notify off: SMB0001 has no interrupt";
        assert_eq!(
            parse_host_notify(bad),
            HostNotify::Failed("SMB0001 has no interrupt".into())
        );
        let irq = "piix4_smbus 0000:00:14.0: ASF Host Notify off: IRQ 7 unavailable (-16)";
        assert_eq!(
            parse_host_notify(irq),
            HostNotify::Failed("IRQ 7 unavailable (-16)".into())
        );
        assert_eq!(
            parse_host_notify(&format!("{bad}\n{ok}")),
            HostNotify::Enabled("IRQ 7".into())
        );
        assert_eq!(parse_host_notify("nothing"), HostNotify::Unknown);
        assert_eq!(
            after("ASF Host Notify off: x ", NOTIFY_OFF),
            Some("x".into())
        );
        assert_eq!(after("ASF Host Notify on, IRQ 7", NOTIFY_OFF), None);
    }

    #[test]
    fn notify_text() {
        assert_eq!(HostNotify::Enabled("m".into()).describe(), "enabled, m");
        assert_eq!(HostNotify::Failed("m".into()).describe(), "not enabled: m");
        assert_eq!(
            HostNotify::Unknown.describe(),
            "no ASF Host Notify messages this boot"
        );
    }

    #[test]
    fn module_text() {
        let s = |origin, loaded| {
            ModuleState {
                name: "m".into(),
                origin,
                loaded,
            }
            .describe()
        };
        assert_eq!(
            s(Origin::Patched, Loaded::Matches),
            "patched build installed and loaded"
        );
        assert!(s(Origin::Patched, Loaded::Differs).contains("another build loaded"));
        assert_eq!(
            s(Origin::Patched, Loaded::No),
            "patched build installed, not loaded"
        );
        assert_eq!(s(Origin::Stock, Loaded::Matches), "stock");
        assert!(s(Origin::Stock, Loaded::Differs).contains("live `try`"));
        assert_eq!(s(Origin::Stock, Loaded::No), "stock, not loaded");
        assert_eq!(s(Origin::Stock, Loaded::Unknown), "loaded, build unknown");
        assert_eq!(s(Origin::Unknown, Loaded::No), "module not found");
        assert_eq!(s(Origin::Unknown, Loaded::Unknown), "loaded, build unknown");
    }

    #[test]
    fn dkms_groups() {
        let e = |name: &str, version: &str, kernel: Option<&str>, state: &str| DkmsEntry {
            name: name.into(),
            version: version.into(),
            kernel: kernel.map(Into::into),
            arch: kernel.map(|_| "x86_64".into()),
            state: state.into(),
        };
        let entries = vec![
            e(
                "amd-intertouch",
                "0.1.0",
                Some("7.2.7-arch1-1"),
                "installed",
            ),
            e(
                "amd-intertouch",
                "0.1.0",
                Some("7.2.7-hardened1-1-hardened"),
                "installed",
            ),
            e("amd-intertouch", "0.0.9", None, "added"),
            e("vboxhost", "7.2", Some("k"), "installed"),
        ];
        let relevant = relevant_dkms(entries);
        assert_eq!(relevant.len(), 3);
        assert_eq!(
            dkms_lines(&relevant),
            vec![
                "amd-intertouch 0.1.0: installed on 7.2.7-arch1-1, installed on 7.2.7-hardened1-1-hardened",
                "amd-intertouch 0.0.9: added",
            ]
        );
    }
}
