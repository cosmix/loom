//! Store layout, summary ids, overview scope and link helpers.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Result};

use crate::planning::kinds::{DocKind, VISION_PATH};
use crate::planning::project::Project;

/// Repository-relative directory containing summaries.
pub const STORE_DIR: &str = "doc/loom/summaries";

/// Validate a summary id before using it as a path.
pub fn validate_id(id: &str) -> Result<()> {
    let valid = match id {
        "product" | "engineering" => true,
        _ => {
            if let Some(digits) = id.strip_prefix("product/section_") {
                digits.len() >= 2 && digits.bytes().all(|b| b.is_ascii_digit())
            } else if let Some(slug) = id.strip_prefix("plans/") {
                !slug.contains("..")
                    && slug
                        .bytes()
                        .next()
                        .is_some_and(|b| b.is_ascii_alphanumeric())
                    && slug
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            } else {
                false
            }
        }
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

fn named_plan(project: &Project, slug: &str) -> bool {
    project.metaplans.iter().any(|(_, entries)| {
        entries
            .iter()
            .any(|entry| entry.slug.as_deref() == Some(slug))
    })
}

/// Return the summary id for a current, eligible document path.
pub fn summary_id_for(project: &Project, rel: &Path) -> Option<String> {
    let doc = project.tree.get(rel)?;
    let id = match &doc.kind {
        DocKind::ProductSection { .. } => {
            format!("product/{}", rel.file_stem()?.to_str()?)
        }
        DocKind::Plan { slug, .. } => {
            let plan = project.tree.plans.get(slug)?;
            if plan.path != rel
                || (rel.parent() != Some(Path::new("doc/plans")) && !named_plan(project, slug))
            {
                return None;
            }
            format!("plans/{slug}")
        }
        _ => return None,
    };
    validate_id(&id).ok().map(|()| id)
}

/// Ordered overview members and documents outside either overview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    /// Product sections listed by the vision.
    pub product: Vec<String>,
    /// Plans listed by metaplans.
    pub engineering: Vec<String>,
    /// Eligible documents not listed in an overview.
    pub outside: Vec<String>,
}

fn document_ids(project: &Project) -> Vec<String> {
    project
        .tree
        .docs
        .iter()
        .filter_map(|doc| summary_id_for(project, &doc.path))
        .collect()
}

/// Compute overview members in source order and eligible documents outside them.
pub fn scope(project: &Project) -> Scope {
    let all = document_ids(project);
    let mut result = Scope::default();
    let mut seen = BTreeSet::new();
    if let Ok(rows) = &project.vision_rows {
        for row in rows {
            for doc in &project.tree.docs {
                if matches!(doc.kind, DocKind::ProductSection { number } if number == row.number) {
                    if let Some(id) = summary_id_for(project, &doc.path) {
                        if seen.insert(id.clone()) {
                            result.product.push(id);
                        }
                    }
                }
            }
        }
    }
    for (_, entries) in &project.metaplans {
        for entry in entries {
            if let Some(plan) = entry
                .slug
                .as_ref()
                .and_then(|slug| project.tree.plans.get(slug))
            {
                if let Some(id) = summary_id_for(project, &plan.path) {
                    if seen.insert(id.clone()) {
                        result.engineering.push(id);
                    }
                }
            }
        }
    }
    result.outside = all
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect();
    result.outside.sort();
    result
}

/// Return eligible document ids and overviews whose source documents exist.
pub fn expected_ids(project: &Project) -> Vec<String> {
    let mut ids = document_ids(project);
    if project.tree.get(Path::new(VISION_PATH)).is_some() {
        ids.push("product".into());
    }
    if !project.metaplans.is_empty() {
        ids.push("engineering".into());
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
mod tests;
