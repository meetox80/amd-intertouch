mod common;

use amd_intertouch::runner::FakeRunner;
use common::*;

#[test]
fn usage_error() {
    let f = p14s_ps2();
    let r = run(&f, &FakeRunner::new(), &["frobnicate"]);
    assert_eq!(r.code, 64);
    assert_eq!(
        r.err,
        "error: unknown command `frobnicate`\nrun `amd-intertouch --help` for usage\n"
    );
    assert_eq!(r.out, "");
}

#[test]
fn help() {
    let f = p14s_ps2();
    let r = run(&f, &FakeRunner::new(), &["--help"]);
    let help = format!("{}\nmade by kotwicha!\n", amd_intertouch::cli::USAGE);
    assert_eq!((r.code, r.out.as_str()), (0, help.as_str()));
    let r = run(&f, &FakeRunner::new(), &[]);
    assert_eq!(r.out, help);
    let r = run(&f, &FakeRunner::new(), &["-V"]);
    assert_eq!(r.out, "amd-intertouch 0.1.2\nmade by kotwicha!\n");
}

#[test]
fn read_only() {
    let f = p14s_ps2();
    f.uid(1000);
    let r = run(&f, &FakeRunner::new(), &["--dry-run", "check"]);
    assert_eq!(r.code, 0);
    assert!(!r.out.contains("dry run"));
    let r = run(&f, &FakeRunner::new(), &["status"]);
    assert_eq!(r.code, 0);
}

#[test]
fn color() {
    let f = p14s_ps2();
    let mut full: Vec<String> = vec!["--root".into(), f.root_str(), "check".into()];
    let env = amd_intertouch::app::Env {
        runner: &FakeRunner::new(),
        sleep: false,
        style: amd_intertouch::output::Style { color: true },
        exe: None,
    };
    let mut out = Vec::new();
    amd_intertouch::app::run_with(&full, &env, &mut out, &mut Vec::new());
    assert!(
        String::from_utf8(out)
            .unwrap()
            .contains("\x1b[32mok  \x1b[0m")
    );
    full.push("--no-color".into());
    let mut out = Vec::new();
    amd_intertouch::app::run_with(&full, &env, &mut out, &mut Vec::new());
    assert!(!String::from_utf8(out).unwrap().contains('\x1b'));
}

#[test]
fn credit() {
    let env = amd_intertouch::app::Env {
        runner: &FakeRunner::new(),
        sleep: false,
        style: amd_intertouch::output::Style { color: true },
        exe: None,
    };
    let mut out = Vec::new();
    amd_intertouch::app::run_with(&["--version".into()], &env, &mut out, &mut Vec::new());
    let colored = amd_intertouch::output::Style { color: true }.credit();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        format!("amd-intertouch 0.1.2\n{colored}\n")
    );
    let mut out = Vec::new();
    amd_intertouch::app::run_with(
        &["--help".into(), "--no-color".into()],
        &env,
        &mut out,
        &mut Vec::new(),
    );
    assert!(
        String::from_utf8(out)
            .unwrap()
            .ends_with("install\n\nmade by kotwicha!\n")
    );
}
