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
        let temp = TempDir::new_in(fixture_base()).unwrap();
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

/// Session id of the closed-session directory; no payload in these tests
/// carries it as its session_id.
const FIXTURE_CLOSED_SESSION: &str = "00000000-0000-4000-8000-000000000000";

/// Base directory for a fixture root, a Rust port of `loom-hooks/tests/_fixture_base.sh`.
/// The base is the temp dir, unless it lies under `/tmp/claude-<uid>` (the Bash tool's
/// TMPDIR, where `allow_tmpdir` lets Read open our plain files, so a credentials block
/// could not be tested) or inside a checkout under `.worktrees/<id>` (where a fixture
/// project would resolve to the enclosing project's live `.loom/work`). Both cases use
/// the closed-session directory that `allow_tmpdir` keeps shut.
fn fixture_base() -> PathBuf {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let uid_root = PathBuf::from(format!("/tmp/claude-{uid}"));
    let closed = uid_root.join("loom-fixtures").join(FIXTURE_CLOSED_SESSION);
    let base = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    let private_root = PathBuf::from(format!("/private{}", uid_root.display()));
    if base.starts_with(&uid_root) || base.starts_with(&private_root) {
        let target = if base.starts_with("/private") {
            PathBuf::from("/private").join(closed.strip_prefix("/").unwrap())
        } else {
            closed
        };
        fs::create_dir_all(&target).unwrap();
        return target;
    }
    let base_str = format!("{}/", base.display());
    let in_worktree = base_str
        .match_indices("/.worktrees/")
        .any(|(index, marker)| {
            let rest = &base_str[index + marker.len()..];
            rest.find('/').is_some_and(|slash| slash > 0)
        });
    if in_worktree && fs::create_dir_all(&closed).is_ok() {
        return closed.canonicalize().unwrap_or(closed);
    }
    base
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
    use crate::fs::permissions::constants as hook;
    for (name, contents) in [
        ("_common.sh", HOOK_COMMON),
        ("_worktree_policy.sh", hook::HOOK_WORKTREE_POLICY_LOADER),
        ("worktree-policy.json", hook::HOOK_WORKTREE_POLICY),
        ("_parser.sh", hook::HOOK_PARSER),
        ("_stage_lookup.sh", hook::HOOK_STAGE_LOOKUP),
        ("_parser_expand.sh", hook::HOOK_PARSER_EXPAND),
        ("_parser_scan.sh", hook::HOOK_PARSER_SCAN),
        ("_containment.sh", hook::HOOK_CONTAINMENT),
        ("_identity.sh", hook::HOOK_IDENTITY),
        ("_heartbeat.sh", hook::HOOK_HEARTBEAT),
        ("_bounded.sh", HOOK_BOUNDED),
        ("_git-command.sh", hook::HOOK_GIT_COMMAND),
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
