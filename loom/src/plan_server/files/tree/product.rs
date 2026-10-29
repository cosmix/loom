//! Product visions, section order and gaps within each feature.

use super::*;

impl Builder<'_> {
    /// Group feature product documents and retain misplaced top-level documents.
    pub(super) fn product(&mut self) -> Vec<TreeEntry> {
        let mut result = Vec::new();
        for feature in self.project.features.clone() {
            result.extend(self.product_feature(&feature));
        }
        let misplaced: Vec<_> = self
            .project
            .docs
            .iter()
            .filter(|(_, doc)| {
                matches!(
                    doc.doc.kind,
                    DocKind::Vision { feature: None }
                        | DocKind::ProductSection { feature: None, .. }
                )
            })
            .map(|(path, _)| path.clone())
            .collect();
        for path in misplaced {
            if let Some(item) = self.document(&path, entry_kind(&classify(&path))) {
                result.push(item);
            }
        }
        group_features(result)
    }

    fn product_feature(&mut self, feature: &Feature) -> Vec<TreeEntry> {
        let mut result = Vec::new();
        if let Some(vision) = self.document(&vision_path(&feature.name), EntryKind::Vision) {
            result.push(vision);
        }
        let sections = self.product_sections(&feature.name);
        if let Ok(rows) = &feature.vision_rows {
            for row in rows {
                if let Some((_, path)) = sections.iter().find(|(n, _)| *n == row.number) {
                    if let Some(item) = self.document(path, EntryKind::Section) {
                        result.push(item);
                    }
                } else {
                    let mut gap = entry(None, &row.name, EntryKind::Section, false);
                    gap.feature = Some(feature.name.clone());
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

    fn product_sections(&self, feature: &str) -> Vec<(u32, PathBuf)> {
        self.project
            .docs
            .iter()
            .filter_map(|(path, doc)| match &doc.doc.kind {
                DocKind::ProductSection {
                    number,
                    feature: Some(owner),
                } if owner == feature => Some((*number, path.clone())),
                _ => None,
            })
            .collect()
    }
}
