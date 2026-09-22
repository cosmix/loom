use std::collections::BTreeMap;

use super::clusters::{fan_in, file_facts, partition, FileFacts};
use crate::context::graph_store::FileEntry;
use crate::context::resolve::fixtures::{edge_at, file_node, func_id, graph_of, source_file};
use crate::context::source_graph::{EdgeProvenance, SourceEdgeKind, UNRESOLVED_TARGET};

fn fact(path: &str, content_hash: &str, symbols: usize) -> FileFacts {
    FileFacts {
        path: path.to_string(),
        content_hash: content_hash.to_string(),
        symbols,
    }
}

/// Root with 3 direct files; `src/` splits into `a` (30 files), `b` (15
/// files), and `c` (5 files, below the residual threshold), with no direct
/// files of its own.
fn split_fixture() -> Vec<FileFacts> {
    let mut files = Vec::new();
    for i in 0..3 {
        files.push(fact(&format!("root{i}.rs"), "sha256:h", 0));
    }
    for i in 0..30 {
        files.push(fact(&format!("src/a/{i}.rs"), "sha256:h", 0));
    }
    for i in 0..15 {
        files.push(fact(&format!("src/b/{i}.rs"), "sha256:h", 0));
    }
    for i in 0..5 {
        files.push(fact(&format!("src/c/{i}.rs"), "sha256:h", 0));
    }
    files
}

#[test]
fn ten_files_two_dirs_form_one_cluster() {
    let mut files: Vec<FileFacts> = (0..5)
        .map(|i| fact(&format!("a/{i}.rs"), "sha256:h", 0))
        .collect();
    files.extend((0..5).map(|i| fact(&format!("b/{i}.rs"), "sha256:h", 0)));

    let clusters = partition(&files, &BTreeMap::new());

    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].id, ".");
    assert_eq!(clusters[0].files.len(), 10);
}

#[test]
fn split_case_partitions_by_directory() {
    let files = split_fixture();
    let clusters = partition(&files, &BTreeMap::new());

    let ids: Vec<&str> = clusters.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec![".", "src", "src/a", "src/b"]);

    let root = clusters.iter().find(|c| c.id == ".").unwrap();
    assert_eq!(root.files.len(), 3);
    assert!(root.files.iter().all(|f| !f.contains('/')));

    // src/c folds into the src residual cluster.
    let src = clusters.iter().find(|c| c.id == "src").unwrap();
    assert_eq!(src.files.len(), 5);
    assert!(src.files.iter().all(|f| f.starts_with("src/c/")));

    let src_a = clusters.iter().find(|c| c.id == "src/a").unwrap();
    assert_eq!(src_a.files.len(), 30);

    let src_b = clusters.iter().find(|c| c.id == "src/b").unwrap();
    assert_eq!(src_b.files.len(), 15);
}

#[test]
fn every_file_lands_in_exactly_one_cluster() {
    let files = split_fixture();
    let clusters = partition(&files, &BTreeMap::new());

    let mut landed: Vec<&str> = clusters
        .iter()
        .flat_map(|c| c.files.iter().map(String::as_str))
        .collect();
    landed.sort_unstable();
    let mut deduped = landed.clone();
    deduped.dedup();
    assert_eq!(
        landed.len(),
        deduped.len(),
        "a file landed in more than one cluster"
    );

    let mut expected: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    expected.sort_unstable();
    assert_eq!(landed, expected);
}

#[test]
fn digest_stable_under_shuffle_changes_on_content_or_addition() {
    let files = vec![fact("a.rs", "sha256:1", 0), fact("b.rs", "sha256:2", 0)];
    let shuffled = vec![files[1].clone(), files[0].clone()];

    let baseline = partition(&files, &BTreeMap::new());
    let from_shuffled = partition(&shuffled, &BTreeMap::new());
    assert_eq!(baseline[0].digest, from_shuffled[0].digest);

    let mut changed = files.clone();
    changed[0].content_hash = "sha256:different".to_string();
    let from_changed = partition(&changed, &BTreeMap::new());
    assert_ne!(baseline[0].digest, from_changed[0].digest);

    let mut added = files.clone();
    added.push(fact("c.rs", "sha256:3", 0));
    let from_added = partition(&added, &BTreeMap::new());
    assert_ne!(baseline[0].digest, from_added[0].digest);
}

#[test]
fn hot_files_top_three_by_fan_in_ties_by_path() {
    let files = vec![
        fact("a.rs", "sha256:h", 0),
        fact("b.rs", "sha256:h", 0),
        fact("c.rs", "sha256:h", 0),
        fact("d.rs", "sha256:h", 0),
        fact("e.rs", "sha256:h", 0),
    ];
    let mut fan = BTreeMap::new();
    fan.insert("a.rs".to_string(), 5);
    fan.insert("b.rs".to_string(), 5); // ties with a.rs; a.rs wins on path
    fan.insert("c.rs".to_string(), 3);
    fan.insert("d.rs".to_string(), 0); // excluded: fan-in is not > 0
                                       // e.rs has no entry at all, equivalent to 0: also excluded.

    let clusters = partition(&files, &fan);

    assert_eq!(
        clusters[0].hot_files,
        vec![
            ("a.rs".to_string(), 5),
            ("b.rs".to_string(), 5),
            ("c.rs".to_string(), 3),
        ]
    );
}

#[test]
fn fan_in_counts_cross_file_edge_and_skips_same_file_and_unresolved() {
    let cross_from = func_id("a.rs", "foo");
    let cross_to = func_id("b.rs", "bar");
    let same_to = func_id("a.rs", "other");

    let edges_a = vec![
        edge_at(
            &cross_from,
            &cross_to,
            SourceEdgeKind::Calls,
            EdgeProvenance::Parser,
            1.0,
        ),
        edge_at(
            &cross_from,
            &same_to,
            SourceEdgeKind::Calls,
            EdgeProvenance::Parser,
            1.0,
        ),
        edge_at(
            &cross_from,
            UNRESOLVED_TARGET,
            SourceEdgeKind::Calls,
            EdgeProvenance::Inferred,
            0.2,
        ),
    ];
    let graph = graph_of(vec![
        ("a.rs", source_file("a.rs", &["foo", "other"], edges_a)),
        ("b.rs", source_file("b.rs", &["bar"], Vec::new())),
    ]);

    let counts = fan_in(&graph);

    assert_eq!(counts.get("b.rs"), Some(&1));
    assert_eq!(counts.get("a.rs"), None);
}

#[test]
fn file_facts_excludes_deleted_and_counts_symbols_only() {
    let graph = graph_of(vec![
        ("a.rs", source_file("a.rs", &["foo", "bar"], Vec::new())),
        ("gone.rs", FileEntry::tombstone()),
    ]);

    let facts = file_facts(&graph);

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].path, "a.rs");
    // 2 function symbols; the whole-file node is not counted.
    assert_eq!(facts[0].symbols, 2);
}

#[test]
fn file_facts_excludes_knowledge_prefix() {
    let graph = graph_of(vec![
        (
            "doc/loom/knowledge/architecture.md",
            source_file("doc/loom/knowledge/architecture.md", &[], Vec::new()),
        ),
        (
            "doc/loom/knowledge/.bootstrap-receipt.json",
            source_file(
                "doc/loom/knowledge/.bootstrap-receipt.json",
                &[],
                Vec::new(),
            ),
        ),
        (
            "src/normal.rs",
            source_file("src/normal.rs", &[], Vec::new()),
        ),
    ]);

    let facts = file_facts(&graph);

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].path, "src/normal.rs");
}

/// Sanity check that the fixtures' file node id is a real, resolvable node —
/// otherwise `fan_in_counts_cross_file_edge_and_skips_same_file_and_unresolved`
/// would pass for the wrong reason (every edge unresolved).
#[test]
fn fixture_file_node_is_present_in_graph() {
    let graph = graph_of(vec![("a.rs", source_file("a.rs", &["foo"], Vec::new()))]);
    assert!(graph.node(&file_node("a.rs").id).is_some());
}
