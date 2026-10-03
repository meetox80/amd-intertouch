#![allow(dead_code)]

use amd_intertouch::app::{Env, run_with};
use amd_intertouch::output::Style;
use amd_intertouch::runner::{FakeRunner, Output, Runner};
use amd_intertouch::sys::Sys;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

pub const KERNEL: &str = "7.2.7-hardened1-1-hardened";
pub const KERNEL2: &str = "7.2.7-arch1-1";

static COUNTER: AtomicU32 = AtomicU32::new(0);

pub struct Fixture {
    pub root: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "amd-intertouch-it-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Fixture { root }
    }

    pub fn path(&self, p: &str) -> PathBuf {
        self.root.join(p.trim_start_matches('/'))
    }

    pub fn file(&self, p: &str, content: &str) -> &Self {
        self.bytes(p, content.as_bytes())
    }

    pub fn bytes(&self, p: &str, content: &[u8]) -> &Self {
        let path = self.path(p);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
        self
    }

    pub fn dir(&self, p: &str) -> &Self {
        fs::create_dir_all(self.path(p)).unwrap();
        self
    }

    pub fn remove(&self, p: &str) -> &Self {
        let path = self.path(p);
        if path.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else if path.exists() {
            fs::remove_file(path).unwrap();
        }
        self
    }

    pub fn read(&self, p: &str) -> String {
        fs::read_to_string(self.path(p)).unwrap()
    }

    pub fn exists(&self, p: &str) -> bool {
        self.path(p).exists()
    }

    pub fn mode(&self, p: &str) -> u32 {
        fs::metadata(self.path(p)).unwrap().permissions().mode() & 0o777
    }

    pub fn sys(&self) -> Sys {
        Sys::new(&self.root)
    }

    pub fn root_str(&self) -> String {
        self.root.to_str().unwrap().to_string()
    }

    pub fn uid(&self, uid: u32) -> &Self {
        self.file(
            "/proc/self/status",
            &format!("Name:\tamd-intertouch\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\n"),
        )
    }

    pub fn module(&self, name: &str, srcversion: &str) -> &Self {
        self.file(&format!("/sys/module/{name}/initstate"), "live\n")
            .file(
                &format!("/sys/module/{name}/srcversion"),
                &format!("{srcversion}\n"),
            )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub const KEYBOARD: &str = "I: Bus=0011 Vendor=0001 Product=0001 Version=ab83
N: Name=\"AT Translated Set 2 keyboard\"
P: Phys=isa0060/serio0/input0
S: Sysfs=/devices/platform/i8042/serio0/input/input3
U: Uniq=
H: Handlers=sysrq kbd leds event3
B: PROP=0
B: EV=120013
";

pub const PS2_POINTERS: &str = "I: Bus=0011 Vendor=0002 Product=0007 Version=01b1
N: Name=\"SynPS/2 Synaptics TouchPad\"
P: Phys=isa0060/serio1/input0
S: Sysfs=/devices/platform/i8042/serio1/input/input16
U: Uniq=
H: Handlers=event15 mouse1
B: PROP=5
B: EV=b

I: Bus=0011 Vendor=0002 Product=000a Version=0063
N: Name=\"TPPS/2 Elan TrackPoint\"
P: Phys=synaptics-pt/serio0/input0
S: Sysfs=/devices/platform/i8042/serio1/serio2/input/input22
U: Uniq=
H: Handlers=event16 mouse2
B: PROP=21
B: EV=7
";

pub const RMI4_POINTERS: &str = "I: Bus=001d Vendor=06cb Product=0000 Version=0000
N: Name=\"Synaptics TM3471-020\"
P: Phys=rmi4-01/input0
S: Sysfs=/devices/pci0000:00/0000:00:14.0/i2c-10/10-002c/rmi4-01/input/input32
U: Uniq=
H: Handlers=event15 mouse1
B: PROP=5
B: EV=b

I: Bus=0011 Vendor=0002 Product=000a Version=0000
N: Name=\"TPPS/2 Elan TrackPoint\"
P: Phys=rmi4-01.fn03/serio0/input0
S: Sysfs=/devices/pci0000:00/0000:00:14.0/i2c-10/10-002c/rmi4-01/rmi4-01.fn03/serio4/input/input33
U: Uniq=
H: Handlers=event16 mouse2
B: PROP=21
B: EV=7
";

pub fn amd_thinkpad(f: &Fixture) {
    f.uid(0)
        .file("/proc/sys/kernel/osrelease", &format!("{KERNEL}\n"))
        .dir(&format!("/usr/lib/modules/{KERNEL}/build"))
        .file(
            &format!("/usr/lib/modules/{KERNEL}/modules.builtin"),
            "kernel/drivers/i2c/i2c-core.ko\nkernel/drivers/input/serio/i8042.ko\n",
        )
        .file("/usr/bin/make", "")
        .file("/usr/bin/cc", "")
        .file("/usr/bin/patch", "")
        .file("/usr/bin/dkms", "")
        .file("/sys/class/dmi/id/sys_vendor", "LENOVO\n")
        .file("/sys/class/dmi/id/product_name", "20Y10007MX\n")
        .file("/sys/class/dmi/id/product_version", "ThinkPad P14s Gen 1\n")
        .file("/sys/bus/pci/devices/0000:00:14.0/vendor", "0x1022\n")
        .file("/sys/bus/pci/devices/0000:00:14.0/device", "0x790b\n")
        .file("/sys/bus/pci/devices/0000:07:00.0/vendor", "0x1002\n")
        .file("/sys/bus/pci/devices/0000:07:00.0/device", "0x1636\n")
        .file("/sys/bus/acpi/devices/SMB0001:00/hid", "SMB0001\n")
        .file("/sys/bus/acpi/devices/SMB0001:00/status", "15\n")
        .file(
            "/sys/bus/acpi/devices/SMB0001:00/path",
            "\\_SB_.PCI0.SMB1\n",
        )
        .file("/sys/bus/acpi/devices/LEN2064:00/hid", "LEN2064\n")
        .file("/sys/bus/acpi/devices/AMDI0010:00/hid", "AMDI0010\n")
        .file("/sys/bus/acpi/devices/AMDI0010:00/status", "15\n")
        .bytes(amd_intertouch::hw::SECURE_BOOT_VAR, &[6, 0, 0, 0, 0])
        .file("/sys/module/module/parameters/sig_enforce", "N\n")
        .file(
            "/sys/kernel/security/lockdown",
            "[none] integrity confidentiality\n",
        )
        .file(
            "/sys/bus/serio/devices/serio0/description",
            "i8042 KBD port\n",
        )
        .file("/sys/bus/serio/devices/serio0/protocol", "\n")
        .file(
            "/sys/bus/serio/devices/serio0/firmware_id",
            "PNP: LEN0071 PNP0303\n",
        );
}

pub fn ps2_pad(f: &Fixture, pnp: &str) {
    f.file(
        "/sys/bus/serio/devices/serio1/description",
        "i8042 AUX port\n",
    )
    .file("/sys/bus/serio/devices/serio1/protocol", "SynPS/2\n")
    .file(
        "/sys/bus/serio/devices/serio1/firmware_id",
        &format!("PNP: {pnp} PNP0f13\n"),
    )
    .file(
        "/sys/bus/serio/devices/serio2/description",
        "Synaptics pass-through\n",
    )
    .file("/sys/bus/serio/devices/serio2/protocol", "TPPS/2\n")
    .file("/sys/bus/serio/devices/serio2/firmware_id", "\n")
    .file("/sys/bus/serio/devices/serio2/rate", "100\n")
    .file("/sys/bus/serio/devices/serio2/sensitivity", "128\n")
    .file(
        "/proc/bus/input/devices",
        &format!("{KEYBOARD}\n{PS2_POINTERS}\n"),
    )
    .module("i2c_piix4", "STOCKPIIX4")
    .module("rmi_smbus", "STOCKRMI")
    .module("psmouse", "STOCKPSMOUSE");
}

pub fn rmi4_pad(f: &Fixture) {
    f.file(
        "/sys/bus/serio/devices/serio1/description",
        "i8042 AUX port\n",
    )
    .file("/sys/bus/serio/devices/serio1/protocol", "SynSMBus\n")
    .file(
        "/sys/bus/serio/devices/serio1/firmware_id",
        "PNP: LEN2064 PNP0f13\n",
    )
    .file(
        "/sys/bus/serio/devices/serio4/description",
        "RMI4 PS/2 pass-through\n",
    )
    .file("/sys/bus/serio/devices/serio4/protocol", "TPPS/2\n")
    .file("/sys/bus/serio/devices/serio4/firmware_id", "\n")
    .file("/sys/bus/serio/devices/serio4/rate", "200\n")
    .file("/sys/bus/serio/devices/serio4/sensitivity", "200\n")
    .file(
        "/proc/bus/input/devices",
        &format!("{KEYBOARD}\n{RMI4_POINTERS}\n"),
    )
    .module("i2c_piix4", "70BF56E401B9CE29D7B57D7")
    .module("rmi_smbus", "C57F5E26C7C3F61FB1CD13A")
    .module("psmouse", "4113D40A5EAC19AF8768F34")
    .file(
        "/sys/module/psmouse/parameters/synaptics_rmi_nosleep",
        "-1\n",
    );
}

pub fn p14s_ps2() -> Fixture {
    let f = Fixture::new();
    amd_thinkpad(&f);
    ps2_pad(&f, "LEN2064");
    f
}

pub fn p14s_rmi4() -> Fixture {
    let f = Fixture::new();
    amd_thinkpad(&f);
    rmi4_pad(&f);
    f
}

pub struct Run {
    pub code: i32,
    pub out: String,
    pub err: String,
}

pub fn run(f: &Fixture, runner: &dyn Runner, args: &[&str]) -> Run {
    run_exe(f, runner, None, args)
}

pub fn run_exe(f: &Fixture, runner: &dyn Runner, exe: Option<PathBuf>, args: &[&str]) -> Run {
    let mut full: Vec<String> = vec!["--root".into(), f.root_str()];
    full.extend(args.iter().map(|s| s.to_string()));
    let env = Env {
        runner,
        sleep: false,
        style: Style::plain(),
        exe,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run_with(&full, &env, &mut out, &mut err);
    Run {
        code,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}

pub fn lines(s: &str) -> Vec<&str> {
    s.lines().collect()
}

pub fn installed_runner() -> FakeRunner {
    FakeRunner::new()
        .on_query(
            "modinfo -n i2c_piix4",
            Output::success(&format!(
                "/lib/modules/{KERNEL}/updates/dkms/i2c-piix4.ko.zst\n"
            )),
        )
        .on_query(
            "modinfo -n rmi_smbus",
            Output::success(&format!(
                "/lib/modules/{KERNEL}/updates/dkms/rmi_smbus.ko.zst\n"
            )),
        )
        .on_query(
            "modinfo -n psmouse",
            Output::success(&format!(
                "/lib/modules/{KERNEL}/updates/dkms/psmouse.ko.zst\n"
            )),
        )
        .on_query(
            "modinfo -F srcversion i2c_piix4",
            Output::success("70BF56E401B9CE29D7B57D7\n"),
        )
        .on_query(
            "modinfo -F srcversion rmi_smbus",
            Output::success("C57F5E26C7C3F61FB1CD13A\n"),
        )
        .on_query(
            "modinfo -F srcversion psmouse",
            Output::success("4113D40A5EAC19AF8768F34\n"),
        )
        .on_query(
            "journalctl -k -b --no-pager -o cat",
            Output::success(
                "piix4_smbus 0000:00:14.0: SMB0001 port 0x0b20+32, GSI 7 level-triggered active-low\n\
                 piix4_smbus 0000:00:14.0: ASF Host Notify on, IRQ 7\n",
            ),
        )
        .on_query(
            "dkms status",
            Output::success(&format!(
                "amd-intertouch/0.1.1, {KERNEL2}, x86_64: installed\n\
                 amd-intertouch/0.1.1, {KERNEL}, x86_64: installed\n\
                 vboxhost/7.2.20_OSE, {KERNEL}, x86_64: installed\n"
            )),
        )
}
