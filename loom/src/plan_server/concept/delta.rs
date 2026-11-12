//! Feature-scoped concept changes between watcher batches.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::links::{implemented_by, implements_view};
use super::page::{load_page, page_key_of, summary_file_hash, PageContext, PageKey, Scope};
use crate::plan_server::wire::concept::{
    ConceptChanged, ConceptPage, DocLink, ImplementsView, PlanLink,
};
use crate::planning::kinds::{feature_dir_name, is_feature_name};
use crate::planning::project::Project;
use crate::planning::summary::evaluate::{Evaluation, SummaryState};
use crate::planning::summary::store::{self, summary_id_for, STORE_DIR};
use crate::planning::watch::Batch;

/// The content and dependencies shown after a watcher batch.
#[derive(Debug)]
pub(crate) struct Snapshot {
    summaries: BTreeMap<String, (SummaryState, Option<String>)>,
    shown: BTreeMap<String, Shown>,
    dependencies: BTreeMap<String, Vec<String>>,
    features: BTreeMap<String, Option<String>>,
}

#[derive(Debug, PartialEq)]
enum Shown {
    KeyPage(Box<ConceptPage>),
    ImplementedBy(Vec<PlanLink>),
    Implements(ImplementsView),
}

/// Capture every feature's overview and document page dependencies.
pub(crate) fn snapshot(project: &Project, eval: &Evaluation, ctx: &PageContext) -> Snapshot {
    let summaries = eval
        .summaries
        .iter()
        .map(|(id, summary)| (id.clone(), (summary.state, summary_file_hash(project, id))))
        .collect();
    let mut result = Snapshot {
        summaries,
        shown: BTreeMap::new(),
        dependencies: BTreeMap::new(),
        features: BTreeMap::new(),
    };
    for feature in &project.features {
        let members = store::scope(project, &feature.name);
        snapshot_key_pages(project, eval, &feature.name, ctx, &mut result);
        for id in eval.summaries.keys() {
            let keys = page_keys(id, &members);
            if !keys.is_empty() {
                result.dependencies.insert(id.clone(), keys);
            }
        }
    }
    snapshot_documents(project, eval, &mut result);
    result
}

fn snapshot_key_pages(
    project: &Project,
    eval: &Evaluation,
    feature: &str,
    ctx: &PageContext,
    result: &mut Snapshot,
) {
    let keys = [
        PageKey::Overview {
            feature: feature.into(),
            scope: Scope::Product,
        },
        PageKey::Summary {
            feature: Some(feature.into()),
            scope: Scope::Metaplan,
            doc: "index".into(),
        },
    ];
    for key in keys {
        if let Some(page) = load_page(project, eval, &key, ctx) {
            result.features.insert(key.as_key(), Some(feature.into()));
            result
                .shown
                .insert(key.as_key(), Shown::KeyPage(Box::new(page)));
        }
    }
}

fn snapshot_documents(project: &Project, eval: &Evaluation, result: &mut Snapshot) {
    for id in eval.summaries.keys() {
        let Some(key) = page_key_of(project, id) else {
            continue;
        };
        let feature = PageKey::parse(&key).and_then(|key| key.feature().map(str::to_owned));
        result.features.insert(key.clone(), feature);
        result
            .dependencies
            .entry(id.clone())
            .or_insert_with(|| vec![key.clone()]);
        if let Some(fields) = document_fields(project, id) {
            result.shown.insert(key, fields);
        }
    }
}

fn document_fields(project: &Project, id: &str) -> Option<Shown> {
    if let Some((feature, section)) = id.split_once("/product/section_") {
        let number = section.parse::<u32>().ok()?;
        return Some(Shown::ImplementedBy(implemented_by(
            project, feature, number,
        )));
    }
    Some(Shown::Implements(implements_view(
        project,
        id.strip_prefix("plans/")?,
    )))
}

/// Compare snapshots and emit one payload per affected feature (or standalone documents).
pub(crate) fn delta(
    project: &Project,
    before: &Snapshot,
    after: &Snapshot,
    batch: &Batch,
) -> Vec<ConceptChanged> {
    let (mut pages, out_of_date) = summary_changes(before, after);
    let keys: BTreeSet<_> = before.shown.keys().chain(after.shown.keys()).collect();
    pages.extend(
        keys.into_iter()
            .filter(|key| before.shown.get(*key) != after.shown.get(*key))
            .cloned(),
    );
    pages.extend(key_row_pages(before, after));
    pages.extend(
        batch_paths(batch).filter_map(|path| page_key_of(project, &summary_id_for(project, path)?)),
    );
    group_changes(before, after, pages, out_of_date, batch)
}

fn summary_changes(before: &Snapshot, after: &Snapshot) -> (BTreeSet<String>, BTreeSet<String>) {
    let ids: BTreeSet<_> = before
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
        for snapshot in [before, after] {
            pages.extend(snapshot.dependencies.get(id).into_iter().flatten().cloned());
        }
        if new.is_some_and(|(state, _)| is_pending(*state))
            && old.map(|(state, _)| *state) != new.map(|(state, _)| *state)
        {
            out_of_date.insert(id.clone());
        }
    }
    (pages, out_of_date)
}

fn group_changes(
    before: &Snapshot,
    after: &Snapshot,
    pages: BTreeSet<String>,
    out_of_date: BTreeSet<String>,
    batch: &Batch,
) -> Vec<ConceptChanged> {
    let mut groups = source_groups(batch);
    for page in pages {
        let feature = after
            .features
            .get(&page)
            .or_else(|| before.features.get(&page))
            .cloned()
            .flatten();
        group(&mut groups, feature).pages.push(page);
    }
    for id in out_of_date {
        let own_page = after
            .dependencies
            .get(&id)
            .or_else(|| before.dependencies.get(&id))
            .and_then(|keys| keys.first());
        let feature = own_page
            .and_then(|page| {
                after
                    .features
                    .get(page)
                    .or_else(|| before.features.get(page))
            })
            .cloned()
            .flatten();
        group(&mut groups, feature).out_of_date.push(id);
    }
    groups.into_values().collect()
}

fn group(
    groups: &mut BTreeMap<Option<String>, ConceptChanged>,
    feature: Option<String>,
) -> &mut ConceptChanged {
    groups
        .entry(feature.clone())
        .or_insert_with(|| ConceptChanged {
            feature,
            pages: Vec::new(),
            out_of_date: Vec::new(),
            sources: Vec::new(),
        })
}

fn key_row<'a>(snapshot: &'a Snapshot, key: &str) -> Vec<(&'a DocLink, u8, &'a str, bool)> {
    let Some(Shown::KeyPage(page)) = snapshot.shown.get(key) else {
        return Vec::new();
    };
    page.key_chips
        .iter()
        .map(|chip| (&chip.doc, chip.colour, chip.label.as_str(), chip.no_plan))
        .collect()
}

/// A changed key row invalidates the listed and unlisted pages of that feature and scope.
fn key_row_pages(before: &Snapshot, after: &Snapshot) -> Vec<String> {
    let keys: BTreeSet<_> = before.shown.keys().chain(after.shown.keys()).collect();
    let mut pages = BTreeSet::new();
    for key in keys {
        if key_row(before, key) == key_row(after, key) {
            continue;
        }
        let (feature, scope) = match PageKey::parse(key) {
            Some(PageKey::Overview { feature, scope }) => (feature, scope),
            Some(PageKey::Summary {
                feature: Some(feature),
                scope: Scope::Metaplan,
                ..
            }) => (feature, Scope::Engineering),
            _ => continue,
        };
        for key in before.features.keys().chain(after.features.keys()) {
            if matches!(PageKey::parse(key), Some(PageKey::Summary { feature: Some(ref f), scope: s, .. })
                if *f == feature && (s == scope
                    || (scope == Scope::Engineering && s == Scope::Metaplan)))
            {
                pages.insert(key.clone());
            }
        }
    }
    pages.into_iter().collect()
}

fn is_pending(state: SummaryState) -> bool {
    matches!(
        state,
        SummaryState::Stale | SummaryState::Incomplete | SummaryState::Rebuilding
    )
}

/// Source-only payloads when the engine cannot produce an evaluation.
pub(crate) fn source_deltas(batch: &Batch) -> Vec<ConceptChanged> {
    source_groups(batch).into_values().collect()
}

fn source_groups(batch: &Batch) -> BTreeMap<Option<String>, ConceptChanged> {
    let mut groups = BTreeMap::new();
    for path in sources(batch) {
        let feature = feature_dir_name(std::path::Path::new(&path))
            .filter(|name| is_feature_name(name))
            .map(str::to_owned);
        group(&mut groups, feature).sources.push(path);
    }
    groups
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

fn batch_paths(batch: &Batch) -> impl Iterator<Item = &PathBuf> {
    let moved = batch.renamed.iter().flat_map(|(from, to)| [from, to]);
    batch
        .added
        .iter()
        .chain(&batch.changed)
        .chain(&batch.removed)
        .chain(moved)
}

/// Pages that show summary `id` within this feature's scope. Unknown plans are excluded.
pub(crate) fn page_keys(id: &str, scope: &store::Scope) -> Vec<String> {
    if store::is_overview(id) {
        return PageKey::parse(id)
            .filter(|key| key.feature() == Some(scope.feature.as_str()))
            .map(|key| vec![key.as_key()])
            .unwrap_or_default();
    }
    if store::metaplan_feature(id) == Some(scope.feature.as_str()) {
        return vec![id.to_owned()];
    }
    if id.starts_with(&format!("{}/product/", scope.feature)) {
        let mut keys = vec![id.to_owned()];
        if scope.product.iter().any(|member| member == id) {
            keys.push(format!("overview/{}/product", scope.feature));
        }
        return keys;
    }
    if let Some(slug) = id.strip_prefix("plans/") {
        if scope
            .engineering
            .iter()
            .chain(&scope.outside)
            .any(|member| member == id)
        {
            return vec![format!("{}/engineering/{slug}", scope.feature)];
        }
    }
    Vec::new()
}
