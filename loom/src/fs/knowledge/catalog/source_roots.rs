//! Cargo-package source-root discovery for catalog source references.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Whether a backticked source reference names an existing project file.
///
/// The canonical spelling is project-relative (`crates/core/src/models/constants.rs`),
/// but knowledge prose can also use a module-relative Rust path
/// (`models/constants.rs`). The latter resolves only through a source root
/// declared by an actual Cargo package: the project package, an explicit
/// workspace member, or a direct-child package. The direct-child scan is
/// deliberately one level deep so a project with no root workspace manifest
/// can still host a single crate without turning this into an unbounded source
/// tree search.
///
/// A module-relative path must match **exactly one** declared package source
/// root. Multiple matches are ambiguous and remain a `MissingSourceRef`,
/// rather than silently choosing a crate by traversal order.
///
/// Callers have already rejected absolute and parent-relative paths, so no
/// candidate can escape its root.
pub(super) fn repository_source_path_exists(
    project_root: &Path,
    cargo_source_roots: &[PathBuf],
    source_path: &str,
) -> bool {
    fs::metadata(project_root.join(source_path)).is_ok()
        || cargo_source_roots
            .iter()
            .filter(|source_root| fs::metadata(source_root.join(source_path)).is_ok())
            .take(2)
            .count()
            == 1
}

/// Return the `src` directories of Cargo packages that can be established
/// from the repository root without recursively searching arbitrary paths.
///
/// Explicit workspace members cover conventional multi-crate repositories;
/// the direct-child scan supports repositories that use a single nested crate
/// but have no workspace manifest (including this repository). A directory is
/// accepted only when its own `Cargo.toml` parses and contains `[package]`.
pub(super) fn cargo_package_source_roots(project_root: &Path) -> Vec<PathBuf> {
    let mut package_roots = BTreeSet::new();
    insert_cargo_package_root(project_root, &mut package_roots);

    let root_manifest = project_root.join("Cargo.toml");
    for member in cargo_workspace_members(&root_manifest) {
        insert_cargo_package_root(&project_root.join(member), &mut package_roots);
    }

    if let Ok(entries) = fs::read_dir(project_root) {
        for entry in entries.filter_map(Result::ok) {
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                insert_cargo_package_root(&entry.path(), &mut package_roots);
            }
        }
    }

    package_roots
        .into_iter()
        .map(|package_root| package_root.join("src"))
        .collect()
}

/// Add `directory` when it is a concrete Cargo package directory.
fn insert_cargo_package_root(directory: &Path, package_roots: &mut BTreeSet<PathBuf>) {
    let manifest = directory.join("Cargo.toml");
    if cargo_manifest_has_package(&manifest) {
        package_roots.insert(directory.to_path_buf());
    }
}

/// Parse the root manifest's literal workspace-member paths. Globs and paths
/// that escape the root are intentionally ignored: expanding them would make
/// the diagnostic's accepted source roots depend on an unbounded filesystem
/// walk instead of on a finite, auditable manifest declaration.
fn cargo_workspace_members(manifest: &Path) -> Vec<PathBuf> {
    let Ok(content) = fs::read_to_string(manifest) else {
        return Vec::new();
    };
    let Ok(manifest) = toml::from_str::<toml::Value>(&content) else {
        return Vec::new();
    };
    manifest
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .filter_map(valid_workspace_member_path)
        .collect()
}

/// A literal workspace member path contained within its declaring project.
fn valid_workspace_member_path(member: &str) -> Option<PathBuf> {
    let path = Path::new(member);
    (!path.is_absolute()
        && !member.contains('*')
        && !member.contains('?')
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir)))
    .then(|| path.to_path_buf())
}

/// Whether `manifest` is a readable TOML Cargo manifest for one package.
fn cargo_manifest_has_package(manifest: &Path) -> bool {
    fs::read_to_string(manifest)
        .ok()
        .and_then(|content| toml::from_str::<toml::Value>(&content).ok())
        .is_some_and(|manifest| manifest.get("package").is_some_and(toml::Value::is_table))
}
