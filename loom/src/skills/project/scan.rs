use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use super::{markers, ProjectProfile, ProjectType};

const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 20_000;
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".loom",
    ".work",
    ".worktrees",
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    ".next",
    ".venv",
    "venv",
    "__pycache__",
    ".cache",
    ".codex",
    ".claude",
    ".agents",
    ".terraform",
];

/// This marker is only a traversal boundary; it grants no repository authority.
///
/// Checked with `is_real_git_dir` rather than bare existence: an ancestor
/// directory can hold an empty `.git` directory left behind by an unrelated
/// process sharing the same OS temp root (observed under `/tmp/claude-*`),
/// which would otherwise make every scan below it treat that unrelated
/// directory as the checkout root and pick up its unrelated files.
pub(super) fn checkout_root(cwd: &Path) -> PathBuf {
    for ancestor in cwd.ancestors() {
        if crate::fs::git_marker::is_real_git_dir(&ancestor.join(".git")) {
            return ancestor.to_path_buf();
        }
    }
    cwd.ancestors()
        .find(|path| markers::package_boundary(path))
        .unwrap_or(cwd)
        .to_path_buf()
}

pub(super) fn discover(root: PathBuf) -> ProjectProfile {
    let mut profile = ProjectProfile {
        root,
        ..ProjectProfile::default()
    };
    let mut pending = VecDeque::from([(profile.root.clone(), 0)]);
    let mut remaining = MAX_ENTRIES;
    // Paths of packages already recorded as `cpp` from a `CMakeLists.txt`.
    // The queue is processed breadth-first, so every ancestor of `dir` below
    // is recorded here before `dir` itself is reached.
    let mut cpp_package_roots: Vec<PathBuf> = Vec::new();
    while let Some((dir, depth)) = pending.pop_front() {
        let mut types = markers::detect(&dir);
        let path = dir.strip_prefix(&profile.root).unwrap_or(Path::new(""));
        resolve_nested_cmake(&mut types, path, &mut cpp_package_roots);
        if !types.is_empty() || markers::package_boundary(&dir) {
            profile.packages.push(path.to_path_buf());
        }
        for kind in types {
            profile.types.push(ProjectType {
                kind,
                path: path.to_path_buf(),
            });
        }
        let (children, exhausted) = children(&dir, &mut remaining);
        if exhausted {
            profile.truncated = true;
            break;
        }
        if depth == MAX_DEPTH {
            profile.truncated |= !children.is_empty();
        } else {
            pending.extend(children.into_iter().map(|child| (child, depth + 1)));
        }
    }
    profile.types.sort();
    profile
}

/// A `CMakeLists.txt` below an already-known CMake package belongs to that
/// ancestor's build (`add_subdirectory`), not a package of its own: it must
/// not become a second `cpp`/ctest package whose `package_dir` has no
/// `build/` of its own. Otherwise, a new `cpp` package root is recorded.
fn resolve_nested_cmake(
    types: &mut BTreeSet<String>,
    path: &Path,
    cpp_package_roots: &mut Vec<PathBuf>,
) {
    let nested_cmake = types.contains("cpp")
        && cpp_package_roots
            .iter()
            .any(|known| path != known.as_path() && path.starts_with(known));
    if nested_cmake {
        types.remove("cpp");
    } else if types.contains("cpp") {
        cpp_package_roots.push(path.to_path_buf());
    }
}

fn children(dir: &Path, remaining: &mut usize) -> (Vec<PathBuf>, bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (Vec::new(), false);
    };
    let mut children = Vec::new();
    for entry in entries {
        if *remaining == 0 {
            return (children, true);
        }
        *remaining -= 1;
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        if SKIP_DIRS.contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            // Nested checkouts belong to a different project. An empty `.git`
            // directory left behind by an unrelated process does not count
            // (see `checkout_root`), so this subtree is still scanned.
            if !crate::fs::git_marker::is_real_git_dir(&entry.path().join(".git")) {
                children.push(entry.path());
            }
        }
    }
    children.sort();
    (children, false)
}
