use crate::fs::work_dir::WorkDir;
use crate::parser::markdown::parse_frontmatter_strings;
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;

pub fn validate_markdown_files(dir: &std::path::Path, entity_type: &str) -> Result<usize> {
    let mut issues = 0;

    if !dir.exists() {
        return Ok(0);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            let content =
                fs::read_to_string(&path).with_context(|| format!("Failed to read {path:?}"))?;

            if let Err(e) = parse_frontmatter_strings(&content) {
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
    }

    Ok(issues)
}

pub fn validate_references(_work_dir: &WorkDir) -> Result<usize> {
    // Runner/track validation removed - these concepts no longer exist
    Ok(0)
}
