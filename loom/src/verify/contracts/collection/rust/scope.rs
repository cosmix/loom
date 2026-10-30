//! The one cargo target that compiles a contract's test file, so the
//! contract runs with `--lib`, `--bin <name>` or `--test <name>` and builds
//! nothing else. Unscoped, `cargo test` compiles every target of the package
//! and a contract goes red through some other target's compile error.

use std::path::Path;

use super::chain::{candidates, walk};
use super::run_dir::run_dir_problems;
use super::targets::{package_of, Kind};
use super::{contract_path, FileSource};
use crate::testrun::TargetScope;

/// The target compiling `contract_file` as `test`, or `None` whenever it is
/// not certain: no owning package, a run directory cargo would not run that
/// package from, a doctest name, a chain that does not parse or does not
/// reach the file, or several targets reaching it. A wrong target makes cargo
/// exit 101, which would read as a build failure and pass the freeze, so an
/// unresolved scope keeps the unscoped command instead of guessing.
pub(super) fn target_scope(
    source: FileSource<'_>,
    contract_file: &str,
    test: &str,
    run_dir: &str,
) -> Option<TargetScope> {
    if !cfg!(feature = "source-graph") || is_doctest(test) {
        return None;
    }
    let file = contract_path(contract_file).ok()?;
    if !run_dir_problems(source, &file, run_dir).is_empty() {
        return None;
    }
    let package = package_of(source, &file)?;
    let (found, _) = candidates(source, &package, &file, test);
    let mut reaching = found
        .iter()
        .filter(|candidate| walk(source, &package, candidate, &file, true).is_ok());
    let root = &reaching.next()?.root;
    if reaching.next().is_some() || names_own_module(root.path.as_str(), &file, test) {
        return None;
    }
    // Cargo auto-discovers no dotfile, so `--test .x` would name no target.
    let named = |name: String| (!name.starts_with('.')).then_some(name);
    match root.kind {
        Kind::Lib => Some(TargetScope::Lib),
        Kind::Bin => package
            .target_name(root)
            .and_then(named)
            .map(TargetScope::Bin),
        Kind::Test => package
            .target_name(root)
            .and_then(named)
            .map(TargetScope::Test),
    }
}

/// Whether `file` is its own root and `test` starts with its stem. Run as its
/// own crate, the file's tests are named without that module (`helper`, not
/// `common::helper`), while the unscoped run may have reached it as a module
/// of another root (`tests/it.rs` declaring `mod common;`).
fn names_own_module(root: &str, file: &str, test: &str) -> bool {
    let stem = Path::new(file).file_stem().and_then(|stem| stem.to_str());
    root == file && stem.is_some_and(|stem| test.split("::").next() == Some(stem))
}

/// A doctest is named `path - fn (line N)`; no libtest target holds it.
fn is_doctest(test: &str) -> bool {
    test.contains(" - ") || test.contains("(line ")
}

// The chain walk needs tree-sitter; without it every scope is `None`.
#[cfg(all(test, feature = "source-graph"))]
#[path = "tests_scope.rs"]
mod tests;
