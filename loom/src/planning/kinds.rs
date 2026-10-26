//! Document kinds and ids derived from a path.

use std::path::Path;

use crate::planning::slug::{plan_slug, plan_state, PlanState};

/// The role inferred from a document's path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DocKind {
    /// The product vision.
    Vision,
    /// A numbered product section.
    ProductSection {
        /// The section number.
        number: u32,
    },
    /// An attachment to the product vision.
    ProductNote,
    /// An engineering metaplan.
    Metaplan,
    /// A plan in its current lifecycle state.
    Plan {
        /// The plan's stable slug.
        slug: String,
        /// The state inferred from its path.
        state: PlanState,
    },
    /// A file in a plan's brief bundle.
    Brief {
        /// The bundle's plan slug.
        slug: String,
    },
    /// A review of a plan run.
    RunReview {
        /// The reviewed plan's slug.
        slug: String,
    },
    /// A knowledge-base document.
    Knowledge,
    /// A document without a more specific role.
    Other,
}

/// A stable document identifier.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DocId(pub String);

/// Repository-relative path of the product vision document.
pub const VISION_PATH: &str = "doc/product/index.md";

/// Classify a repository-relative document path without reading the file.
pub fn classify(rel: &Path) -> DocKind {
    if rel == Path::new(VISION_PATH) {
        return DocKind::Vision;
    }
    if let Some(kind) = product_kind(rel) {
        return kind;
    }
    let name = rel.file_name().and_then(|name| name.to_str()).unwrap_or("");
    let parent = rel.parent();
    if parent == Some(Path::new("doc/plans"))
        && (name == "index.md" || (name.contains("metaplan") && name.ends_with(".md")))
    {
        return DocKind::Metaplan;
    }
    if let (Some(state), Some(slug)) = (plan_state(rel), plan_slug(name)) {
        return DocKind::Plan { slug, state };
    }
    if let Ok(bundle) = rel.strip_prefix("doc/plans/briefs") {
        let mut parts = bundle.components();
        if let (Some(slug), Some(_)) = (parts.next(), parts.next()) {
            return DocKind::Brief {
                slug: slug.as_os_str().to_string_lossy().into_owned(),
            };
        }
    }
    if parent == Some(Path::new("doc/plans")) || parent == Some(Path::new("doc/plans/archive")) {
        if let Some(slug) = name.strip_prefix("REVIEW-").and_then(plan_slug) {
            return DocKind::RunReview { slug };
        }
    }
    if rel.starts_with("doc/loom/knowledge") {
        return DocKind::Knowledge;
    }
    DocKind::Other
}

fn product_kind(rel: &Path) -> Option<DocKind> {
    let rest = rel.strip_prefix("doc/product").ok()?;
    let number = (rest.parent() == Some(Path::new("")))
        .then(|| rest.to_str())
        .flatten()
        .and_then(|name| name.strip_prefix("section_")?.strip_suffix(".md"))
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|digits| digits.parse().ok());
    Some(match number {
        Some(number) => DocKind::ProductSection { number },
        None => DocKind::ProductNote,
    })
}

/// Return the stable identifier for a document and its inferred kind.
pub fn doc_id(rel: &Path, kind: &DocKind) -> DocId {
    let id = match kind {
        DocKind::Vision => "product/index".to_string(),
        DocKind::ProductSection { number } => format!("product/section_{number:02}"),
        DocKind::Plan { slug, .. } => format!("plan/{slug}"),
        _ => rel
            .strip_prefix("doc")
            .unwrap_or(rel)
            .to_string_lossy()
            .into_owned(),
    };
    DocId(id)
}
