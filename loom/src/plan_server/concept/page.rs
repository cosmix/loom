//! Concept pages: page keys, the page context, and loading a page from the evaluated project.

use std::path::Path;
use std::sync::Arc;

use crate::plan_server::concept::body::{self, doc_link, Level};
use crate::plan_server::concept::key::{colour_of, key_chips};
use crate::plan_server::concept::links::{implemented_by, implements_view, plan_state_name};
use crate::plan_server::concept::state::out_of_date_reason;
use crate::plan_server::summaries::rebuilds::Rebuilds;
use crate::plan_server::wire::concept::{
    ConceptPage, DocLink, EmptyKind, EmptyState, KeyChip, Progress, SummaryStateView, SummaryView,
};
use crate::planning::discover::read_doc;
use crate::planning::kinds::DocKind;
use crate::planning::pressure;
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;
use crate::planning::summary::evaluate::{Evaluation, SummaryState};
use crate::planning::summary::store::{self, source_of};

use crate::planning::summary::write::read_summary;

use super::empty::empty_state;
pub use super::page_key::{PageKey, Scope};

/// Server state a page depends on beyond the project and its evaluation.
#[derive(Clone, Default)]
pub struct PageContext {
    /// `Some(reason)` when `loom plan` runs read-only.
    pub read_only: Option<String>,
    /// Scheduler reasons, running progress and cost estimates, when installed.
    pub rebuilds: Option<Arc<Rebuilds>>,
}

impl std::fmt::Debug for PageContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageContext")
            .field("read_only", &self.read_only)
            .field("rebuilds", &self.rebuilds.as_ref().map(|_| "Rebuilds"))
            .finish()
    }
}

/// Builds the page `key` addresses; `None` when the project has no such document.
pub fn load_page(
    project: &Project,
    eval: &Evaluation,
    key: &PageKey,
    ctx: &PageContext,
) -> Option<ConceptPage> {
    if let Some(feature) = key.feature() {
        project.feature(feature)?;
    }
    match key {
        PageKey::Overview { feature } => Some(overview_page(project, eval, feature, ctx)),
        PageKey::Summary {
            feature,
            scope,
            doc,
        } => summary_page(project, eval, feature.as_deref(), *scope, doc, ctx),
    }
}

/// The running rebuild's progress and the estimated tokens to build missing summaries.
pub fn progress(ctx: &PageContext) -> (Option<Progress>, Option<String>) {
    let Some(rebuilds) = &ctx.rebuilds else {
        return (None, None);
    };
    (
        rebuilds.building().map(Progress::from),
        rebuilds.estimate_text(),
    )
}

fn blank(key: &PageKey, title: String, ctx: &PageContext) -> ConceptPage {
    let (building, estimate) = progress(ctx);
    ConceptPage {
        feature: key.feature().map(str::to_owned),
        key: key.as_key(),
        title,
        path: None,
        file_hash: None,
        colour: None,
        state: None,
        plan_state: None,
        last_pressure: None,
        key_chips: Vec::new(),
        blocks: Vec::new(),
        detail: Vec::new(),
        unlisted: Vec::new(),
        implements: None,
        implemented_by: Vec::new(),
        empty: None,
        building,
        estimate,
    }
}

fn state_view(id: &str, state: SummaryState, ctx: &PageContext) -> SummaryView {
    let view = match state {
        SummaryState::Current => SummaryStateView::Current,
        SummaryState::Incomplete => SummaryStateView::Incomplete,
        SummaryState::Stale => SummaryStateView::Stale,
        SummaryState::Rebuilding => SummaryStateView::Rebuilding,
        SummaryState::Missing => SummaryStateView::Missing,
    };
    SummaryView {
        state: view,
        out_of_date: out_of_date_reason(id, state, ctx),
    }
}

/// `normalized_hash` of the summary file's text on disk, the hash a summary attachment records.
/// Read from disk because project discovery skips the summary store.
pub(crate) fn summary_file_hash(project: &Project, id: &str) -> Option<String> {
    let text = read_summary(&project.root, id).ok()??;
    Some(normalized_hash(&text))
}

/// The concept key of a summary, using its source to identify a plan's feature.
pub(crate) fn page_key_of(project: &Project, id: &str) -> Option<String> {
    if store::is_overview(id) {
        return PageKey::parse(id).map(|key| key.as_key());
    }
    let path = source_of(project, id)?;
    let feature = project.tree.get(&path)?.kind.feature().map(str::to_owned);
    let (scope, doc) = match id.strip_prefix("plans/") {
        Some(slug) => (Scope::Engineering, slug),
        None if store::is_metaplan(id) => (Scope::Metaplan, id.rsplit('/').next()?),
        None => (Scope::Product, id.rsplit('/').next()?),
    };
    let key = PageKey::Summary {
        feature,
        scope,
        doc: doc.to_owned(),
    };
    PageKey::parse(&key.as_key()).map(|key| key.as_key())
}

fn overview_page(
    project: &Project,
    eval: &Evaluation,
    feature: &str,
    ctx: &PageContext,
) -> ConceptPage {
    let key = PageKey::Overview {
        feature: feature.to_owned(),
    };
    let id = key.as_key();
    let mut page = blank(&key, "Product overview".to_owned(), ctx);
    let chips = key_chips(project, eval, feature, Scope::Product, ctx);
    let summary = eval.summaries.get(&id);
    page.state = summary.map(|summary| state_view(&id, summary.state, ctx));
    match summary.and_then(|summary| summary.file.as_ref()) {
        Some(file) => {
            let rendered = body::blocks(project, &id, file, eval, &chips, Level::Overview, ctx);
            page.blocks = rendered.blocks;
            page.detail = rendered.detail;
            page.file_hash = summary_file_hash(project, &id);
        }
        None => page.empty = Some(empty_state(project, eval, feature)),
    }
    page.unlisted = unlisted(project, feature, page.empty.as_ref());
    page.key_chips = chips;
    page
}

fn unlisted(project: &Project, feature: &str, empty: Option<&EmptyState>) -> Vec<DocLink> {
    if empty.is_some_and(|empty| empty.kind == EmptyKind::SectionsNoVision) {
        return Vec::new();
    }
    let members = store::scope(project, feature);
    let prefix = format!("{feature}/product/");
    members
        .outside
        .iter()
        .filter(|id| id.starts_with(&prefix))
        .filter_map(|id| summary_link(project, id))
        .collect()
}

fn summary_link(project: &Project, id: &str) -> Option<DocLink> {
    doc_link(project, &source_of(project, id)?)
}

fn summary_page(
    project: &Project,
    eval: &Evaluation,
    feature: Option<&str>,
    scope: Scope,
    doc: &str,
    ctx: &PageContext,
) -> Option<ConceptPage> {
    let id = match scope {
        Scope::Product => format!("{}/product/{doc}", feature?),
        Scope::Engineering => format!("plans/{doc}"),
        Scope::Metaplan => format!("{}/metaplan/{doc}", feature?),
    };
    let path = source_of(project, &id)?;
    let document = project.docs.get(&path)?;
    if document.doc.kind.feature() != feature {
        return None;
    }
    let title = document.title.clone();
    let key = PageKey::Summary {
        feature: feature.map(str::to_owned),
        scope,
        doc: doc.to_owned(),
    };
    let mut page = blank(&key, title, ctx);
    let chips = feature
        .map(|feature| key_chips(project, eval, feature, scope, ctx))
        .unwrap_or_default();
    let path_text = path.to_string_lossy().into_owned();
    page.colour = colour_of(&chips, &path_text);
    summary_body(project, eval, &id, &chips, &mut page, ctx);
    document_fields(project, &mut page, &path);
    page.path = Some(path_text);
    page.key_chips = chips;
    Some(page)
}

fn summary_body(
    project: &Project,
    eval: &Evaluation,
    id: &str,
    chips: &[KeyChip],
    page: &mut ConceptPage,
    ctx: &PageContext,
) {
    let summary = eval.summaries.get(id);
    page.state = summary.map(|summary| state_view(id, summary.state, ctx));
    match summary.and_then(|summary| summary.file.as_ref()) {
        Some(file) => {
            let rendered = body::blocks(project, id, file, eval, chips, Level::Document, ctx);
            page.blocks = rendered.blocks;
            page.detail = rendered.detail;
            page.file_hash = summary_file_hash(project, id);
        }
        None => page.empty = Some(summary_empty(summary.map(|one| one.state))),
    }
}

fn summary_empty(state: Option<SummaryState>) -> EmptyState {
    let rebuilding = state == Some(SummaryState::Rebuilding);
    EmptyState {
        kind: if rebuilding {
            EmptyKind::Building
        } else {
            EmptyKind::NeverBuilt
        },
        count: (!rebuilding).then_some(1),
        offer: None,
        docs: Vec::new(),
    }
}

/// The fields that depend on what kind of document the page summarizes.
fn document_fields(project: &Project, page: &mut ConceptPage, path: &Path) {
    let Some(doc) = project.tree.get(path) else {
        return;
    };
    match &doc.kind {
        DocKind::Plan { slug, state, .. } => {
            page.plan_state = Some(plan_state_name(*state).to_owned());
            page.implements = Some(implements_view(project, slug));
            let text = read_doc(&project.root, path).ok();
            let dates = text.iter().flat_map(|text| pressure::records(text));
            page.last_pressure = dates.map(|record| record.date).max();
        }
        DocKind::ProductSection {
            feature: Some(feature),
            number,
        } => {
            page.implemented_by = implemented_by(project, feature, *number);
        }
        _ => {}
    }
}
