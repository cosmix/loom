//! Post-completion commit: keep the default branch clean after a plan is
//! marked done.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::path::Path;

use crate::fs::work_dir::WorkDir;
use crate::git::branch::{branch_ref, current_branch};
use crate::git::merge::control_paths::changed_paths;
use crate::git::runner::{run_git_checked, run_git_with_env_within};
use crate::git::signing::{self, SigningEnv};
use crate::git::target_guard;

/// Commit tracked changes to keep the default branch clean after plan completion.
///
/// After all stages are merged and the plan is renamed to DONE, this commits:
/// - The plan file rename (IN_PROGRESS → DONE)
/// - Any other tracked modifications (e.g., knowledge files updated by integration-verify)
pub(super) fn commit_post_completion_changes(
    work_dir: &WorkDir,
    old_plan_path: &Path,
    new_plan_path: &Path,
) -> Result<()> {
    let repo_root = work_dir
        .project_root()
        .context("Failed to determine project root")?;

    // Stage the plan file rename: add the new DONE file and stage deletion of old
    run_git_checked(&["add", &new_plan_path.display().to_string()], repo_root)?;
    run_git_checked(&["add", &old_plan_path.display().to_string()], repo_root)?;

    // Stage any other tracked modifications (modified + deleted tracked files).
    // Does NOT add untracked files — safe for automated use.
    // Typically catches knowledge files updated by integration-verify.
    run_git_checked(&["add", "-u"], repo_root)?;

    // Check if there's anything staged to commit
    let staged = run_git_checked(&["diff", "--cached", "--name-only"], repo_root)?;
    if staged.is_empty() {
        return Ok(());
    }

    // Commit with a descriptive message
    let plan_name = new_plan_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plan");
    commit_signed(
        repo_root,
        &format!("chore(loom): mark plan complete — {plan_name}"),
        signing::installed(),
    )?;

    attest_completion_commit(work_dir, repo_root, old_plan_path, new_plan_path);

    println!(
        "  {} Committed post-completion changes to default branch",
        "✓".green().bold(),
    );

    Ok(())
}

/// Commit what is staged in `repo_root` with `message`. The daemon's process
/// environment no longer holds `GNUPGHOME` and `SSH_AUTH_SOCK`, so the captured
/// signing environment `env` is handed to this one command, bounded by
/// [`signing::SIGN_TIMEOUT`]; the runner keeps repository hooks off.
fn commit_signed(repo_root: &Path, message: &str, env: &SigningEnv) -> Result<()> {
    let output = run_git_with_env_within(
        repo_root,
        &["commit", "-m", message],
        &env.env_pairs(),
        signing::SIGN_TIMEOUT,
    )?;
    if output.status.success() {
        return Ok(());
    }
    let code = output
        .status
        .code()
        .map_or_else(|| "a signal".to_string(), |code| code.to_string());
    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("git commit failed (exit {code}): {}", stderr.trim())
}

/// Attest the post-completion commit when it touches only the plan's own
/// files: both plan paths and the plan's review document. A failure is a
/// warning; the commit then stays unattested and the guard holds it for the
/// operator.
fn attest_completion_commit(
    work_dir: &WorkDir,
    repo_root: &Path,
    old_plan_path: &Path,
    new_plan_path: &Path,
) {
    let plan_id = work_dir
        .load_config()
        .ok()
        .flatten()
        .and_then(|config| config.plan_id().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string());
    let review = Path::new("doc/plans").join(format!("REVIEW-{plan_id}.md"));
    let allowed = [old_plan_path, new_plan_path, review.as_path()];
    if let Err(error) = attest_plan_commit(work_dir.root(), repo_root, &allowed) {
        tracing::warn!("could not attest the post-completion commit: {error:#}");
    }
}

/// Attest loom's own commit at `HEAD` of the main checkout in the target
/// guard's ledger, because loom's git runs without hooks and the
/// `reference-transaction` hook never sees it.
///
/// Attested only when `HEAD`'s branch is a guarded target and every path the
/// commit changes is one of `allowed` or under the knowledge prefix. Any other
/// path, `doc/plans/` siblings included, leaves the commit unattested: the
/// commit may carry a session's edit, and the guard then holds it for the
/// operator. `allowed` paths are relative to `repo_root` or under it.
pub(crate) fn attest_plan_commit(
    work_dir: &Path,
    repo_root: &Path,
    allowed: &[&Path],
) -> Result<()> {
    // `HEAD` is read after the commit: a read before it can be stale.
    let after = run_git_checked(&["rev-parse", "HEAD"], repo_root)?;
    let from = run_git_checked(&["rev-parse", &format!("{after}^")], repo_root)?;
    let reference = branch_ref(&current_branch(repo_root)?);
    if !target_guard::guarded_refs(work_dir)?.contains(&reference) {
        return Ok(());
    }
    let paths = changed_paths(repo_root, &from, &after)?;
    if !paths.iter().all(|path| is_own(path, repo_root, allowed)) {
        tracing::info!("not attesting {after}: it changes more than the plan's own files");
        return Ok(());
    }
    target_guard::append_attestation(work_dir, &reference, &from, &after)
}

/// Whether `path` (as git reports it) is one of `allowed` or under the
/// knowledge prefix.
fn is_own(path: &str, repo_root: &Path, allowed: &[&Path]) -> bool {
    path.starts_with(target_guard::knowledge_prefix())
        || allowed
            .iter()
            .copied()
            .any(|own| own.strip_prefix(repo_root).unwrap_or(own) == Path::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::signing::tests::fake_signer;
    use crate::git::target_guard::test_support::{activate, commit_file, git, repo, Repo};
    use crate::git::target_guard::{check, GuardState, LEDGER_FILE};

    const OLD: &str = "doc/plans/IN_PROGRESS-PLAN-x.md";
    const NEW: &str = "doc/plans/DONE-PLAN-x.md";
    const OTHER_PLAN: &str = "doc/plans/PLAN-other.md";
    const REVIEW: &str = "doc/plans/REVIEW-x.md";
    const OTHER_REVIEW: &str = "doc/plans/REVIEW-other.md";

    /// An attesting repository with the plan, another plan, both plans'
    /// review documents, a knowledge file and a source file committed, and
    /// `main` recorded.
    fn plan_repo() -> Repo {
        let repo = repo();
        activate(&repo.root);
        let paths = [OLD, OTHER_PLAN, REVIEW, OTHER_REVIEW];
        for path in paths
            .into_iter()
            .chain(["doc/loom/knowledge/a.md", "src/x.rs"])
        {
            commit_file(&repo.root, path, "v1\n");
        }
        let first = check(&repo.root, &repo.work, "main").unwrap();
        assert!(matches!(first, Some(GuardState::Clear { .. })));
        repo
    }

    /// Rename the plan, edit a knowledge file and run the post-completion
    /// commit, as `rename_completed_plan` does.
    fn complete(repo: &Repo) {
        std::fs::rename(repo.root.join(OLD), repo.root.join(NEW)).unwrap();
        std::fs::write(repo.root.join("doc/loom/knowledge/a.md"), "v2\n").unwrap();
        let work_dir = WorkDir::new(&repo.root).unwrap();
        let (old, new) = (repo.root.join(OLD), repo.root.join(NEW));
        commit_post_completion_changes(&work_dir, &old, &new).unwrap();
    }

    /// Name `x` as the active plan in the state directory's `config.toml`.
    fn configure_plan_x(repo: &Repo) {
        std::fs::write(repo.work.join("config.toml"), "[plan]\nplan_id = \"x\"\n").unwrap();
    }

    fn ledger(repo: &Repo) -> String {
        std::fs::read_to_string(repo.work.join(LEDGER_FILE)).unwrap_or_default()
    }

    fn guard(repo: &Repo) -> GuardState {
        check(&repo.root, &repo.work, "main").unwrap().unwrap()
    }

    #[test]
    fn the_plan_done_commit_signs_with_the_signing_environment() {
        let repo = plan_repo();
        let argv_log = fake_signer(&repo.root, false);

        complete(&repo);

        let commit = git(&repo.root, &["cat-file", "commit", "HEAD"]);
        assert!(
            commit.contains("gpgsig"),
            "the DONE commit is signed: {commit}"
        );
        let calls = std::fs::read_to_string(argv_log).unwrap();
        assert_eq!(calls.lines().count(), 1);
    }

    #[test]
    fn the_plan_done_commit_hands_the_given_signing_environment_to_the_signer() {
        let repo = plan_repo();
        let log = fake_signer(&repo.root, false);
        std::fs::write(repo.root.join("src/x.rs"), "v2\n").unwrap();
        git(&repo.root, &["add", "src/x.rs"]);
        let env = SigningEnv {
            gnupghome: Some("/loom-test/gnupghome".into()),
            ssh_auth_sock: None,
        };

        commit_signed(&repo.root, "chore: signed", &env).unwrap();

        let calls = std::fs::read_to_string(log).unwrap();
        assert_eq!(calls.lines().count(), 1, "{calls}");
        assert!(
            calls.trim_end().ends_with("GNUPGHOME=/loom-test/gnupghome"),
            "the signer did not see the given GNUPGHOME: {calls}"
        );
    }

    #[test]
    fn a_failing_signer_fails_the_plan_done_commit() {
        let repo = plan_repo();
        fake_signer(&repo.root, true);
        let head = git(&repo.root, &["rev-parse", "HEAD"]);
        std::fs::rename(repo.root.join(OLD), repo.root.join(NEW)).unwrap();
        let work_dir = WorkDir::new(&repo.root).unwrap();
        let (old, new) = (repo.root.join(OLD), repo.root.join(NEW));

        let error = commit_post_completion_changes(&work_dir, &old, &new).unwrap_err();

        assert!(format!("{error:#}").contains("git commit"), "{error:#}");
        assert_eq!(git(&repo.root, &["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn a_commit_of_only_the_plan_files_and_knowledge_is_attested() {
        let repo = plan_repo();

        complete(&repo);

        let after = git(&repo.root, &["rev-parse", "HEAD"]);
        let from = git(&repo.root, &["rev-parse", "HEAD^"]);
        assert!(ledger(&repo).contains(&format!("attest {from} {after} refs/heads/main\n")));
        assert!(matches!(guard(&repo), GuardState::Clear { .. }));
    }

    #[test]
    fn a_commit_that_also_carries_source_is_not_attested() {
        let repo = plan_repo();
        std::fs::write(repo.root.join("src/x.rs"), "v2\n").unwrap();

        complete(&repo);

        assert_eq!(ledger(&repo), "");
        assert!(matches!(guard(&repo), GuardState::Held(_)));
    }

    #[test]
    fn a_commit_that_also_carries_another_plan_is_not_attested() {
        let repo = plan_repo();
        std::fs::write(repo.root.join(OTHER_PLAN), "sandbox: changed\n").unwrap();

        complete(&repo);

        assert_eq!(ledger(&repo), "");
        assert!(matches!(guard(&repo), GuardState::Held(_)));
    }

    #[test]
    fn a_commit_on_an_unguarded_branch_is_not_attested() {
        let repo = plan_repo();
        git(&repo.root, &["checkout", "-q", "-b", "scratch"]);

        complete(&repo);

        assert_eq!(ledger(&repo), "");
    }

    #[test]
    fn a_commit_of_the_plans_own_review_document_is_attested() {
        let repo = plan_repo();
        configure_plan_x(&repo);
        std::fs::write(repo.root.join(REVIEW), "v2\n").unwrap();

        complete(&repo);

        let after = git(&repo.root, &["rev-parse", "HEAD"]);
        let from = git(&repo.root, &["rev-parse", "HEAD^"]);
        assert!(ledger(&repo).contains(&format!("attest {from} {after} refs/heads/main\n")));
        assert!(matches!(guard(&repo), GuardState::Clear { .. }));
    }

    #[test]
    fn a_commit_of_another_plans_review_document_is_not_attested() {
        let repo = plan_repo();
        configure_plan_x(&repo);
        std::fs::write(repo.root.join(OTHER_REVIEW), "sandbox: changed\n").unwrap();

        complete(&repo);

        assert_eq!(ledger(&repo), "");
        assert!(matches!(guard(&repo), GuardState::Held(_)));
    }
}
