//! A selection of libtest-named tests, split by the cargo target that compiles
//! each one. A bare `cargo test -- <names>` builds and visits every test
//! binary of the package and matches each name as a substring; naming the
//! target (`--lib`, `--test <name>`) builds only it, and `--exact` matches
//! whole libtest paths.
//!
//! A test whose target is not certain (`target_scope`) keeps the unscoped
//! selection, for that test only. The groups run as one `&&` chain, so a
//! selection stays one command: one id, one timeout, one comparison against
//! the stage's base. Cargo itself stops at the first failing test binary.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use super::failure::package_label;
use crate::context::source_graph::file_node_id;
use crate::fs::safe_read::read_to_string_bounded;
use crate::testrun::{SelectionNames, TargetScope, TestRunnerAdapter, TestTarget};
use crate::verify::contracts::collection::rust::target_scope;
use crate::verify::contracts::store::MAX_FROZEN_FILE_BYTES;

/// The command running `targets` of `package` (checkout-relative, under
/// `root`), or `None` when `adapter` cannot select them. Only a runner
/// selecting by libtest path is split by target; any other gets
/// `TestRunnerAdapter::select_command` over all of them.
pub(super) fn select_command(
    root: &Path,
    package: &Path,
    adapter: &dyn TestRunnerAdapter,
    targets: &[TestTarget],
) -> Option<String> {
    let package_dir = root.join(package);
    if !matches!(
        adapter.selection_names(),
        SelectionNames::LibtestPaths { .. }
    ) {
        return adapter.select_command(targets, &package_dir);
    }
    let mut scopes = Scopes::new(root, package);
    // Unscoped tests sort first; the order does not depend on the targets'.
    let mut groups: BTreeMap<Option<TargetScope>, Vec<TestTarget>> = BTreeMap::new();
    for target in targets {
        groups
            .entry(scopes.of(target))
            .or_default()
            .push(target.clone());
    }
    // A runner that cannot name a build target selects every test in one
    // unscoped command rather than one per target.
    let mut unscoped: Vec<TestTarget> = Vec::new();
    let mut commands: Vec<String> = Vec::new();
    for (scope, group) in groups {
        match scope.and_then(|scope| adapter.scoped_select_command(&scope, &group, &package_dir)) {
            Some(command) => commands.push(command),
            None => unscoped.extend(group),
        }
    }
    commands.splice(0..0, adapter.select_command(&unscoped, &package_dir));
    (!commands.is_empty()).then(|| commands.join(" && "))
}

/// The cargo target of each test in one package, resolved once per crate root.
struct Scopes<'a> {
    root: &'a Path,
    package: &'a Path,
    /// The package as `target_scope` names its run directory.
    run_dir: String,
    memo: HashMap<(String, bool), Option<TargetScope>>,
}

impl<'a> Scopes<'a> {
    fn new(root: &'a Path, package: &'a Path) -> Self {
        Self {
            root,
            package,
            run_dir: package_label(package),
            memo: HashMap::new(),
        }
    }

    /// The target compiling `target`, `None` when it is unnamed or not certain.
    /// A libtest-named target's `file` is the crate root its path starts at,
    /// so the target is that root's.
    fn of(&mut self, target: &TestTarget) -> Option<TargetScope> {
        let name = target.name.as_deref()?;
        let file = file_node_id(&self.package.join(&target.file));
        // `target_scope` refuses a test named for its own root file's module,
        // the only way the name alone decides the target.
        let stem = Path::new(&file).file_stem().and_then(|stem| stem.to_str());
        let key = (file.clone(), name.split("::").next() == stem);
        if let Some(known) = self.memo.get(&key) {
            return known.clone();
        }
        let root = self.root;
        let source =
            |path: &str| read_to_string_bounded(root, Path::new(path), MAX_FROZEN_FILE_BYTES).ok();
        let scope = target_scope(&source, &file, name, &self.run_dir);
        self.memo.insert(key, scope.clone());
        scope
    }
}

#[cfg(test)]
#[path = "scoped_tests.rs"]
mod tests;
