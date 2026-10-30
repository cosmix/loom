//! `GET /api/doc?path=`: one document as file mode shows it, and the editability rule.
//!
//! The `path` parameter is relative to `doc/`. It passes `check_relpath_syntax` (else 400) and
//! must be a document discovery lists (else 404) before anything is read. The text comes from
//! the `TextStore`, which reads with plan 1's bounded, no-follow `read_doc`, so a symlinked
//! document reaches only a file inside `doc/`. Every path in the answer is repo-relative.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use super::plan_view::{last_change, plan_view, product_sections};
use super::texts::{PinnedText, TextStore};
use super::validate::ValidationCache;
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::files::{
    DocView, EntryKind, LockKind, LockView, PlanView, SectionView, ValidationView,
};
use crate::planning::kinds::{feature_dir_name, is_feature_name, DocKind};
use crate::planning::paths::{check_relpath_syntax, classify_write, DocArea, LockReason};
use crate::planning::project::Project;
use crate::planning::sections::{sections_of, DocSections};
use crate::planning::slug::PlanState;

/// Largest document file mode edits or shows in full (section 06, "What may be edited").
pub const MAX_EDIT_BYTES: usize = 1_048_576;

/// Lock notices, verbatim from section 06 "Locked files".
pub const RUNNING_NOTICE: &str =
    "This plan is running. `loom run` works from the committed copy, so the app keeps it read-only.";
pub const DONE_NOTICE: &str = "This plan is done. Record changes in a new plan.";
pub const KNOWLEDGE_NOTICE: &str =
    "The knowledge base changes through `loom knowledge`. Ask in chat.";
pub const SUMMARIES_NOTICE: &str = "The summary store changes through `loom plan summary seal`.";

/// The document route.
pub fn doc_route(req: &ApiRequest<'_>) -> ApiResponse {
    respond(checked_path(req.query.get("path")).and_then(|rel| doc_view(req.ctx, &rel)))
}

/// The view of `rel` (repo-relative, `doc/...`): 404 unless discovery lists it and it has text.
pub fn doc_view(ctx: &PlanContext, rel: &Path) -> Result<DocView, ApiResponse> {
    let store = text_store(ctx)?;
    let facts = {
        let project = ctx.project();
        Facts::of(&project, rel)?
    };
    let pinned = store.current(rel).ok_or_else(|| {
        let message = format!("{} has no readable text", rel.display());
        ApiResponse::error(404, "not_found", &message)
    })?;
    // The project view parsed the text the watcher last saw; when the store already holds a
    // newer one, parse that, so lines and stages always match the text sent.
    let sections = match facts.sections {
        Some(cached) if cached.file_hash == pinned.hash => Some(cached),
        _ if is_markdown(rel) => Some(sections_of(&pinned.text)),
        _ => None,
    };
    let plan = match (&facts.kind, &sections) {
        (DocKind::Plan { .. }, Some(sections)) => Some(plan_panel(
            ctx,
            rel,
            &pinned.text,
            sections,
            &facts.product_sections,
        )),
        _ => None,
    };
    let size = pinned.text.len();
    let too_large = size > MAX_EDIT_BYTES;
    Ok(DocView {
        path: rel.to_string_lossy().into_owned(),
        title: facts.title,
        text: shown_text(&pinned, too_large),
        hash: pinned.hash.clone(),
        kind: entry_kind(&facts.kind),
        feature: document_feature(rel, &facts.kind),
        misplaced: misplaced(&facts.kind),
        editable: facts.editable && !too_large,
        lock: facts.lock,
        sections: sections.as_ref().map(section_views).unwrap_or_default(),
        plan,
        last_change: last_change(ctx.root(), rel),
        too_large,
        truncated: too_large,
        size: u64::try_from(size).unwrap_or(u64::MAX),
    })
}

fn plan_panel(
    ctx: &PlanContext,
    rel: &Path,
    text: &str,
    sections: &DocSections,
    product: &BTreeMap<u32, String>,
) -> PlanView {
    let validation = cached_validation(ctx, rel, text);
    plan_view(ctx.root(), rel, text, sections, product, validation)
}

/// What the project view knows of a discovered document, copied out so the project lock is
/// released before the text is read and the plan view built.
struct Facts {
    kind: DocKind,
    title: String,
    sections: Option<DocSections>,
    lock: Option<LockView>,
    editable: bool,
    /// Product section paths by number, for a plan's `Implements:` links (empty otherwise).
    product_sections: BTreeMap<u32, String>,
}

impl Facts {
    fn of(project: &Project, rel: &Path) -> Result<Facts, ApiResponse> {
        let Some(doc) = project.tree.get(rel) else {
            return Err(not_found(rel));
        };
        let parsed = project.docs.get(rel);
        let title = parsed.map(|p| p.title.clone()).unwrap_or_else(|| {
            let name = rel.file_name().unwrap_or(rel.as_os_str());
            name.to_string_lossy().into_owned()
        });
        Ok(Facts {
            kind: doc.kind.clone(),
            title,
            sections: parsed.map(|p| p.sections.clone()),
            lock: lock_view(project, rel),
            editable: editable(project, rel),
            product_sections: match doc.kind {
                DocKind::Plan { ref feature, .. } => {
                    product_sections(&project.tree, feature.as_deref())
                }
                _ => BTreeMap::new(),
            },
        })
    }
}

/// The full text, or for a document over 1 MiB its first 1 MiB cut on a char boundary.
fn shown_text(pinned: &PinnedText, too_large: bool) -> String {
    let text = &*pinned.text;
    if too_large {
        text[..text.floor_char_boundary(MAX_EDIT_BYTES)].to_owned()
    } else {
        text.to_owned()
    }
}

/// Heading sections with 1-based lines, the end inclusive.
fn section_views(sections: &DocSections) -> Vec<SectionView> {
    let line = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    sections
        .headings
        .iter()
        .map(|heading| SectionView {
            level: heading.level,
            title: heading.title.clone(),
            anchor: heading.anchor.clone(),
            start_line: line(heading.span.start_line.saturating_add(1)),
            end_line: line(heading.span.end_line),
        })
        .collect()
}

/// The plan's validation from the cache, which holds unlocked plans only (one the startup
/// pass has not reached is validated from `text` now); `None` for a locked plan, or when the
/// cache is not installed.
fn cached_validation(ctx: &PlanContext, rel: &Path, text: &str) -> Option<ValidationView> {
    ctx.extensions()
        .get::<ValidationCache>()?
        .view_or_validate(ctx, rel, text)
}

/// Whether file mode may edit `rel` (repo-relative, discovered) at its current size: Markdown,
/// in the plain `doc/` area (not locked, not under `doc/loom/`). The 1 MiB limit is the
/// caller's, applied to the text it holds.
pub fn editable(project: &Project, rel: &Path) -> bool {
    is_markdown(rel)
        && !is_under_loom(rel)
        && !feature_dir_name(rel).is_some_and(|name| !is_feature_name(name))
        && matches!(classify_write(&project.locked, rel), DocArea::Doc)
}

/// Why `rel` (repo-relative) is read-only in file mode, with section 06's notice; `None` for a
/// path whose area places no lock on it.
pub fn lock_view(project: &Project, rel: &Path) -> Option<LockView> {
    let view = |reason, notice: &str, plan| LockView {
        reason,
        notice: notice.to_owned(),
        plan,
    };
    match classify_write(&project.locked, rel) {
        DocArea::Locked(reason) => Some(locked_view(project, reason)),
        DocArea::Summaries => Some(view(LockKind::Summaries, SUMMARIES_NOTICE, None)),
        DocArea::Knowledge => Some(view(LockKind::Knowledge, KNOWLEDGE_NOTICE, None)),
        // Section 06 keeps all of `doc/loom/` read-only, not only the knowledge base.
        DocArea::Doc if is_under_loom(rel) => {
            Some(view(LockKind::Knowledge, KNOWLEDGE_NOTICE, None))
        }
        DocArea::Doc | DocArea::OutsideDoc => None,
    }
}

/// A retired plan and a run review take the done notice: section 06 words no other.
fn locked_view(project: &Project, reason: LockReason) -> LockView {
    let (reason, notice, plan) = match reason {
        LockReason::PlanState(PlanState::Running) => (LockKind::Running, RUNNING_NOTICE, None),
        LockReason::PlanState(_) | LockReason::RunReview => (LockKind::Done, DONE_NOTICE, None),
        LockReason::Archive => (LockKind::Archive, DONE_NOTICE, None),
        LockReason::BriefOfLockedPlan { slug } => return brief_lock(project, &slug),
    };
    LockView {
        reason,
        notice: notice.to_owned(),
        plan,
    }
}

/// The locked plan's notice, naming the plan; `plan` is its repo-relative path.
fn brief_lock(project: &Project, slug: &str) -> LockView {
    let entry = project.tree.plan(slug);
    let running = entry.is_some_and(|plan| plan.state == PlanState::Running);
    let notice = if running { RUNNING_NOTICE } else { DONE_NOTICE };
    LockView {
        reason: LockKind::BriefOfLockedPlan,
        notice: notice.replacen("This plan", &format!("The plan `{slug}`"), 1),
        plan: entry.map(|plan| plan.path.to_string_lossy().into_owned()),
    }
}

/// The wire kind of a discovered document.
pub fn entry_kind(kind: &DocKind) -> EntryKind {
    match kind {
        DocKind::Vision { .. } => EntryKind::Vision,
        DocKind::ProductSection { .. } => EntryKind::Section,
        DocKind::ProductNote { .. } => EntryKind::Note,
        DocKind::Metaplan { .. } => EntryKind::Metaplan,
        DocKind::Plan { .. } => EntryKind::Plan,
        DocKind::Brief { .. } => EntryKind::Brief,
        DocKind::RunReview { .. } => EntryKind::Review,
        DocKind::Knowledge => EntryKind::Knowledge,
        DocKind::Other => EntryKind::Other,
    }
}

/// The feature of a document, including briefs and reviews whose kind stores only a slug.
pub(crate) fn document_feature(rel: &Path, kind: &DocKind) -> Option<String> {
    kind.feature()
        .or_else(|| feature_dir_name(rel))
        .filter(|name| is_feature_name(name))
        .map(str::to_owned)
}

/// Whether a document needs to move from the top level into a feature.
pub(crate) fn misplaced(kind: &DocKind) -> bool {
    matches!(
        kind,
        DocKind::Vision { feature: None }
            | DocKind::ProductSection { feature: None, .. }
            | DocKind::Metaplan { feature: None }
    )
}

/// Whether `rel` names a Markdown file.
pub(crate) fn is_markdown(rel: &Path) -> bool {
    rel.extension().is_some_and(|ext| ext == "md")
}

/// Under `doc/loom/`, compared lowercased like `planning::paths` (case-insensitive file systems).
fn is_under_loom(rel: &Path) -> bool {
    let folded = PathBuf::from(rel.to_string_lossy().to_lowercase());
    folded.starts_with("doc/loom")
}

/// The `path` query parameter (relative to `doc/`) checked and made repo-relative; 400 when it
/// is missing or fails `check_relpath_syntax`.
pub(crate) fn checked_path(raw: Option<&str>) -> Result<PathBuf, ApiResponse> {
    let raw =
        raw.ok_or_else(|| ApiResponse::error(400, "bad_request", "the path parameter is missing"))?;
    check_relpath_syntax(raw)
        .map_err(|error| ApiResponse::error(400, "bad_request", &error.to_string()))
}

/// 404 unless discovery lists `rel` (repo-relative).
pub(crate) fn discovered(ctx: &PlanContext, rel: &Path) -> Result<(), ApiResponse> {
    match ctx.project().tree.get(rel) {
        Some(_) => Ok(()),
        None => Err(not_found(rel)),
    }
}

fn not_found(rel: &Path) -> ApiResponse {
    let message = format!("{} is not a discovered document", rel.display());
    ApiResponse::error(404, "not_found", &message)
}

/// The `TextStore` `files::install` put in the extensions; 500 when it is missing.
pub(crate) fn text_store(ctx: &PlanContext) -> Result<Arc<TextStore>, ApiResponse> {
    ctx.extensions().get::<TextStore>().ok_or_else(|| {
        tracing::warn!("file mode: the text store is not installed");
        ApiResponse::error(500, "internal", "the text store is not available")
    })
}

/// 200 with `value` as JSON, or the refusal.
pub(crate) fn respond<T: Serialize>(result: Result<T, ApiResponse>) -> ApiResponse {
    match result {
        Ok(value) => ApiResponse::json(200, &value),
        Err(refusal) => refusal,
    }
}
