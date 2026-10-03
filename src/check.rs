use crate::assets;
use crate::exit;
use crate::hw::{self, Transport};
use crate::known::{self, Evidence, Model};
use crate::sys::Sys;

const TOOLS: &[(&str, &[&str])] = &[
    ("make", &["make"]),
    ("cc", &["cc", "gcc"]),
    ("patch", &["patch"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Info,
    Warn,
    Fail,
}

impl Level {
    pub fn tag(self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub level: Level,
    pub label: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Supported,
    Untested,
    Unsupported,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub items: Vec<Item>,
    pub hardware: Verdict,
    pub model: Option<&'static Model>,
}

impl Report {
    pub fn ready(&self) -> bool {
        self.items.iter().all(|i| i.level != Level::Fail)
    }

    pub fn exit_code(&self) -> i32 {
        match (self.ready(), self.hardware) {
            (true, Verdict::Supported) => exit::OK,
            (true, _) => exit::UNTESTED,
            (false, _) => exit::FAILURE,
        }
    }

    pub fn summary(&self) -> String {
        match (self.hardware, self.ready()) {
            (Verdict::Unsupported, _) => "this machine cannot use the fix".to_string(),
            (Verdict::Supported, true) => "supported: ready for `try` and `install`".to_string(),
            (Verdict::Untested, true) => {
                "hardware looks right but this model is not confirmed: start with `try`".to_string()
            }
            (_, false) => "hardware looks right; fix the failed items above first".to_string(),
        }
    }
}

fn item(level: Level, label: &'static str, detail: impl Into<String>) -> Item {
    Item {
        level,
        label,
        detail: detail.into(),
    }
}

pub fn run(sys: &Sys) -> Report {
    let mut items = Vec::new();
    let mut hardware_ok = true;

    let dmi = hw::dmi(sys);
    items.push(if dmi.is_lenovo() {
        item(Level::Ok, "machine", dmi.label())
    } else {
        item(
            Level::Warn,
            "machine",
            format!("{} (not a Lenovo, untested)", dmi.label()),
        )
    });

    if hw::pci_present(sys, hw::FCH_SMBUS_VENDOR, hw::FCH_SMBUS_DEVICE) {
        items.push(item(
            Level::Ok,
            "smbus",
            "AMD FCH SMBus controller (1022:790b)",
        ));
    } else {
        hardware_ok = false;
        items.push(item(
            Level::Fail,
            "smbus",
            "no AMD FCH SMBus controller (1022:790b)",
        ));
    }

    match hw::asf_device(sys) {
        Some(dev) => items.push(item(
            Level::Ok,
            "asf",
            format!("{} at {}", hw::ASF_HID, dev.path.unwrap_or(dev.name)),
        )),
        None => {
            hardware_ok = false;
            items.push(item(
                Level::Fail,
                "asf",
                "no ACPI SMB0001 device, so Host Notify cannot be enabled",
            ));
        }
    }

    let transport = hw::touchpad_transport(&hw::input_devices(sys));
    let pnp = hw::aux_port(sys).and_then(|s| s.vendor_pnp());
    let pnp_label = pnp.clone().unwrap_or_else(|| "unknown id".to_string());
    match &transport {
        Transport::Rmi4(name) => items.push(item(
            Level::Ok,
            "touchpad",
            format!("already on rmi4/smbus ({name}, {pnp_label})"),
        )),
        Transport::SynapticsPs2 => items.push(item(
            Level::Ok,
            "touchpad",
            format!("Synaptics on ps/2 ({pnp_label})"),
        )),
        Transport::ElantechPs2(name) | Transport::OtherPs2(name) => {
            hardware_ok = false;
            items.push(item(
                Level::Fail,
                "touchpad",
                format!("{name}: only Synaptics touchpads are handled"),
            ));
        }
        Transport::Missing => {
            hardware_ok = false;
            items.push(item(
                Level::Fail,
                "touchpad",
                "no ps/2 or rmi4 touchpad found",
            ));
        }
    }

    let model = pnp.as_deref().and_then(known::lookup);
    match (model, &pnp) {
        (Some(m), Some(id)) if m.evidence == Evidence::Confirmed => items.push(item(
            Level::Ok,
            "model",
            format!("{} ({id}): {}", m.name, m.note),
        )),
        (Some(m), Some(id)) => items.push(item(
            Level::Warn,
            "model",
            format!("{} ({id}): {}", m.name, m.note),
        )),
        (_, Some(id)) => items.push(item(
            Level::Warn,
            "model",
            format!("{id} is not a known model; untested"),
        )),
        (_, None) => items.push(item(Level::Warn, "model", "touchpad id unknown; untested")),
    }

    let source = assets::kernel_source();
    match hw::kernel_release(sys) {
        Some(release) => {
            if hw::series(&release) == hw::series(source) {
                items.push(item(
                    Level::Ok,
                    "kernel",
                    format!("{release} (sources from {source})"),
                ));
            } else {
                items.push(item(
                    Level::Warn,
                    "kernel",
                    format!("{release}: sources are from {source}, the build may fail"),
                ));
            }
            if sys.is_dir(format!("/usr/lib/modules/{release}/build")) {
                items.push(item(
                    Level::Ok,
                    "headers",
                    format!("/usr/lib/modules/{release}/build"),
                ));
            } else {
                items.push(item(
                    Level::Fail,
                    "headers",
                    format!("kernel headers for {release} are not installed"),
                ));
            }
        }
        None => items.push(item(
            Level::Fail,
            "kernel",
            "cannot read the kernel release",
        )),
    }

    let missing: Vec<&str> = TOOLS
        .iter()
        .filter(|(_, names)| names.iter().all(|n| sys.find_program(n).is_none()))
        .map(|(label, _)| *label)
        .collect();
    if missing.is_empty() {
        items.push(item(Level::Ok, "toolchain", "make, cc, patch"));
    } else {
        items.push(item(
            Level::Fail,
            "toolchain",
            format!("missing: {}", missing.join(", ")),
        ));
    }

    if sys.find_program("dkms").is_some() {
        items.push(item(Level::Ok, "dkms", "available"));
    } else {
        items.push(item(
            Level::Warn,
            "dkms",
            "not installed: needed for `install`, not for `try`",
        ));
    }

    let lockdown = hw::lockdown(sys).filter(|m| m != "none");
    if hw::sig_enforce(sys) {
        items.push(item(
            Level::Fail,
            "signing",
            "module.sig_enforce is on: unsigned modules are refused",
        ));
    } else if let Some(mode) = lockdown {
        items.push(item(
            Level::Fail,
            "signing",
            format!("kernel lockdown ({mode}) refuses unsigned modules"),
        ));
    } else if hw::secure_boot(sys) == Some(true) {
        items.push(item(
            Level::Warn,
            "signing",
            "Secure Boot is on: unsigned modules may be refused unless signed with an enrolled MOK",
        ));
    } else {
        items.push(item(Level::Ok, "signing", "unsigned modules can load"));
    }

    if sys.is_dir(format!("/var/lib/dkms/{}", crate::LEGACY_DKMS)) {
        items.push(item(
            Level::Warn,
            "conflict",
            format!(
                "{} is installed through DKMS; remove it before `install`",
                crate::LEGACY_DKMS
            ),
        ));
    }

    let hardware = match (hardware_ok, model.map(|m| m.evidence)) {
        (false, _) => Verdict::Unsupported,
        (true, Some(Evidence::Confirmed)) => Verdict::Supported,
        (true, _) => Verdict::Untested,
    };
    Report {
        items,
        hardware,
        model,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        for l in [Level::Ok, Level::Info, Level::Warn, Level::Fail] {
            assert_eq!(l.tag(), l.tag().to_lowercase());
        }
        assert!(Level::Fail > Level::Warn && Level::Warn > Level::Info && Level::Info > Level::Ok);
    }

    #[test]
    fn exit_codes() {
        let ok = item(Level::Ok, "a", "");
        let bad = item(Level::Fail, "a", "");
        let r = |items: Vec<Item>, hardware| Report {
            items,
            hardware,
            model: None,
        };
        assert_eq!(r(vec![ok.clone()], Verdict::Supported).exit_code(), 0);
        assert_eq!(r(vec![ok.clone()], Verdict::Untested).exit_code(), 2);
        assert_eq!(r(vec![bad.clone()], Verdict::Supported).exit_code(), 1);
        assert_eq!(r(vec![bad.clone()], Verdict::Unsupported).exit_code(), 1);
        assert!(
            r(vec![ok.clone()], Verdict::Supported)
                .summary()
                .starts_with("supported")
        );
        assert!(
            r(vec![ok.clone()], Verdict::Untested)
                .summary()
                .contains("start with `try`")
        );
        assert!(
            r(vec![bad.clone()], Verdict::Unsupported)
                .summary()
                .contains("cannot use")
        );
        assert!(
            r(vec![bad], Verdict::Supported)
                .summary()
                .contains("fix the failed items")
        );
    }
}
