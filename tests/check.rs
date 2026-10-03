mod common;

use amd_intertouch::runner::FakeRunner;
use common::*;

#[test]
fn p14s() {
    let f = p14s_ps2();
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 0, "{}", r.out);
    assert_eq!(
        r.out,
        "ok   machine    LENOVO ThinkPad P14s Gen 1 20Y10007MX\n\
         ok   smbus      AMD FCH SMBus controller (1022:790b)\n\
         ok   asf        SMB0001 at \\_SB_.PCI0.SMB1\n\
         ok   touchpad   Synaptics on ps/2 (LEN2064)\n\
         ok   model      ThinkPad T14 / P14s Gen 1 AMD (LEN2064): works; NoSleep is on by default for this pad\n\
         ok   kernel     7.2.7-hardened1-1-hardened (sources from 7.2.3)\n\
         ok   headers    /usr/lib/modules/7.2.7-hardened1-1-hardened/build\n\
         ok   toolchain  make, cc, patch\n\
         ok   dkms       available\n\
         ok   signing    unsigned modules can load\n\
         \n\
         supported: ready for `try` and `install`\n"
    );
    assert_eq!(r.err, "");
}

#[test]
fn on_rmi4() {
    let f = p14s_rmi4();
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 0);
    assert!(
        r.out
            .contains("ok   touchpad   already on rmi4/smbus (Synaptics TM3471-020, LEN2064)\n")
    );
}

#[test]
fn l14() {
    let f = Fixture::new();
    amd_thinkpad(&f);
    ps2_pad(&f, "LEN0411");
    f.file("/sys/class/dmi/id/product_version", "ThinkPad L14 Gen 1\n");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 2);
    assert!(
        r.out
            .contains("warn model      ThinkPad L14 Gen 1 AMD (LEN0411): same chipset")
    );
    assert!(
        r.out
            .ends_with("hardware looks right but this model is not confirmed: start with `try`\n")
    );
}

#[test]
fn unknown_id() {
    let f = Fixture::new();
    amd_thinkpad(&f);
    ps2_pad(&f, "LEN9999");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 2);
    assert!(
        r.out
            .contains("warn model      LEN9999 is not a known model; untested\n")
    );
}

#[test]
fn missing_id() {
    let f = p14s_ps2();
    f.file(
        "/sys/bus/serio/devices/serio1/firmware_id",
        "PNP: PNP0f13\n",
    );
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 2);
    assert!(
        r.out
            .contains("warn model      touchpad id unknown; untested\n")
    );
    assert!(r.out.contains("Synaptics on ps/2 (unknown id)"));
}

#[test]
fn intel() {
    let f = p14s_ps2();
    f.file("/sys/bus/pci/devices/0000:00:14.0/vendor", "0x8086\n")
        .remove("/sys/bus/acpi/devices/SMB0001:00");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 1);
    assert!(
        r.out
            .contains("fail smbus      no AMD FCH SMBus controller (1022:790b)\n")
    );
    assert!(
        r.out
            .contains("fail asf        no ACPI SMB0001 device, so Host Notify cannot be enabled\n")
    );
    assert!(r.out.ends_with("this machine cannot use the fix\n"));
}

#[test]
fn smb0001_off() {
    let f = p14s_ps2();
    f.file("/sys/bus/acpi/devices/SMB0001:00/status", "0\n");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 1);
    assert!(r.out.contains("fail asf"));
}

#[test]
fn asf_no_path() {
    let f = p14s_ps2();
    f.remove("/sys/bus/acpi/devices/SMB0001:00/path");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert!(r.out.contains("ok   asf        SMB0001 at SMB0001:00\n"));
}

#[test]
fn elan() {
    let f = p14s_ps2();
    f.file(
        "/proc/bus/input/devices",
        "N: Name=\"ETPS/2 Elantech Touchpad\"\nP: Phys=isa0060/serio1/input0\n",
    );
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 1);
    assert!(r.out.contains(
        "fail touchpad   ETPS/2 Elantech Touchpad: only Synaptics touchpads are handled\n"
    ));
    f.file(
        "/proc/bus/input/devices",
        "N: Name=\"AlpsPS/2 ALPS GlidePoint TouchPad\"\nP: Phys=isa0060/serio1/input0\n",
    );
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert!(
        r.out
            .contains("fail touchpad   AlpsPS/2 ALPS GlidePoint TouchPad: only Synaptics")
    );
}

#[test]
fn no_touchpad() {
    let f = p14s_ps2();
    f.file("/proc/bus/input/devices", KEYBOARD);
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 1);
    assert!(
        r.out
            .contains("fail touchpad   no ps/2 or rmi4 touchpad found\n")
    );
}

#[test]
fn other_kernel() {
    let f = p14s_ps2();
    f.file("/proc/sys/kernel/osrelease", "7.3.1-arch1-1\n");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 1);
    assert!(
        r.out.contains(
            "warn kernel     7.3.1-arch1-1: sources are from 7.2.3, the build may fail\n"
        )
    );
    assert!(
        r.out
            .contains("fail headers    kernel headers for 7.3.1-arch1-1 are not installed\n")
    );
    assert!(
        r.out
            .ends_with("hardware looks right; fix the failed items above first\n")
    );
}

#[test]
fn no_release() {
    let f = p14s_ps2();
    f.remove("/proc/sys/kernel/osrelease");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert!(
        r.out
            .contains("fail kernel     cannot read the kernel release\n")
    );
    assert!(!r.out.contains("headers"));
}

#[test]
fn toolchain() {
    let f = p14s_ps2();
    f.remove("/usr/bin/make")
        .remove("/usr/bin/patch")
        .remove("/usr/bin/dkms");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert!(r.out.contains("fail toolchain  missing: make, patch\n"));
    assert!(
        r.out
            .contains("warn dkms       not installed: needed for `install`, not for `try`\n")
    );
    f.remove("/usr/bin/cc").file("/usr/sbin/gcc", "");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert!(r.out.contains("fail toolchain  missing: make, patch\n"));
    f.remove("/usr/sbin/gcc");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert!(r.out.contains("fail toolchain  missing: make, cc, patch\n"));
}

#[test]
fn signing() {
    let f = p14s_ps2();
    f.bytes(amd_intertouch::hw::SECURE_BOOT_VAR, &[6, 0, 0, 0, 1]);
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 0);
    assert!(r.out.contains("warn signing    Secure Boot is on: unsigned modules may be refused unless signed with an enrolled MOK\n"));
    f.file(
        "/sys/kernel/security/lockdown",
        "none [integrity] confidentiality\n",
    );
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 1);
    assert!(
        r.out
            .contains("fail signing    kernel lockdown (integrity) refuses unsigned modules\n")
    );
    f.file("/sys/module/module/parameters/sig_enforce", "Y\n");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert!(
        r.out
            .contains("fail signing    module.sig_enforce is on: unsigned modules are refused\n")
    );
    f.file("/sys/module/module/parameters/sig_enforce", "N\n")
        .remove("/sys/kernel/security/lockdown")
        .remove("/sys/firmware");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert!(
        r.out
            .contains("ok   signing    unsigned modules can load\n")
    );
}

#[test]
fn legacy() {
    let f = p14s_ps2();
    f.dir("/var/lib/dkms/thinkpad-t14-amd-touchpad/0.2.2");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert_eq!(r.code, 0);
    assert!(r.out.contains("warn conflict   thinkpad-t14-amd-touchpad is installed through DKMS; remove it before `install`\n"));
}

#[test]
fn non_lenovo() {
    let f = p14s_ps2();
    f.file("/sys/class/dmi/id/sys_vendor", "Framework\n");
    let r = run(&f, &FakeRunner::new(), &["check"]);
    assert!(r.out.starts_with(
        "warn machine    Framework ThinkPad P14s Gen 1 20Y10007MX (not a Lenovo, untested)\n"
    ));
}
