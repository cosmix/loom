//! Knowledge file type definitions.

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
}
