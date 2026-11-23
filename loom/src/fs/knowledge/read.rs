//! The one bounded read every knowledge-tree consumer goes through.

use anyhow::{Context, Result};
use std::path::Path;

/// Largest knowledge file read into memory. A tier-2 file is capped at 400
/// lines by the size rules, so a file over this is already broken and fails
/// the read instead of being skipped. Revisit if a legitimate oversized
/// knowledge file appears.
pub(crate) const MAX_KNOWLEDGE_FILE_BYTES: usize = 1024 * 1024;

/// Read `relative` beneath the knowledge `root` without following symlinks,
/// refusing anything that is not a regular file or exceeds
/// [`MAX_KNOWLEDGE_FILE_BYTES`]. The root is the trusted anchor and is resolved
/// first, so a knowledge directory that is itself a symlink still reads; only
/// entries beneath it are refused when they are symlinks.
pub(crate) fn read_knowledge_file(root: &Path, relative: &Path) -> Result<Vec<u8>> {
    let anchor = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    crate::fs::safe_read::read_bounded(&anchor, relative, MAX_KNOWLEDGE_FILE_BYTES).with_context(
        || {
            format!(
                "Failed to read knowledge file: {}",
                root.join(relative).display()
            )
        },
    )
}
