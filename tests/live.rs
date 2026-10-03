mod common;

use amd_intertouch::assets::ASSETS;
use amd_intertouch::dkmsconf;
use amd_intertouch::live;
use amd_intertouch::runner::{FakeRunner, Output};
use common::*;

fn swap_section(out: &str) -> Vec<&str> {
    let start = out
        .lines()
        .position(|l| l.starts_with("swapping in"))
        .unwrap();
    out.lines().skip(start + 1).collect()
}

#[test]
fn dry_run() {
    let f = p14s_ps2();
    f.uid(1000);
    let r = run(&f, &FakeRunner::new(), &["--dry-run", "try"]);
    assert_eq!(r.code, 0, "{}", r.err);
    let out = lines(&r.out);
    assert_eq!(out[0], "dry run: nothing will be changed");
    assert_eq!(out[1], format!("building the patched modules for {KERNEL}"));
    let dir = live::build_dir(KERNEL);
    let writes = out
        .iter()
        .filter(|l| l.starts_with(&format!("  write   {dir}/")))
        .count();
    assert_eq!(writes, ASSETS.len() + 1);
    let patches: Vec<&&str> = out.iter().filter(|l| l.contains("&& patch -p1")).collect();
    let expected = dkmsconf::embedded().patches;
    assert_eq!(patches.len(), expected.len());
    for (line, p) in patches.iter().zip(&expected) {
        assert!(line.ends_with(&format!("patches/{p})")));
    }
    assert!(out.contains(
        &format!("  run     make -C /usr/lib/modules/{KERNEL}/build M={dir} modules").as_str()
    ));
    assert_eq!(
        swap_section(&r.out),
        vec![
            "  run     modprobe i2c_smbus".to_string(),
            "  run     modprobe rmi_core".to_string(),
            "  run     rmmod psmouse".to_string(),
            "  run     rmmod rmi_smbus".to_string(),
            "  run     rmmod i2c_piix4".to_string(),
            format!("  run     insmod {dir}/drivers/i2c/busses/i2c-piix4.ko"),
            format!("  run     insmod {dir}/drivers/input/rmi4/rmi_smbus.ko"),
            format!("  run     insmod {dir}/drivers/input/mouse/psmouse.ko synaptics_intertouch=1"),
            "  wait    5000 ms".to_string(),
            "  on failure:".to_string(),
            "    run     rmmod psmouse  (failure ignored)".to_string(),
            "    run     rmmod rmi_smbus  (failure ignored)".to_string(),
            "    run     rmmod i2c_piix4  (failure ignored)".to_string(),
            "    run     modprobe i2c_piix4".to_string(),
            "    run     modprobe rmi_smbus".to_string(),
            "    run     modprobe psmouse".to_string(),
        ]
    );
    assert!(!f.exists(&dir));
}

#[test]
fn success() {
    let f = p14s_ps2();
    f.file(
        "/proc/bus/input/devices",
        &format!("{KEYBOARD}\n{RMI4_POINTERS}\n"),
    );
    let runner = FakeRunner::new().on_query(
        "journalctl -k --no-pager -o cat --since @",
        Output::success(
            "piix4_smbus 0000:00:14.0: ASF Host Notify on, IRQ 7\n\
             usb 3-1: unrelated\n\
             rmi4_smbus 10-002c: registering SMbus-connected sensor\n",
        ),
    );
    let r = run(&f, &runner, &["try"]);
    assert_eq!(r.code, 0, "{}\n{}", r.out, r.err);
    let dir = live::build_dir(KERNEL);
    for a in ASSETS {
        assert_eq!(
            std::fs::read(f.path(&format!("{dir}/{}", a.path))).unwrap(),
            a.data,
            "{}",
            a.path
        );
    }
    assert_eq!(f.mode(&format!("{dir}/tools/pre-build.sh")), 0o755);
    assert_eq!(f.read(&live::stamp_path(KERNEL)), live::stamp_text(KERNEL));
    let execs = runner.execs();
    let patches = dkmsconf::embedded().patches;
    for (i, p) in patches.iter().enumerate() {
        assert_eq!(
            execs[i],
            format!("(cd {dir} && patch -p1 --forward --no-backup-if-mismatch -s -i patches/{p})")
        );
    }
    assert_eq!(
        execs[patches.len()..].to_vec(),
        vec![
            format!("make -C /usr/lib/modules/{KERNEL}/build M={dir} modules"),
            "modprobe i2c_smbus".to_string(),
            "modprobe rmi_core".to_string(),
            "rmmod psmouse".to_string(),
            "rmmod rmi_smbus".to_string(),
            "rmmod i2c_piix4".to_string(),
            format!("insmod {dir}/drivers/i2c/busses/i2c-piix4.ko"),
            format!("insmod {dir}/drivers/input/rmi4/rmi_smbus.ko"),
            format!("insmod {dir}/drivers/input/mouse/psmouse.ko synaptics_intertouch=1"),
        ]
    );
    assert!(r.out.contains("kernel log since the swap:\n  piix4_smbus 0000:00:14.0: ASF Host Notify on, IRQ 7\n  rmi4_smbus 10-002c: registering SMbus-connected sensor\n"));
    assert!(!r.out.contains("unrelated"));
    assert!(
        r.out
            .contains("touchpad     rmi4/smbus (Synaptics TM3471-020)\n")
    );
    assert!(r.out.ends_with(
        "nothing is installed yet: `sudo amd-intertouch install` keeps it, `sudo amd-intertouch restore` or a reboot undoes it\n"
    ));
}

#[test]
fn cached() {
    let f = p14s_ps2();
    let dir = live::build_dir(KERNEL);
    f.file(&live::stamp_path(KERNEL), &live::stamp_text(KERNEL));
    for (_, ko) in live::MODULES {
        f.file(&format!("{dir}/{ko}"), "");
    }
    let runner = FakeRunner::new();
    let r = run(&f, &runner, &["try"]);
    assert_eq!(r.code, 1);
    assert!(
        r.out
            .starts_with(&format!("using the cached build in {dir}\n"))
    );
    assert!(
        runner
            .execs()
            .iter()
            .all(|e| !e.contains("patch") && !e.starts_with("make"))
    );
    assert!(r.out.contains("  (nothing relevant)\n"));
    assert!(r.out.ends_with(
        "the touchpad did not switch to rmi4/smbus; `sudo amd-intertouch restore` reloads the installed modules\n"
    ));
}

#[test]
fn stale_stamp() {
    let f = p14s_ps2();
    let dir = live::build_dir(KERNEL);
    f.file(&live::stamp_path(KERNEL), "0000000000000000 old\n");
    for (_, ko) in live::MODULES {
        f.file(&format!("{dir}/{ko}"), "");
    }
    let r = run(&f, &FakeRunner::new(), &["--dry-run", "try"]);
    assert!(r.out.contains("building the patched modules"));
}

#[test]
fn insmod_fail() {
    let f = p14s_ps2();
    let dir = live::build_dir(KERNEL);
    let runner =
        FakeRunner::new().on_exec(&format!("insmod {dir}/drivers/input/mouse/psmouse.ko"), 1);
    let r = run(&f, &runner, &["try"]);
    assert_eq!(r.code, 1);
    let execs = runner.execs();
    let tail: Vec<String> = execs[execs.len() - 6..].to_vec();
    assert_eq!(
        tail,
        vec![
            "rmmod psmouse",
            "rmmod rmi_smbus",
            "rmmod i2c_piix4",
            "modprobe i2c_piix4",
            "modprobe rmi_smbus",
            "modprobe psmouse"
        ]
    );
    assert!(r.out.contains("rolling back"));
    assert!(r.err.contains("exited with status 1"));
}

#[test]
fn loaded_only() {
    let f = p14s_ps2();
    f.remove("/sys/module/rmi_smbus");
    let r = run(&f, &FakeRunner::new(), &["--dry-run", "try"]);
    assert!(
        r.out
            .contains("  run     rmmod psmouse\n  run     rmmod i2c_piix4\n")
    );
    assert!(!r.out.contains("  run     rmmod rmi_smbus\n"));
}

#[test]
fn builtin() {
    let f = p14s_ps2();
    f.file(
        &format!("/usr/lib/modules/{KERNEL}/modules.builtin"),
        "kernel/drivers/input/mouse/psmouse.ko\n",
    );
    let r = run(&f, &FakeRunner::new(), &["try"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.err,
        "error: psmouse is built into this kernel, so it cannot be swapped live\n"
    );
}

#[test]
fn no_headers() {
    let f = p14s_ps2();
    f.remove(&format!("/usr/lib/modules/{KERNEL}/build"));
    let r = run(&f, &FakeRunner::new(), &["try"]);
    assert_eq!(
        r.err,
        format!("error: kernel headers for {KERNEL} are not installed\n")
    );
    f.remove("/proc/sys/kernel/osrelease");
    let r = run(&f, &FakeRunner::new(), &["try"]);
    assert_eq!(r.err, "error: cannot read the kernel release\n");
}

#[test]
fn needs_root() {
    let f = p14s_ps2();
    f.uid(1000);
    let runner = FakeRunner::new();
    let r = run(&f, &runner, &["try"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.err,
        "error: `try` needs root: run it with sudo, or add --dry-run to see what it would do\n"
    );
    assert!(runner.log().is_empty());
    f.remove("/proc/self/status");
    assert_eq!(run(&f, &runner, &["try"]).code, 1);
}

#[test]
fn restore() {
    let f = p14s_ps2();
    let runner = FakeRunner::new();
    let r = run(&f, &runner, &["restore"]);
    assert_eq!(r.code, 0);
    assert_eq!(
        runner.execs(),
        vec![
            "rmmod psmouse",
            "rmmod rmi_smbus",
            "rmmod i2c_piix4",
            "modprobe i2c_piix4",
            "modprobe rmi_smbus",
            "modprobe psmouse"
        ]
    );
    assert_eq!(
        r.out,
        "reloading the installed modules\ntouchpad     ps/2 (SynPS/2 Synaptics TouchPad)\n"
    );
    let r = run(&f, &runner, &["--dry-run", "restore"]);
    assert!(r.out.starts_with("dry run: nothing will be changed\nreloading the installed modules\n  run     rmmod psmouse  (failure ignored)\n"));
    assert!(!r.out.contains("touchpad"));
}
