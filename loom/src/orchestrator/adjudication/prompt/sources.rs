//! The evidence the briefing quotes, gathered from outside the process: the
//! evidence commit's diff, a shallow listing of the tree, and the disputed
//! stage's own block of the plan file.
//!
//! Every one of these degrades to a message rather than failing: a briefing
//! missing its diff is still a briefing, and the session can go and look.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fs::work_dir::WorkDir;
use crate::git::runner::run_git_pinned;

/// The repository root `work_dir`'s state directory sits in, for Git evidence.
///
/// The hop count from the state root is layout-dependent (two for
/// `.loom/work`, one for a legacy `.work`) and lives in exactly one place:
/// `WorkDir::project_root`. A bare `parent()` here would be right for one
/// layout and wrong for the other, running `git show` from `<repo>/.loom`.
fn project_root_of(work_dir: &Path) -> PathBuf {
    WorkDir::new(work_dir)
        .ok()
        .and_then(|wd| wd.project_root().map(Path::to_path_buf))
        .unwrap_or_else(|| work_dir.to_path_buf())
}

pub(super) fn run_git_show(work_dir: &Path, commit: &str) -> Result<String> {
    // Defence-in-depth: the dispute RPC writes `evidence_commit` straight
    // through from the agent. A value starting with `-` (e.g.
    // `--output=/tmp/x`) would be parsed by `git show` as an option.
    // Require a SHA-shaped string (4–64 hex chars — 4 matches git's
    // minimum unambiguous short SHA). The revision must precede `--`,
    // which separates revisions from paths.
    let is_sha =
        commit.len() >= 4 && commit.len() <= 64 && commit.chars().all(|c| c.is_ascii_hexdigit());
    if !is_sha {
        anyhow::bail!("refusing git show: evidence_commit is not a SHA-shaped string");
    }
    let project_root = project_root_of(work_dir);
    let output = run_git_pinned(
        &["show", "--no-color", "--stat", "-p", commit, "--"],
        None,
        &project_root,
    )
    .context("Failed to invoke git show")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        anyhow::bail!("git show exited non-zero: {stderr}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(super) fn run_listing(root: &Path) -> Result<String> {
    // POSIX find expressions avoid the non-portable `-maxdepth` option.
    let output = Command::new("find")
        .args([
            ".", "(", "-name", ".*", "!", "-name", ".", ")", "-prune", "-o", "-path", "./*/*/*",
            "-print", "-prune", "-o", "-print",
        ])
        .current_dir(root)
        .output();
    match output {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            anyhow::bail!("find exited non-zero: {stderr}")
        }
        Err(e) => Err(anyhow::anyhow!("could not run find: {e}")),
    }
}

pub(super) fn read_plan_excerpt(plan_path: &Path, stage_id: &str) -> Result<String> {
    let raw = std::fs::read_to_string(plan_path).context("read plan")?;
    let yaml = match crate::plan::parser::extract_yaml_metadata(&raw) {
        Ok(yaml) => yaml,
        Err(_) => return Ok(raw),
    };
    let metadata: serde_json::Value =
        crate::yaml::from_str(&yaml).context("parse plan metadata")?;
    let stage = metadata["loom"]["stages"]
        .as_array()
        .and_then(|stages| {
            stages
                .iter()
                .find(|stage| stage["id"].as_str() == Some(stage_id))
        })
        .with_context(|| format!("stage '{stage_id}' missing from plan metadata"))?;
    crate::yaml::to_string(stage).context("serialize stage evidence")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_git_show_rejects_non_sha_evidence_commit() {
        // The agent supplies evidence_commit via the dispute RPC. A value
        // that doesn't look like a SHA must be rejected BEFORE git is
        // invoked, so option-injection (`--output=...`) cannot reach the
        // process. The SHA check also bounds the input to a small ASCII-
        // hex string so even creative byte sequences cannot become
        // arguments after the positional `--`.
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();
        // Leading dash — classic option-injection attempt.
        let err = run_git_show(&work, "--output=/tmp/escape").unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
        // Non-hex characters.
        let err = run_git_show(&work, "deadbeef; rm -rf /").unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
        // Path-traversal shaped.
        let err = run_git_show(&work, "../etc/passwd").unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
        // Empty.
        let err = run_git_show(&work, "").unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
        // Too short (below git's 4-char short-SHA minimum). All-hex but
        // sub-minimum length must be rejected before git is invoked.
        for short in ["a", "ab", "abc"] {
            let err = run_git_show(&work, short).unwrap_err();
            assert!(
                format!("{err:#}").contains("not a SHA-shaped string"),
                "len {} should be rejected; got {err:#}",
                short.len(),
            );
        }
        // Too long (65 hex chars).
        let too_long = "a".repeat(65);
        let err = run_git_show(&work, &too_long).unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
    }

    #[test]
    fn plan_excerpt_narrows_to_the_disputed_stage() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = tmp.path().join("PLAN.md");
        std::fs::write(
            &plan,
            "# Plan\n\n```yaml\nwrong: fence\n```\n<!-- loom METADATA\n```yaml\nloom:\n  stages:\n    - id: s10\n      name: other\n    - id: 's1'\n      name: mine\n```\n<!-- END loom METADATA -->\n",
        )
        .unwrap();
        let excerpt = read_plan_excerpt(&plan, "s1").unwrap();
        assert!(excerpt.contains("id: s1"));
        assert!(!excerpt.contains("s10"));
        assert!(!excerpt.contains("wrong"));
    }

    #[test]
    fn git_show_quotes_requested_revision() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(root.join("evidence"), "first revision\n").unwrap();
        git(&["add", "evidence"]);
        git(&["commit", "-qm", "first"]);
        let first = git(&["rev-parse", "HEAD"]);
        std::fs::write(root.join("evidence"), "second revision\n").unwrap();
        git(&["commit", "-qam", "second"]);
        let work = root.join(".loom/work");
        std::fs::create_dir_all(&work).unwrap();
        let evidence = run_git_show(&work, first.trim()).unwrap();
        assert!(evidence.contains("+first revision"));
        assert!(!evidence.contains("second revision"));
    }

    /// The daemon can inherit `GIT_DIR` from the shell that started it. The
    /// evidence must still come from the state directory's own repository, so
    /// the read runs in a child whose whole environment carries the redirect.
    #[test]
    fn git_show_ignores_an_inherited_repository_redirect() {
        const WORK_ENV: &str = "LOOM_TEST_GIT_SHOW_WORK";
        const COMMIT_ENV: &str = "LOOM_TEST_GIT_SHOW_COMMIT";
        if let Some(work) = std::env::var_os(WORK_ENV) {
            let commit = std::env::var(COMMIT_ENV).unwrap();
            let evidence = run_git_show(Path::new(&work), &commit).unwrap();
            assert!(evidence.contains("+the real evidence"), "{evidence}");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let git = |root: &Path, args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout).unwrap()
        };
        let [root, other] = ["root", "other"].map(|name| tmp.path().join(name));
        for (repo, text) in [(&root, "the real evidence\n"), (&other, "unrelated\n")] {
            std::fs::create_dir_all(repo).unwrap();
            git(repo, &["init", "-q"]);
            git(repo, &["config", "user.email", "test@example.com"]);
            git(repo, &["config", "user.name", "Test"]);
            std::fs::write(repo.join("evidence"), text).unwrap();
            git(repo, &["add", "evidence"]);
            git(repo, &["commit", "-qm", "evidence"]);
        }
        let commit = git(&root, &["rev-parse", "HEAD"]);
        let work = root.join(".loom/work");
        std::fs::create_dir_all(&work).unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "orchestrator::adjudication::prompt::sources::tests::\
                 git_show_ignores_an_inherited_repository_redirect",
                "--nocapture",
            ])
            .env(WORK_ENV, &work)
            .env(COMMIT_ENV, commit.trim())
            .env("GIT_DIR", other.join(".git"))
            .env("GIT_WORK_TREE", &other)
            .status()
            .unwrap();
        assert!(status.success(), "the evidence read followed GIT_DIR");
    }
}
