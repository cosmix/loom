//! Tests for the parent module.

use super::*;
use tempfile::TempDir;

#[test]
fn test_knowledge_dir_initialize() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    let knowledge = KnowledgeDir::new(project_root);
    assert!(!knowledge.exists());

    knowledge.initialize().unwrap();
    assert!(knowledge.exists());
    assert!(project_root.join("doc/loom/knowledge").exists());

    // Check all files were created
    for file_type in KnowledgeFile::all() {
        let path = knowledge.file_path(*file_type);
        assert!(path.exists(), "File should exist: {}", file_type.filename());
    }
}

#[test]
fn test_knowledge_append() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    let knowledge = KnowledgeDir::new(project_root);
    knowledge.initialize().unwrap();

    // Append to entry-points
    knowledge
        .append(
            KnowledgeFile::EntryPoints,
            "## New Section\n\n- New entry point",
        )
        .unwrap();

    let content = knowledge.read(KnowledgeFile::EntryPoints).unwrap();
    assert!(content.contains("## New Section"));
    assert!(content.contains("- New entry point"));
}

#[test]
fn test_generate_summary() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    let knowledge = KnowledgeDir::new(project_root);
    knowledge.initialize().unwrap();

    // Add some content
    knowledge
        .append(
            KnowledgeFile::EntryPoints,
            "## CLI Entry Point\n\n- main.rs - CLI definition",
        )
        .unwrap();

    let summary = knowledge.generate_summary().unwrap();
    assert!(summary.contains("Knowledge Summary"));
    assert!(summary.contains("CLI Entry Point"));
}

#[test]
fn test_initialize_idempotent() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    let knowledge = KnowledgeDir::new(project_root);
    knowledge.initialize().unwrap();

    // Append extra content to a file
    knowledge
        .append(KnowledgeFile::Mistakes, "## A Mistake\n\nDon't do this")
        .unwrap();
    let content_before = knowledge.read(KnowledgeFile::Mistakes).unwrap();

    // Re-initialize should NOT overwrite existing files
    knowledge.initialize().unwrap();
    let content_after = knowledge.read(KnowledgeFile::Mistakes).unwrap();
    assert_eq!(
        content_before, content_after,
        "initialize() must not overwrite existing files"
    );
}

#[test]
fn test_replace_section_existing() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    let knowledge = KnowledgeDir::new(project_root);
    knowledge.initialize().unwrap();

    // Add two sections
    knowledge
        .append(
            KnowledgeFile::Patterns,
            "## Section A\n\nOriginal A content",
        )
        .unwrap();
    knowledge
        .append(
            KnowledgeFile::Patterns,
            "## Section B\n\nOriginal B content",
        )
        .unwrap();

    // Replace Section A
    knowledge
        .replace_section(KnowledgeFile::Patterns, "Section A", "Updated A content")
        .unwrap();

    let content = knowledge.read(KnowledgeFile::Patterns).unwrap();
    assert!(
        content.contains("Updated A content"),
        "Should contain updated content"
    );
    assert!(
        !content.contains("Original A content"),
        "Should not contain old content"
    );
    // Section B should be untouched
    assert!(
        content.contains("Original B content"),
        "Section B should be preserved"
    );
}

#[test]
fn test_replace_section_not_found_appends() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    let knowledge = KnowledgeDir::new(project_root);
    knowledge.initialize().unwrap();

    knowledge
        .replace_section(KnowledgeFile::Patterns, "New Heading", "Brand new content")
        .unwrap();

    let content = knowledge.read(KnowledgeFile::Patterns).unwrap();
    assert!(content.contains("## New Heading"));
    assert!(content.contains("Brand new content"));
}

#[test]
fn test_replace_section_at_eof() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    let knowledge = KnowledgeDir::new(project_root);
    knowledge.initialize().unwrap();

    // Add a section that is at the end of the file (no following ## heading)
    knowledge
        .append(
            KnowledgeFile::Patterns,
            "## Last Section\n\nOld last content",
        )
        .unwrap();

    knowledge
        .replace_section(KnowledgeFile::Patterns, "Last Section", "New last content")
        .unwrap();

    let content = knowledge.read(KnowledgeFile::Patterns).unwrap();
    assert!(content.contains("New last content"));
    assert!(!content.contains("Old last content"));
}

#[test]
fn test_replace_section_exact_heading_match() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    let knowledge = KnowledgeDir::new(project_root);
    knowledge.initialize().unwrap();

    // Add sections with similar names
    knowledge
        .append(KnowledgeFile::Patterns, "## Merge Flow\n\nMerge content")
        .unwrap();
    knowledge
        .append(
            KnowledgeFile::Patterns,
            "## Merge Flow Extended\n\nExtended content",
        )
        .unwrap();

    // Replace only the exact match
    knowledge
        .replace_section(KnowledgeFile::Patterns, "Merge Flow", "Updated merge")
        .unwrap();

    let content = knowledge.read(KnowledgeFile::Patterns).unwrap();
    assert!(content.contains("Updated merge"));
    // The "Extended" section should be preserved
    assert!(
        content.contains("Extended content"),
        "Exact match should not affect similar-named sections"
    );
}

#[test]
fn test_append_target_scaffolds_new_topic() {
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();

    let target = KnowledgeTarget::parse("architecture/merge-flow").unwrap();
    knowledge.append_target(&target, "Body content.").unwrap();

    let content = knowledge.read_target(&target).unwrap();
    assert!(content.starts_with("# Merge Flow\n"));
    assert!(content.contains("> Topic notes for the architecture knowledge area."));
    assert!(content.contains("Body content."));
}

#[test]
fn test_append_target_refreshes_index_only_when_hierarchical() {
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();

    let target = KnowledgeTarget::parse("architecture/merge-flow").unwrap();

    // Downgrade to the flat layout a pre-hierarchy project has on disk.
    std::fs::remove_file(knowledge.index_path()).unwrap();

    // Legacy layout: no INDEX.md before or after.
    assert_eq!(knowledge.layout(), KnowledgeLayout::Legacy);
    knowledge.append_target(&target, "Body content.").unwrap();
    assert!(
        !knowledge.index_path().exists(),
        "must not create INDEX.md in legacy layout"
    );

    // Switch to hierarchical layout and append again: INDEX.md must now
    // be refreshed to mention the topic.
    knowledge.write_index().unwrap();
    assert_eq!(knowledge.layout(), KnowledgeLayout::Hierarchical);
    knowledge.append_target(&target, "More content.").unwrap();
    let index = knowledge.read_index().unwrap();
    assert!(index.contains("architecture/merge-flow.md"));
}

#[test]
fn test_initialize_creates_index_for_fresh_dir() {
    // A brand-new knowledge dir starts hierarchical, so every entry point that
    // calls initialize() (`loom init`, and the implicit init on
    // `loom knowledge update`) produces the tiered layout.
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();

    assert!(knowledge.index_path().exists());
    assert_eq!(knowledge.layout(), KnowledgeLayout::Hierarchical);
}

#[test]
fn test_initialize_never_migrates_an_existing_flat_dir() {
    // The compatibility contract: an existing flat dir must never gain an
    // INDEX.md behind the user's back, even though initialize() is called on
    // it repeatedly (e.g. the implicit init inside `loom knowledge update`).
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();
    std::fs::remove_file(knowledge.index_path()).unwrap();

    knowledge.initialize().unwrap();

    assert!(
        !knowledge.index_path().exists(),
        "initialize() must not migrate an existing flat knowledge dir"
    );
    assert_eq!(knowledge.layout(), KnowledgeLayout::Legacy);
}

/// Exact-output pins for the four splice paths. The tier-1 write path was
/// refactored into `splice_section` + `append_target` during the hierarchy
/// work; these assert full strings (not `contains`) so any whitespace or
/// trailing-newline drift fails loudly.
mod exact_output {
    use super::*;

    fn patterns_scaffold() -> String {
        crate::fs::knowledge::templates::default_content(KnowledgeFile::Patterns)
    }

    #[test]
    fn append_to_empty_file_uses_scaffold() {
        let temp = TempDir::new().unwrap();
        let knowledge = KnowledgeDir::new(temp.path());
        knowledge.initialize().unwrap();
        std::fs::write(knowledge.file_path(KnowledgeFile::Patterns), "").unwrap();

        knowledge
            .append(KnowledgeFile::Patterns, "## A\n\nBody")
            .unwrap();

        assert_eq!(
            knowledge.read(KnowledgeFile::Patterns).unwrap(),
            format!("{}\n## A\n\nBody\n", patterns_scaffold())
        );
    }

    #[test]
    fn replace_section_mid_file_is_exact() {
        let temp = TempDir::new().unwrap();
        let knowledge = KnowledgeDir::new(temp.path());
        knowledge.initialize().unwrap();
        std::fs::write(
            knowledge.file_path(KnowledgeFile::Patterns),
            "# T\n\n## A\n\nold a\n\n## B\n\nkeep b\n",
        )
        .unwrap();

        knowledge
            .replace_section(KnowledgeFile::Patterns, "A", "new a")
            .unwrap();

        assert_eq!(
            knowledge.read(KnowledgeFile::Patterns).unwrap(),
            "# T\n\n## A\n\nnew a\n\n## B\n\nkeep b\n"
        );
    }

    #[test]
    fn replace_section_at_eof_is_exact() {
        let temp = TempDir::new().unwrap();
        let knowledge = KnowledgeDir::new(temp.path());
        knowledge.initialize().unwrap();
        std::fs::write(
            knowledge.file_path(KnowledgeFile::Patterns),
            "# T\n\n## Last\n\nold\n",
        )
        .unwrap();

        knowledge
            .replace_section(KnowledgeFile::Patterns, "Last", "new")
            .unwrap();

        assert_eq!(
            knowledge.read(KnowledgeFile::Patterns).unwrap(),
            "# T\n\n## Last\n\nnew\n"
        );
    }

    #[test]
    fn replace_section_not_found_appends_exactly() {
        let temp = TempDir::new().unwrap();
        let knowledge = KnowledgeDir::new(temp.path());
        knowledge.initialize().unwrap();
        std::fs::write(knowledge.file_path(KnowledgeFile::Patterns), "# T\n").unwrap();

        knowledge
            .replace_section(KnowledgeFile::Patterns, "New", "body")
            .unwrap();

        assert_eq!(
            knowledge.read(KnowledgeFile::Patterns).unwrap(),
            "# T\n\n## New\n\nbody\n"
        );
    }
}
