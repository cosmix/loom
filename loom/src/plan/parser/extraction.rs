//! Markdown and YAML extraction utilities

use anyhow::{bail, Result};

/// Extract plan name from first H1 header
pub fn extract_plan_name(content: &str) -> Result<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            let name = trimmed.trim_start_matches("# ").trim();
            // Remove "PLAN:" prefix if present
            let name = name.strip_prefix("PLAN:").unwrap_or(name).trim();
            return Ok(name.to_string());
        }
    }
    bail!("No H1 header found in plan document")
}

/// Extract YAML content from metadata block
pub fn extract_yaml_metadata(content: &str) -> Result<String> {
    // Find the metadata markers
    let start_marker = "<!-- loom METADATA";
    let end_marker = "<!-- END loom METADATA";

    let start_pos = content
        .find(start_marker)
        .ok_or_else(|| anyhow::anyhow!("No loom METADATA block found"))?;

    let end_pos = content
        .find(end_marker)
        .ok_or_else(|| anyhow::anyhow!("No END loom METADATA marker found"))?;

    if end_pos <= start_pos {
        bail!("Invalid metadata block: END marker before START");
    }

    let metadata_section = &content[start_pos..end_pos];

    // Find YAML code block within metadata section
    // Support variable-length backtick fences (```, ````, etc.)
    let (yaml_start, fence_len) = find_yaml_fence(metadata_section)
        .ok_or_else(|| anyhow::anyhow!("No ```yaml block in metadata"))?;

    let yaml_content_start = yaml_start + fence_len + "yaml".len();

    // Find the closing fence with the same number of backticks
    let closing_fence = "`".repeat(fence_len);
    let yaml_end = metadata_section[yaml_content_start..]
        .find(&closing_fence)
        .ok_or_else(|| anyhow::anyhow!("No closing {closing_fence} for YAML block"))?;

    let yaml_content = &metadata_section[yaml_content_start..yaml_content_start + yaml_end];

    Ok(yaml_content.trim().to_string())
}

/// Find the start of a YAML code fence and return (position, fence_length)
/// Supports ```, ````, ````` etc.
fn find_yaml_fence(content: &str) -> Option<(usize, usize)> {
    let mut pos = 0;
    while pos < content.len() {
        if let Some(backtick_start) = content[pos..].find('`') {
            let abs_start = pos + backtick_start;
            // Count consecutive backticks
            let fence_len = content[abs_start..]
                .chars()
                .take_while(|&c| c == '`')
                .count();

            if fence_len >= 3 {
                // Check if followed by "yaml"
                let after_fence = abs_start + fence_len;
                if content[after_fence..].starts_with("yaml") {
                    return Some((abs_start, fence_len));
                }
            }
            pos = abs_start + fence_len.max(1);
        } else {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_plan_name() {
        let content = "# PLAN: Test Plan\n\nSome content";
        assert_eq!(extract_plan_name(content).unwrap(), "Test Plan");

        let content2 = "# My Plan\n\nContent";
        assert_eq!(extract_plan_name(content2).unwrap(), "My Plan");
    }

    #[test]
    fn test_extract_plan_name_no_header() {
        let content = "Some content without header";
        assert!(extract_plan_name(content).is_err());
    }

    #[test]
    fn test_extract_yaml_metadata() {
        let content = r#"
# Test Plan

Some content

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Test Stage"
```

<!-- END loom METADATA -->
"#;

        let yaml = extract_yaml_metadata(content).unwrap();
        assert!(yaml.contains("loom:"));
        assert!(yaml.contains("stage-1"));
    }

    #[test]
    fn test_extract_yaml_no_metadata_block() {
        let content = "# Plan\n\nNo metadata here";
        assert!(extract_yaml_metadata(content).is_err());
    }

    #[test]
    fn test_extract_yaml_no_end_marker() {
        let content = r#"
# Plan

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
```
"#;
        assert!(extract_yaml_metadata(content).is_err());
    }

    #[test]
    fn test_extract_yaml_metadata_four_backticks() {
        // Plans with embedded code examples use 4 backticks for the outer fence
        let content = r#"
# Test Plan

Some content with code:
```rust
fn example() {}
```

<!-- loom METADATA -->

````yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Stage with code"
      description: |
        Example code:
        ```rust
        fn inner() {}
        ```
      dependencies: []
````

<!-- END loom METADATA -->
"#;

        let yaml = extract_yaml_metadata(content).unwrap();
        assert!(yaml.contains("loom:"));
        assert!(yaml.contains("stage-1"));
        assert!(yaml.contains("```rust")); // Inner code block preserved
    }

    #[test]
    fn test_find_yaml_fence() {
        // 3 backticks
        let content = "```yaml\nfoo";
        let (pos, len) = find_yaml_fence(content).unwrap();
        assert_eq!(pos, 0);
        assert_eq!(len, 3);

        // 4 backticks
        let content = "````yaml\nfoo";
        let (pos, len) = find_yaml_fence(content).unwrap();
        assert_eq!(pos, 0);
        assert_eq!(len, 4);

        // 5 backticks
        let content = "`````yaml\nfoo";
        let (pos, len) = find_yaml_fence(content).unwrap();
        assert_eq!(pos, 0);
        assert_eq!(len, 5);

        // With leading content
        let content = "some text\n````yaml\nfoo";
        let (pos, len) = find_yaml_fence(content).unwrap();
        assert_eq!(pos, 10);
        assert_eq!(len, 4);

        // Skip non-yaml fences
        let content = "```rust\ncode\n```\n````yaml\nfoo";
        let (_pos, len) = find_yaml_fence(content).unwrap();
        assert_eq!(len, 4);
    }
}
