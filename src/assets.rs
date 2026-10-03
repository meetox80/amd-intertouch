use crate::exec::Action;

#[derive(Debug)]
pub struct Asset {
    pub path: &'static str,
    pub data: &'static [u8],
    pub mode: u32,
}

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

pub fn get(path: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|a| a.path == path)
}

pub fn text(path: &str) -> &'static str {
    let asset = get(path).unwrap_or_else(|| panic!("missing embedded asset {path}"));
    std::str::from_utf8(asset.data).expect("embedded text assets are utf-8")
}

pub fn kernel_source() -> &'static str {
    text("KERNEL_SOURCE").trim().trim_start_matches('v')
}

pub fn write_actions(dest: &str) -> Vec<Action> {
    ASSETS
        .iter()
        .map(|a| Action::write(&format!("{dest}/{}", a.path), a.data, a.mode))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_tree() {
        for required in [
            "dkms.conf",
            "Makefile",
            "KERNEL_SOURCE",
            "tools/pre-build.sh",
            "drivers/i2c/busses/i2c-piix4.c",
            "drivers/input/rmi4/rmi_smbus.c",
            "drivers/input/mouse/synaptics.c",
            "drivers/input/mouse/psmouse-base.c",
        ] {
            assert!(get(required).is_some(), "{required} not embedded");
        }
        assert!(get("nope").is_none());
    }

    #[test]
    fn modes() {
        assert_eq!(get("tools/pre-build.sh").unwrap().mode, 0o755);
        assert_eq!(get("dkms.conf").unwrap().mode, 0o644);
        assert!(ASSETS.iter().all(|a| a.mode == 0o755 || a.mode == 0o644));
    }

    #[test]
    fn kernel_version() {
        let v = kernel_source();
        assert!(v.split('.').count() >= 2, "{v}");
        assert!(v.chars().all(|c| c.is_ascii_digit() || c == '.'), "{v}");
    }

    #[test]
    fn write_all() {
        let actions = write_actions("/usr/src/x");
        assert_eq!(actions.len(), ASSETS.len());
        assert!(actions.contains(&Action::write(
            "/usr/src/x/tools/pre-build.sh",
            get("tools/pre-build.sh").unwrap().data,
            0o755
        )));
    }

    #[test]
    #[should_panic(expected = "missing embedded asset")]
    fn missing_panics() {
        text("missing");
    }

    #[test]
    fn hash() {
        assert_ne!(ASSETS_HASH, 0);
    }
}
