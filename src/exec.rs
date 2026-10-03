use crate::error::{Error, Result};
use crate::runner::{Cmd, Runner};
use crate::sys::Sys;
use std::fmt;
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Run(Cmd),
    TryRun(Cmd),
    Write {
        path: String,
        data: Vec<u8>,
        mode: u32,
    },
    Attr {
        path: String,
        value: String,
    },
    Remove(String),
    RemoveTree(String),
    Sleep(u64),
}

impl Action {
    pub fn write(path: &str, data: impl Into<Vec<u8>>, mode: u32) -> Self {
        Action::Write {
            path: path.to_string(),
            data: data.into(),
            mode,
        }
    }

    pub fn attr(path: &str, value: impl ToString) -> Self {
        Action::Attr {
            path: path.to_string(),
            value: value.to_string(),
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Run(c) => write!(f, "run     {}", c.line()),
            Action::TryRun(c) => write!(f, "run     {}  (failure ignored)", c.line()),
            Action::Write { path, data, mode } => {
                write!(f, "write   {path}  ({} bytes, mode {mode:o})", data.len())
            }
            Action::Attr { path, value } => write!(f, "set     {path} = {value}"),
            Action::Remove(p) => write!(f, "remove  {p}"),
            Action::RemoveTree(p) => write!(f, "remove  {p}  (recursive)"),
            Action::Sleep(ms) => write!(f, "wait    {ms} ms"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Action>,
    pub rollback: Vec<Action>,
}

impl Plan {
    pub fn new() -> Self {
        Plan::default()
    }

    pub fn push(&mut self, a: Action) {
        self.steps.push(a);
    }

    pub fn extend(&mut self, items: impl IntoIterator<Item = Action>) {
        self.steps.extend(items);
    }

    pub fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self.steps.iter().map(ToString::to_string).collect();
        if !self.rollback.is_empty() {
            out.push("on failure:".to_string());
            out.extend(self.rollback.iter().map(|a| format!("  {a}")));
        }
        out
    }
}

pub struct Executor<'a> {
    pub sys: &'a Sys,
    pub runner: &'a dyn Runner,
    pub dry_run: bool,
    pub sleep: bool,
}

impl Executor<'_> {
    pub fn execute(&self, plan: &Plan, out: &mut dyn Write) -> Result<()> {
        if self.dry_run {
            for line in plan.lines() {
                writeln!(out, "  {line}")?;
            }
            return Ok(());
        }
        for action in &plan.steps {
            if let Err(e) = self.apply(action) {
                if !plan.rollback.is_empty() {
                    writeln!(out, "failed: {e}")?;
                    writeln!(out, "rolling back")?;
                    for undo in &plan.rollback {
                        if let Err(u) = self.apply(undo) {
                            writeln!(out, "  rollback step failed: {u}")?;
                        }
                    }
                }
                return Err(e);
            }
        }
        Ok(())
    }

    fn apply(&self, action: &Action) -> Result<()> {
        match action {
            Action::Run(cmd) => {
                let code = self.runner.exec(cmd)?;
                if code == 0 {
                    Ok(())
                } else {
                    Err(Error(format!("`{}` exited with status {code}", cmd.line())))
                }
            }
            Action::TryRun(cmd) => {
                let _ = self.runner.exec(cmd);
                Ok(())
            }
            Action::Write { path, data, mode } => {
                let target = self.sys.path(path);
                let ctx = |e: std::io::Error| Error(format!("{path}: {e}"));
                fs::create_dir_all(target.parent().unwrap_or(Path::new("/"))).map_err(ctx)?;
                fs::write(&target, data).map_err(ctx)?;
                fs::set_permissions(&target, fs::Permissions::from_mode(*mode)).map_err(ctx)
            }
            Action::Attr { path, value } => {
                fs::write(self.sys.path(path), value).map_err(|e| Error(format!("{path}: {e}")))
            }
            Action::Remove(path) => match fs::remove_file(self.sys.path(path)) {
                Err(e) if e.kind() != ErrorKind::NotFound => Err(Error(format!("{path}: {e}"))),
                _ => Ok(()),
            },
            Action::RemoveTree(path) => match fs::remove_dir_all(self.sys.path(path)) {
                Err(e) if e.kind() != ErrorKind::NotFound => Err(Error(format!("{path}: {e}"))),
                _ => Ok(()),
            },
            Action::Sleep(ms) => {
                if self.sleep {
                    std::thread::sleep(Duration::from_millis(*ms));
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::FakeRunner;
    use crate::testutil::temp_dir;

    #[test]
    fn display() {
        let lines: Vec<String> = [
            Action::Run(Cmd::new("rmmod").arg("psmouse")),
            Action::TryRun(Cmd::new("rmmod").arg("x")),
            Action::write("/etc/a.conf", "abc", 0o644),
            Action::attr("/sys/x/rate", 200),
            Action::Remove("/etc/a".into()),
            Action::RemoveTree("/usr/src/a".into()),
            Action::Sleep(5),
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            lines,
            vec![
                "run     rmmod psmouse",
                "run     rmmod x  (failure ignored)",
                "write   /etc/a.conf  (3 bytes, mode 644)",
                "set     /sys/x/rate = 200",
                "remove  /etc/a",
                "remove  /usr/src/a  (recursive)",
                "wait    5 ms",
            ]
        );
    }

    #[test]
    fn rollback_lines() {
        let mut p = Plan::new();
        p.push(Action::Sleep(1));
        p.extend([Action::Sleep(2)]);
        p.rollback.push(Action::Sleep(3));
        assert_eq!(
            p.lines(),
            vec![
                "wait    1 ms",
                "wait    2 ms",
                "on failure:",
                "  wait    3 ms"
            ]
        );
    }

    #[test]
    fn dry_run() {
        let dir = temp_dir();
        let sys = Sys::new(&dir);
        let runner = FakeRunner::new();
        let ex = Executor {
            sys: &sys,
            runner: &runner,
            dry_run: true,
            sleep: false,
        };
        let mut plan = Plan::new();
        plan.push(Action::write("/etc/x", "1", 0o644));
        plan.push(Action::Run(Cmd::new("rmmod").arg("psmouse")));
        let mut out = Vec::new();
        ex.execute(&plan, &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "  write   /etc/x  (1 bytes, mode 644)\n  run     rmmod psmouse\n"
        );
        assert!(!dir.join("etc/x").exists());
        assert!(runner.log().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_actions() {
        let dir = temp_dir();
        let sys = Sys::new(&dir);
        fs::create_dir_all(dir.join("sys/x")).unwrap();
        fs::write(dir.join("sys/x/rate"), "100").unwrap();
        fs::create_dir_all(dir.join("old/tree")).unwrap();
        fs::write(dir.join("old/file"), "").unwrap();
        let runner = FakeRunner::new();
        let ex = Executor {
            sys: &sys,
            runner: &runner,
            dry_run: false,
            sleep: true,
        };
        let mut plan = Plan::new();
        plan.push(Action::write("/etc/deep/x.sh", "#!/bin/sh\n", 0o755));
        plan.push(Action::attr("/sys/x/rate", 200));
        plan.push(Action::Remove("/old/file".into()));
        plan.push(Action::Remove("/old/missing".into()));
        plan.push(Action::RemoveTree("/old/tree".into()));
        plan.push(Action::RemoveTree("/old/none".into()));
        plan.push(Action::Sleep(1));
        plan.push(Action::TryRun(Cmd::new("false")));
        ex.execute(&plan, &mut Vec::new()).unwrap();
        let meta = fs::metadata(dir.join("etc/deep/x.sh")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o755);
        assert_eq!(fs::read_to_string(dir.join("sys/x/rate")).unwrap(), "200");
        assert!(!dir.join("old/file").exists());
        assert!(!dir.join("old/tree").exists());
        assert_eq!(runner.execs(), vec!["false"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rollback() {
        let dir = temp_dir();
        let sys = Sys::new(&dir);
        let runner = FakeRunner::new()
            .on_exec("insmod", 1)
            .on_exec("modprobe bad", 2);
        let ex = Executor {
            sys: &sys,
            runner: &runner,
            dry_run: false,
            sleep: false,
        };
        let mut plan = Plan::new();
        plan.push(Action::Run(Cmd::new("rmmod").arg("psmouse")));
        plan.push(Action::Run(Cmd::new("insmod").arg("/x.ko")));
        plan.push(Action::Run(Cmd::new("never")));
        plan.rollback = vec![
            Action::Run(Cmd::new("modprobe").arg("bad")),
            Action::Run(Cmd::new("modprobe").arg("psmouse")),
        ];
        let mut out = Vec::new();
        let err = ex.execute(&plan, &mut out).unwrap_err();
        assert_eq!(err.0, "`insmod /x.ko` exited with status 1");
        assert_eq!(
            runner.execs(),
            vec![
                "rmmod psmouse",
                "insmod /x.ko",
                "modprobe bad",
                "modprobe psmouse"
            ]
        );
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("failed: `insmod /x.ko` exited with status 1"));
        assert!(text.contains("rollback step failed: `modprobe bad` exited with status 2"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn no_rollback() {
        let dir = temp_dir();
        let sys = Sys::new(&dir);
        let runner = FakeRunner::new();
        let ex = Executor {
            sys: &sys,
            runner: &runner,
            dry_run: false,
            sleep: false,
        };
        let mut plan = Plan::new();
        plan.push(Action::attr("/sys/missing/attr", 1));
        let mut out = Vec::new();
        let err = ex.execute(&plan, &mut out).unwrap_err();
        assert!(err.0.starts_with("/sys/missing/attr: "));
        assert!(out.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn error_paths() {
        let dir = temp_dir();
        let sys = Sys::new(&dir);
        fs::write(dir.join("blocker"), "").unwrap();
        fs::create_dir_all(dir.join("adir/inner")).unwrap();
        let runner = FakeRunner::new();
        let ex = Executor {
            sys: &sys,
            runner: &runner,
            dry_run: false,
            sleep: false,
        };
        let mut plan = Plan::new();
        plan.push(Action::write("/blocker/x", "1", 0o644));
        assert!(
            ex.execute(&plan, &mut Vec::new())
                .unwrap_err()
                .0
                .starts_with("/blocker/x: ")
        );
        let mut plan = Plan::new();
        plan.push(Action::Remove("/adir".into()));
        assert!(
            ex.execute(&plan, &mut Vec::new())
                .unwrap_err()
                .0
                .starts_with("/adir: ")
        );
        let mut plan = Plan::new();
        plan.push(Action::RemoveTree("/blocker".into()));
        assert!(
            ex.execute(&plan, &mut Vec::new())
                .unwrap_err()
                .0
                .starts_with("/blocker: ")
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
