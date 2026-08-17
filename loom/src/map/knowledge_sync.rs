//! Idempotent projection of a codebase analysis into the knowledge files.
//!
//! The sections written here are DERIVED from the codebase, not authored by
//! hand: re-running the analysis and writing it back must converge on the
//! same file content, never grow it. Each `## <heading>` section in an
//! [`AnalysisResult`](crate::map::AnalysisResult) blob is spliced into its
//! tier-1 file in place, so a second run with the same findings leaves the
//! file byte-identical to the first.

use anyhow::{Context, Result};

use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile};
use crate::map::AnalysisResult;

/// Write the analysis into the knowledge files, one `## ` section at a time.
///
/// Returns the tier-1 files that were touched, in a stable order, so the
/// caller can report them.
///
/// `overwrite` false leaves a section that is already present alone; true
/// replaces it. Both modes are idempotent.
pub fn write_analysis(
    knowledge: &KnowledgeDir,
    result: &AnalysisResult,
    overwrite: bool,
) -> Result<Vec<KnowledgeFile>> {
    let blobs: [(&str, KnowledgeFile); 4] = [
        (&result.architecture, KnowledgeFile::Architecture),
        (&result.stack, KnowledgeFile::Stack),
        (&result.conventions, KnowledgeFile::Conventions),
        (&result.concerns, KnowledgeFile::Concerns),
    ];

    let mut touched = Vec::new();
    for (blob, file_type) in blobs {
        if blob.is_empty() {
            continue;
        }

        let mut wrote_any = false;
        for (heading, body) in sections(blob) {
            if !overwrite && heading_present(knowledge, file_type, &heading)? {
                continue;
            }
            knowledge.replace_section(file_type, &heading, &body)?;
            wrote_any = true;
        }

        if wrote_any {
            touched.push(file_type);
        }
    }

    Ok(touched)
}

/// Whether some line of `file_type`'s file, `trim_end()`-ed, equals
/// `## <heading>`. A missing file counts as absent.
fn heading_present(
    knowledge: &KnowledgeDir,
    file_type: KnowledgeFile,
    heading: &str,
) -> Result<bool> {
    let path = knowledge.file_path(file_type);
    if !path.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", file_type.filename()))?;
    let target_line = format!("## {heading}");
    Ok(content.lines().any(|line| line.trim_end() == target_line))
}

/// Split a rendered analysis blob into its `## <heading>` sections.
///
/// Returns `(heading, body)` pairs. Text before the first heading is ignored
/// (the analyzer never emits any). The blank separator line right after a
/// heading is dropped, and each body is `trim_end()`-ed, so a body never
/// carries a leading or trailing blank run.
fn sections(blob: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    // (heading, body lines seen so far, has real content started)
    let mut current: Option<(String, Vec<&str>, bool)> = None;

    for line in blob.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if let Some((heading, body, _)) = current.take() {
                result.push((heading, body.join("\n").trim_end().to_string()));
            }
            current = Some((heading.trim_end().to_string(), Vec::new(), false));
            continue;
        }

        if let Some((_, body, started)) = current.as_mut() {
            if !*started && line.is_empty() {
                continue;
            }
            *started = true;
            body.push(line);
        }
    }

    if let Some((heading, body, _)) = current.take() {
        result.push((heading, body.join("\n").trim_end().to_string()));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_result() -> AnalysisResult {
        AnalysisResult {
            architecture: "## Entry Points\n\nsrc/main.rs\n\n## Directory Structure\n\nsrc/\n\n"
                .to_string(),
            stack: "## Project Type\n\nRust\n\n## Key Dependencies\n\nserde\n\n".to_string(),
            conventions: "## Detected Conventions\n\n4-space indent\n\n".to_string(),
            concerns: "## Potential Concerns\n\nSome debt\n\n".to_string(),
        }
    }

    #[test]
    fn sections_splits_two_section_blob() {
        let blob = "## Entry Points\n\nsrc/main.rs\nsrc/lib.rs\n\n## Directory Structure\n\nsrc/\n  map/\n\n\n";
        let parsed = sections(blob);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "Entry Points");
        assert_eq!(parsed[0].1, "src/main.rs\nsrc/lib.rs");
        assert!(!parsed[0].1.ends_with('\n'));
        assert_eq!(parsed[1].0, "Directory Structure");
        assert_eq!(parsed[1].1, "src/\n  map/");
        assert!(!parsed[1].1.ends_with('\n'));
    }

    #[test]
    fn write_analysis_converges_on_second_run() {
        let temp = TempDir::new().unwrap();
        let knowledge = KnowledgeDir::new(temp.path());
        knowledge.initialize().unwrap();
        let result = sample_result();

        let touched_files = [
            KnowledgeFile::Architecture,
            KnowledgeFile::Stack,
            KnowledgeFile::Conventions,
            KnowledgeFile::Concerns,
        ];

        write_analysis(&knowledge, &result, true).unwrap();
        let after_first: Vec<String> = touched_files
            .iter()
            .map(|f| std::fs::read_to_string(knowledge.file_path(*f)).unwrap())
            .collect();

        write_analysis(&knowledge, &result, true).unwrap();
        for (file_type, first_content) in touched_files.iter().zip(after_first.iter()) {
            let second_content = std::fs::read_to_string(knowledge.file_path(*file_type)).unwrap();
            assert_eq!(
                &second_content,
                first_content,
                "{} changed between runs",
                file_type.filename()
            );
        }

        let arch =
            std::fs::read_to_string(knowledge.file_path(KnowledgeFile::Architecture)).unwrap();
        let count = arch
            .lines()
            .filter(|line| line.trim_end() == "## Entry Points")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn write_analysis_overwrite_false_preserves_existing_but_adds_missing() {
        let temp = TempDir::new().unwrap();
        let knowledge = KnowledgeDir::new(temp.path());
        knowledge.initialize().unwrap();

        knowledge
            .replace_section(
                KnowledgeFile::Architecture,
                "Entry Points",
                "hand-written note",
            )
            .unwrap();

        let result = AnalysisResult {
            architecture: "## Entry Points\n\nsrc/main.rs\n\n## Directory Structure\n\nsrc/\n\n"
                .to_string(),
            ..Default::default()
        };

        write_analysis(&knowledge, &result, false).unwrap();

        let content =
            std::fs::read_to_string(knowledge.file_path(KnowledgeFile::Architecture)).unwrap();
        assert!(content.contains("hand-written note"));
        assert!(!content.contains("src/main.rs"));
        assert!(content.contains("## Directory Structure"));
        assert!(content.contains("src/"));
    }

    #[test]
    fn write_analysis_overwrite_true_replaces_body_in_place() {
        let temp = TempDir::new().unwrap();
        let knowledge = KnowledgeDir::new(temp.path());
        knowledge.initialize().unwrap();

        let result_v1 = AnalysisResult {
            architecture: "## Entry Points\n\nsrc/main.rs\n\n".to_string(),
            ..Default::default()
        };
        write_analysis(&knowledge, &result_v1, true).unwrap();

        let result_v2 = AnalysisResult {
            architecture: "## Entry Points\n\nsrc/replaced.rs\n\n".to_string(),
            ..Default::default()
        };
        write_analysis(&knowledge, &result_v2, true).unwrap();

        let content =
            std::fs::read_to_string(knowledge.file_path(KnowledgeFile::Architecture)).unwrap();
        assert!(content.contains("src/replaced.rs"));
        assert!(!content.contains("src/main.rs"));

        let count = content
            .lines()
            .filter(|line| line.trim_end() == "## Entry Points")
            .count();
        assert_eq!(count, 1);
    }
}
