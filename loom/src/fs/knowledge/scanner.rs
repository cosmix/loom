//! Shared knowledge-document fence and ATX heading scanner.
//!
//! Fences may have up to three leading spaces; tabs are not indentation.
//! Headings start at column zero and have one to six hashes followed by a
//! space, tab, or end of line. Closing fences have only trailing whitespace.

#[derive(Default)]
pub(super) struct Scanner {
    open: Option<(u8, usize)>,
}

impl Scanner {
    /// Returns true for fence delimiters and all lines inside a fence.
    pub(super) fn fenced(&mut self, line: &str) -> bool {
        let delimiter = fence_delimiter(line);
        match self.open {
            Some((ch, len)) => {
                if delimiter.is_some_and(|(c, n, rest)| {
                    c == ch && n >= len && rest.trim_matches([' ', '\t', '\r']).is_empty()
                }) {
                    self.open = None;
                }
                true
            }
            None => {
                if let Some((ch, len, rest)) = delimiter {
                    if ch != b'`' || !rest.contains('`') {
                        self.open = Some((ch, len));
                        return true;
                    }
                }
                false
            }
        }
    }

    pub(super) fn unclosed(&self) -> bool {
        self.open.is_some()
    }
}

fn fence_delimiter(line: &str) -> Option<(u8, usize, &str)> {
    let spaces = line.bytes().take_while(|&b| b == b' ').count();
    if spaces > 3 {
        return None;
    }
    let text = &line[spaces..];
    let ch = *text.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = text.bytes().take_while(|&b| b == ch).count();
    (len >= 3).then(|| (ch, len, &text[len..]))
}

pub(super) fn atx_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_end();
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &trimmed[hashes..];
    match rest.as_bytes().first() {
        None => Some((hashes, "")),
        Some(b' ') | Some(b'\t') => Some((hashes, rest.trim_start_matches([' ', '\t']))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{chunker, splice};
    use std::path::Path;

    #[test]
    fn chunk_references_and_edits_share_fence_boundaries() {
        for (outer, inner) in [("````", "```"), ("~~~~", "~~~")] {
            let example = format!(
                "  {outer} example\n{inner}\n## Target\n`src/fake.rs`\n{outer} trailing\n`src/also_fake.rs`\n  {outer}\n"
            );
            let document = format!("## Intro\n{example}\n##\tTarget\n`src/real.rs`\n");
            let chunks = chunker::chunk_file(Path::new("topic.md"), document.as_bytes()).unwrap();
            assert_eq!(
                chunks
                    .iter()
                    .map(|c| c.heading.as_str())
                    .collect::<Vec<_>>(),
                ["Intro", "Target"]
            );
            assert!(chunks[0].source_paths.is_empty());
            assert_eq!(chunks[1].source_paths, ["src/real.rs"]);

            let (edited, outcome) = splice::splice_section(document, "Target", "new").unwrap();
            assert_eq!(outcome, splice::SectionOutcome::Replaced { level: 2 });
            assert!(edited.contains(&example));
            assert!(edited.contains("##\tTarget\n\nnew\n"));
        }
    }

    #[test]
    fn four_space_indent_is_not_a_fence() {
        let text = "## One\n    ```\n## Two\n`src/real.rs`\n";
        let chunks = chunker::chunk_file(Path::new("topic.md"), text.as_bytes()).unwrap();
        assert_eq!(
            chunks
                .iter()
                .map(|c| c.heading.as_str())
                .collect::<Vec<_>>(),
            ["One", "Two"]
        );
        assert_eq!(chunks[1].source_paths, ["src/real.rs"]);
        assert!(splice::splice_section(text.to_owned(), "Two", "new").is_ok());
    }

    #[test]
    fn unclosed_tail_preserves_retrieval_prefix_and_rejects_file_edits() {
        use super::super::{
            dir::KnowledgeDir,
            types::{KnowledgeFile, KnowledgeTarget},
        };
        use crate::context::schema::LifecycleState;

        let text = "## Intro\n```\n## Target\n`src/fake.rs`\n```\n## Target\n`src/real.rs`\n~~~\n## Hidden\n`src/hidden.rs`\n";
        let chunks = chunker::chunk_file(Path::new("architecture.md"), text.as_bytes()).unwrap();
        assert_eq!(
            chunks
                .iter()
                .map(|c| c.heading.as_str())
                .collect::<Vec<_>>(),
            ["Intro", "Target"]
        );
        assert!(chunks[0].source_paths.is_empty());
        assert_eq!(chunks[1].source_paths, ["src/real.rs"]);

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("architecture.md");
        std::fs::write(&path, text).unwrap();
        let dir = KnowledgeDir::from_root(temp.path());
        let target = KnowledgeTarget::Tier1(KnowledgeFile::Architecture);
        assert!(dir
            .replace_section_target(&target, "Target", "new")
            .is_err());
        assert!(dir
            .replace_section_target(&target, "Missing", "new")
            .is_err());
        assert!(dir.delete_section_target(&target, "Target").is_err());
        assert!(dir
            .set_section_state_target(&target, "Target", LifecycleState::Draft)
            .is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), text);
    }
}
