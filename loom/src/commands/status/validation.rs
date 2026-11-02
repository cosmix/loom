use crate::parser::markdown::parse_frontmatter_strings;
use anyhow::{Context, Result};
use colored::Colorize;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Read each Markdown file once and retain its parse result for the caller's policy.
pub(super) fn scan_markdown(dir: &Path) -> Result<Vec<(PathBuf, Result<()>)>> {
    let mut results = Vec::new();
    if !dir.exists() {
        return Ok(results);
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            let content =
                fs::read_to_string(&path).with_context(|| format!("Failed to read {path:?}"))?;
            let parsed = parse_frontmatter_strings(&content).map(|_| ());
            results.push((path, parsed));
        }
    }
    Ok(results)
}

pub fn validate_markdown_files(dir: &Path, entity_type: &str) -> Result<usize> {
    let mut issues = 0;
    for (path, parsed) in scan_markdown(dir)? {
        if let Err(e) = parsed {
            let file_name = path
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Path has no file name: {}", path.display()))?;
            println!(
                "{} Failed to parse {} file: {:?}",
                "ERROR:".red().bold(),
                entity_type,
                file_name
            );
            println!("  {e}");
            issues += 1;
        }
    }
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_reports_parse_failures_and_unreadable_content() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("good.md"), "---\nname: ok\n---\n").unwrap();
        fs::write(dir.path().join("bad.md"), "---\nname: [\n---\n").unwrap();
        fs::write(dir.path().join("ignored.txt"), [0xff]).unwrap();
        let results = scan_markdown(dir.path()).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(
            results.iter().filter(|(_, result)| result.is_err()).count(),
            1
        );
        fs::write(dir.path().join("unreadable.md"), [0xff]).unwrap();
        assert!(scan_markdown(dir.path()).is_err());
    }
}
