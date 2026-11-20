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
//! Hiding each such file's metadata too makes it absent, as on Linux. A
//! `file-read*` deny does not do it: a rule naming `file-read-metadata`
//! outranks the wildcard, so the profile's global metadata allowance keeps
//! `stat` working, and `access(F_OK)` is its own `file-test-existence`
//! operation; each must be denied by name. Only
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

/// Seatbelt rules denying every operation that reads `hidden`, metadata and
/// existence included, under each of [`seatbelt_spellings`]; they must follow
/// the global metadata allowance.
#[cfg(any(target_os = "macos", test))]
pub(super) fn seatbelt_rules(hidden: &[PathBuf], quote: impl Fn(&Path) -> String) -> String {
    let mut rules = String::new();
    for path in hidden.iter().flat_map(|path| seatbelt_spellings(path)) {
        let path = quote(&path);
        rules.push_str(&format!(
            "(deny file-read* (literal {path}))\
             (deny file-read-metadata file-test-existence (literal {path}))"
        ));
    }
    rules
}

/// `path`, then the other forms Seatbelt might match it under: without the
/// `/private` prefix of a root symlink (`/var`, `/tmp`, `/etc`), and below
/// `/System/Volumes/Data`, the data volume that firmlinks `/private`, `/Users`
/// and the other writable roots resolve to. The kernel reports the canonical
/// form, which is `path` itself; the others cost one rule each and cover a
/// release that reports another. Revisit when the macOS probe in
/// `confine_seatbelt_tests.rs` shows which form the kernel matches.
#[cfg(any(target_os = "macos", test))]
pub(super) fn seatbelt_spellings(path: &Path) -> Vec<PathBuf> {
    let mut spellings = vec![path.to_path_buf()];
    if let Ok(rest) = path.strip_prefix("/private") {
        if ["var", "tmp", "etc"]
            .iter()
            .any(|root| rest.starts_with(root))
        {
            spellings.push(Path::new("/").join(rest));
        }
    }
    if !path.starts_with("/System/Volumes") {
        if let Ok(rest) = path.strip_prefix("/") {
            spellings.push(Path::new("/System/Volumes/Data").join(rest));
        }
    }
    spellings
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

    /// A rule naming `file-read-metadata` outranks a `file-read*` one, so only
    /// a named deny overrides the profile's global metadata allowance.
    #[test]
    fn each_spelling_of_a_hidden_file_gets_a_read_and_a_named_metadata_deny() {
        let hidden = [PathBuf::from("/r/Cargo.toml")];
        let quote = |path: &Path| format!("<{}>", path.display());
        let rules = |path: &str| {
            format!(
                "(deny file-read* (literal <{path}>))\
                 (deny file-read-metadata file-test-existence (literal <{path}>))"
            )
        };
        assert_eq!(
            seatbelt_rules(&hidden, quote),
            rules("/r/Cargo.toml") + &rules("/System/Volumes/Data/r/Cargo.toml")
        );
    }

    #[test]
    fn a_private_root_path_is_also_spelled_through_its_symlink_and_the_data_volume() {
        let spell = |path: &str| seatbelt_spellings(Path::new(path));
        assert_eq!(
            spell("/private/var/folders/x/Cargo.toml"),
            [
                PathBuf::from("/private/var/folders/x/Cargo.toml"),
                PathBuf::from("/var/folders/x/Cargo.toml"),
                PathBuf::from("/System/Volumes/Data/private/var/folders/x/Cargo.toml"),
            ]
        );
        assert_eq!(
            spell("/private/varnish/Cargo.toml")[1],
            PathBuf::from("/System/Volumes/Data/private/varnish/Cargo.toml")
        );
        assert_eq!(
            spell("/System/Volumes/Data/Users/a/Cargo.toml"),
            [PathBuf::from("/System/Volumes/Data/Users/a/Cargo.toml")]
        );
    }
}
