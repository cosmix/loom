//! Contract `state-root-parent-left-renameable`: in a tree holding its state
//! root at `.loom/work`, the parent `.loom` is pinned so a confined command
//! cannot rename it and carry the read-only state root away. Bubblewrap
//! applies mounts in order, so the parent's self-bind must come before the
//! state root's read-only bind, or it covers and unprotects it. Nothing here
//! starts a sandbox.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::confine::host::protected::protected;

/// A canonical checkout: `.loom/work` holding `config.toml`, and the
/// `.git`, `.worktrees` and `.claude` control directories.
fn checkout() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let state_root = root.join(".loom").join("work");
    std::fs::create_dir_all(&state_root).unwrap();
    std::fs::write(state_root.join("config.toml"), "[plan]\n").unwrap();
    for dir in [".git", ".worktrees", ".claude"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    (tmp, root)
}

/// Index of the first `flag path path` triple in `args`.
#[cfg(target_os = "linux")]
fn triple(args: &[&std::ffi::OsStr], flag: &str, path: &Path) -> Option<usize> {
    args.windows(3)
        .position(|w| w[0] == flag && w[1] == path.as_os_str() && w[2] == path.as_os_str())
}

#[test]
fn the_state_roots_parent_is_pinned_before_its_read_only_bind() {
    let (_tmp, root) = checkout();
    let parent = root.join(".loom");

    let found = protected(&root);
    assert_eq!(found.pinned, vec![parent.clone()]);

    #[cfg(target_os = "linux")]
    {
        let state_root = parent.join("work");
        let mut command = std::process::Command::new("bwrap");
        super::confine::host::protected::bind(&mut command, &found);
        let args: Vec<&std::ffi::OsStr> = command.get_args().collect();
        let pin = triple(&args, "--bind", &parent).expect("the parent is self-bound");
        let read_only =
            triple(&args, "--ro-bind", &state_root).expect("the state root is bound read-only");
        assert!(
            pin < read_only,
            "the parent's bind at {pin} covers the state root's read-only bind at {read_only}: {args:?}"
        );
    }
    #[cfg(target_os = "macos")]
    {
        let quote = |path: &Path| format!("\"{}\"", path.display());
        let rules = super::confine::host::protected::seatbelt_rules(&found, quote);
        let pin = format!("(deny file-write* (literal \"{}\"))", parent.display());
        assert!(rules.contains(&pin), "{rules}");
    }
}
