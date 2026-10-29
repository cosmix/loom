//! Build feature-grouped file-mode navigation from the discovered project.

mod archive;
mod bundles;
mod knowledge;
mod plans;
mod product;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::plan_server::wire::files::{EntryKind, FileTree, PlanStateView, TreeEntry};
use crate::planning::kinds::{classify, plans_dir, vision_path, DocKind};
use crate::planning::project::{Feature, Project};
use crate::planning::slug::{plan_home, PlanEntry, PlanState};

use super::doc::{document_feature, entry_kind, is_markdown, misplaced};
use super::plan_view::latest_pressure_date;
use super::texts::TextStore;

/// The navigation tree of `project`, grouped by feature within each shelf. Text and
/// validation counts come from the stores; building the tree never reads a file.
pub fn build_tree(
    project: &Project,
    texts: &TextStore,
    errors: &HashMap<PathBuf, u32>,
) -> FileTree {
    let mut builder = Builder {
        project,
        texts,
        errors,
        used: HashSet::new(),
    };
    let product = builder.product();
    let plans = builder.plans();
    let knowledge = builder.knowledge();
    let archive = group_features(builder.archive());
    let other = project
        .docs
        .iter()
        .filter(|(path, _)| !builder.used.contains(*path) && is_markdown(path))
        .map(|(path, doc)| {
            entry(
                Some(path),
                &doc.title,
                entry_kind(&doc.doc.kind),
                doc.lock.is_some(),
            )
        })
        .collect();
    FileTree {
        product,
        plans,
        knowledge,
        other: group_features(other),
        archive,
    }
}

struct Builder<'a> {
    project: &'a Project,
    texts: &'a TextStore,
    errors: &'a HashMap<PathBuf, u32>,
    used: HashSet<PathBuf>,
}

impl Builder<'_> {
    fn document(&mut self, path: &Path, kind: EntryKind) -> Option<TreeEntry> {
        let doc = self.project.docs.get(path)?;
        self.used.insert(path.to_path_buf());
        Some(entry(Some(path), &doc.title, kind, doc.lock.is_some()))
    }

    /// The stored text of `path` when it is the text the project view parsed.
    fn text(&self, path: &Path) -> Option<Arc<str>> {
        let pinned = self.texts.current(path)?;
        (self.project.content_hash(path)? == pinned.hash).then_some(pinned.text)
    }

    fn plan(&mut self, plan: &PlanEntry) -> TreeEntry {
        let mut item = self
            .document(&plan.path, EntryKind::Plan)
            .unwrap_or_else(|| entry(Some(&plan.path), &plan.slug, EntryKind::Plan, false));
        item.state = Some(match plan.state {
            PlanState::Written => PlanStateView::Written,
            PlanState::Running => PlanStateView::Running,
            PlanState::Done => PlanStateView::Done,
            PlanState::Retired => PlanStateView::Retired,
        });
        item.pressure_date = self
            .text(&plan.path)
            .and_then(|text| latest_pressure_date(&text));
        item.errors = self.errors.get(&plan.path).copied().unwrap_or(0);
        if let Some(home) = plan_home(&plan.path) {
            item.children = self.bundle(&home.join("briefs").join(&plan.slug));
        }
        item
    }
}

fn group_features(entries: Vec<TreeEntry>) -> Vec<TreeEntry> {
    let mut groups: BTreeMap<String, Vec<TreeEntry>> = BTreeMap::new();
    let mut ungrouped = Vec::new();
    for item in entries {
        if let Some(feature) = &item.feature {
            groups.entry(feature.clone()).or_default().push(item);
        } else {
            ungrouped.push(item);
        }
    }
    let mut result: Vec<_> = groups
        .into_iter()
        .map(|(name, children)| {
            let mut group = entry(None, &name, EntryKind::Feature, false);
            group.feature = Some(name);
            group.children = children;
            group
        })
        .collect();
    result.extend(ungrouped);
    result
}

fn entry(path: Option<&Path>, title: &str, kind: EntryKind, locked: bool) -> TreeEntry {
    let doc_kind = path.map(classify);
    TreeEntry {
        path: path.map(|p| p.to_string_lossy().into_owned()),
        title: title.to_owned(),
        file_name: path
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        kind,
        feature: path
            .zip(doc_kind.as_ref())
            .and_then(|(path, kind)| document_feature(path, kind)),
        misplaced: doc_kind.as_ref().is_some_and(misplaced),
        state: None,
        locked,
        gap: false,
        pressure_date: None,
        errors: 0,
        blurb: None,
        children: Vec::new(),
    }
}
