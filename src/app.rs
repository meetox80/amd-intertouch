use crate::check;
use crate::cli::{self, Command};
use crate::error::{Error, Result};
use crate::exec::Executor;
use crate::hw::{self, Transport};
use crate::install;
use crate::live;
use crate::modprobe::{self, NoSleep};
use crate::output::{Style, row};
use crate::runner::{Cmd, HostRunner, Runner, query_ok};
use crate::status;
use crate::sys::Sys;
use crate::trackpoint;
use crate::{NAME, VERSION, exit};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Env<'a> {
    pub runner: &'a dyn Runner,
    pub sleep: bool,
    pub style: Style,
    pub exe: Option<PathBuf>,
}

pub fn run(args: &[String]) -> i32 {
    let env = Env {
        runner: &HostRunner,
        sleep: true,
        style: Style::detect(),
        exe: std::env::current_exe().ok(),
    };
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    run_with(args, &env, &mut stdout.lock(), &mut stderr.lock())
}

pub fn run_with(args: &[String], env: &Env, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let parsed = match cli::parse(args) {
        Ok(a) => a,
        Err(e) => {
            let _ = writeln!(err, "error: {e}\nrun `{NAME} --help` for usage");
            return exit::USAGE;
        }
    };
    let style = if parsed.no_color {
        Style::plain()
    } else {
        env.style
    };
    let sys = parsed
        .root
        .as_deref()
        .map(Sys::new)
        .unwrap_or_else(Sys::host);
    if parsed.command.needs_root() && !parsed.dry_run && sys.euid() != Some(0) {
        let _ = writeln!(
            err,
            "error: `{}` needs root: run it with sudo, or add --dry-run to see what it would do",
            parsed.command.name()
        );
        return exit::FAILURE;
    }
    let ex = Executor {
        sys: &sys,
        runner: env.runner,
        dry_run: parsed.dry_run,
        sleep: env.sleep,
    };
    if parsed.dry_run && parsed.command.needs_root() {
        let _ = writeln!(out, "dry run: nothing will be changed");
    }
    let result = match parsed.command {
        Command::Help => write!(out, "{}\n{}\n", cli::USAGE, style.credit())
            .map(|_| exit::OK)
            .map_err(Error::from),
        Command::Version => writeln!(out, "{NAME} {VERSION}\n{}", style.credit())
            .map(|_| exit::OK)
            .map_err(Error::from),
        Command::Check => cmd_check(&sys, out, style),
        Command::Status => cmd_status(&sys, env.runner, out),
        Command::Try => cmd_try(&sys, env.runner, &ex, out),
        Command::Restore => cmd_restore(&sys, &ex, out),
        Command::Install { nosleep } => {
            cmd_install(&sys, env.runner, &ex, nosleep, env.exe.as_deref(), out)
        }
        Command::Uninstall => cmd_uninstall(&sys, env.runner, &ex, out),
        Command::NoSleep { mode, now } => cmd_nosleep(&sys, &ex, mode, now, out),
        Command::Trackpoint {
            rate,
            sensitivity,
            remove,
        } => cmd_trackpoint(&sys, &ex, rate, sensitivity, remove, out),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            exit::FAILURE
        }
    }
}

fn cmd_check(sys: &Sys, out: &mut dyn Write, style: Style) -> Result<i32> {
    let report = check::run(sys);
    for item in &report.items {
        writeln!(
            out,
            "{} {:<10} {}",
            style.level(item.level),
            item.label,
            item.detail
        )?;
    }
    writeln!(out)?;
    writeln!(out, "{}", style.bold(&report.summary()))?;
    Ok(report.exit_code())
}

fn cmd_status(sys: &Sys, runner: &dyn Runner, out: &mut dyn Write) -> Result<i32> {
    let s = status::collect(sys, runner);
    writeln!(out, "{}", row("touchpad", &s.transport.describe()))?;
    writeln!(out, "{}", row("host notify", &s.host_notify.describe()))?;
    for m in &s.modules {
        writeln!(out, "{}", row(&m.name, m.describe()))?;
    }
    let dkms = status::dkms_lines(&status::relevant_dkms(s.dkms));
    if dkms.is_empty() {
        writeln!(out, "{}", row("dkms", &format!("{NAME} is not registered")))?;
    }
    for line in dkms {
        writeln!(out, "{}", row("dkms", &line))?;
    }
    let nosleep = match s.nosleep {
        Some(mode) => mode.describe().to_string(),
        None => "not available: the loaded psmouse has no amd-intertouch patch".to_string(),
    };
    writeln!(out, "{}", row("nosleep", &nosleep))?;
    let tp = match &s.trackpoint {
        Some(tp) => format!(
            "{}: rate {}, sensitivity {}",
            tp.serio,
            tp.rate.map_or("?".to_string(), |v| v.to_string()),
            tp.sensitivity.map_or("?".to_string(), |v| v.to_string())
        ),
        None => "not found".to_string(),
    };
    writeln!(out, "{}", row("trackpoint", &tp))?;
    Ok(exit::OK)
}

fn kernel(sys: &Sys) -> Result<String> {
    hw::kernel_release(sys).ok_or_else(|| Error("cannot read the kernel release".to_string()))
}

fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn report_transport(sys: &Sys, out: &mut dyn Write) -> Result<Transport> {
    let t = hw::touchpad_transport(&hw::input_devices(sys));
    writeln!(out, "{}", row("touchpad", &t.describe()))?;
    Ok(t)
}

fn cmd_try(sys: &Sys, runner: &dyn Runner, ex: &Executor, out: &mut dyn Write) -> Result<i32> {
    let kernel = kernel(sys)?;
    live::ensure_swappable(sys, &kernel)?;
    if live::needs_build(sys, &kernel) {
        writeln!(out, "building the patched modules for {kernel}")?;
        ex.execute(&live::build_plan(&kernel), out)?;
    } else {
        writeln!(
            out,
            "using the cached build in {}",
            live::build_dir(&kernel)
        )?;
    }
    writeln!(
        out,
        "swapping in the patched modules: touchpad and trackpoint drop out for a few seconds"
    )?;
    let start = epoch();
    ex.execute(&live::swap_plan(sys, &kernel), out)?;
    if ex.dry_run {
        return Ok(exit::OK);
    }
    writeln!(out)?;
    writeln!(out, "kernel log since the swap:")?;
    let log = query_ok(
        runner,
        &Cmd::new("journalctl").args([
            "-k",
            "--no-pager",
            "-o",
            "cat",
            "--since",
            &format!("@{start}"),
        ]),
    )
    .map(|l| live::filter_log(&l))
    .unwrap_or_default();
    if log.is_empty() {
        writeln!(out, "  (nothing relevant)")?;
    }
    for line in log {
        writeln!(out, "  {line}")?;
    }
    writeln!(out)?;
    match report_transport(sys, out)? {
        Transport::Rmi4(_) => {
            if install::dkms_entries(runner).iter().any(|e| e.name == NAME) {
                writeln!(
                    out,
                    "this build is loaded until reboot: `sudo {NAME} install` replaces the installed one with it, `sudo {NAME} restore` or a reboot goes back"
                )?;
            } else {
                writeln!(
                    out,
                    "nothing is installed yet: `sudo {NAME} install` keeps it, `sudo {NAME} restore` or a reboot undoes it"
                )?;
            }
            Ok(exit::OK)
        }
        _ => {
            writeln!(
                out,
                "the touchpad did not switch to rmi4/smbus; `sudo {NAME} restore` reloads the installed modules"
            )?;
            Ok(exit::FAILURE)
        }
    }
}

fn cmd_restore(sys: &Sys, ex: &Executor, out: &mut dyn Write) -> Result<i32> {
    writeln!(out, "reloading the installed modules")?;
    ex.execute(&live::restore_plan(), out)?;
    if !ex.dry_run {
        report_transport(sys, out)?;
    }
    Ok(exit::OK)
}

fn cmd_install(
    sys: &Sys,
    runner: &dyn Runner,
    ex: &Executor,
    nosleep: Option<NoSleep>,
    exe: Option<&Path>,
    out: &mut dyn Write,
) -> Result<i32> {
    let (plan, notes) = install::install_plan(sys, runner, nosleep, exe)?;
    for note in notes {
        writeln!(out, "{note}")?;
    }
    if plan.steps.is_empty() {
        writeln!(out, "nothing to do")?;
        return Ok(exit::OK);
    }
    ex.execute(&plan, out)?;
    if !ex.dry_run {
        writeln!(out, "installed; reboot, then check with `{NAME} status`")?;
    }
    Ok(exit::OK)
}

fn cmd_uninstall(
    sys: &Sys,
    runner: &dyn Runner,
    ex: &Executor,
    out: &mut dyn Write,
) -> Result<i32> {
    let plan = install::uninstall_plan(sys, runner)?;
    ex.execute(&plan, out)?;
    if !ex.dry_run {
        writeln!(out, "removed; reboot to return to the stock modules")?;
    }
    Ok(exit::OK)
}

fn cmd_nosleep(
    sys: &Sys,
    ex: &Executor,
    mode: NoSleep,
    now: bool,
    out: &mut dyn Write,
) -> Result<i32> {
    if !sys.exists(modprobe::NOSLEEP_PARAM) {
        writeln!(
            out,
            "note: the loaded psmouse has no synaptics_rmi_nosleep parameter yet; the setting applies once {NAME} is loaded"
        )?;
    }
    ex.execute(&modprobe::nosleep_plan(sys, mode, now), out)?;
    if !ex.dry_run {
        if now {
            writeln!(out, "nosleep {}: psmouse reloaded", mode.describe())?;
        } else {
            writeln!(
                out,
                "nosleep {}: applies at the next boot, or rerun with --now",
                mode.describe()
            )?;
        }
    }
    Ok(exit::OK)
}

fn cmd_trackpoint(
    sys: &Sys,
    ex: &Executor,
    rate: u32,
    sensitivity: u32,
    remove: bool,
    out: &mut dyn Write,
) -> Result<i32> {
    let plan = if remove {
        trackpoint::remove_plan()
    } else {
        trackpoint::apply_plan(sys, rate, sensitivity)?
    };
    if trackpoint::legacy_rule_present(sys) {
        writeln!(
            out,
            "note: {} also sets trackpoint attributes; remove it so only one rule applies",
            trackpoint::LEGACY_RULE
        )?;
    }
    if !remove && hw::trackpoint(sys).is_none() {
        writeln!(
            out,
            "note: no trackpoint found right now; the rule applies when one appears"
        )?;
    }
    ex.execute(&plan, out)?;
    if !ex.dry_run {
        if remove {
            writeln!(
                out,
                "trackpoint rule removed; current values stay until the next boot"
            )?;
        } else {
            writeln!(
                out,
                "trackpoint: rate {rate}, sensitivity {sensitivity}, kept across reboots"
            )?;
        }
    }
    Ok(exit::OK)
}
