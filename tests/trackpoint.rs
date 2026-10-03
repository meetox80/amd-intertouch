mod common;

use amd_intertouch::runner::FakeRunner;
use common::*;

#[test]
fn apply() {
    let f = p14s_ps2();
    let runner = FakeRunner::new();
    let r = run(&f, &runner, &["trackpoint"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(
        f.read("/etc/udev/rules.d/70-amd-intertouch-trackpoint.rules"),
        "ACTION==\"bind\", SUBSYSTEM==\"serio\", DRIVER==\"psmouse\", ATTR{protocol}==\"TPPS/2\", ATTR{rate}=\"200\", ATTR{sensitivity}=\"200\"\n"
    );
    assert_eq!(f.read("/sys/bus/serio/devices/serio2/rate"), "200");
    assert_eq!(f.read("/sys/bus/serio/devices/serio2/sensitivity"), "200");
    assert_eq!(runner.execs(), vec!["udevadm control --reload"]);
    assert_eq!(
        r.out,
        "trackpoint: rate 200, sensitivity 200, kept across reboots\n"
    );
}

#[test]
fn custom() {
    let f = p14s_rmi4();
    let r = run(
        &f,
        &FakeRunner::new(),
        &["trackpoint", "--rate", "100", "--sensitivity", "160"],
    );
    assert_eq!(r.code, 0);
    assert_eq!(f.read("/sys/bus/serio/devices/serio4/rate"), "100");
    assert_eq!(f.read("/sys/bus/serio/devices/serio4/sensitivity"), "160");
}

#[test]
fn no_sensitivity() {
    let f = p14s_ps2();
    f.remove("/sys/bus/serio/devices/serio2/sensitivity");
    let r = run(&f, &FakeRunner::new(), &["--dry-run", "trackpoint"]);
    assert!(
        r.out
            .contains("set     /sys/bus/serio/devices/serio2/rate = 200")
    );
    assert!(!r.out.contains("sensitivity = "));
}

#[test]
fn remove_rule() {
    let f = p14s_ps2();
    f.file("/etc/udev/rules.d/70-amd-intertouch-trackpoint.rules", "x")
        .file("/etc/udev/rules.d/70-thinkpad-pointer.rules", "x");
    let runner = FakeRunner::new();
    let r = run(&f, &runner, &["trackpoint", "--remove"]);
    assert_eq!(r.code, 0);
    assert!(!f.exists("/etc/udev/rules.d/70-amd-intertouch-trackpoint.rules"));
    assert_eq!(
        r.out,
        "note: /etc/udev/rules.d/70-thinkpad-pointer.rules also sets trackpoint attributes; remove it so only one rule applies\n\
         trackpoint rule removed; current values stay until the next boot\n"
    );
    f.remove("/sys/bus/serio/devices/serio2");
    let r = run(&f, &runner, &["--dry-run", "trackpoint"]);
    assert!(
        r.out
            .contains("note: no trackpoint found right now; the rule applies when one appears\n")
    );
}

#[test]
fn invalid() {
    let f = p14s_ps2();
    let runner = FakeRunner::new();
    let r = run(&f, &runner, &["trackpoint", "--rate", "150"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.err,
        "error: rate must be one of 10, 20, 40, 60, 80, 100, 200, not 150\n"
    );
    assert!(!f.exists("/etc/udev/rules.d/70-amd-intertouch-trackpoint.rules"));
    assert!(runner.execs().is_empty());
}
