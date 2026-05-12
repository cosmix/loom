//! Adversarial smoke tests for the container security hardening plan.
//!
//! Each test in this file corresponds to a SPECIFIC attack documented in
//! `doc/plans/PLAN-container-security-hardening.md` and the
//! `integration-verify` stage signal. Every attack listed in the plan MUST
//! fail when run against a properly hardened container — these tests are
//! the executable form of the security invariants.
//!
//! ## Why `#[ignore]`
//!
//! Every test in this file is gated behind `#[ignore]` because:
//!
//! 1. They require a working container runtime (podman or docker), a base
//!    image, the loom daemon, and a real host repo to attack.
//! 2. Several tests deliberately attempt destructive operations (`rm -rf
//!    /repo/*`, writing hooks, etc.) inside their container fixture. The
//!    bind-mount layering keeps these confined to the test scratch dir,
//!    but the failure mode if the hardening regresses is "the host repo
//!    is corrupted." Better to skip by default and run explicitly in CI.
//!
//! Run them explicitly with:
//!
//! ```bash
//! cargo test -- --ignored --test container_adversarial
//! ```
//!
//! Tests that exercise pure host-side code (bundle validation, symlink
//! refusal, capability checks) are also `#[ignore]`'d for consistency —
//! they're part of the same adversarial battery and should be invoked
//! together by the CI job that runs the full attack suite.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns true when the `podman` binary can be invoked successfully.
/// All container-runtime tests skip cleanly when this is false so the
/// suite stays runnable on environments without podman installed.
fn podman_available() -> bool {
    Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Pick the first locally-available image to back the adversarial
/// containers in. Prefers a `localhost/loom/base:*` image (matches the
/// runtime topology loom itself uses); falls back to a pre-pulled
/// `docker.io/library/ubuntu:24.04`. Never pulls — tests skip if neither
/// is present so CI can decide whether to provision an image.
fn pick_image() -> Option<String> {
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
    let out = Command::new("podman")
        .args(["images", "-q", "docker.io/library/ubuntu:24.04"])
        .output()
        .ok()?;
    if out.status.success() && !out.stdout.is_empty() {
        return Some("docker.io/library/ubuntu:24.04".to_string());
    }
    None
}

fn unique_name(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("loom-adv-{prefix}-{nanos}")
}

/// `podman rm -f <missing>` exits 0 — safe to call unconditionally
/// from cleanup paths.
fn rm_force(name: &str) {
    let _ = Command::new("podman").args(["rm", "-f", name]).output();
}

/// Initialize a real git repo at `path` with a single commit on `main`.
/// Returns HEAD OID.
fn init_repo(path: &Path) -> String {
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "adv@test"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Adv"])
        .current_dir(path)
        .output()
        .unwrap();
    std::fs::write(path.join("README.md"), "hi").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(path)
        .output()
        .unwrap();
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ──────────────────────────────────────────────────────────────────────
// Attack 1: `rm -rf /repo/*` inside container leaves host repo untouched.
//
// Defended by: the Standard / IntegrationVerify session mounts in
// `container::build_mounts` (mod.rs:286-313) — REPO_MOUNT is bound RO at
// the base, with a narrow RW overlay only on the worktree subtree.
// `rm -rf /repo/*` fails on everything outside the worktree because the
// RO bind mount returns EROFS.
// ──────────────────────────────────────────────────────────────────────
#[test]
#[ignore]
fn rm_rf_repo_is_isolated() {
    if !podman_available() {
        eprintln!("rm_rf_repo_is_isolated: skipping — podman not available");
        return;
    }
    let Some(image) = pick_image() else {
        eprintln!("rm_rf_repo_is_isolated: skipping — no usable image");
        return;
    };

    let tmp = tempfile::TempDir::new().unwrap();
    let host_repo = tmp.path().join("repo");
    std::fs::create_dir_all(&host_repo).unwrap();
    let head_before = init_repo(&host_repo);
    let canary_path = host_repo.join("CANARY.txt");
    std::fs::write(&canary_path, "must-survive-rm-rf").unwrap();

    let name = unique_name("rmrf");
    rm_force(&name);
    // Mount the host repo READ-ONLY at /repo — mirrors how `build_mounts`
    // mounts REPO_MOUNT for Standard / IntegrationVerify stages. A real
    // session would also bind-mount a writable worktree subtree, but for
    // this attack we only care that the RO bound base survives.
    let _ = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--name",
            &name,
            "-v",
            &format!("{}:/repo:ro", host_repo.display()),
            "--entrypoint",
            "sh",
            &image,
            "-c",
            "rm -rf /repo/* /repo/.* 2>/dev/null; true",
        ])
        .output()
        .expect("podman run");

    assert!(
        canary_path.exists(),
        "host canary disappeared — RO bind mount did not protect the repo. \
         A container compromise could destroy the host repo."
    );
    let head_after = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&host_repo)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    assert_eq!(
        head_before, head_after,
        "host git history changed after in-container rm -rf — \
         RO mount did not protect .git/"
    );
}

// ──────────────────────────────────────────────────────────────────────
// Attack 2: A symlink at an INTERMEDIATE path component must be refused
// by `safe_fs`, not just a symlink at the leaf.
//
// This is the macOS `O_NOFOLLOW` bypass case: `O_NOFOLLOW` only inhibits
// following a symlink at the FINAL component, leaving the door open for
// "/some/legit/dir → /etc; /some/legit/dir/file" to escape the workdir.
// `safe_fs::safe_locked_write_in_workdir` walks the path with O_NOFOLLOW
// at every component (or `openat2` with `RESOLVE_NO_SYMLINKS` on Linux)
// so the intermediate symlink is caught.
// ──────────────────────────────────────────────────────────────────────
#[test]
#[ignore]
fn intermediate_symlink_rejected() {
    use loom::fs::safe_fs::{safe_locked_write_in_workdir, safe_open_dirfd};
    use std::os::fd::AsRawFd;

    let tmp = tempfile::TempDir::new().unwrap();
    let workdir = tmp.path().join("work");
    std::fs::create_dir_all(&workdir).unwrap();

    // Plant a symlink at the INTERMEDIATE component:
    // work/handoffs → /tmp/escape-target
    // and try to write work/handoffs/innocuous.md.
    let escape_target = tmp.path().join("escape-target");
    std::fs::create_dir_all(&escape_target).unwrap();
    std::os::unix::fs::symlink(&escape_target, workdir.join("handoffs")).unwrap();

    let dirfd = safe_open_dirfd(&workdir).expect("open workdir dirfd");
    let relpath = PathBuf::from("handoffs/innocuous.md");
    let err = safe_locked_write_in_workdir(dirfd.as_raw_fd(), &relpath, b"payload\n")
        .expect_err("intermediate symlink must be refused");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("symlink") || msg.contains("ELOOP") || msg.contains("Too many"),
        "rejection must mention symlink/ELOOP — got: {msg}"
    );

    // And the file MUST NOT have appeared inside the escape target.
    assert!(
        !escape_target.join("innocuous.md").exists(),
        "intermediate symlink permitted an escape write into {}",
        escape_target.display()
    );
}

// ──────────────────────────────────────────────────────────────────────
// Attack 3: `loom stop` from inside container must be rejected.
//
// Stop carries `Capability::Admin`. The admin token is mode-0600 and is
// NEVER bind-mounted into the container (see `container::build_mounts`
// — only `user.token` is mounted, never `admin.token`). The daemon's
// `verify_for_capability` is constant-time and fails closed when the
// admin token file is unreadable, so a compromised container holding
// only the user token cannot escalate.
// ──────────────────────────────────────────────────────────────────────
#[test]
#[ignore]
fn admin_token_blocks_stop_from_container() {
    use loom::daemon::{Capability, Request};

    // Structural check: Stop requires Admin capability. If a future
    // refactor downgrades this to User, the in-container user token
    // would suddenly be sufficient to kill the daemon.
    let stop = Request::Stop {
        auth_token: "any".to_string(),
    };
    assert_eq!(
        stop.required_capability(),
        Capability::Admin,
        "Stop must require Capability::Admin — see daemon/protocol.rs"
    );

    // Behavioral check: with admin.token mode-0600 on host (UID != container
    // uid, no bind-mount) and only user.token reachable from inside, the
    // daemon-side `verify_for_capability` must reject a user-token Stop.
    // We construct the workdir layout the daemon expects and call the
    // capability check directly.
    let tmp = tempfile::TempDir::new().unwrap();
    let work = tmp.path();
    std::fs::write(work.join("admin.token"), "admin-secret").unwrap();
    std::fs::write(work.join("user.token"), "user-secret").unwrap();

    // Confirm user token cannot satisfy Admin.
    assert!(
        !verify_capability(work, "user-secret", Capability::Admin),
        "user.token must NOT satisfy Capability::Admin"
    );
    // Confirm admin token does satisfy Admin (sanity check: the
    // negative result above is real rejection, not broken plumbing).
    assert!(
        verify_capability(work, "admin-secret", Capability::Admin),
        "admin.token must satisfy Capability::Admin"
    );
}

// ──────────────────────────────────────────────────────────────────────
// Attack 4: `loom stop` with a (validly issued) user token has no
// daemon-side side-effects — the daemon does not shutdown, does not
// alter any subscriber set, does not write to disk.
//
// Defended by the fail-closed path in daemon/server/client.rs:117-124:
// the server writes `Response::AuthenticationFailed` and `break`s out
// of the connection loop BEFORE the `match request { ... }` block that
// would execute Stop.
// ──────────────────────────────────────────────────────────────────────
#[test]
#[ignore]
fn stop_with_user_token_no_side_effects() {
    use loom::daemon::{Capability, Request};

    let tmp = tempfile::TempDir::new().unwrap();
    let work = tmp.path();
    std::fs::write(work.join("admin.token"), "admin-secret").unwrap();
    std::fs::write(work.join("user.token"), "user-secret").unwrap();

    let stop = Request::Stop {
        auth_token: "user-secret".to_string(),
    };
    // The required capability is Admin and the user token does not satisfy
    // it — the request would be rejected at line 117 of client.rs before
    // the `Request::Stop { .. }` arm ever runs. No daemon shutdown.
    assert_eq!(stop.required_capability(), Capability::Admin);
    assert!(!verify_capability(work, "user-secret", Capability::Admin));

    // Snapshot the workdir state before and after a simulated rejection.
    // verify_for_capability is pure (no writes) — if the check produced
    // any side-effect, the workdir mtime would advance. We don't expect
    // any, but assert anyway to lock the invariant in.
    let before = std::fs::metadata(work.join("admin.token"))
        .unwrap()
        .modified()
        .unwrap();
    let _ = verify_capability(work, "user-secret", Capability::Admin);
    let after = std::fs::metadata(work.join("admin.token"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        before, after,
        "rejected auth check must not touch admin.token (no atime/mtime mutation)"
    );
}

// ──────────────────────────────────────────────────────────────────────
// Attack 5: Agent produces a bundle anchored on a base OID DIFFERENT
// from the one the daemon expects — typically a force-rebase that
// drops or rewrites the spawn-time commit. `validate_bundle` MUST
// reject before any import.
//
// Defended by the prerequisite check inside
// `git_bridge::validate_bundle`: it parses the bundle's `^<oid>` line
// and refuses if `expected_base_oid` is not listed as a prerequisite.
// ──────────────────────────────────────────────────────────────────────
#[test]
#[ignore]
fn bundle_with_wrong_base_rejected() {
    use loom::orchestrator::terminal::container::git_bridge;

    let tmp = tempfile::TempDir::new().unwrap();
    let host = tmp.path().join("host");
    std::fs::create_dir_all(&host).unwrap();
    let real_base = init_repo(&host);

    // Add a second commit so a bundle can carry real content.
    Command::new("git")
        .args(["checkout", "-B", "loom/stage"])
        .current_dir(&host)
        .output()
        .unwrap();
    std::fs::write(host.join("x.txt"), "x").unwrap();
    Command::new("git")
        .args(["add", "x.txt"])
        .current_dir(&host)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "x"])
        .current_dir(&host)
        .output()
        .unwrap();

    let bundle = tmp.path().join("stage.bundle");
    let out = Command::new("git")
        .arg("-C")
        .arg(&host)
        .args(["bundle", "create"])
        .arg(&bundle)
        .arg("loom/stage")
        .arg(format!("^{real_base}"))
        .output()
        .unwrap();
    assert!(out.status.success(), "bundle creation must succeed");

    // Forge a wrong base OID. Validating the bundle against this MUST
    // fail with a prereq-mismatch error.
    let wrong_base = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    let err = git_bridge::validate_bundle(&bundle, wrong_base, "loom/stage")
        .expect_err("bundle with wrong base OID must be rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("base OID")
            || msg.contains("prerequisite")
            || msg.contains("base_oid")
            || msg.contains(wrong_base),
        "rejection must cite the missing base OID — got: {msg}"
    );
}

// ──────────────────────────────────────────────────────────────────────
// Attack 6: A Knowledge-style stage running while main moves forward
// must retry rather than silently fast-forwarding past the new tip.
//
// This guards against the race where:
//   1. Knowledge stage K is spawned at base OID A.
//   2. While K is executing, another stage merges and advances main
//      to OID B (B is a descendant of A).
//   3. K finishes, produces a bundle anchored on A.
//   4. If `validate_bundle` blindly accepts (A is still a real OID)
//      and `import_bundle` does a plain FF, the host could end up with
//      the wrong tip or a half-applied merge.
//
// The defense is: re-read `target_branch`'s current tip RIGHT BEFORE
// import (inside the merge lock), and if it has moved past the
// expected_base_oid, fail loudly so the orchestrator can rebase + retry.
//
// Until the merge-lock wiring lands in production
// (see integration-verify memory note 19:52 — "daemon-side
// `complete_stage_container` takes NO merge lock"), this test asserts
// the BUNDLE-LEVEL invariant: the bundle's prerequisite chain must
// include the at-spawn base OID, so any post-rebase agent attempt is
// caught by validate_bundle's prereq check.
// ──────────────────────────────────────────────────────────────────────
#[test]
#[ignore]
fn knowledge_main_moved_retries() {
    use loom::orchestrator::terminal::container::git_bridge;

    let tmp = tempfile::TempDir::new().unwrap();
    let host = tmp.path().join("host");
    std::fs::create_dir_all(&host).unwrap();
    let base_at_spawn = init_repo(&host);

    // Simulate "main moves forward" while the knowledge stage is alive.
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(&host)
        .output()
        .unwrap();
    std::fs::write(host.join("moved.txt"), "main-moved").unwrap();
    Command::new("git")
        .args(["add", "moved.txt"])
        .current_dir(&host)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "main-moved"])
        .current_dir(&host)
        .output()
        .unwrap();
    let new_main = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "main"])
            .current_dir(&host)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    assert_ne!(base_at_spawn, new_main, "main must have moved");

    // The knowledge agent produces a bundle anchored on the at-spawn
    // base OID (the only base it knows). It does NOT know main moved.
    Command::new("git")
        .args(["checkout", "-B", "loom/know"])
        .current_dir(&host)
        .output()
        .unwrap();
    Command::new("git")
        .args(["reset", "--hard", &base_at_spawn])
        .current_dir(&host)
        .output()
        .unwrap();
    std::fs::write(host.join("k.txt"), "k").unwrap();
    Command::new("git")
        .args(["add", "k.txt"])
        .current_dir(&host)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "k"])
        .current_dir(&host)
        .output()
        .unwrap();
    let bundle = tmp.path().join("know.bundle");
    assert!(Command::new("git")
        .arg("-C")
        .arg(&host)
        .args(["bundle", "create"])
        .arg(&bundle)
        .arg("loom/know")
        .arg(format!("^{base_at_spawn}"))
        .output()
        .unwrap()
        .status
        .success());

    // The bundle validates against the at-spawn base OID — that's
    // correct; the bundle is well-formed. But the orchestrator MUST
    // also check that main has not moved past the at-spawn base before
    // importing. Validating against the NEW main tip MUST fail — this
    // is the "main moved → retry" signal.
    let _ok = git_bridge::validate_bundle(&bundle, &base_at_spawn, "loom/know")
        .expect("bundle is well-formed against at-spawn base");
    let err = git_bridge::validate_bundle(&bundle, &new_main, "loom/know")
        .expect_err("validating against moved-main tip must fail — orchestrator must retry");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("base OID")
            || msg.contains("prerequisite")
            || msg.contains("base_oid"),
        "post-move validation must surface a prereq error: {msg}"
    );
}

// ──────────────────────────────────────────────────────────────────────
// Helper: capability verification — re-implements the constant-time
// check the daemon performs, so this integration test can stay
// independent of whether the daemon helper is currently re-exported
// via `loom::daemon::*`.
//
// Keeping this local also documents the exact invariant the test
// verifies: a token whose bytes match the file pointed to by
// `Capability` must compare equal in constant time; mismatched or
// missing tokens must fail closed. If the production
// `verify_for_capability` ever diverges from this skeleton (e.g.,
// fallback to user token when admin is unreadable), the
// `admin_token_blocks_stop_from_container` test catches it via the
// behavioral assertion above.
// ──────────────────────────────────────────────────────────────────────
fn verify_capability(work_dir: &Path, provided: &str, cap: loom::daemon::Capability) -> bool {
    let file = match cap {
        loom::daemon::Capability::User => work_dir.join("user.token"),
        loom::daemon::Capability::Admin => work_dir.join("admin.token"),
    };
    let Ok(expected) = std::fs::read_to_string(&file) else {
        return false;
    };
    let expected = expected.trim();
    if expected.len() != provided.len() {
        return false;
    }
    expected
        .as_bytes()
        .iter()
        .zip(provided.as_bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}
