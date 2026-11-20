//! `loom stage contracts restore`: copy frozen contract files back into the
//! stage worktree.
//!
//! Both ends refuse symlinks at every path component: the frozen copy is read
//! with a bounded no-follow read, and the worktree file is written through a
//! dirfd rooted at the worktree; restoration replaces the target inode.

use anyhow::{bail, Context, Result};
use std::os::fd::AsRawFd;

use crate::fs::safe_fs::{safe_create_dir_all_in_workdir, safe_open_dirfd};
use crate::fs::safe_replace::restore_no_follow;
use crate::verify::contracts::frozen_restore::read_frozen;
use crate::verify::contracts::normalize;
use crate::verify::contracts::store::load_freeze;

use super::ContractSite;

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;

/// `loom stage contracts restore <stage-id> [--contract <id>]`.
pub fn restore(stage_id: String, contract: Option<String>) -> Result<()> {
    let site = ContractSite::load(&stage_id)?;
    restore_at(&site, &stage_id, contract.as_deref())
}

fn restore_at(site: &ContractSite, stage_id: &str, contract: Option<&str>) -> Result<()> {
    let Some(record) = load_freeze(&site.work_dir, stage_id)? else {
        bail!("Stage '{stage_id}' has no frozen contracts to restore");
    };
    let wanted = match contract {
        Some(id) => Some(contract_file(site, id)?),
        None => None,
    };
    let files: Vec<_> = record
        .files
        .iter()
        .filter(|file| wanted.as_ref().is_none_or(|wanted| *wanted == file.path))
        .collect();
    if files.is_empty() {
        bail!("The freeze record of stage '{stage_id}' holds no file to restore");
    }

    let prefix = site.working_dir_prefix();
    let root = safe_open_dirfd(&site.worktree_root)?;
    for file in files {
        let content = read_frozen(&site.work_dir, &record, &file.path, &file.sha256)?;
        let target = prefix.join(&file.path);
        if let Some(parent) = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            safe_create_dir_all_in_workdir(root.as_raw_fd(), parent, 0o755)
                .with_context(|| format!("Refusing to restore {}", target.display()))?;
        }
        restore_no_follow(&site.worktree_root, &target, &content, 0o644)
            .with_context(|| format!("Refusing to restore {}", target.display()))?;
        println!("Restored {}", target.display());
    }
    Ok(())
}

/// The test file of contract `id`, in the form the freeze record names it.
fn contract_file(site: &ContractSite, id: &str) -> Result<String> {
    match site
        .stage
        .contracts
        .iter()
        .find(|contract| contract.id == id)
    {
        Some(contract) => Ok(normalize(&contract.file)),
        None => bail!("Stage '{}' has no contract `{id}`", site.stage.id),
    }
}
