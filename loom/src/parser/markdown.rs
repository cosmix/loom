use anyhow::Result;
use std::collections::HashMap;

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
        let mut frontmatter = HashMap::new();
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() || !lines[0].trim().starts_with("---") {
            return Ok((frontmatter, content.to_string()));
        }

        let mut end_idx = 0;
        for (idx, line) in lines.iter().enumerate().skip(1) {
            if line.trim().starts_with("---") {
                end_idx = idx;
                break;
            }
        }

        if end_idx == 0 {
            return Ok((frontmatter, content.to_string()));
        }

        for line in &lines[1..end_idx] {
            if let Some((key, value)) = line.split_once(':') {
                frontmatter.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        let body = lines[end_idx + 1..].join("\n");
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
