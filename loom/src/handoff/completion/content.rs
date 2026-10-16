//! Bind checks to committed content. Generated untracked Loom scaffolding is
//! excluded; tracked scaffolding is never exempt. Ignore rules do not grant
//! input exemptions: only named generated output locations are permitted.
use crate::git::worktree::{stage_worktree_path, WorktreeGit};
use crate::models::stage::{Stage, StageType};
use anyhow::{bail, ensure, Context, Result};
use std::path::Path;

pub fn committed_content(stage: &Stage, repo_root: &Path) -> Result<String> {
    let git = if stage.stage_type == StageType::Knowledge {
        WorktreeGit::discovered(repo_root)
    } else {
        let root = stage_worktree_path(stage, repo_root)
            .context("completion cannot name the assigned stage worktree")?;
        ensure!(
            root.is_dir(),
            "assigned stage worktree is missing; recover it before completion"
        );
        WorktreeGit::pinned(repo_root, &root)?
    };
    for cached in [true, false] {
        let mut args = vec!["diff", "--quiet", "--ignore-submodules=none"];
        if cached {
            args.push("--cached");
        }
        args.push("HEAD");
        scope(stage, &mut args);
        let output = git.run(&args)?;
        match output.status.code() {
            Some(0) => {},
            Some(1) => bail!("staged changes are not committed or tracked worktree differs from HEAD: run loom stage commit and wait for it with loom request status <id> --wait 90"),
            _ => bail!("completion content check failed: {}", String::from_utf8_lossy(&output.stderr)),
        }
    }
    reject_untracked(stage, &git)?;
    verify_checkout_branch(stage, &git)?;
    let output = git.run(&["rev-parse", "HEAD"])?;
    ensure!(
        output.status.success(),
        "cannot read completion checkout HEAD"
    );
    let head = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(
        head == super::identity::expected_stage_commit(stage, repo_root)?,
        "assigned checkout HEAD differs from stage branch"
    );
    Ok(head)
}

fn scope(stage: &Stage, args: &mut Vec<&str>) {
    if stage.stage_type == StageType::Knowledge {
        args.extend(["--", crate::git::target_guard::knowledge_prefix()]);
    }
}

fn reject_untracked(stage: &Stage, git: &WorktreeGit) -> Result<()> {
    let mut args = vec!["ls-files", "--others", "-z"];
    scope(stage, &mut args);
    let output = git.run(&args)?;
    ensure!(
        output.status.success(),
        "cannot enumerate untracked completion inputs"
    );
    for path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|p| !p.is_empty())
    {
        let path = std::str::from_utf8(path).context("non-UTF-8 untracked completion input")?;
        if !completion_scaffold(path, git.work_tree())
            || super::content_inputs::declared_input(stage, path)?
        {
            bail!(
                "untracked completion input '{path}': commit required inputs before verification"
            );
        }
    }
    Ok(())
}

fn verify_checkout_branch(stage: &Stage, git: &WorktreeGit) -> Result<()> {
    if stage.stage_type == StageType::Knowledge {
        return Ok(());
    }
    let branch = git.run(&["symbolic-ref", "--short", "HEAD"])?;
    ensure!(
        branch.status.success()
            && String::from_utf8_lossy(&branch.stdout).trim()
                == crate::git::branch::branch_name_for_stage(&stage.id),
        "assigned checkout does not belong to the stage branch"
    );
    Ok(())
}

// Keep this list narrower than cleanup's directory-level scaffold exemptions.
fn completion_scaffold(path: &str, root: &Path) -> bool {
    match path {
        "CLAUDE.md" | ".claude/CLAUDE.md" | ".loom/work" | ".work" => root.join(path).is_symlink(),
        ".claude/settings.json" | ".claude/settings.local.json" => true,
        crate::fs::memory::SPOOL_RELPATH
        | crate::fs::stage_request::SPOOL_RELPATH
        | crate::telemetry::TELEMETRY_SPOOL_RELPATH => true,
        _ => [
            ".loom/cache/",
            "target/",
            "loom/target/",
            "web/node_modules/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix)),
    }
}

/// Snapshot taken before any acceptance command, checked again before evidence
/// emission/landing. A command that commits changes cannot move this identity.
#[derive(Debug)]
pub struct VerificationSnapshot {
    pub commit: String,
    pub policy: String,
}

impl VerificationSnapshot {
    pub fn capture(stage: &Stage, work_dir: &Path) -> Result<Self> {
        let root = crate::fs::work_dir::WorkDir::new(work_dir)?
            .main_project_root()
            .context("completion repository is unavailable")?;
        Ok(Self {
            commit: committed_content(stage, &root)?,
            policy: super::policy::effective_policy_hash(stage, work_dir)?,
        })
    }

    pub fn validate(&self, stage: &Stage, work_dir: &Path) -> Result<()> {
        let fresh = crate::verify::transitions::load_stage(&stage.id, work_dir)?;
        self.validate_locked(&fresh, work_dir)
    }

    /// Use the authoritative stage supplied by `update_stage` without acquiring
    /// its directory lock a second time.
    pub fn validate_locked(&self, stage: &Stage, work_dir: &Path) -> Result<()> {
        let current = Self::capture(stage, work_dir)?;
        ensure!(
            self.commit == current.commit,
            "completion content changed during verification"
        );
        ensure!(
            self.policy == current.policy,
            "completion policy changed during verification"
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;
