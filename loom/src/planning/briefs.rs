//! Plan briefs grouped by stage.

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::plan::inputs::collect_inputs;
use crate::planning::discover::DocTree;
use crate::planning::kinds::DocKind;
use crate::planning::slug::{plan_home, PlanEntry};

/// The inputs of a plan, grouped by their place in its bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanBriefs {
    /// Briefs under a directory named for a stage.
    pub by_stage: BTreeMap<String, Vec<PathBuf>>,
    /// The plan-wide common brief.
    pub common: Vec<PathBuf>,
    /// Inputs outside the recognized bundle locations.
    pub other: Vec<PathBuf>,
    /// Invalid or missing input references and their reasons.
    pub broken: Vec<String>,
}

/// Collect and group the same inputs that a plan run uses.
pub fn plan_briefs(root: &Path, entry: &PlanEntry, stage_ids: &[String]) -> Result<PlanBriefs> {
    let inputs = collect_inputs(root, &entry.path)?;
    let home = plan_home(&entry.path).context("plan path has no planning home")?;
    let bundle = home.join("briefs").join(&entry.slug);
    let mut result = PlanBriefs {
        broken: inputs.invalid.iter().map(ToString::to_string).collect(),
        ..PlanBriefs::default()
    };
    for (path, origin) in inputs.files {
        if path == entry.path {
            continue;
        }
        match std::fs::symlink_metadata(root.join(&path)) {
            Ok(_) => group_input(&mut result, &bundle, stage_ids, path),
            Err(error) if error.kind() == ErrorKind::NotFound => result.broken.push(format!(
                "{} (referenced in {}): input does not exist",
                path.display(),
                origin
            )),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(result)
}

fn group_input(result: &mut PlanBriefs, bundle: &Path, stages: &[String], path: PathBuf) {
    if let Ok(relative) = path.strip_prefix(bundle) {
        if relative == Path::new("common.md") {
            result.common.push(path);
            return;
        }
        let mut parts = relative.components();
        if let (Some(stage), Some(_)) = (parts.next(), parts.next()) {
            if let Some(stage) = stage
                .as_os_str()
                .to_str()
                .filter(|s| stages.iter().any(|id| id == s))
            {
                result
                    .by_stage
                    .entry(stage.to_owned())
                    .or_default()
                    .push(path);
                return;
            }
        }
    }
    result.other.push(path);
}

/// Return sorted bundle slugs represented in the tree that have no plan.
pub fn orphan_bundles(tree: &DocTree) -> Vec<String> {
    let mut orphans = BTreeSet::new();
    for doc in &tree.docs {
        if let DocKind::Brief { slug } = &doc.kind {
            if !tree.plans.contains_key(slug) {
                orphans.insert(slug.clone());
            }
        }
    }
    orphans.into_iter().collect()
}
