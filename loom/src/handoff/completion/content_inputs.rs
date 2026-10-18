//! Declared verification inputs override completion exemptions.
use crate::models::stage::Stage;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

pub(super) fn declared_input(stage: &Stage, path: &str) -> Result<bool> {
    for declared in declared_patterns(stage) {
        if matches_input(path, &declared)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `path` and its ancestors that a declaration names once its trailing
/// wildcard-only components are dropped: `packages/**` names `packages`, and
/// `packages/**/__pycache__/**` names every `__pycache__` below it. A `*`
/// stays within one component, so `src/*.json` never names `src/out/x.json`.
pub(super) fn named_inputs(stage: &Stage, path: &str) -> Result<BTreeSet<String>> {
    let options = glob::MatchOptions {
        require_literal_separator: true,
        ..glob::MatchOptions::new()
    };
    let mut named = BTreeSet::new();
    for declared in declared_patterns(stage) {
        let Some(prefix) = named_prefix(&declared) else {
            continue;
        };
        let pattern = glob::Pattern::new(&prefix.to_string_lossy())
            .context("invalid completion input pattern")?;
        for ancestor in Path::new(path).ancestors() {
            if !ancestor.as_os_str().is_empty() && pattern.matches_path_with(ancestor, options) {
                named.insert(ancestor.to_string_lossy().into_owned());
            }
        }
    }
    Ok(named)
}

/// Every declaration, normalized against both bases. Some declarations are
/// repository-relative, others are relative to working_dir; checking both is
/// conservative for exemptions.
fn declared_patterns(stage: &Stage) -> impl Iterator<Item = PathBuf> + '_ {
    let working_dir = stage.working_dir.as_deref().unwrap_or(".");
    stage
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
        .chain(stage.regression_test.iter().map(|test| test.file.as_str()))
        .flat_map(move |pattern| {
            [".", working_dir].map(|base| normalize(&Path::new(base).join(pattern)))
        })
}

fn matches_input(path: &str, declared: &Path) -> Result<bool> {
    let source = declared.to_string_lossy();
    let pattern = glob::Pattern::new(&source).context("invalid completion input pattern")?;
    // A declared directory includes its descendants, including ignored files.
    Ok(Path::new(path)
        .ancestors()
        .any(|ancestor| pattern.matches_path(ancestor)))
}

fn named_prefix(declared: &Path) -> Option<PathBuf> {
    let mut components: Vec<Component> = declared.components().collect();
    while components
        .last()
        .is_some_and(|last| last.as_os_str().to_str().is_some_and(is_wildcard))
    {
        components.pop();
    }
    (!components.is_empty()).then(|| components.iter().collect())
}

fn is_wildcard(component: &str) -> bool {
    component.chars().all(|ch| ch == '*')
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
