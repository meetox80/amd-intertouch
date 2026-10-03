#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    Confirmed,
    Reported,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Model {
    pub pnp: &'static str,
    pub name: &'static str,
    pub evidence: Evidence,
    pub note: &'static str,
}

pub static MODELS: &[Model] = &[
    Model {
        pnp: "LEN2064",
        name: "ThinkPad T14 / P14s Gen 1 AMD",
        evidence: Evidence::Confirmed,
        note: "works; NoSleep is on by default for this pad",
    },
    Model {
        pnp: "LEN2073",
        name: "ThinkPad T14 / P14s Gen 2 AMD",
        evidence: Evidence::Confirmed,
        note: "tested by the author of the Host Notify patch",
    },
    Model {
        pnp: "LEN0411",
        name: "ThinkPad L14 Gen 1 AMD",
        evidence: Evidence::Reported,
        note: "same chipset and SMB0001 resource reported on linux-input; not yet confirmed with this fix",
    },
];

pub fn lookup(pnp: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pnp.eq_ignore_ascii_case(pnp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_case() {
        assert_eq!(lookup("len2064").unwrap().evidence, Evidence::Confirmed);
        assert_eq!(lookup("LEN0411").unwrap().evidence, Evidence::Reported);
        assert!(lookup("LEN9999").is_none());
    }

    #[test]
    fn unique_ids() {
        for (i, m) in MODELS.iter().enumerate() {
            assert!(m.pnp.len() == 7 && m.pnp.starts_with("LEN"), "{}", m.pnp);
            assert!(MODELS[i + 1..].iter().all(|o| o.pnp != m.pnp));
            assert!(!m.name.is_empty() && !m.note.is_empty());
        }
    }

    #[test]
    fn nosleep_list() {
        let patch = crate::assets::text(
            "patches/0005-Input-synaptics-keep-the-RMI4-sensor-awake-on-the-ThinkPad-T14-P14s-Gen-1-AMD.patch",
        );
        assert!(patch.contains(
            "+static const char * const rmi_nosleep_pnp_ids[] = {\n+\t\"LEN2064\",\n+\tNULL\n+};"
        ));
        assert!(lookup("LEN2064").unwrap().note.contains("NoSleep"));
    }
}
