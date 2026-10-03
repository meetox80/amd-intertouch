use crate::error::{Error, Result, fail};
use crate::modprobe::NoSleep;
use crate::trackpoint::{DEFAULT_RATE, DEFAULT_SENSITIVITY};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Check,
    Status,
    Try,
    Restore,
    Install {
        nosleep: Option<NoSleep>,
    },
    Uninstall,
    NoSleep {
        mode: NoSleep,
        now: bool,
    },
    Trackpoint {
        rate: u32,
        sensitivity: u32,
        remove: bool,
    },
    Help,
    Version,
}

impl Command {
    pub fn needs_root(&self) -> bool {
        matches!(
            self,
            Command::Try
                | Command::Restore
                | Command::Install { .. }
                | Command::Uninstall
                | Command::NoSleep { .. }
                | Command::Trackpoint { .. }
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            Command::Check => "check",
            Command::Status => "status",
            Command::Try => "try",
            Command::Restore => "restore",
            Command::Install { .. } => "install",
            Command::Uninstall => "uninstall",
            Command::NoSleep { .. } => "nosleep",
            Command::Trackpoint { .. } => "trackpoint",
            Command::Help => "help",
            Command::Version => "version",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub dry_run: bool,
    pub no_color: bool,
    pub root: Option<String>,
    pub command: Command,
}

pub const USAGE: &str = "\
amd-intertouch: Synaptics InterTouch (RMI4 over SMBus) on AMD ThinkPads

usage: amd-intertouch [--dry-run] [--no-color] <command> [options]

commands:
  check                          is this machine compatible
  status                         what is installed and loaded right now
  try                            load the patched modules until the next reboot (root)
  restore                        reload the installed modules, undoing `try` (root)
  install [--nosleep MODE]       build and install the modules with DKMS (root)
  uninstall                      remove the DKMS modules and config (root)
  nosleep auto|on|off [--now]    keep the touchpad sensor awake between touches (root)
  trackpoint [--rate N] [--sensitivity N] [--remove]
                                 persist trackpoint rate and sensitivity (root)

options:
  -n, --dry-run                  print what would be done, change nothing
      --no-color                 plain output
  -h, --help                     this help
  -V, --version                  version

typical use:
  amd-intertouch check
  sudo amd-intertouch try
  sudo amd-intertouch install
";

fn value<'a>(
    flag: &str,
    inline: Option<&'a str>,
    it: &mut std::slice::Iter<'a, String>,
) -> Result<&'a str> {
    match inline {
        Some(v) => Ok(v),
        None => it
            .next()
            .map(String::as_str)
            .ok_or_else(|| Error(format!("{flag} needs a value"))),
    }
}

fn number(flag: &str, v: &str) -> Result<u32> {
    v.parse()
        .map_err(|_| Error(format!("{flag} expects a number, not `{v}`")))
}

fn split_flag(arg: &str) -> (&str, Option<&str>) {
    match arg.split_once('=') {
        Some((f, v)) if f.starts_with("--") => (f, Some(v)),
        _ => (arg, None),
    }
}

fn no_args(name: &str, rest: &[String]) -> Result<()> {
    match rest.first() {
        Some(extra) => fail(format!("`{name}` takes no arguments, got `{extra}`")),
        None => Ok(()),
    }
}

pub fn parse(args: &[String]) -> Result<Args> {
    let mut dry_run = false;
    let mut no_color = false;
    let mut root = None;
    let mut help = false;
    let mut version = false;
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let (flag, inline) = split_flag(arg);
        match flag {
            "-n" | "--dry-run" if inline.is_none() => dry_run = true,
            "--no-color" if inline.is_none() => no_color = true,
            "-h" | "--help" if inline.is_none() => help = true,
            "-V" | "--version" if inline.is_none() => version = true,
            "--root" => root = Some(value("--root", inline, &mut it)?.to_string()),
            _ => rest.push(arg.clone()),
        }
    }
    let command = if version {
        Command::Version
    } else if help {
        Command::Help
    } else {
        parse_command(&rest)?
    };
    Ok(Args {
        dry_run,
        no_color,
        root,
        command,
    })
}

fn parse_command(rest: &[String]) -> Result<Command> {
    let Some((name, tail)) = rest.split_first() else {
        return Ok(Command::Help);
    };
    match name.as_str() {
        "check" => no_args(name, tail).map(|_| Command::Check),
        "status" => no_args(name, tail).map(|_| Command::Status),
        "try" => no_args(name, tail).map(|_| Command::Try),
        "restore" => no_args(name, tail).map(|_| Command::Restore),
        "uninstall" => no_args(name, tail).map(|_| Command::Uninstall),
        "help" => Ok(Command::Help),
        "install" => {
            let mut nosleep = None;
            let mut it = tail.iter();
            while let Some(arg) = it.next() {
                match split_flag(arg) {
                    ("--nosleep", inline) => {
                        nosleep = Some(NoSleep::parse(value("--nosleep", inline, &mut it)?)?)
                    }
                    _ => return fail(format!("unknown option for `install`: `{arg}`")),
                }
            }
            Ok(Command::Install { nosleep })
        }
        "nosleep" => {
            let mut mode = None;
            let mut now = false;
            for arg in tail {
                match arg.as_str() {
                    "--now" => now = true,
                    a if a.starts_with('-') => {
                        return fail(format!("unknown option for `nosleep`: `{a}`"));
                    }
                    a if mode.is_none() => mode = Some(NoSleep::parse(a)?),
                    a => return fail(format!("`nosleep` takes one mode, got an extra `{a}`")),
                }
            }
            let mode =
                mode.ok_or_else(|| Error("`nosleep` needs a mode: auto, on or off".to_string()))?;
            Ok(Command::NoSleep { mode, now })
        }
        "trackpoint" => {
            let mut rate = None;
            let mut sensitivity = None;
            let mut remove = false;
            let mut it = tail.iter();
            while let Some(arg) = it.next() {
                match split_flag(arg) {
                    ("--rate", inline) => {
                        rate = Some(number("--rate", value("--rate", inline, &mut it)?)?)
                    }
                    ("--sensitivity", inline) => {
                        sensitivity = Some(number(
                            "--sensitivity",
                            value("--sensitivity", inline, &mut it)?,
                        )?)
                    }
                    ("--remove", None) => remove = true,
                    _ => return fail(format!("unknown option for `trackpoint`: `{arg}`")),
                }
            }
            if remove && (rate.is_some() || sensitivity.is_some()) {
                return fail("`trackpoint --remove` takes no other options");
            }
            Ok(Command::Trackpoint {
                rate: rate.unwrap_or(DEFAULT_RATE),
                sensitivity: sensitivity.unwrap_or(DEFAULT_SENSITIVITY),
                remove,
            })
        }
        other => fail(format!("unknown command `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Args> {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    fn cmd(args: &[&str]) -> Command {
        p(args).unwrap().command
    }

    fn err(args: &[&str]) -> String {
        p(args).unwrap_err().0
    }

    #[test]
    fn simple() {
        assert_eq!(cmd(&["check"]), Command::Check);
        assert_eq!(cmd(&["status"]), Command::Status);
        assert_eq!(cmd(&["try"]), Command::Try);
        assert_eq!(cmd(&["restore"]), Command::Restore);
        assert_eq!(cmd(&["uninstall"]), Command::Uninstall);
        assert_eq!(cmd(&["help"]), Command::Help);
        assert_eq!(cmd(&[]), Command::Help);
        assert_eq!(cmd(&["--help", "check"]), Command::Help);
        assert_eq!(cmd(&["check", "-h"]), Command::Help);
        assert_eq!(cmd(&["-V"]), Command::Version);
        assert_eq!(cmd(&["--version", "--help"]), Command::Version);
    }

    #[test]
    fn globals() {
        let a = p(&["try", "--dry-run", "--root", "/r", "--no-color"]).unwrap();
        assert!(a.dry_run && a.no_color);
        assert_eq!(a.root.as_deref(), Some("/r"));
        assert_eq!(a.command, Command::Try);
        let a = p(&["-n", "--root=/x", "status"]).unwrap();
        assert!(a.dry_run && !a.no_color);
        assert_eq!(a.root.as_deref(), Some("/x"));
        assert_eq!(err(&["status", "--root"]), "--root needs a value");
    }

    #[test]
    fn install_opts() {
        assert_eq!(cmd(&["install"]), Command::Install { nosleep: None });
        assert_eq!(
            cmd(&["install", "--nosleep", "on"]),
            Command::Install {
                nosleep: Some(NoSleep::On)
            }
        );
        assert_eq!(
            cmd(&["install", "--nosleep=off"]),
            Command::Install {
                nosleep: Some(NoSleep::Off)
            }
        );
        assert_eq!(err(&["install", "--nosleep"]), "--nosleep needs a value");
        assert_eq!(
            err(&["install", "--nosleep", "maybe"]),
            "nosleep mode must be auto, on or off, not `maybe`"
        );
        assert_eq!(
            err(&["install", "--force"]),
            "unknown option for `install`: `--force`"
        );
    }

    #[test]
    fn nosleep_opts() {
        assert_eq!(
            cmd(&["nosleep", "on"]),
            Command::NoSleep {
                mode: NoSleep::On,
                now: false
            }
        );
        assert_eq!(
            cmd(&["nosleep", "--now", "auto"]),
            Command::NoSleep {
                mode: NoSleep::Auto,
                now: true
            }
        );
        assert_eq!(err(&["nosleep"]), "`nosleep` needs a mode: auto, on or off");
        assert_eq!(
            err(&["nosleep", "on", "off"]),
            "`nosleep` takes one mode, got an extra `off`"
        );
        assert_eq!(
            err(&["nosleep", "--later"]),
            "unknown option for `nosleep`: `--later`"
        );
        assert_eq!(
            err(&["nosleep", "sometimes"]),
            "nosleep mode must be auto, on or off, not `sometimes`"
        );
    }

    #[test]
    fn trackpoint_opts() {
        assert_eq!(
            cmd(&["trackpoint"]),
            Command::Trackpoint {
                rate: 200,
                sensitivity: 200,
                remove: false
            }
        );
        assert_eq!(
            cmd(&["trackpoint", "--rate", "100", "--sensitivity=150"]),
            Command::Trackpoint {
                rate: 100,
                sensitivity: 150,
                remove: false
            }
        );
        assert_eq!(
            cmd(&["trackpoint", "--remove"]),
            Command::Trackpoint {
                rate: 200,
                sensitivity: 200,
                remove: true
            }
        );
        assert_eq!(
            err(&["trackpoint", "--remove", "--rate", "100"]),
            "`trackpoint --remove` takes no other options"
        );
        assert_eq!(
            err(&["trackpoint", "--rate", "fast"]),
            "--rate expects a number, not `fast`"
        );
        assert_eq!(
            err(&["trackpoint", "--sensitivity"]),
            "--sensitivity needs a value"
        );
        assert_eq!(
            err(&["trackpoint", "--sensitivity", "abc"]),
            "--sensitivity expects a number, not `abc`"
        );
        assert_eq!(
            err(&["trackpoint", "--remove=yes"]),
            "unknown option for `trackpoint`: `--remove=yes`"
        );
        assert_eq!(
            err(&["trackpoint", "x"]),
            "unknown option for `trackpoint`: `x`"
        );
    }

    #[test]
    fn unknown() {
        assert_eq!(err(&["fly"]), "unknown command `fly`");
        assert_eq!(
            err(&["check", "now"]),
            "`check` takes no arguments, got `now`"
        );
        assert_eq!(
            err(&["status", "--dry-run=yes"]),
            "`status` takes no arguments, got `--dry-run=yes`"
        );
    }

    #[test]
    fn root_names() {
        let all = [
            (Command::Check, "check", false),
            (Command::Status, "status", false),
            (Command::Try, "try", true),
            (Command::Restore, "restore", true),
            (Command::Install { nosleep: None }, "install", true),
            (Command::Uninstall, "uninstall", true),
            (
                Command::NoSleep {
                    mode: NoSleep::On,
                    now: false,
                },
                "nosleep",
                true,
            ),
            (
                Command::Trackpoint {
                    rate: 1,
                    sensitivity: 1,
                    remove: false,
                },
                "trackpoint",
                true,
            ),
            (Command::Help, "help", false),
            (Command::Version, "version", false),
        ];
        for (c, name, root) in all {
            assert_eq!(c.name(), name);
            assert_eq!(c.needs_root(), root, "{name}");
        }
    }

    #[test]
    fn usage() {
        for c in [
            "check",
            "status",
            "try",
            "restore",
            "install",
            "uninstall",
            "nosleep",
            "trackpoint",
        ] {
            assert!(USAGE.contains(&format!("\n  {c}")), "{c}");
        }
        let shouting: Vec<&str> = USAGE
            .lines()
            .filter(|l| l.chars().filter(|c| c.is_alphabetic()).count() > 3)
            .filter(|l| *l == l.to_uppercase())
            .collect();
        assert!(shouting.is_empty(), "{shouting:?}");
    }
}
