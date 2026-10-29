//! Knowledge folders and index ordering.

use super::*;
use crate::plan_server::files::knowledge_index::parse_index;

impl Builder<'_> {
    /// Build knowledge folders from the stored index and the discovered files.
    pub(super) fn knowledge(&mut self) -> Vec<TreeEntry> {
        let base = Path::new("doc/loom/knowledge");
        let index = base.join("INDEX.md");
        let mut result = Vec::new();
        if let Some(item) = self.document(&index, EntryKind::Knowledge) {
            result.push(item);
        }
        self.add_root_knowledge(base, &index, &mut result);
        let mut folders: BTreeMap<String, Vec<TreeEntry>> = BTreeMap::new();
        let topics = self
            .text(&index)
            .map(|text| parse_index(&text))
            .unwrap_or_default();
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

    fn add_root_knowledge(&mut self, base: &Path, index: &Path, result: &mut Vec<TreeEntry>) {
        let root_files: Vec<_> = self
            .project
            .docs
            .keys()
            .filter(|p| p.parent() == Some(base) && is_markdown(p) && p.as_path() != index)
            .cloned()
            .collect();
        for path in root_files {
            if let Some(item) = self.document(&path, EntryKind::Knowledge) {
                result.push(item);
            }
        }
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
}
