//! Discover locked inputs and existing deny-list paths in every plan home.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{archive_root, fold, symlink_component, LockReason, LockedSet, PLANS};
use crate::plan::inputs::collect_inputs;
use crate::planning::discover::{discover, DocTree};
use crate::planning::kinds::is_feature_name;
use crate::planning::slug::{plan_home, plan_state, PlanState};

impl LockedSet {
    /// Scan the tree under `root` (the repository root). An error, including one from
    /// `collect_inputs`, must make the caller deny: the set would be incomplete.
    pub fn scan(root: &Path) -> Result<LockedSet> {
        let tree = discover(root)?;
        let mut set = LockedSet {
            briefs: BTreeMap::new(),
            bundles: Vec::new(),
            existing: Vec::new(),
        };
        for (slug, plan) in locked_plan_files(&tree) {
            let Some(home) = plan_home(&plan) else {
                continue;
            };
            let bundle = fold(&home.join("briefs").join(&slug));
            if !set.bundles.iter().any(|(prefix, _)| *prefix == bundle) {
                set.bundles.push((bundle, slug.clone()));
            }
            let inputs = collect_inputs(root, &plan).with_context(|| {
                format!("cannot list the inputs of locked plan {}", plan.display())
            })?;
            for file in inputs.files.into_keys().filter(|file| *file != plan) {
                let reason = LockReason::BriefOfLockedPlan { slug: slug.clone() };
                set.briefs.entry(fold(&file)).or_insert(reason);
            }
        }
        set.existing = existing_locked(root, &tree, &set)?;
        Ok(set)
    }
}

/// Include all copies, even cross-home conflicts excluded from a winner's strays.
fn locked_plan_files(tree: &DocTree) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    for entry in tree.plans.values() {
        for path in std::iter::once(&entry.path)
            .chain(&entry.strays)
            .chain(&entry.conflicts)
        {
            if plan_state(path).is_some_and(|state| state > PlanState::Written) {
                files.push((entry.slug.clone(), path.clone()));
            }
        }
    }
    files
}

/// Discovered locked files outside archives, plus each existing archive directory.
fn existing_locked(root: &Path, tree: &DocTree, set: &LockedSet) -> Result<Vec<PathBuf>> {
    let mut existing: Vec<PathBuf> = tree
        .docs
        .iter()
        .map(|doc| &doc.path)
        .filter(|path| archive_root(path).is_none() && set.lock_reason(path).is_some())
        .cloned()
        .collect();
    for home in plan_homes(root)? {
        let archive = home.join("archive");
        if is_real_dir(root, &archive)? {
            existing.push(archive);
        }
    }
    existing.sort();
    Ok(existing)
}

fn plan_homes(root: &Path) -> Result<Vec<PathBuf>> {
    let mut homes = vec![PathBuf::from(PLANS)];
    if !is_real_dir(root, Path::new(PLANS))? {
        return Ok(homes);
    }
    let entries = std::fs::read_dir(root.join(PLANS)).context("cannot list plan homes")?;
    for entry in entries {
        let name = entry?.file_name();
        let Some(name) = name.to_str().filter(|name| is_feature_name(name)) else {
            continue;
        };
        let home = Path::new(PLANS).join(name);
        if is_real_dir(root, &home)? {
            homes.push(home);
        }
    }
    Ok(homes)
}

fn is_real_dir(root: &Path, rel: &Path) -> Result<bool> {
    if symlink_component(root, rel)?.is_some() {
        return Ok(false);
    }
    match std::fs::symlink_metadata(root.join(rel)) {
        Ok(meta) => Ok(meta.is_dir()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("cannot stat {}", rel.display())),
    }
}
