use super::*;
use std::collections::{HashMap, HashSet};

/// `1 XY sub mH mI mW hH hI <path>` entry.
fn changed(xy: &str, path: &str) -> String {
    format!("1 {xy} N... 100644 100644 100644 aaaa bbbb {path}\0")
}

fn untracked(path: &str) -> String {
    format!("? {path}\0")
}

fn diff(entries: &[(&str, &str)]) -> String {
    entries
        .iter()
        .map(|(status, path)| format!("{status}\0{path}\0"))
        .collect()
}

/// A checkout whose disk holds only what is listed; every byte comparison
/// answers `equal`.
#[derive(Default)]
struct Fake {
    equal: bool,
    disk: HashMap<String, DiskEntry>,
    base: HashSet<String>,
}

impl Fake {
    fn equal() -> Self {
        Self {
            equal: true,
            ..Self::default()
        }
    }

    fn with_disk(mut self, path: &str, entry: DiskEntry) -> Self {
        self.disk.insert(path.to_string(), entry);
        self
    }
}

impl CheckoutProbe for Fake {
    fn disk(&self, path: &str) -> DiskEntry {
        self.disk.get(path).copied().unwrap_or(DiskEntry::Absent)
    }

    fn equals_blob(&self, _path: &str) -> bool {
        self.equal
    }

    fn in_base_tree(&self, path: &str) -> bool {
        self.base.contains(path)
    }
}

#[test]
fn unrelated_changes_do_not_overlap() {
    let status = changed(".M", "other.txt") + &changed("M.", "staged.txt") + &untracked("new.txt");
    let touched = diff(&[("M", "merged.txt"), ("A", "added.txt")]);
    assert_eq!(
        classify(&status, &touched, &Fake::default()),
        Classification::NoOverlap
    );
}

#[test]
fn equal_untracked_overlap_is_removed() {
    let status = untracked("added.txt");
    let touched = diff(&[("A", "added.txt")]);
    assert_eq!(
        classify(&status, &touched, &Fake::equal()),
        Classification::RemoveUntracked {
            paths: vec!["added.txt".to_string()]
        }
    );
}

#[test]
fn tracked_overlap_blocks() {
    let status = changed(".M", "merged.txt") + &untracked("added.txt");
    let touched = diff(&[("M", "merged.txt"), ("A", "added.txt")]);
    assert_eq!(
        classify(&status, &touched, &Fake::equal()),
        Classification::Blocked {
            paths: vec!["merged.txt".to_string()]
        }
    );
}

#[test]
fn staged_only_change_counts_as_tracked() {
    let status = changed("M.", "merged.txt");
    let touched = diff(&[("M", "merged.txt")]);
    assert_eq!(
        classify(&status, &touched, &Fake::default()),
        Classification::Blocked {
            paths: vec!["merged.txt".to_string()]
        }
    );
}

#[test]
fn differing_untracked_overlap_blocks() {
    let status = changed(".M", "merged.txt") + &untracked("added.txt");
    let touched = diff(&[("M", "merged.txt"), ("A", "added.txt")]);
    assert_eq!(
        classify(&status, &touched, &Fake::default()),
        Classification::Blocked {
            paths: vec!["added.txt".to_string()]
        }
    );
}

#[test]
fn rename_in_the_checkout_counts_both_paths() {
    let status = "2 R. N... 100644 100644 100644 aaaa bbbb R100 new name.txt\0old.txt\0";
    let touched = diff(&[("D", "old.txt")]);
    assert_eq!(
        classify(status, &touched, &Fake::default()),
        Classification::Blocked {
            paths: vec!["old.txt".to_string()]
        }
    );
}

#[test]
fn rename_in_the_merge_is_a_delete_plus_add() {
    let status = changed(".M", "old.txt") + &untracked("renamed.txt");
    let touched = diff(&[("D", "old.txt"), ("A", "renamed.txt")]);
    assert_eq!(
        classify(&status, &touched, &Fake::equal()),
        Classification::Blocked {
            paths: vec!["old.txt".to_string()]
        }
    );
}

#[test]
fn paths_with_spaces_and_newlines_survive() {
    let status = changed(".M", "dir/with space.txt") + &untracked("odd\nname.txt");
    let touched = diff(&[("M", "dir/with space.txt"), ("A", "odd\nname.txt")]);
    assert_eq!(
        classify(&status, &touched, &Fake::equal()),
        Classification::Blocked {
            paths: vec!["dir/with space.txt".to_string()]
        }
    );
}

#[test]
fn unmerged_entries_block_even_without_overlap() {
    let status = "u UU N... 100644 100644 100644 100644 aaaa bbbb cccc sp ace.txt\0";
    let touched = diff(&[("M", "merged.txt")]);
    assert_eq!(
        classify(status, &touched, &Fake::default()),
        Classification::Blocked {
            paths: vec!["sp ace.txt".to_string()]
        }
    );
}

#[test]
fn ignored_and_header_lines_are_skipped() {
    let status = "# branch.oid abc\0! target/debug\0";
    let touched = diff(&[("M", "target/debug")]);
    assert_eq!(
        classify(status, &touched, &Fake::default()),
        Classification::NoOverlap
    );
}

fn blocked(paths: &[&str]) -> Classification {
    Classification::Blocked {
        paths: paths.iter().map(|p| p.to_string()).collect(),
    }
}

#[test]
fn untracked_file_below_an_added_file_blocks() {
    let status = untracked("foo/bar");
    let touched = diff(&[("A", "foo")]);
    assert_eq!(
        classify(&status, &touched, &Fake::equal()),
        blocked(&["foo/bar"])
    );
}

#[test]
fn untracked_file_above_an_added_path_blocks() {
    let status = untracked("foo");
    let touched = diff(&[("A", "foo/bar")]);
    assert_eq!(
        classify(&status, &touched, &Fake::equal()),
        blocked(&["foo"])
    );
}

#[test]
fn tracked_change_below_or_above_a_touched_path_is_a_tracked_overlap() {
    let touched = diff(&[("D", "foo"), ("A", "dir/x")]);
    for (status, expected) in [
        (changed(".M", "foo/bar"), "foo/bar"),
        (changed(".M", "dir"), "dir"),
    ] {
        assert_eq!(
            classify(&status, &touched, &Fake::default()),
            Classification::Blocked {
                paths: vec![expected.to_string()]
            }
        );
    }
}

#[test]
fn an_ignored_regular_file_the_merge_adds_is_removed_when_equal() {
    let touched = diff(&[("A", "ign.txt")]);
    let probe = Fake::equal().with_disk("ign.txt", DiskEntry::RegularFile);
    assert_eq!(
        classify("", &touched, &probe),
        Classification::RemoveUntracked {
            paths: vec!["ign.txt".to_string()]
        }
    );
}

#[test]
fn an_ignored_regular_file_the_merge_adds_blocks_when_different() {
    let touched = diff(&[("A", "ign.txt")]);
    let probe = Fake::default().with_disk("ign.txt", DiskEntry::RegularFile);
    assert_eq!(classify("", &touched, &probe), blocked(&["ign.txt"]));
}

#[test]
fn an_ignored_symlink_or_directory_at_an_added_path_blocks() {
    let touched = diff(&[("A", "x")]);
    for entry in [DiskEntry::Other, DiskEntry::Directory] {
        let probe = Fake::equal().with_disk("x", entry);
        assert_eq!(classify("", &touched, &probe), blocked(&["x"]));
    }
}

#[test]
fn a_file_where_the_merge_needs_a_directory_blocks() {
    let touched = diff(&[("A", "a/b/c.txt")]);
    let probe = Fake::default()
        .with_disk("a", DiskEntry::Directory)
        .with_disk("a/b", DiskEntry::RegularFile);
    assert_eq!(classify("", &touched, &probe), blocked(&["a/b"]));
}

#[test]
fn a_parent_file_the_base_tree_has_does_not_block() {
    let touched = diff(&[("D", "a"), ("A", "a/c.txt")]);
    let mut probe = Fake::default().with_disk("a", DiskEntry::RegularFile);
    probe.base.insert("a".to_string());
    assert_eq!(classify("", &touched, &probe), Classification::NoOverlap);
}
