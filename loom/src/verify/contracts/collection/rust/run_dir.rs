//! Where cargo runs a contract's tests: the run directory must select the
//! package that owns the contract file.

use super::targets::{ancestors, no_package, package_of, shown};
use super::{clean, contract_path, join, FileSource};

/// Problems with running `contract_file`'s tests from `run_dir`: cargo picks
/// the manifest nearest the directory it runs in, and from a workspace root
/// above the package it compiles `default-members` or every member, so a
/// same-named test in another member could stand in for the frozen one.
pub(super) fn run_dir_problems(
    source: FileSource<'_>,
    contract_file: &str,
    run_dir: &str,
) -> Vec<String> {
    let file = match contract_path(contract_file) {
        Ok(file) => file,
        Err(problem) => return vec![problem],
    };
    let Some(package) = package_of(source, &file) else {
        return vec![no_package(&file)];
    };
    let Some(run) = clean(run_dir) else {
        return vec![format!(
            "run directory {run_dir} is not a plain relative path"
        )];
    };
    let want = shown(&package.dir);
    let nearest = ancestors(&run).find(|dir| source(&join(dir, "Cargo.toml")).is_some());
    match nearest {
        Some(dir) if dir == package.dir && package.selects_itself() => Vec::new(),
        Some(dir) => vec![format!(
            "cargo runs contract tests in {}, whose manifest {} does not select package {want}; \
             set the contract's cwd to {want}",
            shown(&run),
            join(dir, "Cargo.toml")
        )],
        None => vec![format!(
            "cargo runs contract tests in {}, which has no Cargo.toml at or above it; \
             set the contract's cwd to {want}",
            shown(&run)
        )],
    }
}
