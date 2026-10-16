//! Declared verification inputs override generated-output exemptions.
use crate::models::stage::Stage;
use anyhow::{Context, Result};
use std::path::{Component, Path, PathBuf};

pub(super) fn declared_input(stage: &Stage, path: &str) -> Result<bool> {
    let working_dir = stage.working_dir.as_deref().unwrap_or(".");
    let patterns = stage
        .files
        .iter()
        .chain(&stage.artifacts)
        .chain(&stage.harness)
        .chain(&stage.ratchet_files)
        .map(String::as_str)
        .chain(
            stage
                .contracts
                .iter()
                .map(|contract| contract.file.as_str()),
        )
        .chain(stage.wiring.iter().map(|check| check.source.as_str()))
        .chain(stage.regression_test.iter().map(|test| test.file.as_str()));
    for pattern in patterns {
        // Some declarations are repository-relative, others are relative to
        // working_dir. Checking both is conservative for output exemptions.
        for base in [".", working_dir] {
            if matches_input(path, &normalize(&Path::new(base).join(pattern)))? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn matches_input(path: &str, declared: &Path) -> Result<bool> {
    let source = declared.to_string_lossy();
    let pattern = glob::Pattern::new(&source).context("invalid completion input pattern")?;
    // A declared directory includes its descendants, including ignored files.
    Ok(Path::new(path)
        .ancestors()
        .any(|ancestor| pattern.matches_path(ancestor)))
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
