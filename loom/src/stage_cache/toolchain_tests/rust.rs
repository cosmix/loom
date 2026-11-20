//! Rust: registry dependencies stay fresh across commands, which holds only
//! while every input Cargo fingerprints (source paths, `SDKROOT`) is stable.

use super::{home, output, Rig};
use std::path::PathBuf;

/// The crate the fixture depends on: small, with no dependencies of its own.
const DEPENDENCY: &str = "cfg-if";

/// The newest version of [`DEPENDENCY`] unpacked in the host registry, so the
/// offline build resolves it from there.
fn host_version() -> Option<String> {
    let cargo = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".cargo"));
    let prefix = format!("{DEPENDENCY}-");
    let mut found = Vec::new();
    for index in std::fs::read_dir(cargo.join("registry/src"))
        .ok()?
        .flatten()
    {
        for entry in std::fs::read_dir(index.path()).ok()?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            match name.strip_prefix(&prefix) {
                Some(version) if version.starts_with(|c: char| c.is_ascii_digit()) => {
                    found.push(version.to_string());
                }
                _ => {}
            }
        }
    }
    found.into_iter().max()
}

#[test]
#[serial_test::serial]
fn cargo_dependencies_stay_fresh() {
    let test = "cargo_dependencies_stay_fresh";
    let Some(rig) = Rig::start(test, &["cargo"]) else {
        return;
    };
    let version = host_version();
    if crate::process::sandbox_probe::skip_unless(
        version.is_some(),
        test,
        "no cfg-if in the host Cargo registry",
    ) {
        return;
    }
    rig.write(
        "Cargo.toml",
        &format!(
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n{DEPENDENCY} = \"={}\"\n",
            version.unwrap()
        ),
    );
    rig.write(
        "src/main.rs",
        "fn main() { cfg_if::cfg_if! { if #[cfg(unix)] { println!(\"unix\"); } \
         else { println!(\"other\"); } } }\n",
    );
    let build = "cargo build -v --offline";
    let (first, second) = rig.run_twice(build, build);
    let compiling = format!("Compiling {DEPENDENCY} ");
    assert!(output(&first).contains(&compiling), "{first:?}");
    assert!(!output(&second).contains(&compiling), "{second:?}");
    let fresh = format!("Fresh {DEPENDENCY} ");
    assert!(output(&second).contains(&fresh), "{second:?}");
}
