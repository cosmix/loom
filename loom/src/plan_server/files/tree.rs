//! Build the file-mode navigation tree from the discovered project.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::plan_server::wire::files::{
    EntryKind, FileTree, PlanStateView, TreeEntry, ValidationView,
};
use crate::planning::sections::normalized_hash;
use crate::planning::slug::{PlanEntry, PlanState};
use crate::planning::{briefs, discover::read_doc, kinds::DocKind, pressure, project::Project};

use super::doc::entry_kind;
use super::knowledge_index::parse_index;

pub fn build_tree(project: &Project, validation: &HashMap<PathBuf, ValidationView>) -> FileTree {
    let mut builder = Builder {
        project,
        validation,
        used: HashSet::new(),
    };
    let product = builder.product();
    let plans = builder.plans();
    let knowledge = builder.knowledge();
    let archive = builder.archive();
    let other = project
        .docs
        .iter()
        .filter(|(path, _)| !builder.used.contains(*path))
        .filter(|(path, _)| is_markdown(path))
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
        other,
        archive,
    }
}

struct Builder<'a> {
    project: &'a Project,
    validation: &'a HashMap<PathBuf, ValidationView>,
    used: HashSet<PathBuf>,
}

impl Builder<'_> {
    fn document(&mut self, path: &Path, kind: EntryKind) -> Option<TreeEntry> {
        let doc = self.project.docs.get(path)?;
        self.used.insert(path.to_path_buf());
        Some(entry(Some(path), &doc.title, kind, doc.lock.is_some()))
    }

    fn text(&self, path: &Path) -> Option<String> {
        let text = read_doc(&self.project.root, path).ok()?;
        (self.project.content_hash(path)? == normalized_hash(&text)).then_some(text)
    }

    fn product(&mut self) -> Vec<TreeEntry> {
        let mut result = Vec::new();
        if let Some(vision) = self.document(Path::new("doc/product/index.md"), EntryKind::Vision) {
            result.push(vision);
        }
        let sections: Vec<_> = self
            .project
            .docs
            .iter()
            .filter_map(|(path, doc)| match doc.doc.kind {
                DocKind::ProductSection { number } => Some((number, path.clone())),
                _ => None,
            })
            .collect();
        if let Ok(rows) = self.project.vision_rows.clone() {
            for row in rows {
                if let Some((_, path)) = sections.iter().find(|(n, _)| *n == row.number) {
                    if let Some(item) = self.document(path, EntryKind::Section) {
                        result.push(item);
                    }
                } else {
                    let mut gap = entry(None, &row.name, EntryKind::Section, false);
                    gap.gap = true;
                    result.push(gap);
                }
            }
        }
        for (_, path) in sections {
            if !self.used.contains(&path) {
                if let Some(item) = self.document(&path, EntryKind::Section) {
                    result.push(item);
                }
            }
        }
        result
    }

    fn plans(&mut self) -> Vec<TreeEntry> {
        let mut result = Vec::new();
        let mut named = HashSet::new();
        for (path, rows) in self.project.metaplans.clone() {
            let Some(mut meta) = self.document(&path, EntryKind::Metaplan) else {
                continue;
            };
            for row in rows {
                let plan = row
                    .slug
                    .as_ref()
                    .and_then(|slug| self.project.tree.plans.get(slug))
                    .cloned();
                if let Some(plan) = plan {
                    named.insert(plan.slug.clone());
                    meta.children.push(self.plan(&plan));
                } else {
                    let mut gap = entry(None, &row.heading, EntryKind::MetaplanEntry, false);
                    gap.gap = true;
                    meta.children.push(gap);
                }
            }
            result.push(meta);
        }
        for plan in self
            .project
            .tree
            .plans
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            if !named.contains(&plan.slug)
                && matches!(plan.state, PlanState::Written | PlanState::Running)
            {
                result.push(self.plan(&plan));
            }
        }
        result
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
            .and_then(|text| pressure::records(&text).last().map(|r| r.date.clone()));
        item.errors = self
            .validation
            .get(&plan.path)
            .map_or(0, |v| v.errors.len().try_into().unwrap_or(u32::MAX));
        item.children = self.bundle(&plan.slug);
        item
    }

    fn bundle(&mut self, slug: &str) -> Vec<TreeEntry> {
        let base = Path::new("doc/plans/briefs").join(slug);
        let paths: Vec<_> = self
            .project
            .docs
            .keys()
            .filter(|p| p.starts_with(&base) && is_markdown(p))
            .cloned()
            .collect();
        let mut common = Vec::new();
        let mut folders: BTreeMap<String, Vec<TreeEntry>> = BTreeMap::new();
        let mut root = Vec::new();
        for path in paths {
            let relative = path.strip_prefix(&base).unwrap();
            let parts: Vec<_> = relative.components().collect();
            let folder =
                (parts.len() > 1).then(|| parts[0].as_os_str().to_string_lossy().into_owned());
            if let Some(item) = self.document(&path, EntryKind::Brief) {
                if let Some(folder) = folder {
                    folders.entry(folder).or_default().push(item);
                } else if relative == Path::new("common.md") {
                    common.push(item);
                } else {
                    root.push(item);
                }
            }
        }
        common.extend(root);
        for (name, children) in folders {
            let mut folder = entry(None, &name, EntryKind::BriefFolder, false);
            folder.children = children;
            common.push(folder);
        }
        common
    }

    fn knowledge(&mut self) -> Vec<TreeEntry> {
        let base = Path::new("doc/loom/knowledge");
        let index = base.join("INDEX.md");
        let topics = self
            .text(&index)
            .map(|text| parse_index(&text))
            .unwrap_or_default();
        let mut result = Vec::new();
        if let Some(item) = self.document(&index, EntryKind::Knowledge) {
            result.push(item);
        }
        let root_files: Vec<_> = self
            .project
            .docs
            .keys()
            .filter(|p| p.parent() == Some(base) && is_markdown(p) && **p != index)
            .cloned()
            .collect();
        for path in root_files {
            if let Some(item) = self.document(&path, EntryKind::Knowledge) {
                result.push(item);
            }
        }
        let mut folders: BTreeMap<String, Vec<TreeEntry>> = BTreeMap::new();
        for topic in topics {
            let path = base.join(&topic.path);
            if let Some(mut item) = self.document(&path, EntryKind::Knowledge) {
                item.blurb = Some(topic.blurb);
                folders.entry(topic.category).or_default().push(item);
            }
        }
        self.add_unindexed_knowledge(base, &mut folders);
        for (name, children) in folders {
            let mut folder = entry(None, &name, EntryKind::KnowledgeFolder, false);
            folder.children = children;
            result.push(folder);
        }
        result
    }

    /// Unindexed knowledge remains reachable, including newly created topics.
    fn add_unindexed_knowledge(
        &mut self,
        base: &Path,
        folders: &mut BTreeMap<String, Vec<TreeEntry>>,
    ) {
        let remaining: Vec<_> = self
            .project
            .docs
            .keys()
            .filter(|p| p.starts_with(base) && is_markdown(p) && !self.used.contains(*p))
            .cloned()
            .collect();
        for path in remaining {
            let category = path
                .strip_prefix(base)
                .ok()
                .and_then(|p| p.components().next())
                .map(|c| c.as_os_str().to_string_lossy().into_owned());
            if let (Some(category), Some(item)) =
                (category, self.document(&path, EntryKind::Knowledge))
            {
                folders.entry(category).or_default().push(item);
            }
        }
    }

    fn archive(&mut self) -> Vec<TreeEntry> {
        let mut result = Vec::new();
        for plan in self
            .project
            .tree
            .plans
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            if !self.used.contains(&plan.path)
                && matches!(plan.state, PlanState::Done | PlanState::Retired)
            {
                result.push(self.plan(&plan));
            }
        }
        let paths: Vec<_> = self
            .project
            .docs
            .keys()
            .filter(|p| {
                p.starts_with("doc/plans/archive") && is_markdown(p) && !self.used.contains(*p)
            })
            .cloned()
            .collect();
        for path in paths {
            let entry_kind = entry_kind(&self.project.docs[&path].doc.kind);
            if let Some(item) = self.document(&path, entry_kind) {
                result.push(item);
            }
        }
        for slug in briefs::orphan_bundles(&self.project.tree) {
            let mut folder = entry(None, &slug, EntryKind::BriefFolder, false);
            folder.children = self.bundle(&slug);
            if !folder.children.is_empty() {
                result.push(folder);
            }
        }
        result
    }
}

fn entry(path: Option<&Path>, title: &str, kind: EntryKind, locked: bool) -> TreeEntry {
    TreeEntry {
        path: path.map(|p| p.to_string_lossy().into_owned()),
        title: title.to_owned(),
        file_name: path
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        kind,
        state: None,
        locked,
        gap: false,
        pressure_date: None,
        errors: 0,
        blurb: None,
        children: Vec::new(),
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "md")
}
