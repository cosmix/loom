/// Count the backticks on a closing fence line, without its newline.
pub(crate) fn closing_run(line: &str) -> Option<usize> {
    let after_indent = line.trim_start_matches(' ');
    if line.len() - after_indent.len() > 3 {
        return None;
    }
    let after_run = after_indent.trim_start_matches('`');
    let run = after_indent.len() - after_run.len();
    (run >= 3 && after_run.trim().is_empty()).then_some(run)
}

/// Find the start of the first line after the line holding `from` that closes
/// a fence of `fence_len` backticks.
pub(super) fn find_closing_fence(section: &str, from: usize, fence_len: usize) -> Option<usize> {
    let mut offset = from + section[from..].find('\n')? + 1;
    for line in section[offset..].split_inclusive('\n') {
        let text = line.strip_suffix('\n').unwrap_or(line);
        if closing_run(text).is_some_and(|run| run >= fence_len) {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// Choose a fence at least as wide as `minimum` that no line in `body` can close.
pub(crate) fn fence_len_for(body: &str, minimum: usize) -> usize {
    body.lines()
        .filter_map(closing_run)
        .max()
        .map_or(minimum, |run| minimum.max(run + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::parser::extract_yaml_metadata;

    const YAML: &str = "description: \"wrap code in ``` fences\"\nnext: value";

    #[test]
    fn closing_runs_require_their_own_line() {
        for (line, expected) in [
            ("```", Some(3)),
            ("````", Some(4)),
            ("   ```", Some(3)),
            ("    ```", None),
            ("``", None),
            ("```inner", None),
            ("``` \t\r", Some(3)),
        ] {
            assert_eq!(closing_run(line), expected, "{line:?}");
        }
    }

    #[test]
    fn closing_fence_skips_inline_backticks() {
        let section = format!("```yaml\n{YAML}\n   ```\n");
        assert_eq!(
            find_closing_fence(&section, "```yaml".len(), 3),
            section.find("   ```")
        );
    }

    #[test]
    fn closing_fence_skips_a_shorter_run() {
        let section = "````yaml\n```\n`````\n";
        assert_eq!(find_closing_fence(section, "````yaml".len(), 4), Some(13));
    }

    #[test]
    fn closing_fence_reports_the_line_start_with_crlf() {
        let section = "```yaml\r\nx: é\r\n  ```\r\n";
        assert_eq!(
            find_closing_fence(section, "```yaml".len(), 3),
            section.find("  ```")
        );
    }

    #[test]
    fn closing_fence_requires_a_line_after_the_opening_line() {
        assert_eq!(find_closing_fence("```yaml ```", 7, 3), None);
        assert_eq!(find_closing_fence("```yaml\nx: 1\n", 7, 3), None);
    }

    #[test]
    fn fence_width_accounts_for_closing_runs_in_the_body() {
        assert_eq!(fence_len_for("a\n```\nb\n", 3), 4);
        assert_eq!(fence_len_for("x: 1\n", 3), 3);
    }

    #[test]
    fn extracts_the_whole_metadata_body_with_inline_backticks() {
        let plan = format!(
            "# Plan\n\n<!-- loom METADATA -->\n```yaml\n{YAML}\n```\n<!-- END loom METADATA -->\n"
        );
        assert_eq!(extract_yaml_metadata(&plan).unwrap(), YAML);
    }
}
