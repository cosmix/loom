//! Tests for the parent module.

use super::*;
use crate::fs::knowledge::dir::KnowledgeDir;
use tempfile::TempDir;

#[test]
fn test_legacy_layout_advisory_and_no_aggregate_reason() {
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();
    // Downgrade to the flat layout a pre-hierarchy project has on disk.
    std::fs::remove_file(knowledge.index_path()).unwrap();

    // Push every tier-1 file well past a huge line cap so total_lines is
    // large too — the aggregate must never appear as a reason.
    let mut big = String::from("## Big Section\n\n");
    for i in 0..300 {
        big.push_str(&format!("- Line {i}\n"));
    }
    for file in KnowledgeFile::all() {
        knowledge.append(*file, &big).unwrap();
    }

    let metrics = analyze_gc_metrics(knowledge.root(), 10_000, 10_000).unwrap();
    assert_eq!(metrics.layout, KnowledgeLayout::Legacy);
    assert!(metrics.total_lines > 800);
    assert_eq!(
        metrics
            .reasons
            .iter()
            .filter(|r| r.contains("flat layout"))
            .count(),
        1,
        "exactly one flat-layout advisory expected"
    );
    assert!(metrics
        .reasons
        .iter()
        .any(|r| r.contains("loom knowledge gc")));
    assert!(
        !metrics
            .reasons
            .iter()
            .any(|r| r.to_lowercase().contains("total")),
        "there is no aggregate-lines reason: {:?}",
        metrics.reasons
    );
}

#[test]
fn test_oversized_section_detection() {
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();

    let mut big_section = String::from("## Big\n\n");
    for i in 0..(SECTION_EXTRACT_THRESHOLD + 5) {
        big_section.push_str(&format!("- Line {i}\n"));
    }
    knowledge
        .append(KnowledgeFile::Patterns, &big_section)
        .unwrap();
    knowledge
        .append(KnowledgeFile::Patterns, "## Small\n\n- one\n- two")
        .unwrap();

    let metrics = analyze_gc_metrics(knowledge.root(), 10_000, 10_000).unwrap();
    let patterns = metrics
        .tier1
        .iter()
        .find(|t| t.file_type == KnowledgeFile::Patterns)
        .unwrap();
    assert!(patterns.oversized_sections.iter().any(|(h, _)| h == "Big"));
    assert!(!patterns
        .oversized_sections
        .iter()
        .any(|(h, _)| h == "Small"));
}

#[test]
fn test_broken_link_and_orphan_topic_detection() {
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();
    let root = knowledge.root().to_path_buf();

    // Tier-1 links to a topic that does not exist on disk.
    knowledge
        .append(
            KnowledgeFile::Architecture,
            "## See Also\n\nSee [merge flow](architecture/merge-flow.md) for detail.",
        )
        .unwrap();

    // An unrelated topic exists but nothing links to it.
    fs::create_dir_all(root.join("patterns")).unwrap();
    fs::write(
        root.join("patterns").join("orphan-topic.md"),
        "# Orphan Topic\n\n> Nothing links here.\n",
    )
    .unwrap();

    let metrics = analyze_gc_metrics(&root, 10_000, 10_000).unwrap();
    assert_eq!(
        metrics.broken_links(),
        vec![("architecture.md", "architecture/merge-flow.md")]
    );
    let orphans: Vec<_> = metrics.topics.iter().filter(|t| t.is_orphan).collect();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].slug, "orphan-topic");
}

#[test]
fn test_hierarchical_index_stale_when_missing_entries() {
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();
    let root = knowledge.root().to_path_buf();

    fs::create_dir_all(root.join("architecture")).unwrap();
    fs::write(
        root.join("architecture").join("merge-flow.md"),
        "# Merge Flow\n\n> details\n",
    )
    .unwrap();
    // Hand-written stale INDEX.md that mentions nothing.
    fs::write(root.join(INDEX_FILENAME), "stale index, mentions nothing").unwrap();

    let metrics = analyze_gc_metrics(&root, 10_000, 10_000).unwrap();
    assert_eq!(metrics.layout, KnowledgeLayout::Hierarchical);
    assert!(metrics.index_stale);
    assert!(metrics
        .reasons
        .iter()
        .any(|r| r.contains("INDEX.md is stale")));
    // Hierarchical layout must NOT carry the legacy flat-layout advisory.
    assert!(!metrics.reasons.iter().any(|r| r.contains("flat layout")));
}

#[test]
fn test_h3_headings_are_not_treated_as_h2_sections() {
    // Regression guard: "### Foo" must NOT match the "## " prefix, so an H2
    // section is measured through its H3 subsections rather than truncated,
    // and no advisory ever names a phantom "# Foo" heading.
    assert!(!"### Core Abstractions".starts_with("## "));
    assert_eq!("### Core Abstractions".strip_prefix("## "), None);

    let mut content = String::from("## Big\n\n");
    for i in 0..20 {
        content.push_str(&format!("- a{i}\n"));
    }
    content.push_str("\n### Subsection\n\n");
    for i in 0..30 {
        content.push_str(&format!("- b{i}\n"));
    }
    let sections = find_oversized_sections(&content);
    assert_eq!(
        sections.len(),
        1,
        "the H3 must not split the H2 section: {sections:?}"
    );
    assert_eq!(sections[0].0, "Big");
    assert!(
        sections[0].1 > 50,
        "H2 body must span its H3 subsections, got {}",
        sections[0].1
    );
}

#[test]
fn test_ordinary_repo_links_are_not_broken_topic_links() {
    // Only links into a real category directory are topic pointers. An
    // ordinary two-segment repo link must not be reported as a missing topic,
    // which would otherwise force gc_recommended on a clean hierarchical dir.
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();

    knowledge
        .append(
            KnowledgeFile::Architecture,
            "## Links\n\nSee [readme](loom/README.md) and [design](docs/design.md).",
        )
        .unwrap();

    let metrics = analyze_gc_metrics(knowledge.root(), 10_000, 10_000).unwrap();
    assert!(
        metrics.broken_links().is_empty(),
        "non-category links must not count as broken topics: {:?}",
        metrics.broken_links()
    );
    assert!(!metrics.reasons.iter().any(|r| r.contains("missing topic")));
}
