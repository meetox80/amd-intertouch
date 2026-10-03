mod common;

use amd_intertouch::runner::FakeRunner;
use common::*;

#[test]
fn save() {
    let f = p14s_rmi4();
    let runner = FakeRunner::new();
    let r = run(&f, &runner, &["nosleep", "on"]);
    assert_eq!(r.code, 0);
    assert_eq!(
        f.read("/etc/modprobe.d/amd-intertouch-nosleep.conf"),
        "options psmouse synaptics_rmi_nosleep=1\n"
    );
    assert_eq!(
        f.read("/sys/module/psmouse/parameters/synaptics_rmi_nosleep"),
        "1"
    );
    assert!(runner.execs().is_empty());
    assert_eq!(
        r.out,
        "nosleep on: applies at the next boot, or rerun with --now\n"
    );
}

#[test]
fn auto_now() {
    let f = p14s_rmi4();
    f.file("/etc/modprobe.d/amd-intertouch-nosleep.conf", "x");
    let runner = FakeRunner::new();
    let r = run(&f, &runner, &["nosleep", "auto", "--now"]);
    assert_eq!(r.code, 0);
    assert!(!f.exists("/etc/modprobe.d/amd-intertouch-nosleep.conf"));
    assert_eq!(
        f.read("/sys/module/psmouse/parameters/synaptics_rmi_nosleep"),
        "-1"
    );
    assert_eq!(runner.execs(), vec!["rmmod psmouse", "modprobe psmouse"]);
    assert_eq!(
        r.out,
        "nosleep auto (on for known pads: LEN2064): psmouse reloaded\n"
    );
}

#[test]
fn no_param() {
    let f = p14s_ps2();
    let r = run(&f, &FakeRunner::new(), &["--dry-run", "nosleep", "off"]);
    assert_eq!(
        r.out,
        "dry run: nothing will be changed\n\
         note: the loaded psmouse has no synaptics_rmi_nosleep parameter yet; the setting applies once amd-intertouch is loaded\n\
         \x20 write   /etc/modprobe.d/amd-intertouch-nosleep.conf  (40 bytes, mode 644)\n"
    );
}

#[test]
fn reload_fail() {
    let f = p14s_rmi4();
    let runner = FakeRunner::new().on_exec("modprobe psmouse", 1);
    let r = run(&f, &runner, &["nosleep", "off", "--now"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        runner.execs(),
        vec!["rmmod psmouse", "modprobe psmouse", "modprobe psmouse"]
    );
}
