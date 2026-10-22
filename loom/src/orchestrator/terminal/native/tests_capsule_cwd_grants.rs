//! Unit tests for `session_settings/contents.rs`: a plan `allow_write` entry
//! inside the session's working directory reaches neither the OS write list
//! nor the `Edit(...)` rules of the capsule.

use super::contents::{capsule_settings, CapsuleInputs};
use super::tests_contents::{denies, sandbox, strings, HOOKS_DIR};
use crate::models::session::SessionType;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn capsule_in(cwd: &Path, worktree_rooted: bool, allow_write: &[String]) -> Value {
    let mut config = sandbox(false);
    config.filesystem.allow_write.extend_from_slice(allow_write);
    let denies = denies(worktree_rooted, false);
    let kind = if worktree_rooted {
        SessionType::Stage
    } else {
        SessionType::Merge
    };
    capsule_settings(&CapsuleInputs {
        kind,
        sandbox: &config,
        worktree_rooted,
        cwd,
        state_root: Path::new("/repo/.loom/work"),
        repo_root: Path::new("/repo"),
        hooks_dir: Path::new(HOOKS_DIR),
        scratch_dir: Path::new("/scratch/session-1"),
        approved: &[],
        checkout_settings: None,
        denies: &denies,
        python3: None,
        python_hooks: &[],
        hidden_sockets: &[],
        workflow_licensed: false,
    })
    .unwrap()
}

#[test]
fn a_plan_grant_inside_the_session_cwd_reaches_neither_layer() {
    for worktree_rooted in [true, false] {
        let cwd = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let file = cwd.path().join("vitest.config.ts");
        std::fs::write(&file, "").unwrap();
        let absolute = file.to_str().unwrap().to_string();
        let kept = outside.path().to_str().unwrap().to_string();
        let grants = ["vitest.config.ts".to_string(), absolute, kept.clone()];

        let settings = capsule_in(cwd.path(), worktree_rooted, &grants);

        let allow_write = strings(&settings, "/sandbox/filesystem/allowWrite");
        let allow = strings(&settings, "/permissions/allow");
        for grant in allow_write.iter().chain(&allow) {
            assert!(
                !grant.contains("vitest.config.ts"),
                "worktree={worktree_rooted}: {grant} grants a file inside the cwd"
            );
        }
        assert!(
            allow_write.contains(&kept),
            "worktree={worktree_rooted}: {allow_write:?}"
        );
        assert!(
            allow.contains(&format!("Edit(/{kept})")),
            "worktree={worktree_rooted}: {allow:?}"
        );
    }
}
