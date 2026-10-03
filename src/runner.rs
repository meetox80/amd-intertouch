use crate::error::{Error, Result};
use std::cell::RefCell;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

impl Cmd {
    pub fn new(program: &str) -> Self {
        Cmd {
            program: program.to_string(),
            args: Vec::new(),
            cwd: None,
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I, S>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(items.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, dir: impl Into<String>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    pub fn line(&self) -> String {
        let mut parts = vec![quote(&self.program)];
        parts.extend(self.args.iter().map(|a| quote(a)));
        let line = parts.join(" ");
        match &self.cwd {
            Some(dir) => format!("(cd {} && {line})", quote(dir)),
            None => line,
        }
    }
}

fn quote(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@+%,".contains(c));
    if plain {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    pub fn success(stdout: &str) -> Self {
        Output {
            code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    pub fn failure(code: i32, stderr: &str) -> Self {
        Output {
            code,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }
}

pub trait Runner {
    fn query(&self, cmd: &Cmd) -> Result<Output>;
    fn exec(&self, cmd: &Cmd) -> Result<i32>;
}

pub fn query_ok(runner: &dyn Runner, cmd: &Cmd) -> Option<String> {
    runner.query(cmd).ok().filter(Output::ok).map(|o| o.stdout)
}

pub struct HostRunner;

impl HostRunner {
    fn command(cmd: &Cmd) -> Command {
        let mut c = Command::new(&cmd.program);
        c.args(&cmd.args).stdin(Stdio::null());
        if let Some(dir) = &cmd.cwd {
            c.current_dir(dir);
        }
        c
    }
}

impl Runner for HostRunner {
    fn query(&self, cmd: &Cmd) -> Result<Output> {
        let out = Self::command(cmd)
            .output()
            .map_err(|e| Error(format!("{}: {e}", cmd.program)))?;
        Ok(Output {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    fn exec(&self, cmd: &Cmd) -> Result<i32> {
        let status = Self::command(cmd)
            .status()
            .map_err(|e| Error(format!("{}: {e}", cmd.program)))?;
        Ok(status.code().unwrap_or(-1))
    }
}

#[derive(Default)]
pub struct FakeRunner {
    queries: Vec<(String, Output)>,
    execs: Vec<(String, i32)>,
    log: RefCell<Vec<String>>,
}

impl FakeRunner {
    pub fn new() -> Self {
        FakeRunner::default()
    }

    pub fn on_query(mut self, prefix: &str, out: Output) -> Self {
        self.queries.push((prefix.to_string(), out));
        self
    }

    pub fn on_exec(mut self, prefix: &str, code: i32) -> Self {
        self.execs.push((prefix.to_string(), code));
        self
    }

    pub fn log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }

    pub fn execs(&self) -> Vec<String> {
        self.log
            .borrow()
            .iter()
            .filter_map(|l| l.strip_prefix("exec ").map(str::to_string))
            .collect()
    }
}

impl Runner for FakeRunner {
    fn query(&self, cmd: &Cmd) -> Result<Output> {
        let line = cmd.line();
        self.log.borrow_mut().push(format!("query {line}"));
        Ok(self
            .queries
            .iter()
            .rev()
            .find(|(p, _)| line.starts_with(p.as_str()))
            .map(|(_, o)| o.clone())
            .unwrap_or_else(|| Output::failure(127, "not stubbed")))
    }

    fn exec(&self, cmd: &Cmd) -> Result<i32> {
        let line = cmd.line();
        self.log.borrow_mut().push(format!("exec {line}"));
        Ok(self
            .execs
            .iter()
            .rev()
            .find(|(p, _)| line.starts_with(p.as_str()))
            .map(|(_, c)| *c)
            .unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        let c = Cmd::new("insmod").arg("/a/b.ko").arg("x=1");
        assert_eq!(c.line(), "insmod /a/b.ko x=1");
        let c = Cmd::new("echo").args(["two words", "it's", ""]);
        assert_eq!(c.line(), "echo 'two words' 'it'\\''s' ''");
        let c = Cmd::new("patch").arg("-p1").cwd("/var/cache/x y");
        assert_eq!(c.line(), "(cd '/var/cache/x y' && patch -p1)");
    }

    #[test]
    fn output() {
        assert!(Output::success("hi").ok());
        assert_eq!(Output::success("hi").stdout, "hi");
        let f = Output::failure(3, "bad");
        assert!(!f.ok());
        assert_eq!((f.code, f.stderr.as_str()), (3, "bad"));
    }

    #[test]
    fn fake() {
        let r = FakeRunner::new()
            .on_query("modinfo -n psmouse", Output::success("/x\n"))
            .on_exec("rmmod", 1);
        assert_eq!(
            r.query(&Cmd::new("modinfo").args(["-n", "psmouse"]))
                .unwrap()
                .stdout,
            "/x\n"
        );
        assert_eq!(r.query(&Cmd::new("dkms")).unwrap().code, 127);
        assert_eq!(r.exec(&Cmd::new("rmmod").arg("psmouse")).unwrap(), 1);
        assert_eq!(r.exec(&Cmd::new("modprobe").arg("psmouse")).unwrap(), 0);
        assert_eq!(r.execs(), vec!["rmmod psmouse", "modprobe psmouse"]);
        assert_eq!(r.log().len(), 4);
        let layered = FakeRunner::new()
            .on_query("modinfo", Output::success("general"))
            .on_query("modinfo -n", Output::success("specific"))
            .on_exec("rmmod", 1)
            .on_exec("rmmod psmouse", 2);
        assert_eq!(
            layered
                .query(&Cmd::new("modinfo").arg("-n"))
                .unwrap()
                .stdout,
            "specific"
        );
        assert_eq!(
            layered
                .query(&Cmd::new("modinfo").arg("-F"))
                .unwrap()
                .stdout,
            "general"
        );
        assert_eq!(layered.exec(&Cmd::new("rmmod").arg("psmouse")).unwrap(), 2);
        assert_eq!(layered.exec(&Cmd::new("rmmod").arg("x")).unwrap(), 1);
        assert_eq!(
            query_ok(&r, &Cmd::new("modinfo").args(["-n", "psmouse"])).as_deref(),
            Some("/x\n")
        );
        assert_eq!(query_ok(&r, &Cmd::new("dkms")), None);
    }

    #[test]
    fn host() {
        let r = HostRunner;
        let out = r
            .query(&Cmd::new("sh").args(["-c", "echo hi; echo err >&2; exit 3"]))
            .unwrap();
        assert_eq!(
            (out.code, out.stdout.as_str(), out.stderr.as_str()),
            (3, "hi\n", "err\n")
        );
        assert_eq!(r.exec(&Cmd::new("true")).unwrap(), 0);
        assert_eq!(r.exec(&Cmd::new("sh").args(["-c", "exit 4"])).unwrap(), 4);
        let dir = std::env::temp_dir();
        let out = r
            .query(&Cmd::new("pwd").cwd(dir.to_str().unwrap()))
            .unwrap();
        assert_eq!(
            std::path::Path::new(out.stdout.trim())
                .canonicalize()
                .unwrap(),
            dir.canonicalize().unwrap()
        );
        assert!(r.query(&Cmd::new("/nonexistent/program")).is_err());
        assert!(r.exec(&Cmd::new("/nonexistent/program")).is_err());
    }
}
