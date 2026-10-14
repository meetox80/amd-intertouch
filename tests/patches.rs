use amd_intertouch::assets;
use amd_intertouch::dkmsconf;
use amd_intertouch::exec::{Executor, Plan};
use amd_intertouch::runner::FakeRunner;
use amd_intertouch::sys::Sys;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Tree(PathBuf);

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn patched(tag: &str) -> Tree {
    let root = std::env::temp_dir().join(format!(
        "amd-intertouch-patches-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let sys = Sys::new(&root);
    let runner = FakeRunner::new();
    let ex = Executor {
        sys: &sys,
        runner: &runner,
        dry_run: false,
        sleep: false,
    };
    let mut plan = Plan::new();
    plan.extend(assets::write_actions("/tree"));
    ex.execute(&plan, &mut Vec::new()).unwrap();
    let tree = root.join("tree");
    for p in dkmsconf::embedded().patches {
        let out = Command::new("patch")
            .args(["-p1", "--forward", "--no-backup-if-mismatch", "-s", "-i"])
            .arg(format!("patches/{p}"))
            .current_dir(&tree)
            .output()
            .expect("patch(1) is installed");
        assert!(
            out.status.success(),
            "{p} failed to apply:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Tree(root)
}

fn read(tree: &Tree, rel: &str) -> String {
    fs::read_to_string(tree.0.join("tree").join(rel)).unwrap()
}

#[test]
fn apply() {
    let tree = patched("apply");
    let leftovers: Vec<PathBuf> = walk(&tree.0.join("tree"))
        .into_iter()
        .filter(|p| {
            let s = p.to_string_lossy();
            s.ends_with(".rej") || s.ends_with(".orig")
        })
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn features() {
    let tree = patched("features");
    let piix4 = read(&tree, "drivers/i2c/busses/i2c-piix4.c");
    assert!(piix4.contains("module_param(host_notify, bool, 0444);"));
    assert!(piix4.contains("\"ASF Host Notify on, IRQ %d\\n\""));
    let synaptics = read(&tree, "drivers/input/mouse/synaptics.c");
    assert!(
        synaptics.contains(
            "module_param_named(synaptics_rmi_nosleep, synaptics_rmi_nosleep, int, 0644);"
        )
    );
    assert!(synaptics.contains(".nosleep = synaptics_rmi_keep_awake(psmouse) ?"));
    assert!(synaptics.contains(".doze_interval = synaptics_doze(psmouse),"));
    let smbus_list = synaptics
        .split("static const char * const smbus_pnp_ids[] = {")
        .nth(1)
        .unwrap()
        .split("};")
        .next()
        .unwrap();
    for id in ["\"LEN2064\"", "\"LEN2073\"", "\"LEN0411\""] {
        assert!(smbus_list.contains(id), "{id} missing from smbus_pnp_ids");
    }
    let ids: Vec<&str> = smbus_list
        .lines()
        .filter_map(|l| l.trim().strip_prefix('"').and_then(|r| r.split('"').next()))
        .collect();
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "duplicate id in smbus_pnp_ids");
    let at = ids.iter().position(|i| *i == "LEN2064").unwrap();
    assert_eq!((ids[at - 1], ids[at + 1]), ("LEN2058", "LEN2068"));
    let rmi = read(&tree, "drivers/input/rmi4/rmi_smbus.c");
    assert!(rmi.contains("msleep(RMI_SMB_RESET_GRACE_MS);"));
    assert!(rmi.contains("lost = symbol_get(psmouse_smbus_lost);"));
    assert!(rmi.contains("disable_delayed_work_sync(&rmi_smb->check);"));
    let smbus = read(&tree, "drivers/input/mouse/psmouse-smbus.c");
    assert!(smbus.contains("EXPORT_SYMBOL_GPL(psmouse_smbus_lost);"));
    let piix4_retry = piix4.split("static s32 asf_xfer(").nth(1).unwrap();
    assert!(!piix4_retry.contains("!asf_gap("), "NAKs must be retried");
}

#[test]
fn pristine() {
    let source = assets::kernel_source();
    assert!(source.starts_with("7.2."), "{source}");
    let synaptics = assets::text("drivers/input/mouse/synaptics.c");
    assert!(
        !synaptics.contains("synaptics_rmi_nosleep"),
        "embedded sources must be pristine"
    );
    assert!(!assets::text("drivers/i2c/busses/i2c-piix4.c").contains("module_param(host_notify"));
}

#[test]
#[ignore = "compiles kernel modules against the running kernel's headers; run with --ignored"]
fn build() {
    let release = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap();
    let kdir = format!("/usr/lib/modules/{}/build", release.trim());
    assert!(Path::new(&kdir).is_dir(), "no headers at {kdir}");
    let tree = patched("build");
    let dir = tree.0.join("tree");
    let out = Command::new("make")
        .args(["-C", &kdir])
        .arg(format!("M={}", dir.display()))
        .arg("modules")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{log}");
    assert!(!log.contains("warning:"), "{log}");
    for ko in [
        "drivers/i2c/busses/i2c-piix4.ko",
        "drivers/input/rmi4/rmi_smbus.ko",
        "drivers/input/mouse/psmouse.ko",
    ] {
        let path = dir.join(ko);
        assert!(path.is_file(), "{ko} not built");
        let info = Command::new("modinfo").arg(&path).output().unwrap();
        let text = String::from_utf8_lossy(&info.stdout);
        assert!(
            text.contains(&format!("vermagic:       {} ", release.trim())),
            "{ko}: {text}"
        );
        if ko.ends_with("psmouse.ko") {
            assert!(
                text.contains("parm:           synaptics_rmi_nosleep:"),
                "{text}"
            );
        }
        if ko.ends_with("i2c-piix4.ko") {
            assert!(text.contains("parm:           host_notify:"), "{text}");
        }
        if ko.ends_with("rmi_smbus.ko") {
            assert!(text.contains("depends:        rmi_core\n"), "{text}");
        }
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
