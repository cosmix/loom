use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::*;
use crate::context::extract::{extract_file, registry};
use crate::context::graph_store::FileEntry;

const LIB_RS: &str = "pub fn target() {}\npub fn caller() {\n    let _x = 1;\n    target();\n}\n";

/// A directory holding `files` and the graph a build of them would produce.
fn fixture(files: &[(&str, &str)]) -> (TempDir, ResolvedGraph) {
    let temp = tempfile::tempdir().expect("tempdir");
    let extractors = registry();
    let mut graph = ResolvedGraph {
        base_revision: String::new(),
        overlaid: BTreeSet::new(),
        files: Default::default(),
    };
    for (path, body) in files {
        let full = temp.path().join(path);
        fs::create_dir_all(full.parent().expect("parent")).expect("create dirs");
        fs::write(&full, body).expect("write fixture");
        let extraction = extract_file(&extractors, Path::new(path), body.as_bytes());
        graph.files.insert(
            path.to_string(),
            FileEntry::from_extraction(body.as_bytes(), extraction),
        );
    }
    (temp, graph)
}

fn window(temp: &TempDir, graph: &ResolvedGraph, id: &str) -> Result<SourceWindow, WindowError> {
    read_window(graph, temp.path(), id, 60)
}

#[test]
fn a_node_id_returns_exactly_its_declaration_lines() {
    let (temp, graph) = fixture(&[("src/lib.rs", LIB_RS)]);

    let result = window(&temp, &graph, "src/lib.rs#function:caller").expect("window");

    assert_eq!(
        result.text,
        "pub fn caller() {\n    let _x = 1;\n    target();\n}\n"
    );
    assert_eq!(result.path, "src/lib.rs");
    assert_eq!((result.span.line_start, result.span.line_end), (2, 5));
    assert!(!result.truncated);
}

#[test]
fn a_site_id_returns_its_line() {
    let (temp, graph) = fixture(&[("src/lib.rs", LIB_RS)]);
    let start = LIB_RS.rfind("target();").expect("call site");
    let id = format!("src/lib.rs@{start}-{}", start + "target()".len());

    let result = window(&temp, &graph, &id).expect("window");

    assert_eq!(result.text, "    target();\n");
    assert_eq!((result.span.line_start, result.span.line_end), (4, 4));
}

#[test]
fn a_file_node_id_returns_the_file_and_honours_the_line_cap() {
    let (temp, graph) = fixture(&[("src/lib.rs", LIB_RS)]);

    let capped = read_window(&graph, temp.path(), "src/lib.rs", 2).expect("window");

    assert_eq!(capped.text, "pub fn target() {}\npub fn caller() {\n");
    assert!(capped.truncated);
    assert_eq!(capped.span.line_end, 2);
    let whole = window(&temp, &graph, "src/lib.rs").expect("window");
    assert_eq!(whole.text, LIB_RS);
    assert!(!whole.truncated);
}

#[test]
fn a_changed_file_is_reported_and_no_bytes_are_returned() {
    let (temp, graph) = fixture(&[("src/lib.rs", LIB_RS)]);
    fs::write(temp.path().join("src/lib.rs"), "pub fn edited() {}\n").expect("edit");

    let result = window(&temp, &graph, "src/lib.rs#function:caller");

    assert_eq!(
        result,
        Err(WindowError::ChangedSinceSnapshot {
            path: "src/lib.rs".to_string()
        })
    );
}

#[test]
fn a_file_deleted_since_the_snapshot_reads_as_changed() {
    let (temp, graph) = fixture(&[("src/lib.rs", LIB_RS)]);
    fs::remove_file(temp.path().join("src/lib.rs")).expect("remove");

    assert!(matches!(
        window(&temp, &graph, "src/lib.rs@0-5"),
        Err(WindowError::ChangedSinceSnapshot { .. })
    ));
}

#[test]
fn an_id_naming_nothing_in_the_graph_is_unknown() {
    let (temp, mut graph) = fixture(&[("src/lib.rs", LIB_RS)]);
    graph
        .files
        .insert("gone.rs".to_string(), FileEntry::tombstone());

    for id in [
        "src/lib.rs#function:missing",
        "missing.rs@0-5",
        "src/lib.rs@a-b",
        "src/lib.rs@5",
        "gone.rs@0-1",
        "",
    ] {
        assert_eq!(
            window(&temp, &graph, id),
            Err(WindowError::UnknownId(id.to_string())),
            "id {id:?}"
        );
    }
}

#[test]
fn a_site_id_outside_the_graph_never_opens_the_path() {
    let outer = tempfile::tempdir().expect("tempdir");
    let repo = outer.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir");
    fs::write(repo.join("lib.rs"), LIB_RS).expect("write lib");
    fs::write(outer.path().join("outside.txt"), "SECRET-MARKER\n").expect("write outside");
    let extraction = extract_file(&registry(), Path::new("lib.rs"), LIB_RS.as_bytes());
    let mut graph = ResolvedGraph {
        base_revision: String::new(),
        overlaid: BTreeSet::new(),
        files: Default::default(),
    };
    graph.files.insert(
        "lib.rs".to_string(),
        FileEntry::from_extraction(LIB_RS.as_bytes(), extraction),
    );
    let absolute = format!("{}@0-5", outer.path().join("outside.txt").display());

    for id in ["../outside.txt@0-5", absolute.as_str()] {
        let result = read_window(&graph, &repo, id, 60);
        assert_eq!(
            result,
            Err(WindowError::UnknownId(id.to_string())),
            "id {id}"
        );
    }
}

#[test]
fn a_span_splitting_a_multibyte_character_does_not_panic() {
    let body = "aé😀b\nsecond\n";
    let (temp, graph) = fixture(&[("multi.txt", body)]);

    for id in [
        "multi.txt@2-3",
        "multi.txt@4-6",
        "multi.txt@0-9999",
        "multi.txt@9999-99999",
    ] {
        let result = window(&temp, &graph, id).unwrap_or_else(|error| panic!("{id}: {error}"));
        assert!(body.contains(&result.text), "{id}: {:?}", result.text);
    }
    let inner = window(&temp, &graph, "multi.txt@2-3").expect("window");
    assert_eq!(inner.text, "aé😀b\n");
    let past_eof = window(&temp, &graph, "multi.txt@9999-99999").expect("window");
    assert_eq!(past_eof.text, "");
}

#[test]
fn invalid_utf8_is_replaced() {
    let (temp, mut graph) = fixture(&[("bin.txt", "placeholder\n")]);
    let bytes = [b'a', 0xFF, b'b', b'\n'];
    fs::write(temp.path().join("bin.txt"), bytes).expect("write");
    let extraction = extract_file(&registry(), Path::new("bin.txt"), &bytes);
    graph.files.insert(
        "bin.txt".to_string(),
        FileEntry::from_extraction(&bytes, extraction),
    );

    assert_eq!(
        window(&temp, &graph, "bin.txt").expect("window").text,
        "a\u{FFFD}b\n"
    );
}

#[cfg(unix)]
#[test]
fn a_file_swapped_for_a_symlink_is_unreadable() {
    let (temp, graph) = fixture(&[("src/lib.rs", LIB_RS)]);
    let outside = temp.path().join("outside.rs");
    fs::write(&outside, LIB_RS).expect("write outside");
    fs::remove_file(temp.path().join("src/lib.rs")).expect("remove");
    std::os::unix::fs::symlink(&outside, temp.path().join("src/lib.rs")).expect("symlink");

    assert!(matches!(
        window(&temp, &graph, "src/lib.rs"),
        Err(WindowError::Unreadable { .. })
    ));
}

#[test]
fn errors_render_the_exit_message_text() {
    assert_eq!(
        WindowError::UnknownId("x@1-2".to_string()).to_string(),
        "unknown id: x@1-2"
    );
    assert_eq!(
        WindowError::ChangedSinceSnapshot {
            path: "a.rs".to_string()
        }
        .to_string(),
        "changed since snapshot: a.rs"
    );
}

#[test]
fn a_file_past_the_extraction_cap_is_changed_since_snapshot() {
    let (temp, graph) = fixture(&[("big.txt", "small\n")]);
    fs::write(
        temp.path().join("big.txt"),
        vec![b'x'; crate::context::source_graph::MAX_EXTRACTED_FILE_BYTES + 1],
    )
    .expect("grow file");

    let result = window(&temp, &graph, "big.txt@0-5");

    assert_eq!(
        result,
        Err(WindowError::ChangedSinceSnapshot {
            path: "big.txt".to_string()
        })
    );
}

#[test]
fn an_unchanged_oversized_file_returns_its_window() {
    let size = crate::context::source_graph::MAX_EXTRACTED_FILE_BYTES + 1;
    let mut body = "head\n".to_string();
    body.push_str(&"x".repeat(size - body.len()));
    let (temp, graph) = fixture(&[("big.txt", body.as_str())]);
    assert!(matches!(
        graph.files["big.txt"].coverage,
        FileCoverage::Oversized { .. }
    ));

    let result = window(&temp, &graph, "big.txt@0-4").expect("window");

    assert!(result.text.starts_with("head\n"));
}

#[test]
fn oversized_window_hashes_the_tail_and_preserves_final_lines() {
    let body = format!("head\n{}\ntail", "x".repeat(MAX_EXTRACTED_FILE_BYTES * 2));
    let (temp, graph) = fixture(&[("big.txt", &body)]);
    let tail = body.len() - 4;
    let result = window(&temp, &graph, &format!("big.txt@{tail}-{}", body.len())).unwrap();
    assert_eq!(result.text, "tail");
    assert_eq!((result.span.line_start, result.span.line_end), (3, 3));
    let huge = window(&temp, &graph, "big.txt@5-6").unwrap();
    assert_eq!(huge.text.len(), MAX_EXTRACTED_FILE_BYTES * 2 + 1);
    assert!(!huge.truncated);
    let mut changed = body.into_bytes();
    *changed.last_mut().unwrap() = b'X';
    fs::write(temp.path().join("big.txt"), changed).unwrap();
    assert!(matches!(
        window(&temp, &graph, "big.txt@0-4"),
        Err(WindowError::ChangedSinceSnapshot { .. })
    ));
}

#[test]
fn error_text_never_carries_a_control_character_from_the_id_or_path() {
    let hostile = "a\u{1b}]0;x\u{7}.rs";
    let errors = [
        WindowError::UnknownId(format!("{hostile}@0-5")),
        WindowError::ChangedSinceSnapshot {
            path: hostile.to_string(),
        },
        WindowError::Unreadable {
            path: hostile.to_string(),
            detail: format!("refused {hostile}"),
        },
    ];

    for error in errors {
        let text = error.to_string();
        assert!(!text.chars().any(char::is_control), "{text:?}");
        assert!(text.contains("a ]0;x .rs"), "{text:?}");
    }
}
