//! Done plans, archived documents and orphaned brief bundles.

use super::*;

impl Builder<'_> {
    /// Collect archived documents and brief bundles not displayed with a live plan.
    pub(super) fn archive(&mut self) -> Vec<TreeEntry> {
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
            .filter(|path| is_markdown(path) && !self.used.contains(*path))
            .filter(|path| {
                let feature = document_feature(path, &classify(path));
                path.starts_with(plans_dir(feature.as_deref()).join("archive"))
            })
            .cloned()
            .collect();
        for path in paths {
            if let Some(item) = self.document(&path, entry_kind(&classify(&path))) {
                result.push(item);
            }
        }
        result.extend(self.orphan_bundles());
        result
    }

    fn orphan_bundles(&mut self) -> Vec<TreeEntry> {
        let mut bundles = BTreeMap::new();
        for (path, doc) in &self.project.docs {
            if self.used.contains(path) {
                continue;
            }
            if let DocKind::Brief { slug } = &doc.doc.kind {
                let feature = document_feature(path, &doc.doc.kind);
                let base = plans_dir(feature.as_deref()).join("briefs").join(slug);
                bundles.insert(base, (slug.clone(), feature));
            }
        }
        let mut result = Vec::new();
        for (base, (slug, feature)) in bundles {
            let mut folder = entry(None, &slug, EntryKind::BriefFolder, false);
            folder.feature = feature;
            folder.children = self.bundle(&base);
            if !folder.children.is_empty() {
                result.push(folder);
            }
        }
        result
    }
}
