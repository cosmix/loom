//! Record of the directories the last bare install placed loom's Claude and
//! Codex assets into.
//!
//! `default_paths` falls back to `~/.claude` / `~/.codex` whenever neither
//! env override is set, with no memory of where an earlier install actually
//! went. A machine set up with `LOOM_CLAUDECODE_INSTALL_DIR` /
//! `LOOM_CODEX_INSTALL_DIR` exported only for the initial install (as
//! `install.sh` does) would then have `loom update` write into the defaults
//! instead of the tree it maintains. This module persists the roots a bare
//! install resolved to, at `<home>/.config/loom/install-roots.toml`, so a
//! later bare install reuses them.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::InstallPaths;

const RECORD_PATH: &str = ".config/loom/install-roots.toml";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    claude_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_dir: Option<String>,
}

/// The Claude and Codex directories recorded under `home`, resolved
/// independently.
///
/// A missing or unreadable file, malformed TOML, a missing key, or an empty
/// path string all yield `None` for that directory rather than an error: the
/// caller falls back to its own default exactly as if nothing had ever been
/// recorded.
pub(super) fn read(home: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let Ok(content) = fs::read_to_string(home.join(RECORD_PATH)) else {
        return (None, None);
    };
    let Ok(record) = toml::from_str::<Record>(&content) else {
        return (None, None);
    };
    (non_empty(record.claude_dir), non_empty(record.codex_dir))
}

fn non_empty(value: Option<String>) -> Option<PathBuf> {
    value.filter(|path| !path.is_empty()).map(PathBuf::from)
}

/// Persist `paths` under `home` as the roots a future bare install should
/// resolve to.
pub(super) fn write(home: &Path, paths: &InstallPaths) -> Result<()> {
    let record = Record {
        claude_dir: Some(path_to_string(&paths.claude_dir)?),
        codex_dir: Some(path_to_string(&paths.codex_dir)?),
    };
    let rendered = toml::to_string(&record).context("Failed to serialize install roots")?;
    let path = home.join(RECORD_PATH);
    let parent = path
        .parent()
        .context("Install roots path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("Failed to create {}", parent.display()))?;
    fs::write(&path, format!("# Managed by loom\n{rendered}"))
        .with_context(|| format!("Failed to write {}", path.display()))
}

/// Render `path` absolute, so a relative override given at install time still
/// names the same tree when `loom update` runs from another directory.
fn path_to_string(path: &Path) -> Result<String> {
    let path = std::path::absolute(path)
        .with_context(|| format!("Failed to resolve {}", path.display()))?;
    path.to_str()
        .map(str::to_string)
        .with_context(|| format!("{} is not valid UTF-8", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn round_trips_paths_with_spaces_and_quotes() {
        let temp = TempDir::new().unwrap();
        let paths = InstallPaths {
            claude_dir: temp.path().join("with space").join(r#"a "quote""#),
            codex_dir: temp.path().join("codex root"),
        };

        write(temp.path(), &paths).unwrap();
        let (claude_dir, codex_dir) = read(temp.path());

        assert_eq!(claude_dir, Some(paths.claude_dir));
        assert_eq!(codex_dir, Some(paths.codex_dir));
    }

    #[test]
    fn missing_file_yields_no_recorded_directories() {
        let temp = TempDir::new().unwrap();
        assert_eq!(read(temp.path()), (None, None));
    }

    #[test]
    fn malformed_toml_yields_no_recorded_directories() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(RECORD_PATH);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "this is not valid toml =").unwrap();

        assert_eq!(read(temp.path()), (None, None));
    }

    #[test]
    fn an_empty_string_value_yields_none_for_that_directory() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(RECORD_PATH);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "claude_dir = \"\"\ncodex_dir = \"/home/user/.codex\"\n",
        )
        .unwrap();

        let (claude_dir, codex_dir) = read(temp.path());
        assert_eq!(claude_dir, None);
        assert_eq!(codex_dir, Some(PathBuf::from("/home/user/.codex")));
    }
}
