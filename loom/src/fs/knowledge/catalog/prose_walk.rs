//! No-follow traversal for best-effort project prose discovery.

use super::prose_lifecycle;
use crate::fs::safe_read::{list_dir_no_follow, EntryKind};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

/// Collect project-relative markdown paths beneath an already validated root.
/// Every directory is reopened from the project descriptor, so a replacement
/// symlink cannot redirect traversal outside the project.
pub(super) fn collect_prose_files(
    project_fd: &OwnedFd,
    dir: &Path,
    knowledge_relative: Option<&Path>,
    out: &mut Vec<PathBuf>,
) {
    // Curated knowledge already has its own catalog identity and path base.
    if knowledge_relative.is_some_and(|knowledge| dir.starts_with(knowledge)) {
        return;
    }
    let entries = match list_dir_no_follow(project_fd, dir) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::debug!(dir = %dir.display(), %error, "skipping unreadable prose directory");
            return;
        }
    };
    for (name, kind) in entries {
        let display_name = name.to_string_lossy();
        if display_name.starts_with('.') {
            continue;
        }
        let relative = dir.join(&name);
        match kind {
            EntryKind::Directory => {
                collect_prose_files(project_fd, &relative, knowledge_relative, out);
            }
            EntryKind::File if display_name.ends_with(".md") => {
                if prose_lifecycle(&relative, None).is_some() {
                    out.push(normalize_relative(&relative));
                }
            }
            EntryKind::File | EntryKind::Symlink | EntryKind::Special => {}
        }
    }
}

fn normalize_relative(relative: &Path) -> PathBuf {
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
        .into()
}
