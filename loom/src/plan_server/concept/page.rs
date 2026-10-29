//! Concept pages: page keys, the page context, and loading a page from the evaluated project.

use std::path::Path;
use std::sync::Arc;

use crate::plan_server::concept::body::{self, doc_link, Level};
use crate::plan_server::concept::key::{colour_of, key_chips};
use crate::plan_server::concept::links::{implemented_by, implements_view, plan_state_name};
use crate::plan_server::concept::state::out_of_date_reason;
use crate::plan_server::summaries::rebuilds::Rebuilds;
use crate::plan_server::wire::concept::{
    ConceptPage, DocLink, EmptyKind, EmptyState, Progress, SummaryStateView, SummaryView,
};
use crate::planning::checklist::Offer;
use crate::planning::discover::read_doc;
use crate::planning::kinds::{DocKind, VISION_PATH};
use crate::planning::pressure;
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;
use crate::planning::summary::evaluate::{Evaluation, SummaryState};
use crate::planning::summary::store::source_of;
use crate::planning::summary::write::read_summary;

/// Which overview a page belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Product,
    Engineering,
}

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

/// The page a request addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageKey {
    Overview(Scope),
    /// A document summary page; `doc` is a product section file stem or a plan slug.
    Summary {
        scope: Scope,
        doc: String,
    },
}

impl PageKey {
    /// Parses the path after `/api/concept/`; `None` for anything but `product`,
    /// `engineering` or `<scope>/<name>` where the name matches `[A-Za-z0-9._-]{1,128}` and is
    /// not `.` or `..`.
    pub fn parse(rest: &str) -> Option<PageKey> {
        let (scope, doc) = match rest.split_once('/') {
            Some((scope, doc)) => (scope, Some(doc)),
            None => (rest, None),
        };
        let scope = match scope {
            "product" => Scope::Product,
            "engineering" => Scope::Engineering,
            _ => return None,
        };
        let Some(doc) = doc else {
            return Some(PageKey::Overview(scope));
        };
        let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
        let safe =
            (1..=128).contains(&doc.len()) && doc.chars().all(plain) && doc != "." && doc != "..";
        safe.then(|| PageKey::Summary {
            scope,
            doc: doc.to_owned(),
        })
    }

    /// The key as it appears in URLs and `DocLink.page`.
    pub fn as_key(&self) -> String {
        match self {
            PageKey::Overview(scope) => scope_name(*scope).to_owned(),
            PageKey::Summary { scope, doc } => format!("{}/{doc}", scope_name(*scope)),
        }
    }
}

fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Product => "product",
        Scope::Engineering => "engineering",
    }
}

/// Builds the page `key` addresses; `None` when the project has no such document.
pub fn load_page(
    project: &Project,
    eval: &Evaluation,
    key: &PageKey,
    ctx: &PageContext,
) -> Option<ConceptPage> {
    match key {
        PageKey::Overview(scope) => Some(overview_page(project, eval, *scope, ctx)),
        PageKey::Summary { scope, doc } => summary_page(project, eval, *scope, doc, ctx),
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

fn blank(key: String, title: String, ctx: &PageContext) -> ConceptPage {
    let (building, estimate) = progress(ctx);
    ConceptPage {
        key,
        title,
        path: None,
        file_hash: None,
        colour: None,
        state: None,
        plan_state: None,
        last_pressure: None,
        key_chips: Vec::new(),
        blocks: Vec::new(),
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

/// The concept page key of summary `id`: `plans/<slug>` pages live under `engineering/<slug>`,
/// every other id is its own key.
pub(crate) fn page_key_of(id: &str) -> String {
    match id.strip_prefix("plans/") {
        Some(slug) => format!("engineering/{slug}"),
        None => id.to_owned(),
    }
}

fn overview_page(
    project: &Project,
    eval: &Evaluation,
    scope: Scope,
    ctx: &PageContext,
) -> ConceptPage {
    let (id, title) = match scope {
        Scope::Product => ("product", "Product overview"),
        Scope::Engineering => ("engineering", "Engineering overview"),
    };
    let mut page = blank(id.to_owned(), title.to_owned(), ctx);
    let chips = key_chips(project, eval, scope, ctx);
    let summary = eval.summaries.get(id);
    page.state = summary.map(|summary| state_view(id, summary.state, ctx));
    match summary.and_then(|summary| summary.file.as_ref()) {
        Some(file) => {
            page.blocks = body::blocks(project, file, eval, &chips, Level::Overview, ctx);
            page.file_hash = summary_file_hash(project, id);
        }
        None => page.empty = Some(empty_state(project, eval, scope)),
    }
    let listed_in_empty = page.empty.as_ref().is_some_and(|empty| {
        matches!(
            empty.kind,
            EmptyKind::SectionsNoVision | EmptyKind::PlansNoMetaplan
        )
    });
    if !listed_in_empty {
        let outside = eval.scope.outside.iter();
        let ids = outside.filter(|id| id.starts_with(id_prefix(scope)));
        page.unlisted = ids.filter_map(|id| summary_link(project, id)).collect();
    }
    page.key_chips = chips;
    page
}

fn id_prefix(scope: Scope) -> &'static str {
    match scope {
        Scope::Product => "product/",
        Scope::Engineering => "plans/",
    }
}

fn summary_link(project: &Project, id: &str) -> Option<DocLink> {
    doc_link(project, &source_of(project, id)?)
}

/// Why an overview without a file cannot render. `eval.summaries` iterates in id order, which
/// is file-name order for the document summaries.
fn empty_state(project: &Project, eval: &Evaluation, scope: Scope) -> EmptyState {
    let (root_exists, absent, only_documents, offer) = match scope {
        Scope::Product => (
            project.tree.get(Path::new(VISION_PATH)).is_some(),
            EmptyKind::NoVision,
            EmptyKind::SectionsNoVision,
            Offer::StartVision,
        ),
        Scope::Engineering => (
            !project.metaplans.is_empty(),
            EmptyKind::NoMetaplan,
            EmptyKind::PlansNoMetaplan,
            Offer::StartMetaplan,
        ),
    };
    let prefix = id_prefix(scope);
    let documents = eval
        .summaries
        .iter()
        .filter(|(id, _)| id.starts_with(prefix));
    let documents: Vec<_> = documents.collect();
    let mut empty = EmptyState {
        kind: EmptyKind::Building,
        count: None,
        offer: None,
        docs: Vec::new(),
    };
    if !root_exists && documents.is_empty() {
        empty.kind = absent;
        empty.offer = Some(offer.id());
    } else if !root_exists {
        empty.kind = only_documents;
        empty.docs = documents
            .iter()
            .filter_map(|(id, _)| summary_link(project, id))
            .collect();
    } else if !documents.is_empty()
        && documents
            .iter()
            .all(|(_, one)| one.state == SummaryState::Missing)
    {
        empty.kind = EmptyKind::NeverBuilt;
        empty.count = Some(documents.len() as u32);
    }
    empty
}

fn summary_page(
    project: &Project,
    eval: &Evaluation,
    scope: Scope,
    doc: &str,
    ctx: &PageContext,
) -> Option<ConceptPage> {
    let id = format!("{}{doc}", id_prefix(scope));
    let path = source_of(project, &id)?;
    let title = project.docs.get(&path)?.title.clone();
    let key = PageKey::Summary {
        scope,
        doc: doc.to_owned(),
    };
    let mut page = blank(key.as_key(), title, ctx);
    let chips = key_chips(project, eval, scope, ctx);
    let path_text = path.to_string_lossy().into_owned();
    page.colour = colour_of(&chips, &path_text);
    let summary = eval.summaries.get(&id);
    page.state = summary.map(|summary| state_view(&id, summary.state, ctx));
    match summary.and_then(|summary| summary.file.as_ref()) {
        Some(file) => {
            page.blocks = body::blocks(project, file, eval, &chips, Level::Document, ctx);
            page.file_hash = summary_file_hash(project, &id);
        }
        None => {
            let rebuilding = summary.is_some_and(|s| s.state == SummaryState::Rebuilding);
            page.empty = Some(EmptyState {
                kind: if rebuilding {
                    EmptyKind::Building
                } else {
                    EmptyKind::NeverBuilt
                },
                count: (!rebuilding).then_some(1),
                offer: None,
                docs: Vec::new(),
            });
        }
    }
    document_fields(project, &mut page, &path);
    page.path = Some(path_text);
    page.key_chips = chips;
    Some(page)
}

/// The fields that depend on what kind of document the page summarizes.
fn document_fields(project: &Project, page: &mut ConceptPage, path: &Path) {
    let Some(doc) = project.tree.get(path) else {
        return;
    };
    match &doc.kind {
        DocKind::Plan { slug, state } => {
            page.plan_state = Some(plan_state_name(*state).to_owned());
            page.implements = Some(implements_view(project, slug));
            let text = read_doc(&project.root, path).ok();
            let dates = text.iter().flat_map(|text| pressure::records(text));
            page.last_pressure = dates.map(|record| record.date).max();
        }
        DocKind::ProductSection { number } => {
            page.implemented_by = implemented_by(project, *number);
        }
        _ => {}
    }
}
