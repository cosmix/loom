use super::*;
use crate::context::extract::{extract_file, registry};
use crate::context::graph_store::FileEntry;
use crate::context::view::{build_cold, ViewIdentity};

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
    build_cold(graph, ViewIdentity::current("test", "worktree")).graph
}

#[cfg(feature = "source-graph")]
#[test]
fn comments_and_unrelated_names_do_not_wire_a_module() {
    let graph = extracted(&[
        ("src/lib.rs", "// mod store;\nfn store() {}"),
        ("src/store.rs", "pub fn normalize_settings() {}"),
    ]);
    let result = inspect_wiring(&graph, &["src/store.rs".into()].into_iter().collect());
    assert_eq!(result.unwired_files.len(), 1);
    assert!(result.uncertain_files.is_empty());
}

#[cfg(feature = "source-graph")]
#[test]
fn resolved_module_reference_wires_the_exact_path() {
    let graph = extracted(&[
        ("src/lib.rs", "mod store;"),
        ("src/store.rs", "pub fn normalize_settings() {}"),
        ("other/store.rs", "pub fn other_function() {}"),
    ]);
    assert!(is_referenced(&graph, "src/store.rs"));
    assert!(!is_referenced(&graph, "other/store.rs"));
}

#[test]
fn incomplete_graph_reports_uncertainty_instead_of_absence() {
    let mut graph = ResolvedGraph::default();
    graph
        .files
        .insert("src/store.rs".into(), FileEntry::default());
    let result = inspect_wiring(&graph, &["src/store.rs".into()].into_iter().collect());
    assert!(result.unwired_files.is_empty());
    assert_eq!(result.uncertain_files, ["src/store.rs"]);
}

#[test]
fn unavailable_graph_returns_an_error() {
    let temp = tempfile::tempdir().unwrap();
    assert!(detect_unwired_files(temp.path(), "main").is_err());
}

#[cfg(feature = "source-graph")]
#[test]
fn unreadable_sibling_preserves_positive_reference_evidence() {
    let mut graph = extracted(&[
        ("src/lib.rs", "mod store;"),
        ("src/store.rs", "pub fn helper() {}"),
    ]);
    graph
        .files
        .insert("blocked.rs".into(), FileEntry::default());
    let result = inspect_wiring(&graph, &["src/store.rs".into()].into_iter().collect());
    assert!(result.unwired_files.is_empty());
    assert!(result.uncertain_files.is_empty());
}

#[cfg(feature = "source-graph")]
#[test]
fn nested_package_invocation_uses_repository_wiring_coordinates() {
    use crate::git::runner::run_git_checked;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test"],
    ] {
        run_git_checked(&args, root).unwrap();
    }
    let package = root.join("package");
    std::fs::create_dir_all(package.join("src")).unwrap();
    std::fs::write(package.join("src/lib.rs"), "// seed\n").unwrap();
    run_git_checked(&["add", "."], root).unwrap();
    run_git_checked(
        &["-c", "commit.gpgsign=false", "commit", "-m", "seed"],
        root,
    )
    .unwrap();
    std::fs::write(
        package.join("src/store.rs"),
        "pub fn normalize_settings() {}\n",
    )
    .unwrap();
    for directory in [root, package.as_path()] {
        let result = detect_unwired_files(directory, "main").unwrap();
        assert_eq!(result.unwired_files.len(), 1);
        assert_eq!(result.unwired_files[0].path, "package/src/store.rs");
    }
    std::fs::write(package.join("src/lib.rs"), "mod store;\n").unwrap();
    for directory in [root, package.as_path()] {
        let result = detect_unwired_files(directory, "main").unwrap();
        assert!(result.unwired_files.is_empty());
        assert!(result.uncertain_files.is_empty());
    }
}
