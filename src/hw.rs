use crate::sys::Sys;

pub const FCH_SMBUS_VENDOR: &str = "0x1022";
pub const FCH_SMBUS_DEVICE: &str = "0x790b";
pub const ASF_HID: &str = "SMB0001";
pub const SECURE_BOOT_VAR: &str =
    "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dmi {
    pub vendor: String,
    pub product_name: String,
    pub product_version: String,
}

impl Dmi {
    pub fn is_lenovo(&self) -> bool {
        self.vendor.eq_ignore_ascii_case("lenovo")
    }

    pub fn label(&self) -> String {
        let model = [self.product_version.as_str(), self.product_name.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        match (self.vendor.is_empty(), model.is_empty()) {
            (true, true) => "unknown machine".to_string(),
            (false, true) => self.vendor.clone(),
            (true, false) => model.join(" "),
            (false, false) => format!("{} {}", self.vendor, model.join(" ")),
        }
    }
}

pub fn dmi(sys: &Sys) -> Dmi {
    let read = |f: &str| {
        sys.read_trim(format!("/sys/class/dmi/id/{f}"))
            .unwrap_or_default()
    };
    Dmi {
        vendor: read("sys_vendor"),
        product_name: read("product_name"),
        product_version: read("product_version"),
    }
}

pub fn pci_present(sys: &Sys, vendor: &str, device: &str) -> bool {
    sys.list("/sys/bus/pci/devices").iter().any(|d| {
        let id = |f: &str| {
            sys.read_trim(format!("/sys/bus/pci/devices/{d}/{f}"))
                .unwrap_or_default()
        };
        id("vendor").eq_ignore_ascii_case(vendor) && id("device").eq_ignore_ascii_case(device)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpiDevice {
    pub name: String,
    pub hid: String,
    pub status: Option<u32>,
    pub path: Option<String>,
}

impl AcpiDevice {
    pub fn present(&self) -> bool {
        self.status.is_none_or(|s| s & 1 == 1)
    }
}

pub fn acpi_devices(sys: &Sys, hid: &str) -> Vec<AcpiDevice> {
    sys.list("/sys/bus/acpi/devices")
        .into_iter()
        .filter_map(|name| {
            let base = format!("/sys/bus/acpi/devices/{name}");
            let found = sys.read_trim(format!("{base}/hid"))?;
            if !found.eq_ignore_ascii_case(hid) {
                return None;
            }
            Some(AcpiDevice {
                status: sys
                    .read_trim(format!("{base}/status"))
                    .and_then(|s| s.parse().ok()),
                path: sys
                    .read_trim(format!("{base}/path"))
                    .filter(|p| !p.is_empty()),
                hid: found,
                name,
            })
        })
        .collect()
}

pub fn asf_device(sys: &Sys) -> Option<AcpiDevice> {
    acpi_devices(sys, ASF_HID)
        .into_iter()
        .find(AcpiDevice::present)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Serio {
    pub name: String,
    pub description: String,
    pub protocol: String,
    pub firmware_id: String,
}

impl Serio {
    pub fn pnp_ids(&self) -> Vec<String> {
        pnp_ids(&self.firmware_id)
    }

    pub fn vendor_pnp(&self) -> Option<String> {
        self.pnp_ids()
            .into_iter()
            .find(|id| !id.to_ascii_uppercase().starts_with("PNP"))
    }
}

pub fn pnp_ids(firmware_id: &str) -> Vec<String> {
    firmware_id
        .trim()
        .strip_prefix("PNP:")
        .unwrap_or(firmware_id)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

pub fn serios(sys: &Sys) -> Vec<Serio> {
    sys.list("/sys/bus/serio/devices")
        .into_iter()
        .map(|name| {
            let read = |f: &str| {
                sys.read_trim(format!("/sys/bus/serio/devices/{name}/{f}"))
                    .unwrap_or_default()
            };
            Serio {
                description: read("description"),
                protocol: read("protocol"),
                firmware_id: read("firmware_id"),
                name: name.clone(),
            }
        })
        .collect()
}

pub fn aux_port(sys: &Sys) -> Option<Serio> {
    serios(sys)
        .into_iter()
        .find(|s| s.description.contains("AUX") && !s.firmware_id.is_empty())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputDevice {
    pub name: String,
    pub phys: String,
    pub sysfs: String,
    pub handlers: Vec<String>,
}

pub fn parse_input_devices(text: &str) -> Vec<InputDevice> {
    let mut out = Vec::new();
    let mut cur: Option<InputDevice> = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            out.extend(cur.take());
            continue;
        }
        let Some((tag, rest)) = line.split_once(": ") else {
            continue;
        };
        let dev = cur.get_or_insert_with(InputDevice::default);
        let value = |key: &str| rest.strip_prefix(key).unwrap_or(rest).to_string();
        match tag {
            "N" => dev.name = value("Name=").trim_matches('"').to_string(),
            "P" => dev.phys = value("Phys="),
            "S" => dev.sysfs = value("Sysfs="),
            "H" => {
                dev.handlers = value("Handlers=")
                    .split_whitespace()
                    .map(str::to_string)
                    .collect()
            }
            _ => {}
        }
    }
    out.extend(cur);
    out
}

pub fn input_devices(sys: &Sys) -> Vec<InputDevice> {
    sys.read("/proc/bus/input/devices")
        .map(|t| parse_input_devices(&t))
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    Rmi4(String),
    SynapticsPs2,
    ElantechPs2(String),
    OtherPs2(String),
    Missing,
}

impl Transport {
    pub fn describe(&self) -> String {
        match self {
            Transport::Rmi4(name) => format!("rmi4/smbus ({name})"),
            Transport::SynapticsPs2 => "ps/2 (SynPS/2 Synaptics TouchPad)".to_string(),
            Transport::ElantechPs2(name) => format!("ps/2 ({name})"),
            Transport::OtherPs2(name) => format!("ps/2 ({name})"),
            Transport::Missing => "no touchpad found".to_string(),
        }
    }
}

pub fn touchpad_transport(inputs: &[InputDevice]) -> Transport {
    if let Some(d) = inputs
        .iter()
        .find(|d| d.phys.starts_with("rmi4-") && !d.phys.contains(".fn03"))
    {
        return Transport::Rmi4(d.name.clone());
    }
    if inputs
        .iter()
        .any(|d| d.name == "SynPS/2 Synaptics TouchPad")
    {
        return Transport::SynapticsPs2;
    }
    if let Some(d) = inputs.iter().find(|d| {
        d.phys.starts_with("isa0060") && (d.name.contains("ETPS/2") || d.name.contains("Elantech"))
    }) {
        return Transport::ElantechPs2(d.name.clone());
    }
    if let Some(d) = inputs
        .iter()
        .find(|d| d.phys.starts_with("isa0060") && d.name.to_ascii_lowercase().contains("touchpad"))
    {
        return Transport::OtherPs2(d.name.clone());
    }
    Transport::Missing
}

pub fn kernel_release(sys: &Sys) -> Option<String> {
    sys.read_trim("/proc/sys/kernel/osrelease")
        .filter(|s| !s.is_empty())
}

pub fn series(version: &str) -> Option<String> {
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor: String = parts
        .next()?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if major.is_empty() || !major.chars().all(|c| c.is_ascii_digit()) || minor.is_empty() {
        return None;
    }
    Some(format!("{major}.{minor}"))
}

pub fn secure_boot(sys: &Sys) -> Option<bool> {
    sys.read_bytes(SECURE_BOOT_VAR)
        .and_then(|b| b.get(4).map(|v| *v == 1))
}

pub fn sig_enforce(sys: &Sys) -> bool {
    sys.read_trim("/sys/module/module/parameters/sig_enforce")
        .is_some_and(|v| v == "Y" || v == "1")
}

pub fn lockdown(sys: &Sys) -> Option<String> {
    let text = sys.read_trim("/sys/kernel/security/lockdown")?;
    let start = text.find('[')?;
    let end = text[start..].find(']')? + start;
    Some(text[start + 1..end].to_string())
}

pub fn module_builtin(sys: &Sys, kernel: &str, ko: &str) -> bool {
    sys.read(format!("/usr/lib/modules/{kernel}/modules.builtin"))
        .is_some_and(|t| t.lines().any(|l| l.trim().ends_with(&format!("/{ko}"))))
}

pub fn module_loaded(sys: &Sys, name: &str) -> bool {
    sys.exists(format!("/sys/module/{name}/initstate"))
}

pub fn kernels_with_headers(sys: &Sys) -> Vec<String> {
    sys.list("/usr/lib/modules")
        .into_iter()
        .filter(|k| sys.is_dir(format!("/usr/lib/modules/{k}/build")))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trackpoint {
    pub serio: String,
    pub rate: Option<u32>,
    pub sensitivity: Option<u32>,
}

impl Trackpoint {
    pub fn attr(&self, name: &str) -> String {
        format!("/sys/bus/serio/devices/{}/{name}", self.serio)
    }
}

pub fn trackpoint(sys: &Sys) -> Option<Trackpoint> {
    let s = serios(sys).into_iter().find(|s| s.protocol == "TPPS/2")?;
    let num = |f: &str| {
        sys.read_trim(format!("/sys/bus/serio/devices/{}/{f}", s.name))
            .and_then(|v| v.parse().ok())
    };
    Some(Trackpoint {
        rate: num("rate"),
        sensitivity: num("sensitivity"),
        serio: s.name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES: &str = "I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\n\
N: Name=\"AT Translated Set 2 keyboard\"\n\
P: Phys=isa0060/serio0/input0\n\
S: Sysfs=/devices/platform/i8042/serio0/input/input3\n\
H: Handlers=sysrq kbd leds event3 \n\
\n\
N: Name=\"Synaptics TM3471-020\"\n\
P: Phys=rmi4-00/input0\n\
S: Sysfs=/devices/pci0000:00/0000:00:14.0/i2c-10/10-002c/rmi4-00/input/input28\n\
H: Handlers=event15 mouse1 \n\
\n\
N: Name=\"TPPS/2 Elan TrackPoint\"\n\
P: Phys=rmi4-00.fn03/serio0/input0\n\
H: Handlers=event16 mouse2\n";

    #[test]
    fn input_list() {
        let d = parse_input_devices(DEVICES);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].name, "AT Translated Set 2 keyboard");
        assert_eq!(d[0].handlers, vec!["sysrq", "kbd", "leds", "event3"]);
        assert_eq!(d[1].phys, "rmi4-00/input0");
        assert!(d[1].sysfs.ends_with("input28"));
        assert_eq!(d[2].sysfs, "");
        assert!(parse_input_devices("").is_empty());
        assert_eq!(parse_input_devices("garbage line\n").len(), 0);
    }

    #[test]
    fn transports() {
        let dev = |name: &str, phys: &str| InputDevice {
            name: name.into(),
            phys: phys.into(),
            ..Default::default()
        };
        assert_eq!(
            touchpad_transport(&parse_input_devices(DEVICES)),
            Transport::Rmi4("Synaptics TM3471-020".into())
        );
        assert_eq!(
            touchpad_transport(&[dev("SynPS/2 Synaptics TouchPad", "isa0060/serio1/input0")]),
            Transport::SynapticsPs2
        );
        assert_eq!(
            touchpad_transport(&[dev("ETPS/2 Elantech Touchpad", "isa0060/serio1/input0")]),
            Transport::ElantechPs2("ETPS/2 Elantech Touchpad".into())
        );
        assert_eq!(
            touchpad_transport(&[dev(
                "AlpsPS/2 ALPS GlidePoint TouchPad",
                "isa0060/serio1/input0"
            )]),
            Transport::OtherPs2("AlpsPS/2 ALPS GlidePoint TouchPad".into())
        );
        assert_eq!(
            touchpad_transport(&[dev("TPPS/2 Elan TrackPoint", "rmi4-00.fn03/serio0/input0")]),
            Transport::Missing
        );
        assert_eq!(touchpad_transport(&[]), Transport::Missing);
    }

    #[test]
    fn describe() {
        assert_eq!(Transport::Rmi4("X".into()).describe(), "rmi4/smbus (X)");
        assert_eq!(
            Transport::SynapticsPs2.describe(),
            "ps/2 (SynPS/2 Synaptics TouchPad)"
        );
        assert_eq!(Transport::ElantechPs2("E".into()).describe(), "ps/2 (E)");
        assert_eq!(Transport::OtherPs2("O".into()).describe(), "ps/2 (O)");
        assert_eq!(Transport::Missing.describe(), "no touchpad found");
    }

    #[test]
    fn pnp() {
        assert_eq!(pnp_ids("PNP: LEN2064 PNP0f13"), vec!["LEN2064", "PNP0f13"]);
        assert_eq!(pnp_ids("LEN0411"), vec!["LEN0411"]);
        assert!(pnp_ids("").is_empty());
        let s = Serio {
            firmware_id: "PNP: PNP0f13 LEN2064".into(),
            ..Default::default()
        };
        assert_eq!(s.vendor_pnp().as_deref(), Some("LEN2064"));
        let s = Serio {
            firmware_id: "PNP: PNP0f13".into(),
            ..Default::default()
        };
        assert_eq!(s.vendor_pnp(), None);
    }

    #[test]
    fn kernel_series() {
        assert_eq!(series("7.2.7-hardened1-1-hardened").as_deref(), Some("7.2"));
        assert_eq!(series("7.2.3").as_deref(), Some("7.2"));
        assert_eq!(series("7.3-rc1").as_deref(), Some("7.3"));
        assert_eq!(series("6.12.10-200.fc41.x86_64").as_deref(), Some("6.12"));
        assert_eq!(series("7"), None);
        assert_eq!(series("x.2"), None);
        assert_eq!(series("7.rc"), None);
        assert_eq!(series(""), None);
    }

    #[test]
    fn dmi_label() {
        let mut d = Dmi::default();
        assert_eq!(d.label(), "unknown machine");
        d.vendor = "LENOVO".into();
        assert_eq!(d.label(), "LENOVO");
        assert!(d.is_lenovo());
        d.product_version = "ThinkPad P14s Gen 1".into();
        d.product_name = "20Y10007MX".into();
        assert_eq!(d.label(), "LENOVO ThinkPad P14s Gen 1 20Y10007MX");
        d.vendor.clear();
        assert_eq!(d.label(), "ThinkPad P14s Gen 1 20Y10007MX");
        assert!(!d.is_lenovo());
    }

    #[test]
    fn acpi_sta() {
        let dev = |status| AcpiDevice {
            name: "x".into(),
            hid: "SMB0001".into(),
            status,
            path: None,
        };
        assert!(dev(None).present());
        assert!(dev(Some(15)).present());
        assert!(!dev(Some(0)).present());
        assert!(!dev(Some(14)).present());
    }
}
