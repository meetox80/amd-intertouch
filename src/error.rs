use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn fail<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error(msg.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display() {
        assert_eq!(Error("boom".into()).to_string(), "boom");
    }

    #[test]
    fn from_io() {
        let e: Error = std::io::Error::new(std::io::ErrorKind::NotFound, "gone").into();
        assert_eq!(e.0, "gone");
    }

    #[test]
    fn fail_err() {
        let r: Result<()> = fail("nope");
        assert_eq!(r, Err(Error("nope".into())));
    }
}
