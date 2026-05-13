//! Markdown and YAML extraction utilities

use anyhow::{bail, Result};
use std::ops::Range;

/// Byte ranges within plan content for the loom-METADATA section and the
/// YAML fenced block inside it.
///
/// The `metadata_block_range` covers from the start of `<!-- loom METADATA`
/// up to (but not including) the start of `<!-- END loom METADATA` marker.
///
/// The `yaml_fence_range` covers from the opening backticks of the YAML
/// fence to (but not including) the closing backticks of the same fence.
/// The body of the YAML (excluding the `yaml` language hint after the
/// opening fence and excluding the closing fence) is what `yaml` returns.
#[derive(Debug, Clone)]
pub struct ExtractedMetadata {
    /// The trimmed YAML content (same as the legacy `extract_yaml_metadata`).
    pub yaml: String,
    /// Byte range of the entire metadata block (from `<!-- loom METADATA`
    /// through `<!-- END loom METADATA` — exclusive of the END marker).
    pub metadata_block_range: Range<usize>,
    /// Byte range covering the opening fence through (but not including)
    /// the closing fence of the YAML code block.
    pub yaml_fence_range: Range<usize>,
}

/// Extract plan name from first H1 header
pub fn extract_plan_name(content: &str) -> Result<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            let name = trimmed.trim_start_matches("# ").trim();
            // Strip a leading "PLAN:" / "Plan:" / "plan:" prefix so we don't
            // end up rendering "Plan: Plan: Foo" downstream.
            let lower = name.to_ascii_lowercase();
            let name = if let Some(rest) = lower.strip_prefix("plan:") {
                let stripped_len = name.len() - rest.len();
                name[stripped_len..].trim()
            } else {
                name
            };
            return Ok(name.to_string());
        }
    }
    bail!("No H1 header found in plan document")
}

/// Extract YAML content from metadata block.
///
/// Thin shim around [`extract_yaml_metadata_with_ranges`] that returns only
/// the YAML body.
pub fn extract_yaml_metadata(content: &str) -> Result<String> {
    Ok(extract_yaml_metadata_with_ranges(content)?.yaml)
}

/// Extract YAML content from a plan's metadata block AND report the byte
/// ranges that anchor the metadata block and YAML fence within `content`.
///
/// Used by the runtime plan-amendment path so we can splice an amended YAML
/// body back into the markdown without touching surrounding human-readable
/// prose. The ranges are byte offsets into `content` as it was passed in
/// — preserving CRLF, indentation, and trailing whitespace byte-for-byte.
///
/// Handles:
/// - Variable-length backtick fences (```, ````, ...).
/// - Repeated `<!-- loom METADATA-like` strings in prose: only the FIRST
///   START marker and the FIRST END marker AFTER it are used. A string
///   like "the loom METADATA block" inside the human-readable section does
///   NOT match because it lacks the leading `<!-- `.
/// - CRLF line endings (no byte-level rewrites of `\r\n`).
pub fn extract_yaml_metadata_with_ranges(content: &str) -> Result<ExtractedMetadata> {
    // Find the metadata markers
    let start_marker = "<!-- loom METADATA";
    let end_marker = "<!-- END loom METADATA";

    let start_pos = content
        .find(start_marker)
        .ok_or_else(|| anyhow::anyhow!("No loom METADATA block found"))?;

    // The END marker must appear AFTER the START marker. `find` from the
    // tail of `start_pos` guarantees this and avoids picking up a stray
    // END marker that appears earlier in prose.
    let end_pos_rel = content[start_pos..]
        .find(end_marker)
        .ok_or_else(|| anyhow::anyhow!("No END loom METADATA marker found"))?;
    let end_pos = start_pos + end_pos_rel;

    if end_pos == start_pos {
        bail!("Invalid metadata block: END marker at START position");
    }

    let metadata_section = &content[start_pos..end_pos];

    // Find YAML code block within metadata section
    // Support variable-length backtick fences (```, ````, etc.)
    let (yaml_start_in_section, fence_len) = find_yaml_fence(metadata_section)
        .ok_or_else(|| anyhow::anyhow!("No ```yaml block in metadata"))?;

    let yaml_content_start_in_section = yaml_start_in_section + fence_len + "yaml".len();

    // Find the closing fence with the same number of backticks
    let closing_fence = "`".repeat(fence_len);
    let yaml_end_rel = metadata_section[yaml_content_start_in_section..]
        .find(&closing_fence)
        .ok_or_else(|| anyhow::anyhow!("No closing {closing_fence} for YAML block"))?;

    let yaml_body_end_in_section = yaml_content_start_in_section + yaml_end_rel;
    let yaml_content =
        &metadata_section[yaml_content_start_in_section..yaml_body_end_in_section];

    // Absolute byte ranges in `content`:
    // - metadata_block_range: [start_pos, end_pos)
    // - yaml_fence_range: opening fence through (exclusive of) closing fence
    let yaml_fence_start_abs = start_pos + yaml_start_in_section;
    let yaml_fence_end_abs = start_pos + yaml_body_end_in_section;

    Ok(ExtractedMetadata {
        yaml: yaml_content.trim().to_string(),
        metadata_block_range: start_pos..end_pos,
        yaml_fence_range: yaml_fence_start_abs..yaml_fence_end_abs,
    })
}

/// Find the start of a YAML code fence and return (position, fence_length)
/// Supports ```, ````, ````` etc.
/// Returns (byte_position, byte_length) for consistent byte-based indexing
fn find_yaml_fence(content: &str) -> Option<(usize, usize)> {
    let mut pos = 0;
    while pos < content.len() {
        if let Some(backtick_start) = content[pos..].find('`') {
            let abs_start = pos + backtick_start;
            // Count consecutive backticks using bytes (backticks are ASCII, 1 byte each)
            // This keeps everything in byte positions for safe string slicing
            let fence_len = content[abs_start..]
                .bytes()
                .take_while(|&b| b == b'`')
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

        // Mixed-case "Plan:" / "plan:" should also be stripped so we don't
        // render "Plan: Plan: Foo" in the status header.
        let content3 = "# Plan: GitHub Integration\n";
        assert_eq!(extract_plan_name(content3).unwrap(), "GitHub Integration");

        let content4 = "# plan: lower case\n";
        assert_eq!(extract_plan_name(content4).unwrap(), "lower case");
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

    #[test]
    fn test_find_yaml_fence_with_utf8_before() {
        // Non-ASCII content before the fence (emoji are 4 bytes each)
        let content = "🎉🎊🎁 heading\n\n```yaml\nfoo";
        let (pos, len) = find_yaml_fence(content).unwrap();
        assert_eq!(len, 3);
        // Verify the position is correct:
        // 3 emoji × 4 bytes = 12 bytes
        // " heading" = 8 bytes
        // "\n\n" = 2 bytes
        // Total = 22 bytes
        assert_eq!(pos, 22);
        // Verify we can slice at this position
        assert_eq!(&content[pos..pos + len], "```");
    }

    #[test]
    fn test_find_yaml_fence_with_cjk_before() {
        // CJK characters (3 bytes each)
        let content = "你好世界\n\n```yaml\nfoo";
        let (pos, len) = find_yaml_fence(content).unwrap();
        assert_eq!(len, 3);
        // 12 bytes for CJK + 2 bytes for newlines
        assert_eq!(pos, 14);
        assert_eq!(&content[pos..pos + len], "```");
    }

    #[test]
    fn test_extract_yaml_metadata_with_ranges_basic() {
        let content = "PREFIX\n<!-- loom METADATA -->\n\n```yaml\nfoo: 1\n```\n\n<!-- END loom METADATA -->\nSUFFIX";
        let extracted = extract_yaml_metadata_with_ranges(content).unwrap();
        assert_eq!(extracted.yaml.trim(), "foo: 1");

        // metadata_block_range must span from START marker to (excl) END marker
        let start = content.find("<!-- loom METADATA").unwrap();
        let end = content.find("<!-- END loom METADATA").unwrap();
        assert_eq!(extracted.metadata_block_range, start..end);

        // yaml_fence_range must cover the opening fence through (excl) closing fence
        let fence_open = content.find("```yaml").unwrap();
        let yaml_body_start = fence_open + "```yaml".len();
        let fence_close = content[yaml_body_start..].find("```").unwrap() + yaml_body_start;
        assert_eq!(extracted.yaml_fence_range, fence_open..fence_close);
    }

    #[test]
    fn test_extract_yaml_metadata_with_ranges_four_backticks() {
        let content = "<!-- loom METADATA -->\n\n````yaml\nfoo: 1\n```inner\n````\n\n<!-- END loom METADATA -->";
        let extracted = extract_yaml_metadata_with_ranges(content).unwrap();
        // 4-backtick fence is recognised; the inner 3-backtick is NOT a close.
        assert!(extracted.yaml.contains("```inner"));
        // The fence range should end where the 4-backtick closing fence starts,
        // not where the inner 3-backtick lives.
        let close_pos = content.rfind("````").unwrap();
        assert_eq!(extracted.yaml_fence_range.end, close_pos);
    }

    #[test]
    fn test_extract_yaml_metadata_with_ranges_crlf_preserved() {
        // CRLF line endings: ranges must be byte-accurate so splicing leaves
        // the surrounding prose's \r\n intact.
        let content = "PREFIX\r\n<!-- loom METADATA -->\r\n\r\n```yaml\r\nfoo: 1\r\n```\r\n\r\n<!-- END loom METADATA -->\r\nSUFFIX";
        let extracted = extract_yaml_metadata_with_ranges(content).unwrap();
        // Body still contains the CR before \n
        assert!(extracted.yaml.contains("foo: 1"));
        let start = content.find("<!-- loom METADATA").unwrap();
        let end = content.find("<!-- END loom METADATA").unwrap();
        assert_eq!(extracted.metadata_block_range, start..end);
        // Reconstruct: prefix + metadata_block + END suffix must equal original
        let prefix = &content[..extracted.metadata_block_range.start];
        let block = &content[extracted.metadata_block_range.clone()];
        let suffix = &content[extracted.metadata_block_range.end..];
        let recon: String = [prefix, block, suffix].concat();
        assert_eq!(recon, content);
    }

    #[test]
    fn test_extract_yaml_metadata_with_ranges_ignores_prose_mention() {
        // A prose mention of "loom METADATA" without the HTML-comment opener
        // must not be picked as the START marker.
        let content = "First we mention the loom METADATA block in prose.\n\n<!-- loom METADATA -->\n\n```yaml\nfoo: 1\n```\n\n<!-- END loom METADATA -->";
        let extracted = extract_yaml_metadata_with_ranges(content).unwrap();
        let real_start = content.find("<!-- loom METADATA").unwrap();
        assert_eq!(extracted.metadata_block_range.start, real_start);
    }

    #[test]
    fn test_extract_yaml_metadata_with_utf8() {
        let content = r#"
# 测试计划 🎉

Some content with emoji 🚀

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "テスト"
```

<!-- END loom METADATA -->
"#;

        let yaml = extract_yaml_metadata(content).unwrap();
        assert!(yaml.contains("loom:"));
        assert!(yaml.contains("stage-1"));
        assert!(yaml.contains("テスト"));
    }
}
