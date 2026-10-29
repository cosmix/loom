//! Feature metaplans, their listed plans, and standalone plans.

use super::*;
use crate::planning::metaplan::MetaplanEntry;

impl Builder<'_> {
    /// Preserve each feature's metaplan order and add unlisted active plans.
    pub(super) fn plans(&mut self) -> Vec<TreeEntry> {
        let mut result = Vec::new();
        for feature in self.project.features.clone() {
            for (path, rows) in &feature.metaplans {
                if let Some(mut meta) = self.document(path, EntryKind::Metaplan) {
                    for row in rows {
                        meta.children.push(self.metaplan_row(&feature.name, row));
                    }
                    result.push(meta);
                }
            }
        }
        let misplaced: Vec<_> = self
            .project
            .docs
            .iter()
            .filter(|(_, doc)| matches!(doc.doc.kind, DocKind::Metaplan { feature: None }))
            .map(|(path, _)| path.clone())
            .collect();
        for path in misplaced {
            if let Some(item) = self.document(&path, EntryKind::Metaplan) {
                result.push(item);
            }
        }
        for plan in self
            .project
            .tree
            .plans
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            if !self.used.contains(&plan.path)
                && matches!(plan.state, PlanState::Written | PlanState::Running)
            {
                result.push(self.plan(&plan));
            }
        }
        group_features(result)
    }

    fn metaplan_row(&mut self, feature: &str, row: &MetaplanEntry) -> TreeEntry {
        let plan = row
            .slug
            .as_ref()
            .and_then(|slug| self.project.tree.plans.get(slug))
            .filter(|plan| plan.feature.as_deref() == Some(feature))
            .cloned();
        if let Some(plan) = plan {
            return self.plan(&plan);
        }
        let mut gap = entry(None, &row.heading, EntryKind::MetaplanEntry, false);
        gap.feature = Some(feature.to_owned());
        gap.gap = true;
        gap
    }
}
