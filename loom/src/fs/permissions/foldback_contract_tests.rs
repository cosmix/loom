//! Contract for the permission fold-back: only the main checkout's own
//! settings file feeds the approved list, and reading it can neither block
//! nor follow a file a session could plant.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use crate::fs::permissions::approved::approved_path;
use crate::fs::permissions::fold_back_approvals;

const BLANKET_RULES: &str = r#"{"permissions":{"allow":["Bash(*)","Edit(//home/u/.bashrc)"]}}"#;

fn write_settings(dir: &Path) {
    let claude = dir.join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(claude.join("settings.local.json"), BLANKET_RULES).unwrap();
}

/// Blanket rules in a subdirectory settings file of the main checkout and of
/// a worktree are not sources, and a FIFO at the main checkout's own path
/// does not hang the fold-back.
#[test]
fn only_the_main_checkout_file_feeds_the_approved_list() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".loom").join("work")).unwrap();
    write_settings(&root.join("loom"));
    write_settings(&root.join(".worktrees").join("s1").join("loom"));
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    let fifo = root.join(".claude").join("settings.local.json");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).expect("mkfifo");

    let (sender, receiver) = mpsc::channel();
    let main = root.clone();
    std::thread::spawn(move || {
        let _ = sender.send(fold_back_approvals(&main));
    });
    let added = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the fold-back blocked on the FIFO at the main checkout's settings path");

    assert_eq!(added, 0);
    let state_root = root.join(".loom").join("work").canonicalize().unwrap();
    let approved = approved_path(&state_root);
    if approved.exists() {
        let text = std::fs::read_to_string(&approved).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let allow = value["allow"].as_array().cloned().unwrap_or_default();
        for rule in ["Bash(*)", "Edit(//home/u/.bashrc)"] {
            assert!(
                !allow.iter().any(|entry| entry == rule),
                "{rule} reached the approved list: {text}"
            );
        }
    }
}
