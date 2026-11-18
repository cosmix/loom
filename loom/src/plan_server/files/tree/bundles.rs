//! Plan brief bundles, preserving common-first ordering.

use super::*;

impl Builder<'_> {
    /// Build the Markdown children of one standalone or feature plan's bundle.
    pub(super) fn bundle(&mut self, base: &Path) -> Vec<TreeEntry> {
        let paths: Vec<_> = self
            .project
            .docs
            .keys()
            .filter(|p| p.starts_with(base) && is_markdown(p))
            .cloned()
            .collect();
        let mut common = Vec::new();
        let mut folders: BTreeMap<String, Vec<TreeEntry>> = BTreeMap::new();
        let mut root = Vec::new();
        for path in paths {
            let Ok(relative) = path.strip_prefix(base) else {
                continue;
            };
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
            folder.feature = children.first().and_then(|item| item.feature.clone());
            folder.children = children;
            common.push(folder);
        }
        common
    }
}
