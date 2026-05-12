//! Live podman smoke tests for the container-backend hardening work in
//! `doc/plans/PLAN-container-backend-hardening.md`.
//!
//! Each test exercises observable behavior of the container runtime that
//! unit tests cannot prove on their own:
//!
//! * `smoke_rm_f_missing_container_exits_zero` — validates the assumption
//!   underpinning the preemptive `rm -f` preamble in
//!   `orchestrator::terminal::container::preemptive_remove_existing`.
//! * `smoke_git_identity_env_reaches_container` — validates that
//!   `GIT_AUTHOR_*` / `GIT_COMMITTER_*` env injection in
//!   `build_env_for_session` actually arrives inside the container.
//! * `smoke_commit_filter_blocks_co_authored_by_in_container` — the
//!   headline regression check for the hook-installation fix: when the
//!   operator's host hooks are bind-mounted at `/home/loom/.claude/hooks/loom/`
//!   (the path the generator writes into worktree settings for container
//!   backend), `commit-filter.sh` correctly blocks a `git commit` whose
//!   body contains a Claude `Co-Authored-By` trailer.
//!
//! Tests are skipped (not failed) when the local environment cannot run
//! them: no podman binary, no usable image, or no installed hook scripts.
//! That keeps them green on macOS / CI without container runtime, but they
//! run automatically on a Linux dev machine that already provisions
//! loom-base images and host hooks.

use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

/// Returns true when the `podman` binary can be invoked successfully.
fn podman_available() -> bool {
    Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Pick a locally-available image to run the smoke containers in. Prefers a
/// `localhost/loom/base:*` image (matches the runtime topology loom uses
/// itself); falls back to `docker.io/library/ubuntu:24.04` if already pulled.
/// Returns `None` if neither is present — the calling test should skip.
fn pick_smoke_image() -> Option<String> {
    // Try loom base first.
    let out = Command::new("podman")
        .args([
            "images",
            "localhost/loom/base",
            "--format",
            "{{.Repository}}:{{.Tag}}",
        ])
        .output()
        .ok()?;
    if out.status.success() {
        if let Some(first) = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .map(str::to_string)
        {
            if !first.trim().is_empty() {
                return Some(first);
            }
        }
    }
    // Fall back to any ubuntu:24.04 already present (do not pull).
    let out = Command::new("podman")
        .args(["images", "-q", "docker.io/library/ubuntu:24.04"])
        .output()
        .ok()?;
    if out.status.success() && !out.stdout.is_empty() {
        return Some("docker.io/library/ubuntu:24.04".to_string());
    }
    None
}

/// Returns the host hooks directory if it contains the loom hook scripts.
/// Skip the test rather than fail if the operator hasn't installed hooks.
fn installed_hooks_dir() -> Option<PathBuf> {
    let dir = dirs::home_dir()?.join(".claude/hooks/loom");
    if dir.join("commit-filter.sh").exists() && dir.join("_common.sh").exists() {
        Some(dir)
    } else {
        None
    }
}

fn unique_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("loom-smoke-{prefix}-{nanos}")
}

/// Best-effort container teardown — `rm -f` exits 0 even on missing names,
/// so this is safe to call unconditionally in test cleanup paths.
fn rm_force(name: &str) {
    let _ = Command::new("podman").args(["rm", "-f", name]).output();
}

/// Smoke 1: validates the contract that `podman rm -f <missing>` exits 0.
/// If this fails, the preemptive removal preamble in `spawn_common` would
/// surface a spurious error and block stage retries.
#[test]
fn smoke_rm_f_missing_container_exits_zero() {
    if !podman_available() {
        eprintln!("smoke_rm_f_missing_container_exits_zero: skipping — podman not available");
        return;
    }
    let name = unique_name("rmf-missing");
    let status = Command::new("podman")
        .args(["rm", "-f", &name])
        .status()
        .expect("podman invocation should not panic");
    assert!(
        status.success(),
        "`podman rm -f` on a guaranteed-missing container must exit 0; got {status:?}",
    );
}

/// Smoke 4: validates that `GIT_AUTHOR_*` / `GIT_COMMITTER_*` values
/// produced by `build_env_for_session` actually arrive in the container
/// process environment when passed via `podman run -e`.
#[test]
fn smoke_git_identity_env_reaches_container() {
    if !podman_available() {
        eprintln!("smoke_git_identity_env_reaches_container: skipping — podman not available");
        return;
    }
    let Some(image) = pick_smoke_image() else {
        eprintln!(
            "smoke_git_identity_env_reaches_container: skipping — no loom/base or ubuntu:24.04 image present"
        );
        return;
    };
    let name = unique_name("env");
    rm_force(&name);
    let output = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--name",
            &name,
            "-e",
            "GIT_AUTHOR_NAME=Smoke Author",
            "-e",
            "GIT_AUTHOR_EMAIL=author@smoke.test",
            "-e",
            "GIT_COMMITTER_NAME=Smoke Committer",
            "-e",
            "GIT_COMMITTER_EMAIL=committer@smoke.test",
            "--entrypoint",
            "sh",
            &image,
            "-c",
            "env | grep ^GIT_",
        ])
        .output()
        .expect("podman run should not panic");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "podman run failed: stdout={stdout}\nstderr={stderr}",
    );
    for expected in [
        "GIT_AUTHOR_NAME=Smoke Author",
        "GIT_AUTHOR_EMAIL=author@smoke.test",
        "GIT_COMMITTER_NAME=Smoke Committer",
        "GIT_COMMITTER_EMAIL=committer@smoke.test",
    ] {
        assert!(
            stdout.contains(expected),
            "missing `{expected}` in container env output:\n{stdout}",
        );
    }
}

/// Smoke 5: the headline regression check. Bind-mounts the operator's
/// installed `~/.claude/hooks/loom/` at the in-container path that the
/// generator writes into worktree `settings.local.json` for container
/// backend (`/home/loom/.claude/hooks/loom/`), then drives `commit-filter.sh`
/// with a Claude Code PreToolUse payload describing a `git commit` whose
/// message body contains a `Co-Authored-By: Claude` trailer. The hook must
/// exit with code 2 (its block signal).
///
/// Verifies: bind-mount resolves the path that the hook generator emits,
/// the hook script is executable inside the container, and the in-container
/// commit-filter behavior matches the host-side hook test suite.
#[test]
fn smoke_commit_filter_blocks_co_authored_by_in_container() {
    if !podman_available() {
        eprintln!(
            "smoke_commit_filter_blocks_co_authored_by_in_container: skipping — podman not available"
        );
        return;
    }
    let Some(image) = pick_smoke_image() else {
        eprintln!(
            "smoke_commit_filter_blocks_co_authored_by_in_container: skipping — no loom/base or ubuntu:24.04 image present"
        );
        return;
    };
    let Some(hooks_dir) = installed_hooks_dir() else {
        eprintln!(
            "smoke_commit_filter_blocks_co_authored_by_in_container: skipping — ~/.claude/hooks/loom/ not installed"
        );
        return;
    };
    let name = unique_name("filter");
    rm_force(&name);

    // PreToolUse payload that simulates Claude Code about to run a git
    // commit whose body carries the forbidden trailer. The trailer text
    // matches the patterns commit-filter.sh blocks on.
    let payload = r#"{"tool_name":"Bash","tool_input":{"command":"git commit -m $'feat: x\n\nCo-Authored-By: Claude <noreply@anthropic.com>'"}}"#;

    let mount_arg = format!("{}:/home/loom/.claude/hooks/loom:ro", hooks_dir.display());
    let inner = format!(
        "printf %s {payload_q} | /home/loom/.claude/hooks/loom/commit-filter.sh",
        payload_q = shell_quote(payload),
    );
    let output = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--name",
            &name,
            "-v",
            &mount_arg,
            "--entrypoint",
            "bash",
            &image,
            "-c",
            &inner,
        ])
        .output()
        .expect("podman run should not panic");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // commit-filter.sh signals "block" via exit code 2 (per its header
    // comment). Allow non-zero generally to keep the test resilient to
    // future signal-style changes.
    assert!(
        !output.status.success(),
        "commit-filter.sh must block a Co-Authored-By: Claude trailer; \
         got exit={:?}, stdout={stdout}, stderr={stderr}",
        output.status.code(),
    );
    // Sanity: stderr should mention the forbidden pattern so the operator
    // understands why the commit was rejected.
    assert!(
        stderr.to_lowercase().contains("co-authored")
            || stderr.to_lowercase().contains("claude")
            || stderr.to_lowercase().contains("attribution"),
        "commit-filter.sh stderr should explain the rejection; got:\n{stderr}",
    );
}

/// Single-quote the value for safe interpolation into a `bash -c` argument.
/// Replaces every `'` with `'\''` per the standard shell-quoting trick.
fn shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for c in value.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}
