//! Plan slugs, lifecycle states and plan homes.
//!
//! A home is the standalone `doc/plans` shelf or a feature's `doc/plans/<feature>`
//! directory, including its optional `archive/`. Copies within one home are strays;
//! duplicate slugs across homes are conflicts that callers surface as a problem,
//! never a silent winner.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fs::plan_lifecycle::{DONE_PREFIX, IN_PROGRESS_PREFIX, RETIRED_PREFIX};
use crate::planning::kinds::is_feature_name;

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
///
/// Plans live directly in the standalone or feature home, or its `archive/`.
/// Retired names take precedence; archive membership otherwise means done.
pub fn plan_state(rel: &Path) -> Option<PlanState> {
    parent_home(rel)?;
    let parent = rel.parent()?;
    let name = rel.file_name()?.to_str()?;
    plan_slug(name)?;
    if name.starts_with(RETIRED_PREFIX) {
        Some(PlanState::Retired)
    } else if parent.file_name().is_some_and(|name| name == "archive")
        || name.starts_with(DONE_PREFIX)
    {
        Some(PlanState::Done)
    } else if name.starts_with(IN_PROGRESS_PREFIX) {
        Some(PlanState::Running)
    } else {
        Some(PlanState::Written)
    }
}

/// Return a plan's feature, or `None` for standalone plans and non-plan paths.
pub fn plan_feature(rel: &Path) -> Option<String> {
    let home = plan_home(rel)?;
    if home == Path::new("doc/plans") {
        return None;
    }
    home.file_name()?.to_str().map(str::to_owned)
}

/// Return a plan's standalone or feature home with any trailing `archive` removed.
///
/// Non-plan paths return `None`, matching `plan_state`.
pub fn plan_home(rel: &Path) -> Option<PathBuf> {
    plan_state(rel)?;
    parent_home(rel).map(Path::to_path_buf)
}

fn parent_home(rel: &Path) -> Option<&Path> {
    let parent = rel.parent()?;
    let home = if parent.file_name().is_some_and(|name| name == "archive") {
        parent.parent()?
    } else {
        parent
    };
    if home == Path::new("doc/plans")
        || (home.parent() == Some(Path::new("doc/plans"))
            && home.file_name()?.to_str().is_some_and(is_feature_name))
    {
        Some(home)
    } else {
        None
    }
}

/// The winning plan path, same-home strays and cross-home slug conflicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanEntry {
    /// The shared plan identity.
    pub slug: String,
    /// The most advanced copy, with ties resolved by path.
    pub path: PathBuf,
    /// The state of the winning copy.
    pub state: PlanState,
    /// The feature of the winning copy, or `None` for a standalone plan.
    pub feature: Option<String>,
    /// Other copies in the winning plan's home, sorted by path.
    pub strays: Vec<PathBuf>,
    /// Copies in different homes, sorted by path; callers report these as problems.
    pub conflicts: Vec<PathBuf>,
}

/// Resolve plan paths by state, breaking ties by lexicographically smallest path.
///
/// Other copies are split by the final winner's home into strays and conflicts.
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
                    feature: None,
                    strays: Vec::new(),
                    conflicts: Vec::new(),
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
        classify_copies(entry);
    }
    plans
}

fn classify_copies(entry: &mut PlanEntry) {
    let home = plan_home(&entry.path);
    entry.feature = plan_feature(&entry.path);
    (entry.strays, entry.conflicts) = std::mem::take(&mut entry.strays)
        .into_iter()
        .partition(|path| plan_home(path) == home);
    entry.strays.sort();
    entry.conflicts.sort();
}
