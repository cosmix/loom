use super::*;
use crate::context::extract::{extract_file, registry};
use crate::context::graph_store::FileEntry;
use crate::git::runner::run_git_checked;

fn extracted(files: &[(&str, &str)]) -> ResolvedGraph {
    let extractors = registry();
    let mut graph = ResolvedGraph::default();
    for (path, source) in files {
        graph.files.insert(
            path.to_string(),
            FileEntry::from_extraction(
                source.as_bytes(),
                extract_file(&extractors, Path::new(path), source.as_bytes()),
            ),
        );
    }
    graph
}

#[cfg(feature = "source-graph")]
#[test]
fn private_and_restricted_rust_copies_compare_all_changed_definitions_once() {
    for visibility in ["", "pub(crate) ", "pub "] {
        let source = format!("{visibility}fn normalize_settings() -> u32 {{ 42 }}\n");
        let graph = extracted(&[("old.rs", &source), ("new.rs", &source)]);
        for changed in [vec!["new.rs"], vec!["old.rs", "new.rs"]] {
            let changed = changed.into_iter().map(str::to_string).collect();
            let duplicates = duplicate_candidates(&graph, &changed);
            assert_eq!(duplicates.len(), 1);
            assert_eq!(duplicates[0].symbol_name, "normalize_settings");
        }
    }
}

#[cfg(feature = "source-graph")]
#[test]
fn supported_languages_share_definition_candidates() {
    for (extension, source) in [
        (
            "go",
            "package main\nfunc NormalizeSettings() int { return 42 }\n",
        ),
        ("py", "def normalize_settings():\n    return 42\n"),
        (
            "ts",
            "export function normalize_settings() { return 42; }\n",
        ),
    ] {
        let left = format!("left.{extension}");
        let right = format!("right.{extension}");
        let graph = extracted(&[(&left, source), (&right, source)]);
        assert_eq!(
            duplicate_candidates(&graph, &[right].into_iter().collect()).len(),
            1
        );
    }
}

#[cfg(feature = "source-graph")]
#[test]
fn unrelated_methods_and_different_bodies_are_not_duplicates() {
    let graph = extracted(&[
        (
            "a.rs",
            "struct Cat; impl Cat { fn normalize_settings() -> u32 { 42 } }",
        ),
        (
            "b.rs",
            "struct Dog; impl Dog { fn normalize_settings() -> u32 { 42 } }",
        ),
        ("c.rs", "fn normalize_settings() -> u32 { 43 }"),
        ("d.rs", "fn normalize_settings() -> u32 { 44 }"),
    ]);
    let changed = graph.files.keys().cloned().collect();
    assert!(duplicate_candidates(&graph, &changed).is_empty());
}

fn git(root: &Path, args: &[&str]) {
    run_git_checked(args, root).unwrap();
}

#[cfg(feature = "source-graph")]
#[test]
fn root_and_nested_package_see_staged_unstaged_and_new_copies() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    let nested = root.join("package/src");
    std::fs::create_dir_all(&nested).unwrap();
    let source = "pub(crate) fn normalize_settings() -> u32 { 42 }\n";
    std::fs::write(nested.join("old.rs"), source).unwrap();
    git(root, &["add", "."]);
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-m", "seed"],
    );
    std::fs::write(
        nested.join("old.rs"),
        format!("// unrelated edit\n{source}"),
    )
    .unwrap();
    std::fs::write(nested.join("new.rs"), source).unwrap();
    let package = root.join("package");
    for directory in [root, package.as_path()] {
        assert_eq!(
            detect_duplicate_symbols(directory, "main").unwrap().len(),
            1
        );
    }
    git(root, &["add", "."]);
    assert_eq!(detect_duplicate_symbols(root, "main").unwrap().len(), 1);
    std::fs::write(nested.join("third.rs"), source).unwrap();
    assert_eq!(detect_duplicate_symbols(root, "main").unwrap().len(), 3);
}

#[test]
fn unavailable_repository_is_an_explicit_error() {
    let temp = tempfile::tempdir().unwrap();
    assert!(detect_duplicate_symbols(temp.path(), "main").is_err());
}

#[cfg(unix)]
#[test]
fn non_utf8_changed_paths_fail_explicitly_instead_of_lossy_matching() {
    use std::os::unix::ffi::OsStrExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(
        root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-m",
            "seed",
        ],
    );
    let path = std::ffi::OsStr::from_bytes(b"helper\xff.rs");
    // Git supports raw index paths even on filesystems that reject invalid UTF-8.
    std::fs::write(root.join("source.rs"), b"fn normalize_settings() {}\n").unwrap();
    let blob = std::process::Command::new("git")
        .args(["hash-object", "-w", "source.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(blob.status.success());
    let blob = String::from_utf8(blob.stdout).unwrap();
    let status = std::process::Command::new("git")
        .args([
            "update-index",
            "--add",
            "--cacheinfo",
            "100644",
            blob.trim(),
        ])
        .arg(path)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success());
    let error = detect_duplicate_symbols(root, "main").unwrap_err();
    assert!(format!("{error:#}").contains("non-UTF-8 path"));
}
