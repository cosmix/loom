//! Build script: derives `LOOM_VERSION`/`LOOM_COMMIT`/`LOOM_BUILD_DATE`/
//! `LOOM_TARGET` from git state and embeds them as compile-time env vars.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/version/derive.rs"
));

fn main() {
    let describe_exact = run_git(&["describe", "--tags", "--exact-match"]);
    let describe = run_git(&["describe", "--tags"]);
    let short_sha = run_git(&["rev-parse", "--short", "HEAD"]);

    let version = derive_version(
        describe_exact.as_deref(),
        describe.as_deref(),
        short_sha.as_deref(),
    );
    let commit = short_sha.unwrap_or_else(|| "unknown".to_string());
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=LOOM_VERSION={version}");
    println!("cargo:rustc-env=LOOM_COMMIT={commit}");
    println!("cargo:rustc-env=LOOM_BUILD_DATE={}", build_date());
    println!("cargo:rustc-env=LOOM_TARGET={target}");

    emit_rerun_keys();
}

/// Run a `git` subcommand, returning `None` on spawn failure, non-zero exit,
/// non-UTF8 output, or empty trimmed output.
fn run_git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Emit `cargo:rerun-if-changed` for real, existing paths only. A path that
/// does not exist is permanently dirty to cargo, forcing a rebuild every
/// time, so nothing is emitted for one.
///
/// The watched paths are resolved with `git rev-parse --git-path HEAD` (the
/// per-worktree HEAD) and `git rev-parse --git-common-dir` (from which
/// `refs/tags` and `packed-refs` are derived), rather than guessed directly,
/// because this build script's CWD is the package root (`loom/`), and in a
/// linked git worktree `.git` is a file pointing elsewhere, not a directory —
/// so neither path can be assumed literally.
///
/// On a normal branch checkout `HEAD` is a symref (`ref: refs/heads/<branch>`)
/// and its own mtime does not change when a commit lands on that branch — the
/// file that actually moves is the branch ref `HEAD` points at. Watching only
/// `HEAD` would leave `LOOM_COMMIT` stale after every commit on a branch, so
/// the resolved ref file is watched too. `symbolic-ref -q HEAD` fails on a
/// detached HEAD, where `HEAD` itself holds the sha and is already watched
/// above, so that case needs no extra path. A branch whose ref is packed
/// rather than loose falls back to `packed-refs`, which is already watched.
///
/// `logs/HEAD` (the reflog) is watched too as a belt-and-braces trigger: it
/// gains an entry on every commit, checkout, and reset regardless of which
/// path above resolved it, so it catches any HEAD movement the specific
/// cases above did not anticipate.
fn emit_rerun_keys() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/version/derive.rs");

    if let Some(head) = run_git(&["rev-parse", "--git-path", "HEAD"]) {
        emit_if_exists(Path::new(&head));
    }

    if let Some(symbolic) = run_git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(ref_path) = run_git(&["rev-parse", "--git-path", &symbolic]) {
            emit_if_exists(Path::new(&ref_path));
        }
    }

    if let Some(reflog) = run_git(&["rev-parse", "--git-path", "logs/HEAD"]) {
        emit_if_exists(Path::new(&reflog));
    }

    if let Some(common_dir) = run_git(&["rev-parse", "--git-common-dir"]) {
        let common_dir = PathBuf::from(common_dir);
        emit_if_exists(&common_dir.join("refs").join("tags"));
        emit_if_exists(&common_dir.join("packed-refs"));
    }
}

fn emit_if_exists(path: &Path) {
    if let Ok(canonical) = path.canonicalize() {
        println!("cargo:rerun-if-changed={}", canonical.display());
    }
}

/// Compute today's UTC date (`YYYY-MM-DD`) with `std` only. The days-from-civil
/// math lives in `civil_date_from_days` (`src/version/derive.rs`) so it can be
/// unit tested; this just reads the clock.
fn build_date() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = (now.as_secs() / 86400) as i64;

    civil_date_from_days(days)
}
