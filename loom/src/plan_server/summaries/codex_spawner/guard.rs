//! The configuration guard of a Codex summary job: Codex must leave `.claude` and `.codex`
//! unchanged.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::plan_server::pressure::config_guard::{self, ConfigSnapshot};
use crate::plan_session::bridge::EndOutcome;

/// The failure that replaces a job's outcome when its configuration changed or cannot be
/// checked; `None` when the configuration is unchanged.
pub(super) fn guarded_outcome(
    root: &Path,
    before: &ConfigSnapshot,
    codex_before: &BTreeMap<PathBuf, String>,
) -> Option<EndOutcome> {
    let check = || -> Result<bool> {
        let after = config_guard::snapshot(root)?;
        Ok(!config_guard::changed(before, &after).is_empty()
            || *codex_before != codex_snapshot(root)?)
    };
    match check() {
        Ok(false) => None,
        Ok(true) => Some(EndOutcome::Failed(
            "codex changed the .claude or .codex configuration".into(),
        )),
        Err(error) => Some(EndOutcome::Failed(format!(
            "cannot check codex session configuration: {error:#}"
        ))),
    }
}

// The shared pressure guard watches Claude configuration only. Also fingerprint .codex,
// including symlink targets inside the repository, without reading through symlinks.
pub(super) fn codex_snapshot(root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let root = root.canonicalize()?;
    let mut files = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut pending = vec![PathBuf::from(".codex")];
    while let Some(path) = pending.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        if seen.len() > 100_000 {
            bail!(".codex configuration has too many entries");
        }
        let full = root.join(&path);
        let metadata = match std::fs::symlink_metadata(&full) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let fingerprint = if metadata.is_symlink() {
            let target = std::fs::read_link(&full)?;
            if let Ok(resolved) = full.canonicalize() {
                if let Ok(relative) = resolved.strip_prefix(&root) {
                    if relative.as_os_str().is_empty()
                        || relative.starts_with(".git")
                        || relative.starts_with(".loom")
                    {
                        bail!(".codex symlink serves changing repository state");
                    }
                    pending.push(relative.to_path_buf());
                }
            }
            format!("link:{target:?}")
        } else if metadata.is_dir() {
            for entry in std::fs::read_dir(full)? {
                pending.push(path.join(entry?.file_name()));
            }
            "directory".to_string()
        } else if metadata.is_file() {
            let bytes = crate::fs::safe_read::read_bounded(&root, &path, 4 * 1024 * 1024)?;
            crate::context::source_graph::body_hash(&bytes)
        } else {
            bail!("unsupported .codex configuration entry {}", path.display());
        };
        files.insert(path, fingerprint);
    }
    Ok(files)
}
