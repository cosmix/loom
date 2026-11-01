//! Shared filesystem and hook runner fixtures for policy tests.

use super::*;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

pub(super) struct HookFixture {
    pub(super) _temp: TempDir,
    pub(super) hooks: PathBuf,
    pub(super) worktree: PathBuf,
    pub(super) sibling: PathBuf,
    pub(super) outside: PathBuf,
    pub(super) home: PathBuf,
    pub(super) work_dir: PathBuf,
}

impl HookFixture {
    pub(super) fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        // The default install location: the forward guard accepts the
        // `~/.claude/hooks/loom/...` spelling only from a tree that is it.
        let hooks = home.join(".claude/hooks/loom");
        let worktree = repo.join(".worktrees/stage");
        let sibling = repo.join(".worktrees/stage-sibling");
        let outside = temp.path().join("outside.txt");
        let work_dir = repo.join(".loom").join("work");

        for path in [&hooks, &worktree, &sibling, &home, &work_dir] {
            fs::create_dir_all(path).unwrap();
        }
        install_helpers(&hooks);
        fs::write(&outside, "outside").unwrap();
        fs::write(sibling.join("file.txt"), "sibling").unwrap();
        fs::create_dir_all(worktree.join(".loom")).unwrap();
        fs::create_dir(worktree.join(".git")).unwrap();
        symlink("../../../.loom/work", worktree.join(".loom").join("work")).unwrap();
        Self {
            _temp: temp,
            hooks,
            worktree,
            sibling,
            outside,
            home,
            work_dir,
        }
    }

    pub(super) fn install(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.hooks.join(name);
        fs::write(&path, contents).unwrap();
        path
    }
}

pub(super) fn run_hook(
    script: &Path,
    cwd: &Path,
    envs: &[(&str, &Path)],
    payload: &Value,
) -> Output {
    let mut child = Command::new("bash")
        .arg(script)
        .current_dir(cwd)
        .envs(envs.iter().map(|(key, value)| (*key, value.as_os_str())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn install_helpers(hooks: &Path) {
    for (name, contents) in [
        ("_common.sh", HOOK_COMMON),
        ("_parser.sh", crate::fs::permissions::constants::HOOK_PARSER),
        (
            "_stage_lookup.sh",
            crate::fs::permissions::constants::HOOK_STAGE_LOOKUP,
        ),
        (
            "_parser_expand.sh",
            crate::fs::permissions::constants::HOOK_PARSER_EXPAND,
        ),
        (
            "_parser_scan.sh",
            crate::fs::permissions::constants::HOOK_PARSER_SCAN,
        ),
        (
            "_containment.sh",
            crate::fs::permissions::constants::HOOK_CONTAINMENT,
        ),
        (
            "_identity.sh",
            crate::fs::permissions::constants::HOOK_IDENTITY,
        ),
        (
            "_heartbeat.sh",
            crate::fs::permissions::constants::HOOK_HEARTBEAT,
        ),
        ("_bounded.sh", HOOK_BOUNDED),
        ("_lifecycle.sh", HOOK_LIFECYCLE),
        ("_codex_forward.sh", HOOK_CODEX_FORWARD_COMMON),
        ("_progress-classification.sh", HOOK_PROGRESS_CLASSIFICATION),
        ("_post-tool-heartbeat.sh", HOOK_POST_TOOL_HEARTBEAT),
        ("_read_ledger.sh", HOOK_READ_LEDGER),
        ("_read_discipline.sh", HOOK_READ_DISCIPLINE),
    ] {
        fs::write(hooks.join(name), contents).unwrap();
    }
}
