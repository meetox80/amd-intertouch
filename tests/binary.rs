mod common;

use common::*;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_amd-intertouch"))
}

#[test]
fn help() {
    let out = bin().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!(
            "amd-intertouch {}\nmade by kotwicha!\n",
            env!("CARGO_PKG_VERSION")
        )
    );
    let out = bin().arg("--help").output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("{}\nmade by kotwicha!\n", amd_intertouch::cli::USAGE)
    );
}

#[test]
fn usage_error() {
    let out = bin().arg("nope").output().unwrap();
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("error: unknown command `nope`"));
    assert!(out.stdout.is_empty());
}

#[test]
fn check() {
    let f = p14s_ps2();
    let out = bin()
        .args(["--root", &f.root_str(), "check"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("ok   touchpad   Synaptics on ps/2 (LEN2064)\n"),
        "{text}"
    );
    assert!(!text.contains('\x1b'));
}

#[test]
fn unsupported() {
    let f = p14s_ps2();
    f.remove("/sys/bus/acpi/devices/SMB0001:00");
    let out = bin()
        .args(["--root", &f.root_str(), "check"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn needs_root() {
    let f = p14s_ps2();
    f.uid(1000);
    for cmd in [
        &["try"][..],
        &["restore"],
        &["install"],
        &["uninstall"],
        &["nosleep", "on"],
        &["trackpoint"],
    ] {
        let out = bin()
            .args(["--root", &f.root_str()])
            .args(cmd)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "{cmd:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("needs root"),
            "{cmd:?}"
        );
    }
}

#[test]
fn dry_run() {
    let f = p14s_ps2();
    f.uid(1000);
    let out = bin()
        .args(["--root", &f.root_str(), "-n", "trackpoint", "--rate=100"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("write   /etc/udev/rules.d/70-amd-intertouch-trackpoint.rules"));
    assert!(text.contains("set     /sys/bus/serio/devices/serio2/rate = 100"));
    assert!(!f.exists("/etc/udev/rules.d/70-amd-intertouch-trackpoint.rules"));
}
