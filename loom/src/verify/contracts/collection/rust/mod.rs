//! Rust test collection over a [`FileSource`]: whether a contract file is
//! compiled into a libtest test target of its package (the chain check), the
//! scaffold a contract writer may add so it is, and the strict check of that
//! scaffold. Paths are relative to the source root and `/`-separated.
//!
//! `targets` models the package's `Cargo.toml`, `items` parses the `mod`
//! items of one file, `chain` walks the module chain from a target root to
//! the contract file, and `scaffold` derives and checks the scaffold.

mod chain;
mod items;
mod scaffold;
mod targets;

use super::{FileSource, ScaffoldEdit};

/// What a build without tree-sitter reports instead of a verdict (fail
/// closed: the chain cannot be proven without parsing Rust).
const NO_SOURCE_GRAPH: &str =
    "cannot verify Rust contract collection: loom was built without the source-graph feature";

/// Problems with the chain that compiles `contract_file` as `test`: empty
/// when some libtest test target of the owning package reaches the file
/// through plain `mod` declarations. The file itself need not exist.
pub fn chain_problems(source: FileSource<'_>, contract_file: &str, test: &str) -> Vec<String> {
    if !cfg!(feature = "source-graph") {
        return vec![NO_SOURCE_GRAPH.to_string()];
    }
    chain::problems(source, contract_file, test)
}

/// The merged scaffold that makes every `(file, test)` contract part of a
/// test target, derived from `base` alone.
pub fn derive_scaffold(
    base: FileSource<'_>,
    contracts: &[(&str, &str)],
) -> Result<Vec<ScaffoldEdit>, String> {
    if !cfg!(feature = "source-graph") {
        return Err(NO_SOURCE_GRAPH.to_string());
    }
    scaffold::derive(base, contracts)
}

/// Every way `current` departs from `base` beyond the scaffold `edits`
/// allow (the strict diff of Decision 2).
pub fn scaffold_problems(
    edits: &[ScaffoldEdit],
    base: FileSource<'_>,
    current: FileSource<'_>,
) -> Vec<String> {
    scaffold::problems(edits, base, current)
}

/// Empty when the nearest `Cargo.toml` at or above `run_dir` ("" or "." is the
/// source root) is the manifest of the package that owns `contract_file`;
/// otherwise one problem naming both directories.
pub fn run_dir_problems(source: FileSource<'_>, contract_file: &str, run_dir: &str) -> Vec<String> {
    targets::run_dir_problems(source, contract_file, run_dir)
}

/// `name` inside the directory `dir` ("" is the source root).
pub(super) fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// The directory holding `path`, "" for a path at the source root.
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// `path` `/`-joined with empty and `.` components dropped ("" for the
/// source root); `None` for an absolute path or one with a `..` component.
fn clean(path: &str) -> Option<String> {
    if path.starts_with('/') {
        return None;
    }
    let parts: Vec<&str> = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    (!parts.contains(&"..")).then(|| parts.join("/"))
}

/// `contract_file` as a plain path beneath the source root.
fn contract_path(contract_file: &str) -> Result<String, String> {
    match clean(contract_file) {
        Some(file) if !file.is_empty() => Ok(file),
        _ => Err(format!(
            "contract file {contract_file} is not a plain relative path"
        )),
    }
}

/// `text` on one line, whitespace runs collapsed and cut at 120 characters,
/// for quoting source in a problem.
fn quote(text: &str) -> String {
    const LIMIT: usize = 120;
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= LIMIT {
        return line;
    }
    let cut: String = line.chars().take(LIMIT).collect();
    format!("{cut}...")
}

/// An in-memory source tree for the unit tests: `path -> content`.
#[cfg(test)]
fn tree(entries: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let files: std::collections::BTreeMap<String, String> = entries
        .iter()
        .map(|(path, content)| ((*path).to_string(), (*content).to_string()))
        .collect();
    move |path: &str| files.get(path).cloned()
}
