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
#[cfg(any(target_os = "macos", test))]
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

    /// What [`hidden_above`] reports at or below `base`: a host may keep its
    /// own Cargo files above the fixture (`/tmp/Cargo.toml`, a `TMPDIR` inside
    /// a project), and those are no part of the case under test.
    fn hidden_below(base: &Path, tree: &Path, readable: &[PathBuf]) -> Vec<PathBuf> {
        let mut hidden = hidden_above(tree, readable);
        hidden.retain(|path| path.starts_with(base));
        hidden
    }

    #[test]
    fn existing_cargo_files_above_the_tree_are_hidden_and_the_trees_own_are_not() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let repo = base.join("repo");
        let tree = repo.join(".worktrees/s1");
        write(&repo.join("Cargo.toml"));
        write(&repo.join(".cargo/config.toml"));
        write(&tree.join("Cargo.toml"));
        write(&tree.join(".cargo/config.toml"));
        assert_eq!(
            hidden_below(&base, &tree, &[]),
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
        assert!(hidden_below(&base, &tree, &[base.join("a")]).is_empty());
        std::fs::remove_file(base.join("a/Cargo.toml")).unwrap();
        assert!(hidden_below(&base, &tree, &[]).is_empty());
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
        assert_eq!(hidden_below(&base, &tree, &[]), [target.clone(), link]);
        assert!(hidden_below(&base, &tree, &[base.join("elsewhere")]).is_empty());
    }

    /// `file-read*` covers `file-read-metadata`, so the rule overrides the
    /// profile's global metadata allowance; a `file-read-data` rule would
    /// leave `exists()` true and Cargo failing as before.
    #[test]
    fn each_hidden_file_gets_one_literal_deny_of_every_read() {
        let hidden = [
            PathBuf::from("/r/Cargo.toml"),
            PathBuf::from("/r/.cargo/config.toml"),
        ];
        let quote = |path: &Path| format!("<{}>", path.display());
        assert_eq!(
            seatbelt_rules(&hidden, quote),
            "(deny file-read* (literal </r/Cargo.toml>))\
             (deny file-read* (literal </r/.cargo/config.toml>))"
        );
    }
}
