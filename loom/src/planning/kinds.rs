//! Document kinds and ids derived from a path.

use std::path::{Path, PathBuf};

use crate::planning::slug::{plan_feature, plan_slug, plan_state, PlanState};

/// The role inferred from a document's path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DocKind {
    /// The product vision.
    Vision {
        /// The owning feature, or `None` for a misplaced top-level vision.
        feature: Option<String>,
    },
    /// A numbered product section.
    ProductSection {
        /// The owning feature, or `None` for a misplaced top-level section.
        feature: Option<String>,
        /// The section number.
        number: u32,
    },
    /// An attachment to a feature's product vision.
    ProductNote {
        /// The owning feature.
        feature: String,
    },
    /// A feature's only metaplan, `doc/plans/<feature>/index.md`; any other `*metaplan*.md`
    /// is `Other`.
    Metaplan {
        /// The owning feature, or `None` for a misplaced top-level metaplan.
        feature: Option<String>,
    },
    /// A plan in its current lifecycle state.
    Plan {
        /// The plan's stable slug.
        slug: String,
        /// The state inferred from its path.
        state: PlanState,
        /// The owning feature, or `None` for a standalone plan.
        feature: Option<String>,
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

impl DocKind {
    /// Return the owning feature for variants that carry one.
    pub fn feature(&self) -> Option<&str> {
        match self {
            Self::Vision { feature }
            | Self::ProductSection { feature, .. }
            | Self::Metaplan { feature }
            | Self::Plan { feature, .. } => feature.as_deref(),
            Self::ProductNote { feature } => Some(feature),
            Self::Brief { .. } | Self::RunReview { .. } | Self::Knowledge | Self::Other => None,
        }
    }
}

/// A stable document identifier.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DocId(pub String);

/// Maximum feature name length, keeping `PLAN-<name>.md` well below 255-byte NAME_MAX.
pub const MAX_FEATURE_NAME: usize = 64;

/// Accept `[a-z0-9][a-z0-9-]*` up to 64 characters, excluding `archive` and `briefs`
/// and `plans` and `overview`, which are the first segments of summary ids.
pub fn is_feature_name(name: &str) -> bool {
    name.len() <= MAX_FEATURE_NAME
        && !matches!(name, "archive" | "briefs" | "plans" | "overview")
        && is_slug_chars(name)
}

/// The slug character class `[a-z0-9][a-z0-9-]*`: non-empty, a lowercase letter or digit
/// first, then lowercase letters, digits or `-`.
pub(crate) fn is_slug_chars(s: &str) -> bool {
    let mut bytes = s.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Return the first directory below `doc/product` or `doc/plans`, without validation.
///
/// A child component must follow it. Standalone `doc/plans/archive` and
/// `doc/plans/briefs` paths return `None`; callers validate other names separately.
pub fn feature_dir_name(rel: &Path) -> Option<&str> {
    let rest = rel
        .strip_prefix("doc/product")
        .or_else(|_| rel.strip_prefix("doc/plans"))
        .ok()?;
    let mut parts = rest.components();
    let name = parts.next()?.as_os_str().to_str()?;
    parts.next()?;
    if rel.starts_with("doc/plans") && matches!(name, "archive" | "briefs") {
        return None;
    }
    Some(name)
}

/// Return the repository-relative product directory for a feature.
pub fn product_dir(feature: &str) -> PathBuf {
    Path::new("doc/product").join(feature)
}

/// Return a feature's plans directory, or the standalone plans directory.
pub fn plans_dir(feature: Option<&str>) -> PathBuf {
    match feature {
        Some(feature) => Path::new("doc/plans").join(feature),
        None => PathBuf::from("doc/plans"),
    }
}

/// Return the repository-relative vision path for a feature.
pub fn vision_path(feature: &str) -> PathBuf {
    product_dir(feature).join("index.md")
}

/// Return a feature's section path, padding the number to at least two digits.
pub fn section_path(feature: &str, number: u32) -> PathBuf {
    product_dir(feature).join(format!("section_{number:02}.md"))
}

/// Return the written plan path in its feature or standalone home.
pub fn plan_path(feature: Option<&str>, slug: &str) -> PathBuf {
    plans_dir(feature).join(format!("PLAN-{slug}.md"))
}

/// Classify a repository-relative document path without reading the file.
pub fn classify(rel: &Path) -> DocKind {
    if rel.starts_with("doc/product") {
        return product_kind(rel);
    }
    if rel.starts_with("doc/plans") {
        return plans_kind(rel);
    }
    if rel.starts_with("doc/loom/knowledge") {
        return DocKind::Knowledge;
    }
    DocKind::Other
}

fn product_kind(rel: &Path) -> DocKind {
    let feature = feature_dir_name(rel);
    if feature.is_some_and(|name| !is_feature_name(name)) {
        return DocKind::Other;
    }
    let dir = feature.map_or_else(|| PathBuf::from("doc/product"), product_dir);
    if rel.parent() == Some(dir.as_path()) {
        let name = rel.file_name().and_then(|name| name.to_str()).unwrap_or("");
        if name == "index.md" {
            return DocKind::Vision {
                feature: feature.map(str::to_owned),
            };
        }
        if let Some(number) = section_number(name) {
            return DocKind::ProductSection {
                feature: feature.map(str::to_owned),
                number,
            };
        }
    }
    feature.map_or(DocKind::Other, |feature| DocKind::ProductNote {
        feature: feature.to_owned(),
    })
}

fn section_number(name: &str) -> Option<u32> {
    let digits = name.strip_prefix("section_")?.strip_suffix(".md")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn plans_kind(rel: &Path) -> DocKind {
    let feature = feature_dir_name(rel);
    if feature.is_some_and(|name| !is_feature_name(name)) {
        return DocKind::Other;
    }
    let dir = plans_dir(feature);
    let name = rel.file_name().and_then(|name| name.to_str()).unwrap_or("");
    if let (Some(state), Some(slug)) = (plan_state(rel), plan_slug(name)) {
        return DocKind::Plan {
            slug,
            state,
            feature: plan_feature(rel),
        };
    }
    if let Ok(bundle) = rel.strip_prefix(dir.join("briefs")) {
        let mut parts = bundle.components();
        if let (Some(slug), Some(_)) = (parts.next(), parts.next()) {
            return DocKind::Brief {
                slug: slug.as_os_str().to_string_lossy().into_owned(),
            };
        }
    }
    if rel.parent() == Some(dir.as_path()) || rel.parent() == Some(dir.join("archive").as_path()) {
        if let Some(slug) = name.strip_prefix("REVIEW-").and_then(plan_slug) {
            return DocKind::RunReview { slug };
        }
    }
    if rel.parent() == Some(dir.as_path()) && name == "index.md" {
        return DocKind::Metaplan {
            feature: feature.map(str::to_owned),
        };
    }
    DocKind::Other
}

/// Return the stable identifier for a document and its inferred kind.
pub fn doc_id(rel: &Path, kind: &DocKind) -> DocId {
    let id = match kind {
        DocKind::Vision { feature: None } => "product/index".to_string(),
        DocKind::Vision {
            feature: Some(feature),
        } => format!("product/{feature}/index"),
        DocKind::ProductSection {
            feature: None,
            number,
        } => format!("product/section_{number:02}"),
        DocKind::ProductSection {
            feature: Some(feature),
            number,
        } => {
            format!("product/{feature}/section_{number:02}")
        }
        DocKind::Plan { slug, .. } => format!("plan/{slug}"),
        _ => rel
            .strip_prefix("doc")
            .unwrap_or(rel)
            .to_string_lossy()
            .into_owned(),
    };
    DocId(id)
}
