//! Plan slugs and lifecycle-state resolution.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

// These lifecycle prefixes originate in `fs::plan_lifecycle`.
const IN_PROGRESS_PREFIX: &str = "IN_PROGRESS-";
const DONE_PREFIX: &str = "DONE-";
const RETIRED_PREFIX: &str = "RETIRED-";

/// A plan's lifecycle state, ordered from least to most advanced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanState {
    /// The plan has been written but has not started.
    Written,
    /// The plan is running.
    Running,
    /// The plan has completed or is archived.
    Done,
    /// The plan has been retired.
    Retired,
}

/// Extract a nonempty plan slug after stripping at most one lifecycle prefix.
pub fn plan_slug(file_name: &str) -> Option<String> {
    let name = file_name.strip_suffix(".md")?;
    let name = [IN_PROGRESS_PREFIX, DONE_PREFIX, RETIRED_PREFIX]
        .iter()
        .find_map(|prefix| name.strip_prefix(*prefix))
        .unwrap_or(name);
    let slug = name.strip_prefix("PLAN-")?;
    (!slug.is_empty()).then(|| slug.to_owned())
}

/// Identify a plan's state from its repository-relative path.
pub fn plan_state(rel: &Path) -> Option<PlanState> {
    let parent = rel.parent()?;
    if parent != Path::new("doc/plans") && parent != Path::new("doc/plans/archive") {
        return None;
    }
    let name = rel.file_name()?.to_str()?;
    plan_slug(name)?;
    if name.starts_with(RETIRED_PREFIX) {
        Some(PlanState::Retired)
    } else if parent == Path::new("doc/plans/archive") || name.starts_with(DONE_PREFIX) {
        Some(PlanState::Done)
    } else if name.starts_with(IN_PROGRESS_PREFIX) {
        Some(PlanState::Running)
    } else {
        Some(PlanState::Written)
    }
}

/// The winning plan path and any other copies of its slug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanEntry {
    /// The shared plan identity.
    pub slug: String,
    /// The most advanced copy, with ties resolved by path.
    pub path: PathBuf,
    /// The state of the winning copy.
    pub state: PlanState,
    /// Other copies, sorted by path.
    pub strays: Vec<PathBuf>,
}

/// Resolve plan paths by state, breaking ties by lexicographically smallest path.
pub fn resolve_plans(paths: impl Iterator<Item = PathBuf>) -> BTreeMap<String, PlanEntry> {
    let mut plans: BTreeMap<String, PlanEntry> = BTreeMap::new();
    for path in paths {
        let Some(state) = plan_state(&path) else {
            continue;
        };
        let Some(slug) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(plan_slug)
        else {
            continue;
        };
        match plans.entry(slug.clone()) {
            Entry::Vacant(slot) => {
                slot.insert(PlanEntry {
                    slug,
                    path,
                    state,
                    strays: Vec::new(),
                });
            }
            Entry::Occupied(mut slot) => {
                let entry = slot.get_mut();
                if state > entry.state || (state == entry.state && path < entry.path) {
                    entry.strays.push(std::mem::replace(&mut entry.path, path));
                    entry.state = state;
                } else {
                    entry.strays.push(path);
                }
            }
        }
    }
    for entry in plans.values_mut() {
        entry.strays.sort();
    }
    plans
}
