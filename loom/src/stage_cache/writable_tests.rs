//! [`Writable::reached_by`] resolves a path one component at a time, as the
//! kernel does, so a command writing below a writable root cannot steer what
//! a later read root resolves to.

use super::tests::temp;
use super::writable::Writable;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

/// A scratch directory holding the writable root `w` and the directories
/// `o` and `x` outside it.
fn layout() -> (tempfile::TempDir, PathBuf) {
    let (dir, base) = temp();
    for name in ["w", "o", "x"] {
        std::fs::create_dir(base.join(name)).unwrap();
    }
    (dir, base)
}

fn writable(base: &Path) -> Writable {
    Writable::new(&[base.join("w").as_path()])
}

/// `..` after a symlink hop leaves the link's target, as the kernel does:
/// `o/link -> x`, so `o/link/../w` is `w`, while lexically it is `o/w`.
#[test]
fn parent_after_a_hop_leaves_the_target() {
    let (_d, base) = layout();
    symlink(base.join("x"), base.join("o/link")).unwrap();
    let writable = writable(&base);
    assert!(writable.reached_by(&base.join("o/link/../w/file")));
    assert!(!writable.reached_by(&base.join("o/link/../o/file")));
}

/// A relative target resolves against the link's own directory.
#[test]
fn relative_targets_resolve_beside_the_link() {
    let (_d, base) = layout();
    symlink("../w", base.join("o/into")).unwrap();
    symlink("../x", base.join("o/beside")).unwrap();
    let writable = writable(&base);
    assert!(writable.reached_by(&base.join("o/into/file")));
    assert!(!writable.reached_by(&base.join("o/beside/file")));
}

/// A symlink loop never resolves, so it counts as reaching a writable root.
#[test]
fn a_symlink_loop_counts_as_reaching() {
    let (_d, base) = layout();
    symlink("b", base.join("o/a")).unwrap();
    symlink("a", base.join("o/b")).unwrap();
    assert!(writable(&base).reached_by(&base.join("o/a/file")));
}

/// A chain of `hops` links in `o`, `l0 -> l1 -> ... -> x`.
fn chain(base: &Path, hops: usize) -> PathBuf {
    for link in 0..hops {
        let target = if link + 1 == hops {
            PathBuf::from("../x")
        } else {
            PathBuf::from(format!("l{}", link + 1))
        };
        symlink(target, base.join(format!("o/l{link}"))).unwrap();
    }
    base.join("o/l0/file")
}

/// Forty hops resolve; one more counts as reaching, as a loop does.
#[test]
fn hops_past_the_limit_count_as_reaching() {
    let (_d, within) = layout();
    assert!(!writable(&within).reached_by(&chain(&within, 40)));
    let (_d, past) = layout();
    assert!(writable(&past).reached_by(&chain(&past, 41)));
}

/// A writable root given through a symlink is matched by its resolved path,
/// and a path through such a symlink by the root as given.
#[test]
fn a_root_given_through_a_symlink_matches_either_spelling() {
    let (_d, base) = layout();
    symlink(&base, base.join("o/alias")).unwrap();
    let through_alias = Writable::new(&[base.join("o/alias/w").as_path()]);
    assert!(through_alias.reached_by(&base.join("w/file")));
    assert!(writable(&base).reached_by(&base.join("o/alias/w/file")));
    assert!(!through_alias.reached_by(&base.join("x/file")));
}

/// The system temporary directory below `/var`, where macOS links `/var` to
/// `/private/var`, as given and canonical.
#[cfg(target_os = "macos")]
fn var_spellings() -> Option<(tempfile::TempDir, PathBuf, PathBuf)> {
    let dir = tempfile::tempdir_in("/var/tmp").ok()?;
    let given = dir.path().to_path_buf();
    let canonical = given.canonicalize().ok()?;
    (given != canonical).then_some((dir, given, canonical))
}

/// macOS: `/var` and `/private/var` name one writable root.
#[cfg(target_os = "macos")]
#[test]
fn var_and_private_var_name_one_root() {
    let Some((_d, given, canonical)) = var_spellings() else {
        return;
    };
    assert!(Writable::new(&[given.as_path()]).reached_by(&canonical.join("file")));
    assert!(Writable::new(&[canonical.as_path()]).reached_by(&given.join("file")));
}

/// macOS: the firmlink spelling `/System/Volumes/Data/...` names the same
/// directory as the path below `/`, in a root and in a queried path.
#[cfg(target_os = "macos")]
#[test]
fn firmlink_spelling_names_the_same_root() {
    let (_d, base) = layout();
    let firmlinked = Path::new("/System/Volumes/Data").join(base.strip_prefix("/").unwrap());
    if !firmlinked.is_dir() {
        return;
    }
    assert!(writable(&base).reached_by(&firmlinked.join("w/file")));
    let root = firmlinked.join("w");
    assert!(Writable::new(&[root.as_path()]).reached_by(&base.join("w/file")));
    assert!(!writable(&base).reached_by(&firmlinked.join("x/file")));
}
