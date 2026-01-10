use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::fs::work_dir::WorkDir;
use crate::models::worktree::WorktreeStatus;

pub fn display_worktrees(work_dir: &WorkDir) -> Result<()> {
    let work_root = work_dir.root().parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Work directory has no parent: {}",
            work_dir.root().display()
        )
    })?;

    let worktrees_dir = work_root.join(".worktrees");
    if !worktrees_dir.exists() {
        return Ok(());
    }

    let mut worktrees = Vec::new();
    for entry in fs::read_dir(&worktrees_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let stage_id = entry.file_name().to_str().unwrap_or("unknown").to_string();
            let status = detect_worktree_status(&path);
            worktrees.push((stage_id, status));
        }
    }

    if worktrees.is_empty() {
        return Ok(());
    }

    println!("\n{}", "Worktrees".bold());

    for (stage_id, status) in worktrees {
        let status_display = format_worktree_status(&status);
        println!("  {stage_id}  {status_display}");
    }

    Ok(())
}

fn detect_worktree_status(worktree_path: &Path) -> WorktreeStatus {
    if has_merge_conflicts(worktree_path) {
        return WorktreeStatus::Conflict;
    }

    let git_path = worktree_path.join(".git");
    let is_merging = if git_path.is_file() {
        if let Ok(content) = fs::read_to_string(&git_path) {
            if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                let gitdir_path = std::path::PathBuf::from(gitdir.trim());
                gitdir_path.join("MERGE_HEAD").exists()
            } else {
                false
            }
        } else {
            false
        }
    } else {
        worktree_path.join(".git").join("MERGE_HEAD").exists()
    };

    if is_merging {
        return WorktreeStatus::Merging;
    }

    WorktreeStatus::Active
}

fn has_merge_conflicts(worktree_path: &Path) -> bool {
    let output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(worktree_path)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            !stdout.trim().is_empty()
        }
        Err(_) => false,
    }
}

pub fn format_worktree_status(status: &WorktreeStatus) -> colored::ColoredString {
    match status {
        WorktreeStatus::Conflict => "[CONFLICT]".red().bold(),
        WorktreeStatus::Merging => "[MERGING]".yellow().bold(),
        WorktreeStatus::Merged => "[MERGED]".green(),
        WorktreeStatus::Creating => "[CREATING]".cyan(),
        WorktreeStatus::Removed => "[REMOVED]".dimmed(),
        WorktreeStatus::Active => "[ACTIVE]".green(),
    }
}
