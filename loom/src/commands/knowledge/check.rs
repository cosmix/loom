//! Knowledge check command - validate knowledge completeness and coverage.

use crate::fs::knowledge::{
    KnowledgeDir, KnowledgeFile, DEFAULT_MAX_TIER1_LINES, DEFAULT_MAX_TOPIC_LINES,
};
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;

#[derive(Debug)]
pub struct FileCheckResult {
    pub file_type: KnowledgeFile,
    pub exists: bool,
    pub has_content: bool,
    pub section_count: usize,
}

#[derive(Debug)]
pub struct SrcCoverageResult {
    pub src_directories: Vec<String>,
    pub mentioned_directories: Vec<String>,
    pub coverage_percent: f64,
}

#[derive(Debug)]
pub struct KnowledgeCheckResult {
    pub directory_exists: bool,
    pub file_results: Vec<FileCheckResult>,
    pub src_coverage: Option<SrcCoverageResult>,
    pub overall_pass: bool,
}

pub fn check(min_coverage: u8, src_path: Option<String>, quiet: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let knowledge = KnowledgeDir::new(project_root);

    if !knowledge.exists() {
        if !quiet {
            println!(
                "{} Knowledge directory not found at {}",
                "✗".red().bold(),
                knowledge.root().display()
            );
        }
        bail!("Knowledge directory does not exist. Run 'loom knowledge init' first.");
    }

    let result = analyze_knowledge_completeness(&knowledge, project_root, src_path)?;

    let arch_result = result
        .file_results
        .iter()
        .find(|r| r.file_type == KnowledgeFile::Architecture);

    if let Some(arch) = arch_result {
        if !arch.has_content {
            if !quiet {
                println!(
                    "{} architecture.md has no content sections",
                    "✗".red().bold()
                );
            }
            bail!("architecture.md is empty. Architecture documentation is required.");
        }
    }

    if let Some(ref src_coverage) = result.src_coverage {
        if (src_coverage.coverage_percent as u8) < min_coverage {
            if !quiet {
                print_check_results(&result);
                println!(
                    "\n{} Coverage {:.0}% is below minimum {}%",
                    "✗".red().bold(),
                    src_coverage.coverage_percent,
                    min_coverage
                );
            }
            bail!(
                "Source directory coverage ({:.0}%) is below minimum ({}%)",
                src_coverage.coverage_percent,
                min_coverage
            );
        }
    }

    if !quiet {
        print_check_results(&result);

        if let Ok(gc_metrics) =
            knowledge.analyze_gc_metrics(DEFAULT_MAX_TIER1_LINES, DEFAULT_MAX_TOPIC_LINES)
        {
            println!();
            println!("{}", "GC Analysis:".cyan().bold());
            for tier1 in &gc_metrics.tier1 {
                println!(
                    "  {} {} lines",
                    tier1.file_type.filename(),
                    tier1.line_count
                );
            }
            for topic in &gc_metrics.topics {
                println!("  {} {} lines", topic.relative_path(), topic.line_count);
            }
            if gc_metrics.gc_recommended {
                if let Some(first_reason) = gc_metrics.reasons.first() {
                    println!("\n  {} GC advisory: {}", "⚠".yellow(), first_reason);
                }
            }
        }

        println!("\n{} Knowledge check passed", "✓".green().bold());
    }

    Ok(())
}

fn analyze_knowledge_completeness(
    knowledge: &KnowledgeDir,
    project_root: &std::path::Path,
    src_path: Option<String>,
) -> Result<KnowledgeCheckResult> {
    let mut file_results = Vec::new();

    for file_type in KnowledgeFile::all() {
        let path = knowledge.file_path(*file_type);
        let exists = path.exists();
        let (has_content, section_count) = if exists {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            count_content_sections(&content)
        } else {
            (false, 0)
        };

        file_results.push(FileCheckResult {
            file_type: *file_type,
            exists,
            has_content,
            section_count,
        });
    }

    let src_coverage = analyze_src_coverage(knowledge, project_root, src_path)?;

    let arch_has_content = file_results
        .iter()
        .find(|r| r.file_type == KnowledgeFile::Architecture)
        .map(|r| r.has_content)
        .unwrap_or(false);

    let overall_pass = arch_has_content;

    Ok(KnowledgeCheckResult {
        directory_exists: true,
        file_results,
        src_coverage,
        overall_pass,
    })
}

fn count_content_sections(content: &str) -> (bool, usize) {
    let mut section_count = 0;
    for line in content.lines() {
        if line.starts_with("## ")
            && !line.contains("(Add ")
            && !line.contains("append-only")
            && !line.contains("placeholder")
        {
            section_count += 1;
        }
    }
    (section_count > 0, section_count)
}

/// Build the full architecture text used for src/ coverage matching:
/// the tier-1 `architecture.md` summary plus every tier-2 topic filed
/// under the `architecture/` category. Once a finding moves out of the
/// tier-1 summary into a topic, coverage must still see it — otherwise
/// every plan's `loom knowledge check --min-coverage` acceptance would
/// start failing the moment architecture content is restructured.
fn architecture_coverage_text(knowledge: &KnowledgeDir) -> String {
    let arch_path = knowledge.file_path(KnowledgeFile::Architecture);
    let mut content = if arch_path.exists() {
        std::fs::read_to_string(&arch_path).unwrap_or_default()
    } else {
        String::new()
    };

    if let Ok(topics) = knowledge.list_topics() {
        for topic in topics
            .iter()
            .filter(|t| t.category == KnowledgeFile::Architecture)
        {
            if let Ok(topic_content) = std::fs::read_to_string(&topic.path) {
                content.push('\n');
                content.push_str(&topic_content);
            }
        }
    }

    content
}

fn analyze_src_coverage(
    knowledge: &KnowledgeDir,
    project_root: &std::path::Path,
    src_path: Option<String>,
) -> Result<Option<SrcCoverageResult>> {
    let src_directories = get_src_subdirectories(project_root, src_path)?;

    if src_directories.is_empty() {
        return Ok(None);
    }

    let arch_content = architecture_coverage_text(knowledge);
    if arch_content.is_empty() {
        return Ok(Some(SrcCoverageResult {
            src_directories,
            mentioned_directories: Vec::new(),
            coverage_percent: 0.0,
        }));
    }

    let mentioned_directories: Vec<String> = src_directories
        .iter()
        .filter(|dir| is_directory_mentioned(dir, &arch_content))
        .cloned()
        .collect();

    let coverage_percent = if src_directories.is_empty() {
        100.0
    } else {
        (mentioned_directories.len() as f64 / src_directories.len() as f64) * 100.0
    };

    Ok(Some(SrcCoverageResult {
        src_directories,
        mentioned_directories,
        coverage_percent,
    }))
}

pub fn get_src_subdirectories(
    project_root: &std::path::Path,
    src_path: Option<String>,
) -> Result<Vec<String>> {
    let src_dir = if let Some(custom_path) = src_path {
        project_root.join(custom_path)
    } else {
        let candidates = ["src", "loom/src", "lib", "app"];
        let mut found = None;
        for candidate in &candidates {
            let path = project_root.join(candidate);
            if path.is_dir() {
                found = Some(path);
                break;
            }
        }
        match found {
            Some(p) => p,
            None => return Ok(Vec::new()),
        }
    };

    if !src_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut directories = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&src_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if !name.starts_with('.') && name != "target" && name != "__pycache__" {
                        directories.push(name.to_string());
                    }
                }
            }
        }
    }

    directories.sort();
    Ok(directories)
}

pub fn is_directory_mentioned(dir_name: &str, content: &str) -> bool {
    let content_lower = content.to_lowercase();
    let dir_lower = dir_name.to_lowercase();

    let patterns = [
        format!("{}/", dir_lower),
        format!("/{}/", dir_lower),
        format!("`{}`", dir_lower),
        format!("**{}**", dir_lower),
        format!(" {} ", dir_lower),
        format!("\n{}\n", dir_lower),
        format!("{} -", dir_lower),
        format!("- {}", dir_lower),
        format!("/{}", dir_lower),
        format!("{}:", dir_lower),
        format!("## {}", dir_lower),
        format!("### {}", dir_lower),
        dir_lower.replace('_', " "),
        dir_lower.replace('_', "-"),
    ];

    patterns.iter().any(|p| content_lower.contains(p))
}

pub fn print_check_results(result: &KnowledgeCheckResult) {
    println!("{}", "Knowledge Check Results".bold());
    println!();

    println!("{}", "Files:".cyan().bold());
    for file_result in &result.file_results {
        let status_icon = if file_result.has_content {
            "✓".green()
        } else if file_result.exists {
            "○".yellow()
        } else {
            "✗".red()
        };

        let status_text = if file_result.has_content {
            format!("{} sections", file_result.section_count)
        } else if file_result.exists {
            "empty".to_string()
        } else {
            "missing".to_string()
        };

        println!(
            "  {} {} ({})",
            status_icon,
            file_result.file_type.filename(),
            status_text.dimmed()
        );
    }

    if let Some(ref src_coverage) = result.src_coverage {
        println!();
        println!("{}", "Source Coverage:".cyan().bold());
        println!(
            "  Coverage: {:.0}% ({}/{})",
            src_coverage.coverage_percent,
            src_coverage.mentioned_directories.len(),
            src_coverage.src_directories.len()
        );

        if !src_coverage.mentioned_directories.is_empty() {
            println!(
                "  {} Documented: {}",
                "✓".green(),
                src_coverage.mentioned_directories.join(", ")
            );
        }

        let missing: Vec<_> = src_coverage
            .src_directories
            .iter()
            .filter(|d| !src_coverage.mentioned_directories.contains(d))
            .collect();

        if !missing.is_empty() {
            println!(
                "  {} Missing: {}",
                "○".yellow(),
                missing
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

#[cfg(test)]
#[path = "tests_check.rs"]
mod tests;
