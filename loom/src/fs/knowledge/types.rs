//! Knowledge file type definitions.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

/// Filename of the generated knowledge index (tier 0).
pub const INDEX_FILENAME: &str = "INDEX.md";

/// Layout of a knowledge directory.
///
/// `Legacy` is the flat layout every existing loom project already has on disk:
/// the seven tier-1 files and nothing else. `Hierarchical` adds the generated
/// `INDEX.md` (tier 0) and per-category tier-2 topic directories. Legacy
/// directories keep working unchanged — migration is opt-in via
/// `loom knowledge index` or `loom knowledge gc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeLayout {
    /// Flat: tier-1 files only, no generated `INDEX.md`.
    Legacy,
    /// Tiered: generated `INDEX.md` + tier-1 summaries + tier-2 topic files.
    Hierarchical,
}

/// Known knowledge file types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeFile {
    Architecture,
    EntryPoints,
    Patterns,
    Conventions,
    Mistakes,
    Stack,
    Concerns,
}

impl KnowledgeFile {
    /// Get the filename for this knowledge file type
    pub fn filename(&self) -> &'static str {
        match self {
            KnowledgeFile::Architecture => "architecture.md",
            KnowledgeFile::EntryPoints => "entry-points.md",
            KnowledgeFile::Patterns => "patterns.md",
            KnowledgeFile::Conventions => "conventions.md",
            KnowledgeFile::Mistakes => "mistakes.md",
            KnowledgeFile::Stack => "stack.md",
            KnowledgeFile::Concerns => "concerns.md",
        }
    }

    /// Get a description of what this file contains
    pub fn description(&self) -> &'static str {
        match self {
            KnowledgeFile::Architecture => {
                "High-level component relationships, data flow, module dependencies"
            }
            KnowledgeFile::EntryPoints => "Key files agents should read first",
            KnowledgeFile::Patterns => "Architectural patterns discovered in the codebase",
            KnowledgeFile::Conventions => "Coding conventions discovered in the codebase",
            KnowledgeFile::Mistakes => "Mistakes made and lessons learned - what to avoid",
            KnowledgeFile::Stack => "Dependencies, frameworks, and tooling used in the project",
            KnowledgeFile::Concerns => "Technical debt, warnings, and issues to address",
        }
    }

    /// Directory name holding this category's tier-2 topic files
    /// (the filename without its `.md` extension).
    pub fn dir_name(&self) -> &'static str {
        self.filename().trim_end_matches(".md")
    }

    /// Parse a user-supplied tier-1 name: exact filename, bare name, or alias.
    ///
    /// Accepts `patterns.md`, `patterns`, `pattern`, `deps`, ... — the alias set
    /// the `loom knowledge` CLI has always accepted.
    pub fn parse(name: &str) -> Option<Self> {
        if let Some(file_type) = Self::from_filename(name) {
            return Some(file_type);
        }

        if let Some(file_type) = Self::from_filename(&format!("{name}.md")) {
            return Some(file_type);
        }

        match name.to_lowercase().as_str() {
            "arch" | "architecture" | "map" | "overview" => Some(KnowledgeFile::Architecture),
            "entry" | "entries" | "entry-point" | "entrypoints" => Some(KnowledgeFile::EntryPoints),
            "pattern" => Some(KnowledgeFile::Patterns),
            "convention" | "conventions" | "code" | "coding" => Some(KnowledgeFile::Conventions),
            "mistake" | "mistakes" | "lessons" | "lesson" => Some(KnowledgeFile::Mistakes),
            "stack" | "deps" | "dependencies" | "tech" | "tooling" => Some(KnowledgeFile::Stack),
            "concerns" | "concern" | "debt" | "issues" | "warnings" => {
                Some(KnowledgeFile::Concerns)
            }
            _ => None,
        }
    }

    /// Parse from filename
    pub fn from_filename(filename: &str) -> Option<Self> {
        match filename {
            "architecture.md" => Some(KnowledgeFile::Architecture),
            "entry-points.md" => Some(KnowledgeFile::EntryPoints),
            "patterns.md" => Some(KnowledgeFile::Patterns),
            "conventions.md" => Some(KnowledgeFile::Conventions),
            "mistakes.md" => Some(KnowledgeFile::Mistakes),
            "stack.md" => Some(KnowledgeFile::Stack),
            "concerns.md" => Some(KnowledgeFile::Concerns),
            _ => None,
        }
    }

    /// All known knowledge file types
    pub fn all() -> &'static [KnowledgeFile] {
        &[
            KnowledgeFile::Architecture,
            KnowledgeFile::EntryPoints,
            KnowledgeFile::Patterns,
            KnowledgeFile::Conventions,
            KnowledgeFile::Mistakes,
            KnowledgeFile::Stack,
            KnowledgeFile::Concerns,
        ]
    }
}

/// A resolved knowledge write/read target.
///
/// Tier 1 is one of the seven curated summary files; tier 2 is a topic file
/// living under that file's category directory (`architecture/merge-flow.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnowledgeTarget {
    /// A tier-1 summary file, e.g. `patterns.md`.
    Tier1(KnowledgeFile),
    /// A tier-2 topic file, e.g. `architecture/merge-flow.md`.
    Topic {
        category: KnowledgeFile,
        slug: String,
    },
}

impl KnowledgeTarget {
    /// Parse a CLI target: a tier-1 name/alias, or `<category>/<slug>`.
    ///
    /// The slug is validated with [`crate::validation::validate_id`], which
    /// rejects path traversal, reserved names, and over-long names.
    pub fn parse(target: &str) -> Result<Self> {
        let target = target.trim();

        if let Some((category, slug)) = target.split_once('/') {
            let category = KnowledgeFile::parse(category)
                .ok_or_else(|| unknown_target_error(target, category))?;
            let slug = slug.strip_suffix(".md").unwrap_or(slug);
            if slug.contains('/') {
                bail!(
                    "Invalid knowledge target '{target}': topics are one level deep \
                     (use '<category>/<slug>')"
                );
            }
            crate::validation::validate_id(slug)
                .with_context(|| format!("Invalid topic slug in '{target}'"))?;
            return Ok(KnowledgeTarget::Topic {
                category,
                slug: slug.to_string(),
            });
        }

        KnowledgeFile::parse(target)
            .map(KnowledgeTarget::Tier1)
            .ok_or_else(|| unknown_target_error(target, target))
    }

    /// Path of this target relative to the knowledge root.
    pub fn relative_path(&self) -> PathBuf {
        match self {
            KnowledgeTarget::Tier1(file_type) => PathBuf::from(file_type.filename()),
            KnowledgeTarget::Topic { category, slug } => {
                PathBuf::from(category.dir_name()).join(format!("{slug}.md"))
            }
        }
    }

    /// Human-readable name, e.g. `patterns.md` or `architecture/merge-flow.md`.
    pub fn display_name(&self) -> String {
        self.relative_path().to_string_lossy().into_owned()
    }
}

fn unknown_target_error(target: &str, unknown_part: &str) -> anyhow::Error {
    let valid_files: Vec<_> = KnowledgeFile::all().iter().map(|f| f.filename()).collect();
    let scope = if unknown_part == target {
        String::new()
    } else {
        format!(" (in '{target}')")
    };
    anyhow::anyhow!(
        "Unknown knowledge file: '{unknown_part}'{scope}. Valid files: {}. \
         For a tier-2 topic use '<category>/<slug>'.",
        valid_files.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knowledge_file_types() {
        assert_eq!(KnowledgeFile::Architecture.filename(), "architecture.md");
        assert_eq!(KnowledgeFile::EntryPoints.filename(), "entry-points.md");
        assert_eq!(KnowledgeFile::Patterns.filename(), "patterns.md");
        assert_eq!(KnowledgeFile::Conventions.filename(), "conventions.md");
        assert_eq!(KnowledgeFile::Mistakes.filename(), "mistakes.md");
        assert_eq!(KnowledgeFile::Stack.filename(), "stack.md");
        assert_eq!(KnowledgeFile::Concerns.filename(), "concerns.md");
    }

    #[test]
    fn test_knowledge_file_from_filename() {
        assert_eq!(
            KnowledgeFile::from_filename("architecture.md"),
            Some(KnowledgeFile::Architecture)
        );
        assert_eq!(
            KnowledgeFile::from_filename("entry-points.md"),
            Some(KnowledgeFile::EntryPoints)
        );
        assert_eq!(
            KnowledgeFile::from_filename("patterns.md"),
            Some(KnowledgeFile::Patterns)
        );
        assert_eq!(
            KnowledgeFile::from_filename("mistakes.md"),
            Some(KnowledgeFile::Mistakes)
        );
        assert_eq!(
            KnowledgeFile::from_filename("stack.md"),
            Some(KnowledgeFile::Stack)
        );
        assert_eq!(
            KnowledgeFile::from_filename("concerns.md"),
            Some(KnowledgeFile::Concerns)
        );
        assert_eq!(KnowledgeFile::from_filename("unknown.md"), None);
    }

    #[test]
    fn test_knowledge_target_parse_tier1_round_trips() {
        for input in ["patterns", "patterns.md", "pattern"] {
            let target = KnowledgeTarget::parse(input).unwrap();
            assert_eq!(target, KnowledgeTarget::Tier1(KnowledgeFile::Patterns));
            assert_eq!(target.relative_path(), PathBuf::from("patterns.md"));
        }
    }

    #[test]
    fn test_knowledge_target_parse_topic_round_trips() {
        for input in ["architecture/merge-flow", "architecture/merge-flow.md"] {
            let target = KnowledgeTarget::parse(input).unwrap();
            assert_eq!(
                target,
                KnowledgeTarget::Topic {
                    category: KnowledgeFile::Architecture,
                    slug: "merge-flow".to_string(),
                }
            );
            assert_eq!(
                target.relative_path(),
                PathBuf::from("architecture/merge-flow.md")
            );
        }
    }

    #[test]
    fn test_knowledge_target_parse_rejects_traversal() {
        assert!(KnowledgeTarget::parse("architecture/../../etc/passwd").is_err());
        assert!(KnowledgeTarget::parse("architecture/..").is_err());
        assert!(KnowledgeTarget::parse("../secrets").is_err());
    }

    #[test]
    fn test_knowledge_target_parse_rejects_unknown_category() {
        assert!(KnowledgeTarget::parse("bogus/topic").is_err());
        assert!(KnowledgeTarget::parse("bogus").is_err());
    }
}
