//! Files that toolchains read in the directories above a confined tree. Cargo
//! looks in every ancestor of its package, past the first hit, for a manifest
//! whose `[workspace]` might claim the package and for `.cargo/config.toml`
//! files it merges. Under bubblewrap such a file outside the boundary is not
//! mounted, so it is absent. Seatbelt allows metadata reads everywhere and
//! denies only the content: `exists()` holds, the read fails, and Cargo stops
//! with an error naming the file. A stage worktree under `<repo>/.worktrees/`
//! meets this on every Cargo command, since the main checkout above it holds
//! the project's own `Cargo.toml`; so does a tree below a home with
//! `~/.cargo/config.toml`.
//!
//! Hiding each such file's metadata too makes it absent, as on Linux. A
//! `file-read*` deny does not do it: a rule naming `file-read-metadata`
//! outranks the wildcard, so the profile's global metadata allowance keeps
//! `stat` working, and `access(F_OK)` is its own `file-test-existence`
//! operation; each must be denied by name. Only files that exist get a rule:
//! Seatbelt denies with `EPERM`, and a tool that reads any error other than
//! `ENOENT` as fatal (rustfmt's config search) would otherwise fail where no
//! file is. Tools that stop at the first file they find (rustup's
//! `rust-toolchain.toml`, rustfmt's `rustfmt.toml`) find the tree's own copy
//! first in a worktree and need no rule. Revisit when another tool that
//! searches past its first hit fails under Seatbelt on a file above the tree.
//!
//! Hiding does not serve a tool that fails on `EPERM`, which a denied open or
//! stat returns. Node's package-scope lookup reads `package.json` in every
//! directory above a module and is such a tool: vitest's dependency check
//! (`getPackageScopeConfig`) exits 1 on the file above the tree even when every
//! test passes. Those files are readable instead, each existing one by
//! `literal` path: a `package.json` is project metadata, and a `subpath` would
//! expose the rest of the directory. Bubblewrap does not mount them, so there
//! they are absent.
//!
//! The directories above the tree are listable, as bubblewrap's empty parent
//! directories are: Bun reads each directory above its working directory and
//! aborts with `CouldntReadCurrentDirectory` when one is unreadable. Each gets
//! a `file-read*` rule on its `literal` path, which matches the directory
//! alone, so its entries' names are visible and their content is not; a
//! `subpath` would expose every file below those ancestors, the main
//! checkout and the home included. `/` is in the base profile already. The
//! listings precede the file denies, which keep winning for those files.
//!
//! The native worker's Seatbelt profile
//! (`native_worker::boundary::seatbelt_profile`) lists the same ancestors; it
//! reaches this module as `crate::verify::criteria::ancestors`.

use std::path::{Path, PathBuf};

use crate::native_worker::boundary::seatbelt_profile::quote_checked;

/// Files Cargo looks for in every directory above its package; they are hidden.
const HIDDEN_ABOVE: [&str; 2] = ["Cargo.toml", ".cargo/config.toml"];

/// Files Node's package-scope lookup reads in every directory above a module;
/// they are readable, since Node fails on the `EPERM` a denied read returns.
const READABLE_ABOVE: [&str; 1] = ["package.json"];

/// The existing [`HIDDEN_ABOVE`] files in the directories above the canonical
/// `tree` whose canonical path no root in `readable` (canonical) covers: each
/// path, then its canonical form when that differs (a symlink).
pub(crate) fn hidden_above(tree: &Path, readable: &[PathBuf]) -> Vec<PathBuf> {
    existing_above(tree, readable, &HIDDEN_ABOVE)
}

/// The existing [`READABLE_ABOVE`] files above `tree`, selected as
/// [`hidden_above`] selects its files.
pub(crate) fn readable_above(tree: &Path, readable: &[PathBuf]) -> Vec<PathBuf> {
    existing_above(tree, readable, &READABLE_ABOVE)
}

fn existing_above(tree: &Path, readable: &[PathBuf], names: &[&str]) -> Vec<PathBuf> {
    let mut hidden = Vec::new();
    for dir in tree.ancestors().skip(1) {
        for name in names {
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

/// Every Seatbelt rule for the directories and files above the canonical
/// `tree`, in the order that is the policy: the directories listed, then the
/// [`HIDDEN_ABOVE`] files hidden (after the listings, so a deny wins whatever
/// spelling names the file), then each [`READABLE_ABOVE`] file readable.
/// `readable` holds the canonical read roots, whose files need no rule. The
/// completion profile and the native worker's profile both render these.
pub(crate) fn rules_above(tree: &Path, readable: &[PathBuf]) -> anyhow::Result<String> {
    let listed = listed_above(tree);
    let mut rules = listing_rules(&listed, quote_checked(&listed)?);
    let hidden = hidden_above(tree, readable);
    rules.push_str(&seatbelt_rules(&hidden, quote_checked(&hidden)?));
    let package_scope = readable_above(tree, readable);
    rules.push_str(&read_rules(&package_scope, quote_checked(&package_scope)?));
    Ok(rules)
}

/// Seatbelt rules denying every operation that reads `hidden`, metadata and
/// existence included, under each of [`seatbelt_spellings`]; they must follow
/// the global metadata allowance.
pub(crate) fn seatbelt_rules(hidden: &[PathBuf], quote: impl Fn(&Path) -> String) -> String {
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

/// Seatbelt rules letting a command read each of `files` by `literal` path,
/// under each of [`seatbelt_spellings`].
pub(crate) fn read_rules(files: &[PathBuf], quote: impl Fn(&Path) -> String) -> String {
    let mut rules = String::new();
    for path in files.iter().flat_map(|path| seatbelt_spellings(path)) {
        rules.push_str(&format!("(allow file-read* (literal {}))", quote(&path)));
    }
    rules
}

/// The directories above the canonical `tree`, nearest first, without `/`.
pub(crate) fn listed_above(tree: &Path) -> Vec<PathBuf> {
    tree.ancestors()
        .skip(1)
        .filter(|dir| dir.parent().is_some())
        .map(Path::to_path_buf)
        .collect()
}

/// Seatbelt rules letting a command list each of `dirs`, under each of
/// [`seatbelt_spellings`], and read none of the files in them.
pub(crate) fn listing_rules(dirs: &[PathBuf], quote: impl Fn(&Path) -> String) -> String {
    let mut rules = String::new();
    for dir in dirs.iter().flat_map(|dir| seatbelt_spellings(dir)) {
        rules.push_str(&format!("(allow file-read* (literal {}))", quote(&dir)));
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
pub(crate) fn seatbelt_spellings(path: &Path) -> Vec<PathBuf> {
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
    /// own such files above the fixture (`/tmp/Cargo.toml`, a `TMPDIR` inside
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
    fn an_existing_package_json_above_the_tree_is_readable_not_hidden() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let repo = base.join("repo");
        let tree = repo.join("web");
        write(&repo.join("package.json"));
        write(&tree.join("package.json"));
        let above = |files: Vec<PathBuf>| {
            files
                .into_iter()
                .filter(|path| path.starts_with(&base))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            above(readable_above(&tree, &[])),
            [repo.join("package.json")]
        );
        assert!(hidden_below(&base, &tree, &[]).is_empty());
        assert!(above(readable_above(&tree, std::slice::from_ref(&repo))).is_empty());
        std::fs::remove_file(repo.join("package.json")).unwrap();
        assert!(above(readable_above(&tree, &[])).is_empty());
    }

    /// A literal allow per spelling; never a subpath, never the tree's own file.
    #[test]
    fn each_spelling_of_a_readable_file_gets_a_literal_read_allow() {
        let files = [PathBuf::from("/private/var/r/package.json")];
        let rules = read_rules(&files, |path: &Path| format!("<{}>", path.display()));
        assert_eq!(
            rules,
            "(allow file-read* (literal </private/var/r/package.json>))\
             (allow file-read* (literal </var/r/package.json>))\
             (allow file-read* (literal </System/Volumes/Data/private/var/r/package.json>))"
        );
        assert!(!rules.contains("subpath"));
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

    /// A literal per directory and spelling; never a subpath, never `/`.
    #[test]
    fn each_directory_above_the_tree_is_listed_by_literal_in_every_spelling() {
        let listed = listed_above(Path::new("/private/var/t/tree"));
        assert_eq!(
            listed,
            [
                PathBuf::from("/private/var/t"),
                PathBuf::from("/private/var"),
                PathBuf::from("/private"),
            ]
        );
        let rules = listing_rules(&listed[..1], |path: &Path| format!("<{}>", path.display()));
        assert_eq!(
            rules,
            "(allow file-read* (literal </private/var/t>))\
             (allow file-read* (literal </var/t>))\
             (allow file-read* (literal </System/Volumes/Data/private/var/t>))"
        );
        assert!(listed_above(Path::new("/tree")).is_empty());
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
