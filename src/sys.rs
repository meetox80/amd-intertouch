use std::fs;
use std::path::{Path, PathBuf};

pub const PATH_DIRS: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sys {
    root: PathBuf,
}

impl Sys {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Sys { root: root.into() }
    }

    pub fn host() -> Self {
        Sys::new("/")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, p: impl AsRef<Path>) -> PathBuf {
        let p = p.as_ref();
        self.root.join(p.strip_prefix("/").unwrap_or(p))
    }

    pub fn read(&self, p: impl AsRef<Path>) -> Option<String> {
        fs::read_to_string(self.path(p)).ok()
    }

    pub fn read_trim(&self, p: impl AsRef<Path>) -> Option<String> {
        self.read(p).map(|s| s.trim().to_string())
    }

    pub fn read_bytes(&self, p: impl AsRef<Path>) -> Option<Vec<u8>> {
        fs::read(self.path(p)).ok()
    }

    pub fn exists(&self, p: impl AsRef<Path>) -> bool {
        self.path(p).exists()
    }

    pub fn is_dir(&self, p: impl AsRef<Path>) -> bool {
        self.path(p).is_dir()
    }

    pub fn list(&self, p: impl AsRef<Path>) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.path(p))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    pub fn find_program(&self, name: &str) -> Option<String> {
        PATH_DIRS
            .iter()
            .map(|d| format!("{d}/{name}"))
            .find(|p| self.path(p).is_file())
    }

    pub fn euid(&self) -> Option<u32> {
        self.read("/proc/self/status")?
            .lines()
            .find_map(|l| l.strip_prefix("Uid:"))
            .and_then(|rest| rest.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::temp_dir;

    #[test]
    fn paths() {
        let sys = Sys::new("/tmp/root");
        assert_eq!(sys.path("/sys/bus"), PathBuf::from("/tmp/root/sys/bus"));
        assert_eq!(sys.path("rel/x"), PathBuf::from("/tmp/root/rel/x"));
        assert_eq!(Sys::host().path("/proc"), PathBuf::from("/proc"));
        assert_eq!(sys.root(), Path::new("/tmp/root"));
    }

    #[test]
    fn reads() {
        let dir = temp_dir();
        let sys = Sys::new(&dir);
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/f"), " v \n").unwrap();
        fs::write(dir.join("a/c"), "x").unwrap();
        fs::create_dir_all(dir.join("usr/bin")).unwrap();
        fs::write(dir.join("usr/bin/make"), "").unwrap();
        assert_eq!(sys.read("/a/f").as_deref(), Some(" v \n"));
        assert_eq!(sys.read_trim("/a/f").as_deref(), Some("v"));
        assert_eq!(sys.read_bytes("/a/c"), Some(b"x".to_vec()));
        assert_eq!(sys.read("/a/missing"), None);
        assert!(sys.exists("/a/f"));
        assert!(sys.is_dir("/a/b"));
        assert!(!sys.is_dir("/a/f"));
        assert_eq!(sys.list("/a"), vec!["b", "c", "f"]);
        assert!(sys.list("/nope").is_empty());
        assert_eq!(sys.find_program("make").as_deref(), Some("/usr/bin/make"));
        assert_eq!(sys.find_program("cc"), None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn euid() {
        let dir = temp_dir();
        let sys = Sys::new(&dir);
        assert_eq!(sys.euid(), None);
        fs::create_dir_all(dir.join("proc/self")).unwrap();
        fs::write(
            dir.join("proc/self/status"),
            "Name:\tx\nUid:\t1000\t0\t0\t0\nGid:\t1\n",
        )
        .unwrap();
        assert_eq!(sys.euid(), Some(0));
        fs::write(dir.join("proc/self/status"), "Uid:\tbad\n").unwrap();
        assert_eq!(sys.euid(), None);
        fs::remove_dir_all(dir).unwrap();
    }
}
