//! Cargo files in the directories above a confined tree. Cargo looks in every
//! ancestor of its package, past the first hit, for a manifest whose
//! `[workspace]` might claim the package and for `.cargo/config.toml` files it
//! merges. Under bubblewrap such a file outside the boundary is not mounted, so
//! it is absent. Seatbelt allows metadata reads everywhere and denies only the
//! content: `exists()` holds, the read fails, and Cargo stops with an error
//! naming the file. A stage worktree under `<repo>/.worktrees/` meets this on
//! every Cargo command, since the main checkout above it holds the project's
//! own `Cargo.toml`; so does a tree below a home with `~/.cargo/config.toml`.
//!
//! Hiding each such file's metadata too makes it absent, as on Linux. Only
//! files that exist get a rule: Seatbelt denies with `EPERM`, and a tool that
//! reads any error other than `ENOENT` as fatal (rustfmt's config search) would
//! otherwise fail where no file is. Tools that stop at the first file they find
//! (rustup's `rust-toolchain.toml`, rustfmt's `rustfmt.toml`) find the tree's
//! own copy first in a worktree and need no rule. Revisit when another tool
//! that searches past its first hit fails under Seatbelt on a file above the
//! tree.

use std::path::{Path, PathBuf};

/// Files Cargo looks for in every directory above its package.
const SEARCHED_ABOVE: [&str; 2] = ["Cargo.toml", ".cargo/config.toml"];

/// The existing [`SEARCHED_ABOVE`] files in the directories above the
/// canonical `tree` whose canonical path no root in `readable` (canonical)
/// covers: each path, then its canonical form when that differs (a symlink).
pub(super) fn hidden_above(tree: &Path, readable: &[PathBuf]) -> Vec<PathBuf> {
    let mut hidden = Vec::new();
    for dir in tree.ancestors().skip(1) {
        for name in SEARCHED_ABOVE {
            let path = dir.join(name);
            // A missing or dangling file reads as absent already.
            let Ok(canonical) = path.canonicalize() else {
                continue;
            };
            if readable.iter().any(|root| canonical.starts_with(root)) {
                continue;
            }
            if canonical != path {
                hidden.push(canonical);
            }
            hidden.push(path);
        }
    }
    hidden
}

/// Seatbelt rules denying every operation that reads `hidden`, metadata
/// included; they must follow the global metadata allowance.
#[cfg(target_os = "macos")]
pub(super) fn seatbelt_rules(hidden: &[PathBuf], quote: impl Fn(&Path) -> String) -> String {
    hidden
        .iter()
        .map(|path| format!("(deny file-read* (literal {}))", quote(path)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "[package]\n").unwrap();
    }

    #[test]
    fn existing_cargo_files_above_the_tree_are_hidden_and_the_trees_own_are_not() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().canonicalize().unwrap().join("repo");
        let tree = repo.join(".worktrees/s1");
        write(&repo.join("Cargo.toml"));
        write(&repo.join(".cargo/config.toml"));
        write(&tree.join("Cargo.toml"));
        write(&tree.join(".cargo/config.toml"));
        let hidden = hidden_above(&tree, &[]);
        assert_eq!(
            hidden,
            [repo.join("Cargo.toml"), repo.join(".cargo/config.toml")]
        );
    }

    #[test]
    fn a_missing_file_or_one_inside_a_readable_root_gets_no_rule() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let tree = base.join("a/b/tree");
        std::fs::create_dir_all(&tree).unwrap();
        write(&base.join("a/Cargo.toml"));
        assert!(hidden_above(&tree, &[base.join("a")]).is_empty());
        std::fs::remove_file(base.join("a/Cargo.toml")).unwrap();
        assert!(hidden_above(&tree, &[]).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_is_hidden_at_both_paths_unless_its_target_is_readable() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let tree = base.join("repo/tree");
        std::fs::create_dir_all(&tree).unwrap();
        let target = base.join("elsewhere/Cargo.toml");
        write(&target);
        let link = base.join("repo/Cargo.toml");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(hidden_above(&tree, &[]), [target.clone(), link]);
        assert!(hidden_above(&tree, &[base.join("elsewhere")]).is_empty());
    }
}
