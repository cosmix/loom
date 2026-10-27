//! The `concept_changed` delta: which pages changed between two batches, which summaries went
//! out of date, and which source paths the batch touched.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::plan_server::concept::links::{implemented_by, implements_view};
use crate::plan_server::concept::page::{
    load_page, page_key_of, summary_file_hash, PageContext, PageKey, Scope,
};
use crate::plan_server::wire::concept::{
    ConceptChanged, ConceptPage, DocLink, ImplementsView, PlanLink,
};
use crate::planning::project::Project;
use crate::planning::summary::evaluate::{Evaluation, SummaryState};
use crate::planning::summary::store::{self, summary_id_for, STORE_DIR};
use crate::planning::watch::Batch;

/// What the pages showed after one batch, compared with the next batch's to name the pages
/// whose content changed.
#[derive(Debug)]
pub(crate) struct Snapshot {
    /// Per summary id: its state and the content hash of its summary file.
    summaries: BTreeMap<String, (SummaryState, Option<String>)>,
    /// Per page key: what the page shows beyond its own summary.
    shown: BTreeMap<String, Shown>,
}

/// Page content that does not follow from one summary's state and file.
#[derive(Debug, PartialEq)]
enum Shown {
    /// An overview page whole: its key chips, `unlisted` list and empty state list documents
    /// that are added, removed or retitled without any summary changing.
    Overview(Box<ConceptPage>),
    /// A product section page's "Implemented by" plans, which follow the plans' `Implements:`
    /// lines.
    ImplementedBy(Vec<PlanLink>),
    /// A plan page's `Implements:` links, which carry the sections' titles.
    Implements(ImplementsView),
}

pub(crate) fn snapshot(project: &Project, eval: &Evaluation, ctx: &PageContext) -> Snapshot {
    let summaries = eval
        .summaries
        .iter()
        .map(|(id, summary)| {
            let hash = summary_file_hash(project, id);
            (id.clone(), (summary.state, hash))
        })
        .collect();
    let mut shown = BTreeMap::new();
    for scope in [Scope::Product, Scope::Engineering] {
        let key = PageKey::Overview(scope);
        if let Some(page) = load_page(project, eval, &key, ctx) {
            shown.insert(key.as_key(), Shown::Overview(Box::new(page)));
        }
    }
    for id in eval.summaries.keys() {
        if let Some(fields) = document_fields(project, id) {
            shown.insert(page_key_of(id), fields);
        }
    }
    Snapshot { summaries, shown }
}

/// What summary `id`'s page shows from documents other than its own source.
fn document_fields(project: &Project, id: &str) -> Option<Shown> {
    if let Some(number) = id.strip_prefix("product/section_") {
        let number = number.parse::<u32>().ok()?;
        return Some(Shown::ImplementedBy(implemented_by(project, number)));
    }
    let slug = id.strip_prefix("plans/")?;
    Some(Shown::Implements(implements_view(project, slug)))
}

pub(crate) fn delta(
    project: &Project,
    before: &Snapshot,
    after: &Snapshot,
    batch: &Batch,
) -> ConceptChanged {
    let scope = store::scope(project);
    let ids: BTreeSet<&String> = before
        .summaries
        .keys()
        .chain(after.summaries.keys())
        .collect();
    let mut pages = BTreeSet::new();
    let mut out_of_date = BTreeSet::new();
    for id in ids {
        let (old, new) = (before.summaries.get(id), after.summaries.get(id));
        if old == new {
            continue;
        }
        pages.extend(page_keys(id, &scope));
        let became = new.is_some_and(|(state, _)| is_pending(*state));
        if became && old.map(|(state, _)| *state) != new.map(|(state, _)| *state) {
            out_of_date.insert(id.clone());
        }
    }
    let keys: BTreeSet<&String> = before.shown.keys().chain(after.shown.keys()).collect();
    let moved = keys
        .into_iter()
        .filter(|key| before.shown.get(*key) != after.shown.get(*key));
    pages.extend(moved.cloned());
    pages.extend(key_row_pages(before, after));
    // A page shows its source's title, plan state and pressure dates whatever its summary does.
    let touched = batch_paths(batch).filter_map(|path| summary_id_for(project, path));
    pages.extend(touched.map(|id| page_key_of(&id)));
    ConceptChanged {
        pages: pages.into_iter().collect(),
        out_of_date: out_of_date.into_iter().collect(),
        sources: sources(batch),
    }
}

/// An overview's key row without the out-of-date marks: the chips' documents, colours, labels
/// and `no_plan` flags.
fn key_row<'a>(snapshot: &'a Snapshot, overview: &str) -> Vec<(&'a DocLink, u8, &'a str, bool)> {
    let Some(Shown::Overview(page)) = snapshot.shown.get(overview) else {
        return Vec::new();
    };
    let chips = page.key_chips.iter();
    chips
        .map(|chip| (&chip.doc, chip.colour, chip.label.as_str(), chip.no_plan))
        .collect()
}

/// Every summary page of a scope whose key row changed, in either snapshot, listed in the row
/// or not: each of them embeds the scope's key chips and takes its own colour from them, so a
/// document added to, removed from, retitled or recoloured in the row is stale on all of its
/// siblings, including the unlisted summaries (`product/` ids for the product scope, `plans/`
/// ids for engineering). The out-of-date marks stay out of the comparison: the client dashes
/// those from `out_of_date`, and a rebuild names the pages that changed.
fn key_row_pages(before: &Snapshot, after: &Snapshot) -> Vec<String> {
    [("product", "product/"), ("engineering", "plans/")]
        .into_iter()
        .filter(|(overview, _)| key_row(before, overview) != key_row(after, overview))
        .flat_map(|(_, prefix)| {
            [before, after].into_iter().flat_map(move |snapshot| {
                let ids = snapshot.summaries.keys();
                ids.filter(move |id| id.starts_with(prefix))
                    .map(|id| page_key_of(id))
            })
        })
        .collect()
}

fn is_pending(state: SummaryState) -> bool {
    matches!(
        state,
        SummaryState::Stale | SummaryState::Incomplete | SummaryState::Rebuilding
    )
}

/// Every batch path under `doc/` outside the summary store, sorted.
pub(crate) fn sources(batch: &Batch) -> Vec<String> {
    batch_paths(batch)
        .filter(|path| path.starts_with("doc") && !path.starts_with(STORE_DIR))
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Every path the batch names, renames by both ends.
fn batch_paths(batch: &Batch) -> impl Iterator<Item = &PathBuf> {
    let moved = batch.renamed.iter().flat_map(|(from, to)| [from, to]);
    batch
        .added
        .iter()
        .chain(&batch.changed)
        .chain(&batch.removed)
        .chain(moved)
}

/// The page keys whose content depends on summary `id`: its own page and, when it is a member
/// of an overview's scope, that overview.
pub(crate) fn page_keys(id: &str, scope: &store::Scope) -> Vec<String> {
    let (page, overview, members) = if id == "product" || id == "engineering" {
        return vec![id.to_owned()];
    } else if id.starts_with("product/") {
        (id.to_owned(), "product", &scope.product)
    } else if id.starts_with("plans/") {
        (page_key_of(id), "engineering", &scope.engineering)
    } else {
        return Vec::new();
    };
    let mut keys = vec![page];
    if members.iter().any(|member| member == id) {
        keys.push(overview.to_owned());
    }
    keys
}
