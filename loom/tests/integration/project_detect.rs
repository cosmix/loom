//! `loom project detect` end to end: package discovery, runner detection and
//! skill mapping reported as one compact JSON document.

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::helpers::{init_test_repo, loom_cmd};

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture path has a parent"))
        .expect("create fixture directory");
    fs::write(&path, contents).expect("write fixture file");
}

fn package<'a>(packages: &'a [Value], path: &str) -> &'a Value {
    packages
        .iter()
        .find(|package| package["path"] == path)
        .unwrap_or_else(|| panic!("no package {path} in {packages:?}"))
}

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("a JSON array")
        .iter()
        .map(|item| item.as_str().expect("a JSON string"))
        .collect()
}

#[test]
fn project_detect_json_reports_packages() {
    let repo = init_test_repo();
    let root = repo.path();
    write(
        root,
        "rust/Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(root, "rust/src/lib.rs", "");
    write(
        root,
        "web/package.json",
        r#"{"name":"web","devDependencies":{"vitest":"^2.0.0"}}"#,
    );
    write(root, "web/tsconfig.json", "{}\n");

    let output = loom_cmd()
        .args(["project", "detect", "--json"])
        .arg(root)
        .output()
        .expect("run loom project detect");
    assert!(
        output.status.success(),
        "loom project detect failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("stdout is one JSON value");

    let canonical_root = root.canonicalize().expect("canonicalize fixture root");
    assert_eq!(report["root"], canonical_root.to_str().expect("UTF-8 root"));
    assert_eq!(report["truncated"], false);
    let packages = report["packages"].as_array().expect("packages array");

    let rust = package(packages, "rust");
    assert!(strings(&rust["kinds"]).contains(&"rust"), "{rust}");
    assert_eq!(rust["runner"], "cargo-test");
    assert!(strings(&rust["skills"]).contains(&"loom-rust"), "{rust}");

    let web = package(packages, "web");
    assert!(strings(&web["kinds"]).contains(&"typescript"), "{web}");
    assert_eq!(web["runner"], "vitest");
    assert!(
        strings(&web["skills"]).contains(&"loom-typescript"),
        "{web}"
    );
}
