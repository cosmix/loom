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

/// Point `LOOM_HOME` at one temp directory for the rest of this test binary. In-process code
/// that writes under the loom home would otherwise write the developer's real `~/.loom`, which
/// a sandboxed run cannot write. Call it before the first such write; it never changes after.
pub fn isolated_loom_home() {
    static HOME: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    HOME.get_or_init(|| {
        let home = tempfile::tempdir().expect("loom home tempdir");
        std::env::set_var("LOOM_HOME", home.path());
        home
    });
}

pub fn clear_loom_env(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
}
