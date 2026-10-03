mod common;

use amd_intertouch::assets::{self, ASSETS};
use amd_intertouch::dkmsconf;
use amd_intertouch::runner::{FakeRunner, Output};
use common::*;

fn fresh() -> (Fixture, FakeRunner) {
    let f = p14s_ps2();
    f.dir(&format!("/usr/lib/modules/{KERNEL2}/build"));
    let runner = FakeRunner::new().on_query("dkms status", Output::success(""));
    (f, runner)
}

#[test]
fn plan() {
    let (f, runner) = fresh();
    let r = run(&f, &runner, &["--dry-run", "install"]);
    assert_eq!(r.code, 0, "{}", r.err);
    let out = lines(&r.out);
    assert_eq!(out[0], "dry run: nothing will be changed");
    assert_eq!(
        out[1],
        "  remove  /usr/src/amd-intertouch-0.1.1  (recursive)"
    );
    let writes: Vec<&&str> = out
        .iter()
        .filter(|l| l.starts_with("  write   /usr/src/amd-intertouch-0.1.1/"))
        .collect();
    assert_eq!(writes.len(), ASSETS.len());
    let tail: Vec<&str> = out[2 + ASSETS.len()..].to_vec();
    assert_eq!(
        tail,
        vec![
            "  run     dkms add -m amd-intertouch -v 0.1.1",
            "  run     dkms install -m amd-intertouch -v 0.1.1 -k 7.2.7-arch1-1",
            "  run     dkms install -m amd-intertouch -v 0.1.1 -k 7.2.7-hardened1-1-hardened",
            "  write   /etc/modprobe.d/amd-intertouch.conf  (73 bytes, mode 644)",
            "  on failure:",
            "    run     dkms remove -m amd-intertouch -v 0.1.1 --all  (failure ignored)",
            "    remove  /usr/src/amd-intertouch-0.1.1  (recursive)",
            "    remove  /etc/modprobe.d/amd-intertouch.conf",
        ]
    );
    assert!(runner.execs().is_empty());
}

#[test]
fn copies_binary() {
    let (f, runner) = fresh();
    f.file("/home/u/build/amd-intertouch", "binary bytes");
    let exe = Some(f.path("/home/u/build/amd-intertouch"));
    let r = run_exe(&f, &runner, exe.clone(), &["--dry-run", "install"]);
    assert!(
        r.out
            .contains("installing the command to /usr/local/bin/amd-intertouch\n")
    );
    assert!(
        r.out
            .contains("  write   /usr/local/bin/amd-intertouch  (12 bytes, mode 755)\n")
    );
    assert!(
        r.out
            .contains("    remove  /usr/local/bin/amd-intertouch\n")
    );
    let r = run_exe(&f, &runner, exe, &["install"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(f.read("/usr/local/bin/amd-intertouch"), "binary bytes");
    assert_eq!(f.mode("/usr/local/bin/amd-intertouch"), 0o755);
}

#[test]
fn skips_copy() {
    let (f, runner) = fresh();
    f.file("/usr/local/bin/amd-intertouch", "old")
        .file("/usr/bin/amd-intertouch", "pkg");
    for exe in ["/usr/local/bin/amd-intertouch", "/usr/bin/amd-intertouch"] {
        let r = run_exe(&f, &runner, Some(f.path(exe)), &["--dry-run", "install"]);
        assert_eq!(r.code, 0, "{exe}");
        assert!(!r.out.contains("installing the command"), "{exe}");
        assert!(!r.out.contains("/usr/local/bin"), "{exe}");
    }
}

#[test]
fn bad_binary() {
    let (f, runner) = fresh();
    let exe = f.path("/missing/amd-intertouch");
    let r = run_exe(&f, &runner, Some(exe.clone()), &["install"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.err,
        format!(
            "error: {}: No such file or directory (os error 2)\n",
            exe.display()
        )
    );
    assert!(runner.execs().is_empty());
}

#[test]
fn binary_rollback() {
    let (f, _) = fresh();
    f.file("/home/u/amd-intertouch", "bin");
    let runner = FakeRunner::new()
        .on_query("dkms status", Output::success(""))
        .on_exec("dkms add", 3);
    let r = run_exe(
        &f,
        &runner,
        Some(f.path("/home/u/amd-intertouch")),
        &["install"],
    );
    assert_eq!(r.code, 1);
    assert!(!f.exists("/usr/local/bin/amd-intertouch"));
}

#[test]
fn real() {
    let (f, runner) = fresh();
    let r = run(&f, &runner, &["install", "--nosleep", "on"]);
    assert_eq!(r.code, 0, "{}", r.err);
    for a in ASSETS {
        assert_eq!(
            std::fs::read(f.path(&format!("/usr/src/amd-intertouch-0.1.1/{}", a.path))).unwrap(),
            a.data
        );
    }
    assert_eq!(
        f.mode("/usr/src/amd-intertouch-0.1.1/tools/pre-build.sh"),
        0o755
    );
    assert_eq!(f.mode("/usr/src/amd-intertouch-0.1.1/dkms.conf"), 0o644);
    assert_eq!(
        f.read("/etc/modprobe.d/amd-intertouch.conf"),
        "softdep i2c_piix4 post: rmi_smbus\noptions psmouse synaptics_intertouch=1\n"
    );
    assert_eq!(
        f.read("/etc/modprobe.d/amd-intertouch-nosleep.conf"),
        "options psmouse synaptics_rmi_nosleep=1\n"
    );
    assert_eq!(
        runner.execs(),
        vec![
            "dkms add -m amd-intertouch -v 0.1.1",
            "dkms install -m amd-intertouch -v 0.1.1 -k 7.2.7-arch1-1",
            "dkms install -m amd-intertouch -v 0.1.1 -k 7.2.7-hardened1-1-hardened",
        ]
    );
    assert!(
        r.out
            .ends_with("installed; reboot, then check with `amd-intertouch status`\n")
    );
}

#[test]
fn stale_files() {
    let (f, runner) = fresh();
    f.file("/usr/src/amd-intertouch-0.1.1/stale.c", "old");
    run(&f, &runner, &["install"]);
    assert!(!f.exists("/usr/src/amd-intertouch-0.1.1/stale.c"));
    assert!(f.exists("/usr/src/amd-intertouch-0.1.1/dkms.conf"));
}

#[test]
fn reinstall() {
    let (f, _) = fresh();
    let runner = FakeRunner::new().on_query(
        "dkms status",
        Output::success(&format!(
            "amd-intertouch/0.1.1, {KERNEL}, x86_64: installed\n"
        )),
    );
    let r = run(&f, &runner, &["install"]);
    assert_eq!(r.code, 0);
    assert!(r.out.starts_with("reinstalling amd-intertouch 0.1.1\n"));
    assert_eq!(
        runner.execs()[0],
        "dkms remove -m amd-intertouch -v 0.1.1 --all"
    );
    assert_eq!(runner.execs()[1], "dkms add -m amd-intertouch -v 0.1.1");
}

#[test]
fn upgrade() {
    let (f, _) = fresh();
    f.file("/usr/src/amd-intertouch-0.0.9/dkms.conf", "old");
    let runner = FakeRunner::new().on_query(
        "dkms status",
        Output::success(&format!(
            "amd-intertouch/0.0.9, {KERNEL}, x86_64: installed\namd-intertouch/0.0.9, {KERNEL2}, x86_64: installed\n"
        )),
    );
    let r = run(&f, &runner, &["install"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.starts_with("replacing amd-intertouch 0.0.9\n"));
    assert_eq!(
        runner.execs()[0],
        "dkms remove -m amd-intertouch -v 0.0.9 --all"
    );
    assert_eq!(
        runner
            .execs()
            .iter()
            .filter(|e| e.contains("0.0.9"))
            .count(),
        1
    );
    assert!(!f.exists("/usr/src/amd-intertouch-0.0.9"));
}

#[test]
fn packaged() {
    let (f, _) = fresh();
    f.file("/usr/src/amd-intertouch-0.1.1/dkms.conf", "pkg")
        .file("/usr/bin/pacman", "");
    let runner = FakeRunner::new()
        .on_query("dkms status", Output::success(""))
        .on_query(
            "pacman -Qqo /usr/src/amd-intertouch-0.1.1/dkms.conf",
            Output::success("amd-intertouch-dkms\n"),
        );
    let r = run(&f, &runner, &["install", "--nosleep", "off"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out
            .starts_with("amd-intertouch 0.1.1 is managed by the package `amd-intertouch-dkms`")
    );
    assert!(runner.execs().is_empty());
    assert_eq!(f.read("/usr/src/amd-intertouch-0.1.1/dkms.conf"), "pkg");
    assert!(f.exists("/etc/modprobe.d/amd-intertouch.conf"));
    assert!(f.exists("/etc/modprobe.d/amd-intertouch-nosleep.conf"));
}

#[test]
fn packaged_noop() {
    let (f, _) = fresh();
    f.file("/usr/src/amd-intertouch-0.1.1/dkms.conf", "pkg")
        .file("/usr/bin/pacman", "")
        .file("/usr/lib/modprobe.d/amd-intertouch.conf", "x");
    let runner = FakeRunner::new()
        .on_query("dkms status", Output::success(""))
        .on_query("pacman -Qqo", Output::success("amd-intertouch-dkms\n"));
    let r = run(&f, &runner, &["install"]);
    assert_eq!(r.code, 0);
    assert!(r.out.ends_with("nothing to do\n"));
    assert!(!f.exists("/etc/modprobe.d/amd-intertouch.conf"));
}

#[test]
fn packaged_conf() {
    let (f, runner) = fresh();
    f.file("/usr/lib/modprobe.d/amd-intertouch.conf", "x");
    let r = run(&f, &runner, &["install"]);
    assert!(
        r.out.contains(
            "/usr/lib/modprobe.d/amd-intertouch.conf already provides the module options\n"
        )
    );
    assert!(!f.exists("/etc/modprobe.d/amd-intertouch.conf"));
}

#[test]
fn dpkg_rpm() {
    let (f, _) = fresh();
    f.file("/usr/src/amd-intertouch-0.1.1/dkms.conf", "pkg")
        .file("/usr/bin/dpkg", "");
    let runner = FakeRunner::new()
        .on_query("dkms status", Output::success(""))
        .on_query(
            "dpkg -S",
            Output::success("amd-intertouch-dkms: /usr/src/amd-intertouch-0.1.1/dkms.conf\n"),
        );
    let r = run(&f, &runner, &["--dry-run", "install"]);
    assert!(
        r.out
            .contains("managed by the package `amd-intertouch-dkms`")
    );
    f.remove("/usr/bin/dpkg").file("/usr/bin/rpm", "");
    let runner = FakeRunner::new()
        .on_query("dkms status", Output::success(""))
        .on_query("rpm -qf", Output::success("amd-intertouch\n"));
    let r = run(&f, &runner, &["--dry-run", "install"]);
    assert!(r.out.contains("managed by the package `amd-intertouch`"));
    let runner = FakeRunner::new().on_query("dkms status", Output::success(""));
    let r = run(&f, &runner, &["--dry-run", "install"]);
    assert!(r.out.contains("  run     dkms add"));
}

#[test]
fn packaged_old() {
    let (f, _) = fresh();
    f.file("/usr/src/amd-intertouch-0.0.9/dkms.conf", "pkg")
        .file("/usr/bin/pacman", "");
    let runner = FakeRunner::new()
        .on_query(
            "dkms status",
            Output::success("amd-intertouch/0.0.9, k, x86_64: installed\n"),
        )
        .on_query(
            "pacman -Qqo /usr/src/amd-intertouch-0.0.9/dkms.conf",
            Output::success("amd-intertouch-dkms\n"),
        );
    let r = run(&f, &runner, &["install"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.err,
        "error: amd-intertouch 0.0.9 is installed by the package `amd-intertouch-dkms`; remove or upgrade that package instead\n"
    );
}

#[test]
fn dkms_fail() {
    let (f, _) = fresh();
    let runner = FakeRunner::new()
        .on_query("dkms status", Output::success(""))
        .on_exec(
            &format!("dkms install -m amd-intertouch -v 0.1.1 -k {KERNEL}"),
            10,
        );
    let r = run(&f, &runner, &["install"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        runner.execs().last().unwrap(),
        "dkms remove -m amd-intertouch -v 0.1.1 --all"
    );
    assert!(!f.exists("/usr/src/amd-intertouch-0.1.1"));
    assert!(!f.exists("/etc/modprobe.d/amd-intertouch.conf"));
    assert!(r.err.contains("exited with status 10"));
}

#[test]
fn refusals() {
    let (f, runner) = fresh();
    f.dir("/var/lib/dkms/thinkpad-t14-amd-touchpad");
    let r = run(&f, &runner, &["install"]);
    assert_eq!(r.code, 1);
    assert!(
        r.err
            .contains("thinkpad-t14-amd-touchpad is installed through DKMS")
    );
    assert!(
        r.err
            .contains("sudo pacman -Rns thinkpad-t14-amd-touchpad-dkms")
    );
    f.remove("/var/lib/dkms").remove("/usr/bin/dkms");
    assert_eq!(
        run(&f, &runner, &["install"]).err,
        "error: dkms is not installed\n"
    );
    f.file("/usr/bin/dkms", "").remove("/usr/lib/modules");
    assert_eq!(
        run(&f, &runner, &["install"]).err,
        "error: no kernel headers found under /usr/lib/modules/*/build\n"
    );
}

#[test]
fn uninstall() {
    let f = p14s_rmi4();
    f.file("/usr/src/amd-intertouch-0.1.1/dkms.conf", "x")
        .file("/etc/modprobe.d/amd-intertouch.conf", "x")
        .file("/etc/modprobe.d/amd-intertouch-nosleep.conf", "x")
        .file("/usr/local/bin/amd-intertouch", "x");
    let runner = FakeRunner::new().on_query(
        "dkms status",
        Output::success(&format!(
            "amd-intertouch/0.1.1, {KERNEL}, x86_64: installed\n"
        )),
    );
    let r = run(&f, &runner, &["uninstall"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(
        runner.execs(),
        vec!["dkms remove -m amd-intertouch -v 0.1.1 --all"]
    );
    assert!(!f.exists("/usr/src/amd-intertouch-0.1.1"));
    assert!(!f.exists("/etc/modprobe.d/amd-intertouch.conf"));
    assert!(!f.exists("/etc/modprobe.d/amd-intertouch-nosleep.conf"));
    assert!(!f.exists("/usr/local/bin/amd-intertouch"));
    assert!(
        r.out
            .ends_with("removed; reboot to return to the stock modules\n")
    );
}

#[test]
fn uninstall_bare() {
    let f = p14s_ps2();
    f.file("/usr/src/amd-intertouch-0.1.1/dkms.conf", "x");
    let runner = FakeRunner::new();
    let r = run(&f, &runner, &["--dry-run", "uninstall"]);
    assert_eq!(
        lines(&r.out),
        vec![
            "dry run: nothing will be changed",
            "  remove  /usr/src/amd-intertouch-0.1.1  (recursive)",
            "  remove  /etc/modprobe.d/amd-intertouch.conf",
            "  remove  /etc/modprobe.d/amd-intertouch-nosleep.conf",
            "  remove  /usr/local/bin/amd-intertouch",
        ]
    );
    f.remove("/usr/src");
    let r = run(&f, &runner, &["uninstall"]);
    assert_eq!(r.code, 0);
    assert!(runner.execs().is_empty());
}

#[test]
fn uninstall_packaged() {
    let f = p14s_ps2();
    f.file("/usr/src/amd-intertouch-0.1.1/dkms.conf", "x")
        .file("/usr/bin/pacman", "");
    let runner =
        FakeRunner::new().on_query("pacman -Qqo", Output::success("amd-intertouch-dkms\n"));
    let r = run(&f, &runner, &["uninstall"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.err,
        "error: amd-intertouch 0.1.1 is installed by the package `amd-intertouch-dkms`; remove it with your package manager\n"
    );
}

#[test]
fn dkms_conf() {
    let conf = dkmsconf::parse(assets::text("dkms.conf")).unwrap();
    assert_eq!(conf.name, "amd-intertouch");
    assert!(assets::text("dkms.conf").contains("PRE_BUILD=\"tools/pre-build.sh ${kernelver}\""));
}
