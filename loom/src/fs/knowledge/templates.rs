//! Default scaffold content for knowledge files.
//!
//! A tier-1 file gets the curated template it has always had; a tier-2 topic
//! gets a minimal `# Title` + `> blurb` header so [`super::index::scan_topics`]
//! can describe it in the generated index.

use super::index;
use super::types::{KnowledgeFile, KnowledgeTarget};

/// Default scaffold for a target that does not exist yet.
pub fn default_scaffold(target: &KnowledgeTarget) -> String {
    match target {
        KnowledgeTarget::Tier1(file_type) => default_content(*file_type),
        KnowledgeTarget::Topic { category, slug } => {
            let title = index::title_case(slug);
            format!(
                "# {title}\n\n> Topic notes for the {} knowledge area.\n",
                category.dir_name()
            )
        }
    }
}

/// Get default content for a knowledge file type
pub fn default_content(file_type: KnowledgeFile) -> String {
    match file_type {
        KnowledgeFile::Architecture => r#"# Architecture

> High-level component relationships, data flow, and module dependencies.
> This file is append-only - agents add discoveries, never delete.

(Add architecture diagrams and component relationships as you discover them)
"#
        .to_string(),
        KnowledgeFile::EntryPoints => r#"# Entry Points

> Key files agents should read first to understand the codebase.
> This file is append-only - agents add discoveries, never delete.

(Add entry points as you discover them)
"#
        .to_string(),
        KnowledgeFile::Patterns => r#"# Architectural Patterns

> Discovered patterns in the codebase that help agents understand how things work.
> This file is append-only - agents add discoveries, never delete.

(Add patterns as you discover them)
"#
        .to_string(),
        KnowledgeFile::Conventions => r#"# Coding Conventions

> Discovered coding conventions in the codebase.
> This file is append-only - agents add discoveries, never delete.

(Add conventions as you discover them)
"#
        .to_string(),
        KnowledgeFile::Mistakes => r#"# Mistakes & Lessons Learned

> Record mistakes made during development and how to avoid them.
> This file is append-only - agents add discoveries, never delete.
>
> Format: Describe what went wrong, why, and how to avoid it next time.

(Add mistakes and lessons as you encounter them)
"#
        .to_string(),
        KnowledgeFile::Stack => r#"# Stack & Dependencies

> Project technology stack, frameworks, and key dependencies.
> This file is append-only - agents add discoveries, never delete.

(Add stack information as you discover it)
"#
        .to_string(),
        KnowledgeFile::Concerns => r#"# Concerns & Technical Debt

> Technical debt, warnings, issues, and improvements needed.
> This file is append-only - agents add discoveries, never delete.

(Add concerns as you discover them)
"#
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier1_scaffold_matches_default_content() {
        let target = KnowledgeTarget::Tier1(KnowledgeFile::Patterns);
        assert_eq!(
            default_scaffold(&target),
            default_content(KnowledgeFile::Patterns)
        );
    }

    #[test]
    fn test_topic_scaffold_has_title_and_blurb() {
        let target = KnowledgeTarget::Topic {
            category: KnowledgeFile::Architecture,
            slug: "merge-flow".to_string(),
        };
        let scaffold = default_scaffold(&target);
        assert!(scaffold.starts_with("# Merge Flow\n"));
        assert!(scaffold.contains("> Topic notes for the architecture knowledge area."));
    }
}
