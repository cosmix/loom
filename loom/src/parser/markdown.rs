use anyhow::{Context, Result};
use std::collections::HashMap;

use super::frontmatter::split_frontmatter_raw;

#[derive(Debug, Clone)]
pub struct MarkdownDocument {
    pub frontmatter: HashMap<String, String>,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub level: u8,
    pub title: String,
    pub content: String,
}

impl MarkdownDocument {
    pub fn parse(content: &str) -> Result<Self> {
        let (frontmatter, body) = parse_frontmatter_and_body(content)?;
        let sections = Self::parse_sections(body)?;

        Ok(Self {
            frontmatter,
            sections,
        })
    }

    fn parse_sections(body: &str) -> Result<Vec<Section>> {
        let mut sections = Vec::new();
        let mut current_section: Option<Section> = None;
        let mut fence: Option<(char, usize)> = None;

        for line in body.lines() {
            if let Some(marker) = fence_marker(line) {
                match fence {
                    Some(open) if marker.0 == open.0 && marker.1 >= open.1 => fence = None,
                    None => fence = Some(marker),
                    _ => {}
                }
            }
            if fence.is_none() && line.starts_with('#') {
                if let Some(section) = current_section.take() {
                    sections.push(section);
                }

                let level = line.chars().take_while(|&c| c == '#').count() as u8;
                let title = line.trim_start_matches('#').trim().to_string();

                current_section = Some(Section {
                    level,
                    title,
                    content: String::new(),
                });
            } else if let Some(ref mut section) = current_section {
                if !section.content.is_empty() {
                    section.content.push('\n');
                }
                section.content.push_str(line);
            }
        }

        if let Some(section) = current_section {
            sections.push(section);
        }

        Ok(sections)
    }

    pub fn get_section(&self, title: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.title == title)
    }

    pub fn get_frontmatter(&self, key: &str) -> Option<&String> {
        self.frontmatter.get(key)
    }
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let width = trimmed.chars().take_while(|c| *c == marker).count();
    (width >= 3).then_some((marker, width))
}

/// Read only string-valued YAML fields, retaining the wrapper's optional
/// frontmatter behavior. Callers that need no sections avoid scanning the body.
pub fn parse_frontmatter_strings(content: &str) -> Result<HashMap<String, String>> {
    parse_frontmatter_and_body(content).map(|(fields, _)| fields)
}

fn parse_frontmatter_and_body(content: &str) -> Result<(HashMap<String, String>, &str)> {
    let first_line = content.split_once('\n').map_or(content, |(line, _)| line);
    if !first_line.trim().starts_with("---") {
        return Ok((HashMap::new(), content));
    }
    let (raw, body) = split_frontmatter_raw(content)?;
    let yaml: serde_json::Value =
        crate::yaml::from_str(raw).context("Failed to parse YAML frontmatter")?;
    let mut fields = HashMap::new();
    if let serde_json::Value::Object(map) = yaml {
        for (key, value) in map {
            if let serde_json::Value::String(text) = value {
                fields.insert(key, text);
            }
        }
    }
    Ok((fields, body))
}

impl Section {
    pub fn trimmed_content(&self) -> String {
        self.content.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = "---\nid: test-123\nstatus: active\n---\n# Section\nContent";
        let doc = MarkdownDocument::parse(content).unwrap();

        assert_eq!(doc.frontmatter.get("id"), Some(&"test-123".to_string()));
        assert_eq!(doc.frontmatter.get("status"), Some(&"active".to_string()));
        assert_eq!(doc.sections.len(), 1);
        assert_eq!(doc.sections[0].title, "Section");
    }

    #[test]
    fn test_parse_sections() {
        let content = "# First\nContent 1\n## Second\nContent 2";
        let doc = MarkdownDocument::parse(content).unwrap();

        assert_eq!(doc.sections.len(), 2);
        assert_eq!(doc.sections[0].level, 1);
        assert_eq!(doc.sections[1].level, 2);
    }

    #[test]
    fn frontmatter_boundary_matches_canonical_scan() {
        let content =
            "---\r\nid: session-1\r\nnotes: |\r\n  ---\r\n--- # closing comment\r\n# Body\r\n";
        let fields = parse_frontmatter_strings(content).unwrap();
        assert_eq!(fields.get("id").map(String::as_str), Some("session-1"));
        let document = MarkdownDocument::parse(content).unwrap();
        assert_eq!(document.sections.len(), 1);
        assert_eq!(document.sections[0].title, "Body");
        assert!(parse_frontmatter_strings("---\nid: session-1\n...\n").is_err());
    }

    #[test]
    fn optional_frontmatter_and_fenced_headings() {
        let content = "# Main\n```rust\n# code, not a section\n```\n## Next\n";
        assert!(parse_frontmatter_strings(content).unwrap().is_empty());
        let document = MarkdownDocument::parse(content).unwrap();
        assert_eq!(document.sections.len(), 2);
        assert_eq!(document.sections[0].title, "Main");
        assert_eq!(document.sections[1].title, "Next");
    }

    #[test]
    fn frontmatter_only_reader_keeps_string_scalars() {
        let fields =
            parse_frontmatter_strings("---\nid: session-1\ncount: 2\nempty: null\n---\n").unwrap();
        assert_eq!(fields.get("id").map(String::as_str), Some("session-1"));
        assert!(!fields.contains_key("count"));
        assert!(!fields.contains_key("empty"));
    }
}
