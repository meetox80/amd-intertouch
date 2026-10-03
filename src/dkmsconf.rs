use crate::error::{Error, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltModule {
    pub name: String,
    pub location: String,
    pub dest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DkmsConf {
    pub name: String,
    pub version: String,
    pub patches: Vec<String>,
    pub modules: Vec<BuiltModule>,
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    v.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(v)
        .to_string()
}

pub fn parse(text: &str) -> Result<DkmsConf> {
    let mut scalars: BTreeMap<String, String> = BTreeMap::new();
    let mut arrays: BTreeMap<String, BTreeMap<usize, String>> = BTreeMap::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| Error(format!("dkms.conf line {}: expected KEY=value", n + 1)))?;
        let value = unquote(value);
        match key.strip_suffix(']').and_then(|k| k.split_once('[')) {
            Some((name, idx)) => {
                let idx: usize = idx
                    .parse()
                    .map_err(|_| Error(format!("dkms.conf line {}: bad index `{idx}`", n + 1)))?;
                arrays
                    .entry(name.to_string())
                    .or_default()
                    .insert(idx, value);
            }
            None => {
                scalars.insert(key.to_string(), value);
            }
        }
    }
    let scalar = |k: &str| {
        scalars
            .get(k)
            .cloned()
            .ok_or_else(|| Error(format!("dkms.conf: missing {k}")))
    };
    let indexed = |k: &str, i: usize| arrays.get(k).and_then(|m| m.get(&i)).cloned();
    let mut modules = Vec::new();
    for (&idx, name) in arrays.get("BUILT_MODULE_NAME").into_iter().flatten() {
        modules.push(BuiltModule {
            name: name.clone(),
            location: indexed("BUILT_MODULE_LOCATION", idx).unwrap_or_default(),
            dest: indexed("DEST_MODULE_LOCATION", idx)
                .ok_or_else(|| Error(format!("dkms.conf: missing DEST_MODULE_LOCATION[{idx}]")))?,
        });
    }
    Ok(DkmsConf {
        name: scalar("PACKAGE_NAME")?,
        version: scalar("PACKAGE_VERSION")?,
        patches: arrays
            .get("PATCH")
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default(),
        modules,
    })
}

pub fn embedded() -> DkmsConf {
    parse(crate::assets::text("dkms.conf")).expect("embedded dkms.conf parses")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets;

    #[test]
    fn parses() {
        let c = parse(
            "# c\n\nPACKAGE_NAME=\"n\"\nPACKAGE_VERSION=1.2\nPATCH[1]=\"b\"\nPATCH[0]=\"a\"\n\
             MAKE[0]=\"make M=x\"\nBUILT_MODULE_NAME[0]=\"m\"\nDEST_MODULE_LOCATION[0]=\"/updates\"\n",
        )
        .unwrap();
        assert_eq!(c.name, "n");
        assert_eq!(c.version, "1.2");
        assert_eq!(c.patches, vec!["a", "b"]);
        assert_eq!(
            c.modules,
            vec![BuiltModule {
                name: "m".into(),
                location: String::new(),
                dest: "/updates".into()
            }]
        );
    }

    #[test]
    fn malformed() {
        assert_eq!(
            parse("JUNK").unwrap_err().0,
            "dkms.conf line 1: expected KEY=value"
        );
        assert_eq!(
            parse("A[x]=1").unwrap_err().0,
            "dkms.conf line 1: bad index `x`"
        );
        assert_eq!(
            parse("PACKAGE_NAME=a").unwrap_err().0,
            "dkms.conf: missing PACKAGE_VERSION"
        );
        assert_eq!(
            parse("PACKAGE_NAME=a\nPACKAGE_VERSION=1\nBUILT_MODULE_NAME[3]=m")
                .unwrap_err()
                .0,
            "dkms.conf: missing DEST_MODULE_LOCATION[3]"
        );
        assert!(
            parse("PACKAGE_NAME=a\nPACKAGE_VERSION=1")
                .unwrap()
                .patches
                .is_empty()
        );
    }

    #[test]
    fn embedded_conf() {
        let c = embedded();
        assert_eq!(c.name, crate::NAME);
        assert_eq!(c.version, crate::VERSION);
    }

    #[test]
    fn patches_listed() {
        let c = embedded();
        for p in &c.patches {
            assert!(
                assets::get(&format!("patches/{p}")).is_some(),
                "{p} listed but not embedded"
            );
        }
        for a in assets::ASSETS
            .iter()
            .filter(|a| a.path.starts_with("patches/"))
        {
            let rel = a.path.trim_start_matches("patches/");
            assert!(
                c.patches.iter().any(|p| p == rel),
                "{rel} embedded but not listed"
            );
        }
    }

    #[test]
    fn patch_order() {
        let c = embedded();
        let first_backport = c
            .patches
            .iter()
            .position(|p| p.starts_with("backports/"))
            .unwrap();
        assert!(
            c.patches[..first_backport]
                .iter()
                .all(|p| !p.starts_with("backports/"))
        );
        let numbers: Vec<&str> = c.patches[..first_backport]
            .iter()
            .map(|p| &p[..4])
            .collect();
        let mut sorted = numbers.clone();
        sorted.sort();
        assert_eq!(numbers, sorted);
    }

    #[test]
    fn built_modules() {
        let c = embedded();
        let names: Vec<&str> = c.modules.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["i2c-piix4", "rmi_smbus", "psmouse"]);
        assert!(c.modules.iter().all(|m| m.dest == "/updates"));
        for m in &c.modules {
            let src = format!("{}{}.c", m.location, m.name);
            let alt = format!("{}{}-base.c", m.location, m.name);
            assert!(
                assets::get(&src).is_some() || assets::get(&alt).is_some(),
                "no source for {}",
                m.name
            );
        }
    }
}
