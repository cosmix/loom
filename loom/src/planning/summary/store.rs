//! Store layout, summary ids, overview scope and link helpers.
//!
//! Public API (paths are repository-relative):
//! - `STORE_DIR: &str` is `doc/loom/summaries`.
//! - `validate_id(id: &str) -> anyhow::Result<()>` accepts only
//!   `overview/<feature>/{product,engineering}`, `<feature>/product/section_<2+ digits>`,
//!   `<feature>/product/vision`, `<feature>/metaplan/index` and `plans/<slug>`. Features use
//!   `kinds::is_feature_name`; slugs start with an ASCII alphanumeric, continue with
//!   `[A-Za-z0-9._-]` and cannot contain `..`.
//! - `is_overview(id: &str) -> bool` recognizes valid feature overview ids.
//! - `summary_path(id: &str) -> PathBuf` returns `<STORE_DIR>/<id>.md`, empty if invalid.
//! - `summary_id_for(project: &Project, rel: &Path) -> Option<String>` identifies an
//!   eligible section or winning plan; `source_of(project: &Project, id: &str) ->
//!   Option<PathBuf>` finds that document's current path.
//!   Sections and metaplans need a feature (a metaplan summary is a document summary of the
//!   feature's only metaplan, `index.md`); plans must sit directly in their home
//!   (`doc/plans` or `doc/plans/<feature>`) or be named by a metaplan of their own feature.
//!   Losing copies and top-level sections are excluded.
//! - `expected_ids(project: &Project) -> Vec<String>` includes eligible documents and
//!   each feature's overviews with existing vision/metaplan sources, including empty ones.
//! - `scope(project: &Project, feature: &str) -> Scope` computes one feature's ordered
//!   `product` and `engineering` members and sorted `outside` documents. The vision
//!   summary (`<feature>/product/vision`) and the metaplan summary (one per feature) are in none of the three
//!   lists. `Scope.feature` is the requested name; unknown features have empty lists. Other features and standalone
//!   plans are excluded. Standalone plans remain in `expected_ids`.
//! - `Scope::of(project: &Project, overview_id: &str) -> Option<Scope>` selects that
//!   feature's scope; invalid overview ids and unknown features yield `None`.
//! - `link_from_summary(summary_id: &str, target_rel: &Path) -> String` builds a relative
//!   link, empty if invalid. `resolve_link(summary_id: &str, href: &str) ->
//!   Option<(PathBuf, Option<String>)>` resolves a link and optional fragment lexically,
//!   rejecting paths outside the repository.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Result};

use crate::planning::briefs::bundle_files;
use crate::planning::kinds::{is_feature_name, plans_dir, vision_path, DocKind};
use crate::planning::project::{Feature, Project};
use crate::planning::summary::format::PlanCaps;

/// Repository-relative directory containing summaries.
pub const STORE_DIR: &str = "doc/loom/summaries";

/// Whether `id` names a valid feature overview rather than a document summary.
pub fn is_overview(id: &str) -> bool {
    overview_parts(id).is_some()
}

fn overview_parts(id: &str) -> Option<(&str, &str)> {
    let (feature, kind) = id.strip_prefix("overview/")?.split_once('/')?;
    (is_feature_name(feature) && matches!(kind, "product" | "engineering"))
        .then_some((feature, kind))
}

/// The feature of a vision summary id `<feature>/product/vision`, else `None`.
pub fn vision_feature(id: &str) -> Option<&str> {
    id.strip_suffix("/product/vision")
        .filter(|feature| is_feature_name(feature))
}

/// Whether `id` is a feature's vision summary, `<feature>/product/vision`.
pub fn is_vision(id: &str) -> bool {
    vision_feature(id).is_some()
}

/// The feature of a metaplan summary id `<feature>/metaplan/index`, else `None`.
/// A feature has one metaplan, `index.md`, so `index` is the only accepted stem.
pub fn metaplan_feature(id: &str) -> Option<&str> {
    id.strip_suffix("/metaplan/index")
        .filter(|feature| is_feature_name(feature))
}

/// Whether `id` is a feature metaplan's document summary, `<feature>/metaplan/index`.
pub fn is_metaplan(id: &str) -> bool {
    metaplan_feature(id).is_some()
}

/// Whether `id` is a plan's document summary, `plans/<slug>`. Assumes a validated id.
pub fn is_plan(id: &str) -> bool {
    id.starts_with("plans/")
}

/// Validate a summary id before using it as a path.
pub fn validate_id(id: &str) -> Result<()> {
    let valid = if is_overview(id) || is_metaplan(id) {
        true
    } else if let Some(slug) = id.strip_prefix("plans/") {
        !slug.contains("..")
            && slug
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphanumeric())
            && slug
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    } else if let Some((feature, section)) = id.split_once('/') {
        is_feature_name(feature)
            && (section == "product/vision"
                || section
                    .strip_prefix("product/section_")
                    .is_some_and(|digits| {
                        digits.len() >= 2 && digits.bytes().all(|b| b.is_ascii_digit())
                    }))
    } else {
        false
    };
    if !valid {
        bail!("invalid summary id: {id}");
    }
    Ok(())
}

/// Return the repository-relative summary path for a valid id.
/// Invalid ids yield an empty path; callers should validate and report the error first.
pub fn summary_path(id: &str) -> PathBuf {
    if validate_id(id).is_err() {
        return PathBuf::new();
    }
    Path::new(STORE_DIR).join(format!("{id}.md"))
}

fn named_plan(project: &Project, feature: Option<&str>, slug: &str) -> bool {
    feature
        .and_then(|name| project.feature(name))
        .is_some_and(|feature| {
            feature.metaplans.iter().any(|(_, entries)| {
                entries
                    .iter()
                    .any(|entry| entry.slug.as_deref() == Some(slug))
            })
        })
}

/// Return the summary id for a current, eligible document path.
pub fn summary_id_for(project: &Project, rel: &Path) -> Option<String> {
    let doc = project.tree.get(rel)?;
    let id = match &doc.kind {
        DocKind::ProductSection {
            feature: Some(feature),
            ..
        } => {
            format!("{feature}/product/{}", rel.file_stem()?.to_str()?)
        }
        DocKind::Vision {
            feature: Some(feature),
        } => format!("{feature}/product/vision"),
        DocKind::Metaplan {
            feature: Some(feature),
        } => format!("{feature}/metaplan/{}", rel.file_stem()?.to_str()?),
        DocKind::Plan { slug, .. } => {
            let plan = project.tree.plans.get(slug)?;
            if plan.path != rel
                || (rel.parent() != Some(plans_dir(plan.feature.as_deref()).as_path())
                    && !named_plan(project, plan.feature.as_deref(), slug))
            {
                return None;
            }
            format!("plans/{slug}")
        }
        _ => return None,
    };
    validate_id(&id).ok().map(|()| id)
}

/// One feature's ordered overview members and documents outside either overview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    /// The feature whose documents and overview sources are considered.
    pub feature: String,
    /// Product sections listed by the vision.
    pub product: Vec<String>,
    /// Plans listed by metaplans.
    pub engineering: Vec<String>,
    /// Eligible documents of this feature not listed in an overview, sorted by id.
    pub outside: Vec<String>,
}

impl Scope {
    /// Select a known feature by a valid overview id (the overview need not yet exist).
    pub fn of(project: &Project, overview_id: &str) -> Option<Self> {
        let (feature, _) = overview_parts(overview_id)?;
        project.feature(feature)?;
        Some(scope(project, feature))
    }
}

fn document_ids(project: &Project) -> Vec<String> {
    project
        .tree
        .docs
        .iter()
        .filter_map(|doc| summary_id_for(project, &doc.path))
        .collect()
}

fn product_members(project: &Project, feature: &Feature) -> Vec<String> {
    let mut members = Vec::new();
    let mut seen = BTreeSet::new();
    if let Ok(rows) = &feature.vision_rows {
        for row in rows {
            for doc in project.tree.feature_docs(&feature.name) {
                if matches!(doc.kind, DocKind::ProductSection { number, .. } if number == row.number)
                {
                    if let Some(id) = summary_id_for(project, &doc.path) {
                        if seen.insert(id.clone()) {
                            members.push(id);
                        }
                    }
                }
            }
        }
    }
    members
}

fn engineering_members(project: &Project, feature: &Feature) -> Vec<String> {
    let mut members = Vec::new();
    let mut seen = BTreeSet::new();
    for (_, entries) in &feature.metaplans {
        for entry in entries {
            if let Some(plan) = entry
                .slug
                .as_ref()
                .and_then(|slug| project.tree.plans.get(slug))
            {
                if plan.feature.as_deref() != Some(feature.name.as_str()) {
                    continue;
                }
                if let Some(id) = summary_id_for(project, &plan.path) {
                    if seen.insert(id.clone()) {
                        members.push(id);
                    }
                }
            }
        }
    }
    members
}

/// Compute one feature's overview members in source order and its other eligible documents.
pub fn scope(project: &Project, feature: &str) -> Scope {
    let mut result = Scope {
        feature: feature.to_owned(),
        ..Scope::default()
    };
    let Some(feature) = project.feature(feature) else {
        return result;
    };
    result.product = product_members(project, feature);
    result.engineering = engineering_members(project, feature);
    let mut seen: BTreeSet<_> = result
        .product
        .iter()
        .chain(&result.engineering)
        .cloned()
        .collect();
    result.outside = project
        .tree
        .feature_docs(&feature.name)
        .filter_map(|doc| summary_id_for(project, &doc.path))
        .filter(|id| !is_vision(id) && !is_metaplan(id) && seen.insert(id.clone()))
        .collect();
    result.outside.sort();
    result
}

/// Return eligible document ids and overviews whose source documents exist.
pub fn expected_ids(project: &Project) -> Vec<String> {
    let mut ids = document_ids(project);
    for feature in &project.features {
        if project.tree.get(&vision_path(&feature.name)).is_some() {
            ids.push(format!("overview/{}/product", feature.name));
        }
        if !feature.metaplans.is_empty() {
            ids.push(format!("overview/{}/engineering", feature.name));
        }
    }
    ids
}

/// Return the current repository-relative source of an eligible document summary.
pub fn source_of(project: &Project, id: &str) -> Option<PathBuf> {
    validate_id(id).ok()?;
    project.tree.docs.iter().find_map(|doc| {
        (summary_id_for(project, &doc.path).as_deref() == Some(id)).then(|| doc.path.clone())
    })
}

/// Sentence caps of plan summary `id`, scaled to the lines of the plan and its brief files;
/// the floor caps when the plan is unknown.
pub fn plan_caps(project: &Project, id: &str) -> PlanCaps {
    let lines = |path: &Path| {
        let sections = &project.docs.get(path)?.sections;
        let ends = sections.headings.iter().map(|h| h.span.end_line);
        ends.chain([sections.preamble.end_line]).max()
    };
    let total = source_of(project, id).map_or(0, |plan| {
        let briefs = bundle_files(project, &plan);
        std::iter::once(plan)
            .chain(briefs)
            .filter_map(|p| lines(&p))
            .sum()
    });
    PlanCaps::for_source_lines(total)
}

fn normalized(path: &Path) -> Option<PathBuf> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => result.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(result)
}

/// Build a relative link from a summary file to a repository-relative target.
pub fn link_from_summary(summary_id: &str, target_rel: &Path) -> String {
    if validate_id(summary_id).is_err() {
        return String::new();
    }
    let Some(target) = normalized(target_rel) else {
        return String::new();
    };
    let path = summary_path(summary_id);
    let Some(parent) = path.parent() else {
        return String::new();
    };
    let from: Vec<_> = parent.components().collect();
    let to: Vec<_> = target.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts = vec!["..".to_string(); from.len() - common];
    parts.extend(
        to[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

/// Resolve a summary-relative link lexically, rejecting paths outside the repository.
pub fn resolve_link(summary_id: &str, href: &str) -> Option<(PathBuf, Option<String>)> {
    validate_id(summary_id).ok()?;
    let (path, fragment) = match href.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment.to_owned())),
        None => (href, None),
    };
    let link = Path::new(path);
    if link.is_absolute() || path.contains('\\') {
        return None;
    }
    let summary = summary_path(summary_id);
    let resolved = if path.is_empty() {
        summary
    } else {
        summary.parent()?.join(link)
    };
    Some((normalized(&resolved)?, fragment))
}

#[cfg(test)]
mod metaplan_contract_tests;
#[cfg(test)]
mod metaplan_id_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod vision_contract_tests;
