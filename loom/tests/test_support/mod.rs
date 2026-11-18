//! Shared subprocess policy for repository fixtures.
#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

pub mod git_repo;

pub fn write_main_checkout_identity(repo: &Path, work_dir: &Path) {
    use std::os::unix::fs::MetadataExt;

    let checkout = repo.canonicalize().expect("canonical checkout");
    let git_dir = checkout
        .join(".git")
        .canonicalize()
        .expect("canonical git dir");
    let directory = |path: &Path| {
        let meta = std::fs::metadata(path).expect("checkout identity directory");
        serde_json::json!({
            "path": path,
            "device": meta.dev(),
            "inode": meta.ino(),
        })
    };
    let identity = serde_json::json!({
        "checkout": directory(&checkout),
        "git_dir": directory(&git_dir),
        "common_dir": directory(&git_dir),
    });
    std::fs::create_dir_all(work_dir).expect("create Loom work dir");
    std::fs::write(
        work_dir.join("main-checkout-identity.json"),
        serde_json::to_vec(&identity).expect("serialize checkout identity"),
    )
    .expect("write checkout identity");
}

pub fn clear_loom_env(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
}
