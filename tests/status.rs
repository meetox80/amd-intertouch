mod common;

use amd_intertouch::runner::{FakeRunner, Output};
use common::*;

#[test]
fn installed() {
    let f = p14s_rmi4();
    let r = run(&f, &installed_runner(), &["status"]);
    assert_eq!(r.code, 0);
    assert_eq!(
        r.out,
        "touchpad     rmi4/smbus (Synaptics TM3471-020)\n\
         host notify  enabled, IRQ 7\n\
         i2c_piix4    patched build installed and loaded\n\
         rmi_smbus    patched build installed and loaded\n\
         psmouse      patched build installed and loaded\n\
         dkms         amd-intertouch 0.1.0: installed on 7.2.7-arch1-1, installed on 7.2.7-hardened1-1-hardened\n\
         nosleep      auto (on for known pads: LEN2064)\n\
         trackpoint   serio4: rate 200, sensitivity 200\n"
    );
}

#[test]
fn stock() {
    let f = p14s_ps2();
    let runner = FakeRunner::new()
        .on_query(
            "modinfo -n i2c_piix4",
            Output::success(&format!(
                "/lib/modules/{KERNEL}/kernel/drivers/i2c/busses/i2c-piix4.ko.zst\n"
            )),
        )
        .on_query(
            "modinfo -n rmi_smbus",
            Output::success(&format!(
                "/lib/modules/{KERNEL}/kernel/drivers/input/rmi4/rmi_smbus.ko.zst\n"
            )),
        )
        .on_query(
            "modinfo -F srcversion i2c_piix4",
            Output::success("STOCKPIIX4\n"),
        )
        .on_query(
            "modinfo -F srcversion rmi_smbus",
            Output::success("OTHER\n"),
        )
        .on_query("dmesg", Output::success("usb 1-1: hello\n"));
    let r = run(&f, &runner, &["status"]);
    assert_eq!(
        r.out,
        "touchpad     ps/2 (SynPS/2 Synaptics TouchPad)\n\
         host notify  no ASF Host Notify messages this boot\n\
         i2c_piix4    stock\n\
         rmi_smbus    stock installed, another build loaded (live `try`)\n\
         psmouse      loaded, build unknown\n\
         dkms         amd-intertouch is not registered\n\
         nosleep      not available: the loaded psmouse has no amd-intertouch patch\n\
         trackpoint   serio2: rate 100, sensitivity 128\n"
    );
    assert!(runner.log().contains(&"query dmesg".to_string()));
}

#[test]
fn not_loaded() {
    let f = p14s_ps2();
    f.remove("/sys/module")
        .remove("/sys/bus/serio/devices/serio2");
    let runner = FakeRunner::new().on_query(
        "modinfo -n",
        Output::success("/lib/modules/k/updates/dkms/x.ko\n"),
    );
    let r = run(&f, &runner, &["status"]);
    assert!(
        r.out
            .contains("i2c_piix4    patched build installed, not loaded\n")
    );
    assert!(r.out.contains("trackpoint   not found\n"));
    assert!(r.out.contains("host notify  no ASF Host Notify messages"));
}

#[test]
fn legacy_notify() {
    let f = p14s_rmi4();
    f.file("/sys/bus/serio/devices/serio4/rate", "\n");
    let runner = FakeRunner::new()
        .on_query(
            "journalctl -k -b",
            Output::success(
                "piix4_smbus 0000:00:14.0: ASF Host Notify off: SMB0001 has no interrupt\n",
            ),
        )
        .on_query(
            "dkms status",
            Output::success(
                "thinkpad-t14-amd-touchpad/0.2.2, k, x86_64: installed (Original modules exist)\n",
            ),
        );
    let r = run(&f, &runner, &["status"]);
    assert!(
        r.out
            .contains("host notify  not enabled: SMB0001 has no interrupt\n")
    );
    assert!(
        r.out
            .contains("dkms         thinkpad-t14-amd-touchpad 0.2.2: installed on k\n")
    );
    assert!(r.out.contains("i2c_piix4    loaded, build unknown\n"));
    assert!(
        r.out
            .contains("trackpoint   serio4: rate ?, sensitivity 200\n")
    );
}

#[test]
fn other_build() {
    let f = p14s_rmi4();
    let runner =
        installed_runner().on_query("modinfo -F srcversion psmouse", Output::success("NEWER\n"));
    let r = run(&f, &runner, &["status"]);
    assert!(r.out.contains(
        "psmouse      patched build installed, another build loaded (reboot or `restore` to switch)\n"
    ));
    assert!(
        r.out
            .contains("i2c_piix4    patched build installed and loaded\n")
    );
}
