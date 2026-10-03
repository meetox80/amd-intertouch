mod common;

use amd_intertouch::live;
use amd_intertouch::runner::{FakeRunner, Output};
use common::*;

use amd_intertouch::app::{Env, run_with};
use amd_intertouch::output::Style;
use std::io::{self, Write};

struct FailAfter {
    left: usize,
    failed: bool,
}

impl Write for FailAfter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.left == 0 {
            self.failed = true;
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
        }
        self.left -= 1;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

type Scenario = fn() -> (Fixture, FakeRunner, Vec<&'static str>);

fn sweep(name: &str, scenario: Scenario) {
    for n in 0..2000 {
        let (f, runner, args) = scenario();
        let mut full = vec!["--root".to_string(), f.root_str()];
        full.extend(args.iter().map(|s| s.to_string()));
        let env = Env {
            runner: &runner,
            sleep: false,
            style: Style::plain(),
            exe: None,
        };
        let mut out = FailAfter {
            left: n,
            failed: false,
        };
        let mut err = Vec::new();
        let code = run_with(&full, &env, &mut out, &mut err);
        if !out.failed {
            assert!(n > 0, "{name} wrote nothing");
            return;
        }
        assert_eq!(code, 1, "{name}: failing after {n} writes");
        assert_eq!(
            String::from_utf8(err).unwrap(),
            "error: closed\n",
            "{name}: {n}"
        );
    }
    panic!("{name} never finished");
}

fn swapped() -> Fixture {
    let f = p14s_ps2();
    f.file(
        "/proc/bus/input/devices",
        &format!("{KEYBOARD}\n{RMI4_POINTERS}\n"),
    );
    f
}

#[test]
fn broken_output() {
    let scenarios: Vec<(&str, Scenario)> = vec![
        ("help", || (p14s_ps2(), FakeRunner::new(), vec!["--help"])),
        ("version", || {
            (p14s_ps2(), FakeRunner::new(), vec!["--version"])
        }),
        ("check", || (p14s_ps2(), FakeRunner::new(), vec!["check"])),
        ("status", || {
            (p14s_rmi4(), installed_runner(), vec!["status"])
        }),
        ("status-stock", || {
            (p14s_ps2(), FakeRunner::new(), vec!["status"])
        }),
        ("try", || {
            let runner = FakeRunner::new().on_query(
                "journalctl -k --no-pager",
                Output::success("piix4_smbus 0000:00:14.0: ASF Host Notify on, IRQ 7\n"),
            );
            (swapped(), runner, vec!["try"])
        }),
        ("try-ps2", || (p14s_ps2(), FakeRunner::new(), vec!["try"])),
        ("try-cached", || {
            let f = p14s_ps2();
            let dir = live::build_dir(KERNEL);
            f.file(&live::stamp_path(KERNEL), &live::stamp_text(KERNEL));
            for (_, ko) in live::MODULES {
                f.file(&format!("{dir}/{ko}"), "");
            }
            (f, FakeRunner::new(), vec!["try"])
        }),
        ("try-dry", || {
            (p14s_ps2(), FakeRunner::new(), vec!["--dry-run", "try"])
        }),
        ("restore", || {
            (p14s_ps2(), FakeRunner::new(), vec!["restore"])
        }),
        ("install", || {
            let f = p14s_ps2();
            f.file("/usr/lib/modprobe.d/amd-intertouch.conf", "x");
            let runner = FakeRunner::new().on_query(
                "dkms status",
                Output::success("amd-intertouch/0.0.9: added\namd-intertouch/0.1.1: added\n"),
            );
            (f, runner, vec!["install"])
        }),
        ("install-nothing", || {
            let f = p14s_ps2();
            f.file("/usr/src/amd-intertouch-0.1.1/dkms.conf", "pkg")
                .file("/usr/bin/pacman", "")
                .file("/usr/lib/modprobe.d/amd-intertouch.conf", "x");
            let runner = FakeRunner::new().on_query("pacman -Qqo", Output::success("pkg\n"));
            (f, runner, vec!["install"])
        }),
        ("uninstall", || {
            (p14s_ps2(), FakeRunner::new(), vec!["uninstall"])
        }),
        ("nosleep", || {
            (p14s_ps2(), FakeRunner::new(), vec!["nosleep", "on"])
        }),
        ("nosleep-now", || {
            (
                p14s_rmi4(),
                FakeRunner::new(),
                vec!["nosleep", "off", "--now"],
            )
        }),
        ("trackpoint", || {
            let f = p14s_ps2();
            f.file("/etc/udev/rules.d/70-thinkpad-pointer.rules", "x")
                .remove("/sys/bus/serio/devices/serio2");
            (f, FakeRunner::new(), vec!["trackpoint"])
        }),
        ("trackpoint-remove", || {
            (
                p14s_ps2(),
                FakeRunner::new(),
                vec!["trackpoint", "--remove"],
            )
        }),
        ("try-rollback", || {
            let dir = live::build_dir(KERNEL);
            let runner = FakeRunner::new()
                .on_exec(&format!("insmod {dir}/drivers/input/mouse/psmouse.ko"), 1)
                .on_exec("modprobe psmouse", 1);
            (p14s_ps2(), runner, vec!["try"])
        }),
    ];
    for (name, scenario) in scenarios {
        sweep(name, scenario);
    }
}
