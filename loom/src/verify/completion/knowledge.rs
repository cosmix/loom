//! Validate persisted knowledge invariants when a stage changes the tree.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::fs::knowledge::{catalog, index, INDEX_FILENAME};
use crate::verify::impact_tests::base::BaseCheckout;
use crate::verify::review::fingerprint::{changes_since_merge_base, checkout_git};

const KNOWLEDGE: &str = "doc/loom/knowledge";

pub(super) fn check(worktree: &Path, target: &str) -> Result<()> {
    let (base, paths) = changes_since_merge_base(&checkout_git(worktree)?, target)?;
    if !paths
        .iter()
        .any(|p| p == KNOWLEDGE || p.starts_with(&format!("{KNOWLEDGE}/")))
    {
        return Ok(());
    }
    let checkout = BaseCheckout::create(worktree, &base)
        .context("checking knowledge against the stage's merge base")?;
    validate(&checkout.root().join(KNOWLEDGE), &worktree.join(KNOWLEDGE))
}

/// Verification compares the stage against its session's starting commit, so
/// without a session the operator picks how to go on.
pub(crate) fn no_session_recovery(stage_id: &str) -> String {
    format!(
        "knowledge stage '{stage_id}' has no session, so its completion cannot be verified. \
         Recover with one of: `loom stage retry {stage_id}` for a Blocked stage, which keeps \
         verification; `loom stage complete {stage_id} --no-verify` from a status that may go \
         to Completed; `loom stage complete {stage_id} --force-unsafe` from any status"
    )
}

/// Checkout-rooted knowledge sessions retain their initial commit across retries.
pub(crate) fn check_session(repo: &Path, work_dir: &Path, session: &str) -> Result<()> {
    let base = super::knowledge_baseline::read(work_dir, session)
        .context("knowledge session has no trusted starting commit")?;
    let git = checkout_git(repo)?;
    git.run_checked(&["merge-base", "--is-ancestor", &base, "HEAD"])
        .context("knowledge starting commit is no longer an ancestor of HEAD")?;
    let changed = git.run_checked(&["diff", "--name-only", "-z", &base, "--", KNOWLEDGE])?;
    let untracked = git.run_checked(&[
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
        KNOWLEDGE,
    ])?;
    if changed.is_empty() && untracked.is_empty() {
        return Ok(());
    }
    check_against(repo, &base)
}

pub(super) fn check_against(repo: &Path, base: &str) -> Result<()> {
    let checkout = BaseCheckout::create(repo, base)?;
    validate(&checkout.root().join(KNOWLEDGE), &repo.join(KNOWLEDGE))
}

fn validate(base: &Path, current: &Path) -> Result<()> {
    let base_issues = if base.exists() {
        catalog::structural_issues(base)?
    } else {
        Vec::new()
    };
    let baseline = catalog::CheckBaseline::parse(&catalog::render_baseline(&base_issues)).map_err(
        |errors| {
            anyhow::anyhow!(
                "invalid generated knowledge baseline: {}",
                errors.join(", ")
            )
        },
    )?;
    let current_issues = if current.exists() {
        catalog::structural_issues(current)?
    } else {
        Vec::new()
    };
    let new = baseline.compare(&current_issues).new;
    if !new.is_empty() {
        bail!("knowledge check found {} new structural issue(s); run loom knowledge check for details", new.len());
    }
    for issue in &current_issues {
        if let catalog::CatalogIssue::DuplicateHeading {
            file,
            heading,
            occurrences,
        } = issue
        {
            if base_issues.iter().any(|previous| matches!(previous,
                catalog::CatalogIssue::DuplicateHeading { file: old_file, heading: old_heading, occurrences: old_count }
                    if old_file == file && old_heading == heading && occurrences > old_count
            )) {
                bail!("knowledge check found an increased duplicate heading; run loom knowledge check for details");
            }
        }
    }
    let has_index = current.join(INDEX_FILENAME).exists() || base.join(INDEX_FILENAME).exists();
    if has_index && index::index_is_stale(current)? {
        bail!("knowledge INDEX.md is stale; run loom knowledge sync");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::knowledge_baseline as baseline;
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn seed(root: &Path, text: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("patterns.md"), text).unwrap();
        sync(root);
    }

    fn sync(root: &Path) {
        fs::write(
            root.join(INDEX_FILENAME),
            index::generate_index(root).unwrap(),
        )
        .unwrap();
    }

    fn git(root: &Path, args: &[&str]) {
        let result = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn validates_index_and_new_issues_against_base() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("base");
        let current = temp.path().join("current");
        let original = "# Patterns\n\n> Patterns.\n\n## One\n\nText.\n";
        seed(&base, original);
        seed(&current, original);
        assert!(validate(&base, &current).is_ok());
        fs::write(current.join("patterns.md"), format!("{original}\nMore.\n")).unwrap();
        assert!(validate(&base, &current)
            .unwrap_err()
            .to_string()
            .contains("stale"));
        sync(&current);
        assert!(validate(&base, &current).is_ok());
        fs::write(
            current.join("patterns.md"),
            format!("{original}\n## One\n\nAgain.\n"),
        )
        .unwrap();
        sync(&current);
        assert!(validate(&base, &current)
            .unwrap_err()
            .to_string()
            .contains("structural"));
        fs::write(
            base.join("patterns.md"),
            format!("{original}\n## One\n\nAgain.\n"),
        )
        .unwrap();
        assert!(validate(&base, &current).is_ok());
        fs::remove_file(current.join(INDEX_FILENAME)).unwrap();
        assert!(validate(&base, &current).is_err());
    }

    const ORIGINAL: &str = "# Patterns\n\n> Patterns.\n\n## One\n\nText.\n";

    /// A committed knowledge tree, returning the repo, work dir and knowledge root.
    fn seeded_repo(temp: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let repo = temp.join("repo");
        let work = temp.join("work");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&work).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "test"]);
        let root = repo.join(KNOWLEDGE);
        seed(&root, ORIGINAL);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "base"]);
        (repo, work, root)
    }

    #[test]
    fn session_baseline_survives_commits_and_retry() {
        let temp = tempfile::tempdir().unwrap();
        let (repo, work, root) = seeded_repo(temp.path());
        baseline::capture(&work, &repo, "first", None).unwrap();
        fs::write(root.join("patterns.md"), format!("{ORIGINAL}\nMore.\n")).unwrap();
        git(&repo, &["commit", "-qam", "stale"]);
        assert!(check_session(&repo, &work, "first")
            .unwrap_err()
            .to_string()
            .contains("stale"));
        baseline::capture(&work, &repo, "retry", Some("first")).unwrap();
        assert!(check_session(&repo, &work, "retry").is_err());
        sync(&root);
        assert!(check_session(&repo, &work, "retry").is_ok());
        fs::write(
            root.join("patterns.md"),
            format!("{ORIGINAL}\n## One\n\nAgain.\n"),
        )
        .unwrap();
        sync(&root);
        assert!(check_session(&repo, &work, "retry")
            .unwrap_err()
            .to_string()
            .contains("structural"));
        fs::write(root.join("patterns.md"), ORIGINAL).unwrap();
        fs::remove_file(root.join(INDEX_FILENAME)).unwrap();
        assert!(check_session(&repo, &work, "retry")
            .unwrap_err()
            .to_string()
            .contains("INDEX.md"));
    }

    #[test]
    fn a_missing_session_baseline_recovers_only_from_a_trusted_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let (repo, work, root) = seeded_repo(temp.path());
        baseline::capture(&work, &repo, "first", None).unwrap();
        fs::remove_file(root.join(INDEX_FILENAME)).unwrap();
        assert!(check_session(&repo, &work, "missing").is_err());
        let trusted = baseline::read(&work, "first").unwrap();
        assert!(
            baseline::recover(&work, &repo, "missing", &trusted).is_err(),
            "recovery must reject the missing index"
        );
        sync(&root);
        baseline::recover(&work, &repo, "missing", &trusted).unwrap();
        assert!(check_session(&repo, &work, "missing").is_ok());
        assert!(baseline::recover(&work, &repo, "missing", &trusted).is_err());
        assert!(baseline::recover(&work, &repo, "other", "HEAD").is_err());
        let orphan = std::process::Command::new("git")
            .args(["commit-tree", "HEAD^{tree}", "-m", "unrelated"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(orphan.status.success());
        let orphan = String::from_utf8(orphan.stdout).unwrap();
        assert!(baseline::recover(&work, &repo, "other", orphan.trim()).is_err());
    }
}
