use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("readable dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, base, out);
        } else {
            out.push(path.strip_prefix(base).expect("under base").to_path_buf());
        }
    }
}

fn fnv(hash: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *hash ^= u64::from(*b);
        *hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest.join("dkms");
    println!("cargo:rerun-if-changed=dkms");
    let mut files = Vec::new();
    walk(&root, &root, &mut files);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut code = String::from("pub static ASSETS: &[Asset] = &[\n");
    for rel in &files {
        let abs = root.join(rel);
        let data = fs::read(&abs).unwrap_or_else(|e| panic!("{}: {e}", abs.display()));
        let rel = rel.to_str().expect("utf-8 asset path");
        let mode = if rel.ends_with(".sh") { 0o755 } else { 0o644 };
        fnv(&mut hash, rel.as_bytes());
        fnv(&mut hash, &[0]);
        fnv(&mut hash, &data);
        fnv(&mut hash, &[0]);
        writeln!(
            code,
            "    Asset {{ path: {rel:?}, data: include_bytes!({:?}), mode: 0o{mode:o} }},",
            abs.to_str().expect("utf-8 path")
        )
        .expect("write to string");
    }
    code.push_str("];\n");
    writeln!(code, "pub const ASSETS_HASH: u64 = 0x{hash:016x};").expect("write to string");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("assets.rs");
    fs::write(&out, code).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
}
