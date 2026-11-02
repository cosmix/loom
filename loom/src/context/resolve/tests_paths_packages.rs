//! Path conventions that need more than the module path as written: a Rust
//! integration test naming its own crate by Cargo package name, a Rust path
//! read from the citing file's own crate in a workspace, and a Java glob
//! import recorded as the bare package.

use std::collections::BTreeSet;

use super::fixtures::*;
use super::paths::{import_candidates, PathIndex};

fn index_of(paths: &[&str]) -> PathIndex {
    let files = paths
        .iter()
        .map(|path| (*path, source_file(path, &[], vec![])))
        .collect();
    PathIndex::build(&graph_of(files))
}

fn resolve(index: &PathIndex, spec: &str, from: &str) -> Vec<String> {
    import_candidates(spec, from, index, &mut BTreeSet::new())
}

#[test]
fn package_names_without_manifest_evidence_remain_unresolved() {
    let index = index_of(&["src/lib.rs", "tests/add_test.rs"]);

    assert!(resolve(&index, "demo::add", "tests/add_test.rs").is_empty());
    assert!(resolve(&index, "serde::add", "tests/add_test.rs").is_empty());
}

#[test]
fn bench_package_prefix_cannot_be_verified() {
    let index = index_of(&["src/lib.rs", "src/util.rs", "benches/b.rs"]);

    assert!(resolve(&index, "demo::util::helper", "benches/b.rs").is_empty());
    assert!(resolve(&index, "serde::util::helper", "benches/b.rs").is_empty());
}

#[test]
fn package_name_path_inside_a_crate_root_stays_external() {
    let index = index_of(&["src/lib.rs", "src/main.rs", "src/x/y.rs"]);

    assert!(resolve(&index, "demo::add", "src/main.rs").is_empty());
    assert!(resolve(&index, "serde::de::*", "src/x/y.rs").is_empty());
}

#[test]
fn package_directory_does_not_prove_the_package_name() {
    let index = index_of(&["pkg/src/lib.rs", "other/tests/x.rs", "pkg/tests/y.rs"]);

    assert!(resolve(&index, "demo::add", "other/tests/x.rs").is_empty());
    assert!(resolve(&index, "demo::add", "pkg/tests/y.rs").is_empty());
}

#[test]
fn crate_anchor_never_selects_a_foreign_crate() {
    let index = index_of(&[
        "a/src/lib.rs",
        "a/src/caller.rs",
        "b/src/lib.rs",
        "b/src/helper.rs",
    ]);
    assert_eq!(
        resolve(&index, "crate::helper::run", "a/src/caller.rs"),
        ["a/src/lib.rs"]
    );
    let index = index_of(&[
        "a/src/lib.rs",
        "a/src/caller.rs",
        "a/src/helper.rs",
        "b/src/lib.rs",
        "b/src/helper.rs",
    ]);
    assert_eq!(
        resolve(&index, "crate::helper::run", "a/src/caller.rs"),
        ["a/src/helper.rs"]
    );
    assert!(resolve(&index, "crate::helper::run", "tests/unknown.rs").is_empty());
}

#[test]
fn an_unanchored_path_reads_only_the_citing_files_own_crate() {
    let index = index_of(&[
        "crates/a/src/lib.rs",
        "crates/a/src/log.rs",
        "crates/b/src/lib.rs",
        "crates/b/src/x.rs",
    ]);

    assert!(resolve(&index, "log::info", "crates/b/src/x.rs").is_empty());
    assert_eq!(
        resolve(&index, "log::info", "crates/a/src/lib.rs"),
        ["crates/a/src/log.rs"]
    );
}

#[test]
fn a_one_segment_go_path_is_the_standard_library() {
    let index = index_of(&["pkg/errors/errors.go", "main.go"]);

    assert!(resolve(&index, "errors", "main.go").is_empty());
    assert_eq!(
        resolve(&index, "pkg/errors", "main.go"),
        ["pkg/errors/errors.go"]
    );
}

#[test]
fn java_bare_package_spec_is_a_glob_over_the_package_directory() {
    let index = index_of(&[
        "src/main/java/a/b/C.java",
        "src/main/java/a/b/D.java",
        "src/main/java/app/Main.java",
    ]);
    let from = "src/main/java/app/Main.java";

    assert_eq!(
        resolve(&index, "a.b", from),
        ["src/main/java/a/b/C.java", "src/main/java/a/b/D.java"]
    );
    assert_eq!(resolve(&index, "a.b.C", from), ["src/main/java/a/b/C.java"]);
    assert_eq!(
        resolve(&index, "a.b.*", from),
        ["src/main/java/a/b/C.java", "src/main/java/a/b/D.java"]
    );
}
