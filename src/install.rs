use crate::assets;
use crate::error::{Error, Result, fail};
use crate::exec::{Action, Plan};
use crate::hw;
use crate::modprobe::{self, NoSleep};
use crate::runner::{Cmd, Runner, query_ok};
use crate::sys::Sys;
use crate::{LEGACY_DKMS, NAME, VERSION};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub const BIN: &str = "/usr/local/bin/amd-intertouch";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DkmsEntry {
    pub name: String,
    pub version: String,
    pub kernel: Option<String>,
    pub arch: Option<String>,
    pub state: String,
}

pub fn parse_dkms_status(text: &str) -> Vec<DkmsEntry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (left, right) = line.split_once(": ")?;
            let mut fields = left.split(", ");
            let (name, version) = fields.next()?.split_once('/')?;
            if name.is_empty() || version.is_empty() || name.contains(' ') {
                return None;
            }
            let state = right.split(" (").next().unwrap_or(right).trim().to_string();
            Some(DkmsEntry {
                name: name.to_string(),
                version: version.to_string(),
                kernel: fields.next().map(str::to_string),
                arch: fields.next().map(str::to_string),
                state,
            })
        })
        .collect()
}

pub fn dkms_entries(runner: &dyn Runner) -> Vec<DkmsEntry> {
    query_ok(runner, &Cmd::new("dkms").arg("status"))
        .map(|t| parse_dkms_status(&t))
        .unwrap_or_default()
}

pub fn src_dir(version: &str) -> String {
    format!("/usr/src/{NAME}-{version}")
}

pub fn package_owner(sys: &Sys, runner: &dyn Runner, path: &str) -> Option<String> {
    if !sys.exists(path) {
        return None;
    }
    let queries = [
        ("pacman", Cmd::new("pacman").args(["-Qqo", path])),
        ("dpkg", Cmd::new("dpkg").args(["-S", path])),
        (
            "rpm",
            Cmd::new("rpm").args(["-qf", "--qf", "%{NAME}\n", path]),
        ),
    ];
    queries.into_iter().find_map(|(tool, cmd)| {
        sys.find_program(tool)?;
        let out = query_ok(runner, &cmd)?;
        let first = out.lines().next()?.trim();
        let name = first.split(':').next().unwrap_or(first).trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

pub fn self_install_actions(sys: &Sys, exe: Option<&Path>) -> Result<Vec<Action>> {
    let Some(exe) = exe else {
        return Ok(Vec::new());
    };
    let real = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf());
    let installed = sys.path(BIN).canonicalize().is_ok_and(|p| p == real);
    if installed || real.starts_with(sys.path("/usr/bin")) {
        return Ok(Vec::new());
    }
    let data = fs::read(exe).map_err(|e| Error(format!("{}: {e}", exe.display())))?;
    Ok(vec![Action::write(BIN, data, 0o755)])
}

fn dkms(args: &[&str]) -> Cmd {
    Cmd::new("dkms").args(args.iter().copied())
}

pub fn install_plan(
    sys: &Sys,
    runner: &dyn Runner,
    nosleep: Option<NoSleep>,
    exe: Option<&Path>,
) -> Result<(Plan, Vec<String>)> {
    if sys.is_dir(format!("/var/lib/dkms/{LEGACY_DKMS}")) {
        return fail(format!(
            "{LEGACY_DKMS} is installed through DKMS and replaces the same modules; remove it first \
             (sudo pacman -Rns {LEGACY_DKMS}-dkms, or sudo dkms remove {LEGACY_DKMS} --all)"
        ));
    }
    if sys.find_program("dkms").is_none() {
        return fail("dkms is not installed");
    }
    let kernels = hw::kernels_with_headers(sys);
    if kernels.is_empty() {
        return fail("no kernel headers found under /usr/lib/modules/*/build");
    }
    let src = src_dir(VERSION);
    let mut notes = Vec::new();
    let mut plan = Plan::new();
    let entries = dkms_entries(runner);

    if let Some(pkg) = package_owner(sys, runner, &format!("{src}/dkms.conf")) {
        notes.push(format!(
            "{NAME} {VERSION} is managed by the package `{pkg}`, which already builds it with DKMS"
        ));
        if !sys.exists(modprobe::PACKAGE_CONF) && !sys.exists(modprobe::CONF) {
            plan.push(Action::write(modprobe::CONF, modprobe::conf_text(), 0o644));
        }
        if let Some(mode) = nosleep {
            plan.extend(modprobe::nosleep_actions(mode));
        }
        return Ok((plan, notes));
    }

    let old_versions: BTreeSet<&str> = entries
        .iter()
        .filter(|e| e.name == NAME && e.version != VERSION)
        .map(|e| e.version.as_str())
        .collect();
    for old in old_versions {
        let old_src = src_dir(old);
        if let Some(pkg) = package_owner(sys, runner, &format!("{old_src}/dkms.conf")) {
            return fail(format!(
                "{NAME} {old} is installed by the package `{pkg}`; remove or upgrade that package instead"
            ));
        }
        plan.push(Action::Run(dkms(&[
            "remove", "-m", NAME, "-v", old, "--all",
        ])));
        plan.push(Action::RemoveTree(old_src));
        notes.push(format!("replacing {NAME} {old}"));
    }

    if entries
        .iter()
        .any(|e| e.name == NAME && e.version == VERSION)
    {
        plan.push(Action::Run(dkms(&[
            "remove", "-m", NAME, "-v", VERSION, "--all",
        ])));
        notes.push(format!("reinstalling {NAME} {VERSION}"));
    }
    plan.push(Action::RemoveTree(src.clone()));
    plan.extend(assets::write_actions(&src));
    plan.push(Action::Run(dkms(&["add", "-m", NAME, "-v", VERSION])));
    for k in &kernels {
        plan.push(Action::Run(dkms(&[
            "install", "-m", NAME, "-v", VERSION, "-k", k,
        ])));
    }
    if sys.exists(modprobe::PACKAGE_CONF) {
        notes.push(format!(
            "{} already provides the module options",
            modprobe::PACKAGE_CONF
        ));
    } else {
        plan.push(Action::write(modprobe::CONF, modprobe::conf_text(), 0o644));
    }
    if let Some(mode) = nosleep {
        plan.extend(modprobe::nosleep_actions(mode));
    }
    let bin = self_install_actions(sys, exe)?;
    if !bin.is_empty() {
        notes.push(format!("installing the command to {BIN}"));
        plan.extend(bin);
        plan.rollback.push(Action::Remove(BIN.to_string()));
    }

    plan.rollback.push(Action::TryRun(dkms(&[
        "remove", "-m", NAME, "-v", VERSION, "--all",
    ])));
    plan.rollback.push(Action::RemoveTree(src));
    plan.rollback
        .push(Action::Remove(modprobe::CONF.to_string()));
    Ok((plan, notes))
}

pub fn uninstall_plan(sys: &Sys, runner: &dyn Runner) -> Result<Plan> {
    let entries = dkms_entries(runner);
    let mut versions: BTreeSet<String> = entries
        .iter()
        .filter(|e| e.name == NAME)
        .map(|e| e.version.clone())
        .collect();
    if sys.exists(src_dir(VERSION)) {
        versions.insert(VERSION.to_string());
    }
    let mut plan = Plan::new();
    for v in &versions {
        let src = src_dir(v);
        if let Some(pkg) = package_owner(sys, runner, &format!("{src}/dkms.conf")) {
            return fail(format!(
                "{NAME} {v} is installed by the package `{pkg}`; remove it with your package manager"
            ));
        }
        if entries.iter().any(|e| e.name == NAME && &e.version == v) {
            plan.push(Action::Run(dkms(&["remove", "-m", NAME, "-v", v, "--all"])));
        }
        plan.push(Action::RemoveTree(src));
    }
    plan.push(Action::Remove(modprobe::CONF.to_string()));
    plan.push(Action::Remove(modprobe::NOSLEEP_CONF.to_string()));
    plan.push(Action::Remove(BIN.to_string()));
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dkms_status() {
        let text = "amd-intertouch/0.1.0, 7.2.7-arch1-1, x86_64: installed (Original modules exist)\n\
                    amd-intertouch/0.1.0: added\n\
                    vboxhost/7.2.20_OSE, 7.2.7-arch1-1, x86_64: installed\n\
                    Warning: something odd\n\
                    garbage\n\
                    /x: y\n";
        let e = parse_dkms_status(text);
        assert_eq!(e.len(), 3);
        assert_eq!(
            e[0],
            DkmsEntry {
                name: "amd-intertouch".into(),
                version: "0.1.0".into(),
                kernel: Some("7.2.7-arch1-1".into()),
                arch: Some("x86_64".into()),
                state: "installed".into(),
            }
        );
        assert_eq!((e[1].kernel.clone(), e[1].state.as_str()), (None, "added"));
        assert_eq!(e[2].name, "vboxhost");
        assert!(parse_dkms_status("").is_empty());
    }

    #[test]
    fn src_path() {
        assert_eq!(src_dir("1.2.3"), "/usr/src/amd-intertouch-1.2.3");
    }
}
