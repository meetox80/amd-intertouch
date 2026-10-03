use crate::check::Level;
use std::io::IsTerminal;

pub const CREDIT: &str = "made by kotwicha!";
pub const CREDIT_FROM: (u8, u8, u8) = (165, 243, 252);
pub const CREDIT_TO: (u8, u8, u8) = (96, 165, 250);

fn mix(a: u8, b: u8, i: usize, last: usize) -> u8 {
    if last == 0 {
        return a;
    }
    let (a, b) = (i64::from(a), i64::from(b));
    (a + (b - a) * i as i64 / last as i64) as u8
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub color: bool,
}

impl Style {
    pub fn detect() -> Self {
        Style {
            color: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    pub fn plain() -> Self {
        Style { color: false }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn level(&self, level: Level) -> String {
        let code = match level {
            Level::Ok => "32",
            Level::Info => "36",
            Level::Warn => "33",
            Level::Fail => "31",
        };
        self.paint(code, &format!("{:<4}", level.tag()))
    }

    pub fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }

    pub fn gradient(&self, text: &str, from: (u8, u8, u8), to: (u8, u8, u8)) -> String {
        if !self.color {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let last = chars.len().saturating_sub(1);
        let mut out = String::from("\x1b[1m");
        for (i, c) in chars.iter().enumerate() {
            let r = mix(from.0, to.0, i, last);
            let g = mix(from.1, to.1, i, last);
            let b = mix(from.2, to.2, i, last);
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{c}"));
        }
        out.push_str("\x1b[0m");
        out
    }

    pub fn credit(&self) -> String {
        self.gradient(CREDIT, CREDIT_FROM, CREDIT_TO)
    }
}

pub fn row(label: &str, value: &str) -> String {
    format!("{label:<12} {value}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain() {
        let s = Style::plain();
        assert_eq!(s.level(Level::Ok), "ok  ");
        assert_eq!(s.level(Level::Fail), "fail");
        assert_eq!(s.bold("x"), "x");
    }

    #[test]
    fn ansi() {
        let s = Style { color: true };
        assert_eq!(s.level(Level::Warn), "\x1b[33mwarn\x1b[0m");
        assert_eq!(s.level(Level::Info), "\x1b[36minfo\x1b[0m");
        assert_eq!(s.level(Level::Ok), "\x1b[32mok  \x1b[0m");
        assert_eq!(s.level(Level::Fail), "\x1b[31mfail\x1b[0m");
        assert_eq!(s.bold("x"), "\x1b[1mx\x1b[0m");
    }

    #[test]
    fn rows() {
        assert_eq!(row("dkms", "ok"), "dkms         ok");
        assert_eq!(row("a-very-long-label", "v"), "a-very-long-label v");
    }

    fn strip(s: &str) -> String {
        let mut out = String::new();
        let mut esc = false;
        for c in s.chars() {
            match (esc, c) {
                (false, '\x1b') => esc = true,
                (true, 'm') => esc = false,
                (true, _) => {}
                (false, c) => out.push(c),
            }
        }
        out
    }

    #[test]
    fn gradient_steps() {
        let s = Style { color: true };
        assert_eq!(
            s.gradient("abc", (0, 0, 0), (10, 20, 30)),
            "\x1b[1m\x1b[38;2;0;0;0ma\x1b[38;2;5;10;15mb\x1b[38;2;10;20;30mc\x1b[0m"
        );
        assert_eq!(
            s.gradient("x", (1, 2, 3), (9, 9, 9)),
            "\x1b[1m\x1b[38;2;1;2;3mx\x1b[0m"
        );
        assert_eq!(s.gradient("", (1, 2, 3), (9, 9, 9)), "\x1b[1m\x1b[0m");
        assert_eq!(
            s.gradient("ab", (200, 10, 0), (100, 0, 50)),
            "\x1b[1m\x1b[38;2;200;10;0ma\x1b[38;2;100;0;50mb\x1b[0m"
        );
    }

    #[test]
    fn credit_colors() {
        let colored = Style { color: true }.credit();
        assert!(colored.starts_with("\x1b[1m\x1b[38;2;165;243;252mm"));
        assert!(colored.ends_with("\x1b[38;2;96;165;250m!\x1b[0m"));
        assert_eq!(strip(&colored), "made by kotwicha!");
        assert_eq!(Style::plain().credit(), "made by kotwicha!");
        assert_eq!(Style::plain().gradient("x", (0, 0, 0), (1, 1, 1)), "x");
    }

    #[test]
    fn detect() {
        let _ = Style::detect();
    }
}
