use anyhow::Result;
use std::collections::HashMap;

use super::frontmatter::extract_yaml_frontmatter;

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
        let (frontmatter, body) = Self::extract_frontmatter(content)?;
        let sections = Self::parse_sections(&body)?;

        Ok(Self {
            frontmatter,
            sections,
        })
    }

    fn extract_frontmatter(content: &str) -> Result<(HashMap<String, String>, String)> {
        let lines: Vec<&str> = content.lines().collect();

        // Check if frontmatter exists
        if lines.is_empty() || !lines[0].trim().starts_with("---") {
            return Ok((HashMap::new(), content.to_string()));
        }

        // Use the canonical frontmatter parser
        let yaml_value = extract_yaml_frontmatter(content)?;

        // Convert YAML value to HashMap<String, String> for backward compatibility
        let mut frontmatter = HashMap::new();
        if let serde_json::Value::Object(map) = yaml_value {
            for (k, value) in map {
                if let serde_json::Value::String(v) = value {
                    frontmatter.insert(k, v);
                }
            }
        }

        // Find the end of frontmatter to extract body
        let mut end_idx = 0;
        for (idx, line) in lines.iter().enumerate().skip(1) {
            // The closing delimiter must be exactly "---" or "..." (after trimming)
            // This prevents matching YAML content like "--- # comment" or "----"
            let trimmed = line.trim();
            if trimmed == "---" || trimmed == "..." {
                end_idx = idx;
                break;
            }
        }

        let body = if end_idx > 0 {
            lines[end_idx + 1..].join("\n")
        } else {
            content.to_string()
        };

        Ok((frontmatter, body))
    }

    fn parse_sections(body: &str) -> Result<Vec<Section>> {
        let mut sections = Vec::new();
        let lines: Vec<&str> = body.lines().collect();
        let mut current_section: Option<Section> = None;

        for line in lines {
            if line.starts_with('#') {
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
}
