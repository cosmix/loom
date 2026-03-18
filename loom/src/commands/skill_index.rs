//! Build skill keyword index for the skill-trigger hook
//!
//! Scans ~/.claude/skills/*/SKILL.md files, extracts trigger keywords from
//! YAML frontmatter, and builds an inverted keyword index at
//! ~/.claude/hooks/loom/skill-keywords.json.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

/// YAML frontmatter structure for SKILL.md files
#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    triggers: Option<Vec<String>>,
    #[serde(default, rename = "trigger-keywords")]
    trigger_keywords: Option<String>,
}

/// Single-word keywords too generic to be useful as skill triggers.
/// Multi-word keywords containing these are NOT filtered.
const STOPWORDS: &[&str] = &[
    "add", "build", "change", "check", "close", "copy", "create", "debug", "delete", "deploy",
    "find", "fix", "get", "help", "install", "list", "make", "move", "open", "pull", "push",
    "read", "remove", "run", "send", "set", "show", "start", "stop", "test", "update", "use",
    "write", "app", "bug", "class", "code", "config", "data", "error", "file", "function", "issue",
    "log", "method", "new", "old", "output", "plan", "project", "script", "setup", "tool", "type",
    "value", "claude", "loom",
];

/// Execute the skill-index command
pub fn execute() -> Result<()> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let skills_dir = home.join(".claude/skills");
    let output_dir = home.join(".claude/hooks/loom");
    let output_file = output_dir.join("skill-keywords.json");

    if !skills_dir.is_dir() {
        println!("Skills directory not found: {}", skills_dir.display());
        return Ok(());
    }

    // Build stopword lookup set
    let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();

    // keyword -> set of skill names (BTreeMap for sorted JSON output)
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut skill_count: usize = 0;

    let entries = fs::read_dir(&skills_dir)
        .with_context(|| format!("Failed to read {}", skills_dir.display()))?;

    for entry in entries.flatten() {
        let skill_dir = entry.path();
        if !skill_dir.is_dir() {
            continue;
        }

        let skill_file = skill_dir.join("SKILL.md");
        if !skill_file.exists() {
            continue;
        }

        let skill_name = entry.file_name().to_string_lossy().to_string();

        match parse_skill_triggers(&skill_file) {
            Ok(triggers) => {
                skill_count += 1;
                for keyword in triggers {
                    let normalized = normalize_keyword(&keyword);
                    if normalized.is_empty() {
                        continue;
                    }
                    if is_stopword(&normalized, &stopwords) {
                        continue;
                    }
                    let skills = index.entry(normalized).or_default();
                    if !skills.contains(&skill_name) {
                        skills.push(skill_name.clone());
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to parse {}: {}", skill_file.display(), e);
            }
        }
    }

    // Ensure output directory exists
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("Failed to create {}", output_dir.display()))?;

    // Serialize and write
    let json = serde_json::to_string_pretty(&index).context("Failed to serialize index to JSON")?;
    fs::write(&output_file, &json)
        .with_context(|| format!("Failed to write {}", output_file.display()))?;

    println!(
        "Built skill keyword index: {} keywords from {} skills",
        index.len(),
        skill_count
    );

    Ok(())
}

/// Parse a SKILL.md file and extract trigger keywords from all sources.
///
/// Tries serde_yaml parsing first; falls back to line-by-line text extraction
/// when the YAML frontmatter contains unquoted colons or other syntax that
/// trips up the YAML parser.
fn parse_skill_triggers(path: &Path) -> Result<Vec<String>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    let frontmatter_text = extract_frontmatter_raw(&content)?;

    // Try strict YAML deserialization first
    let fm: SkillFrontmatter = serde_yaml::from_str(&frontmatter_text).unwrap_or_default();

    let mut triggers = Vec::new();

    // Source 1: triggers YAML list
    if let Some(ref list) = fm.triggers {
        for t in list {
            let cleaned = strip_quotes(t.trim());
            if !cleaned.is_empty() {
                triggers.push(cleaned);
            }
        }
    }

    // Source 1 fallback: parse triggers list from raw text if serde missed it
    if triggers.is_empty() {
        triggers.extend(parse_triggers_list_raw(&frontmatter_text));
    }

    // Source 2: trigger-keywords CSV field
    if let Some(ref csv) = fm.trigger_keywords {
        for part in csv.split(',') {
            let cleaned = strip_quotes(part.trim());
            if !cleaned.is_empty() {
                triggers.push(cleaned);
            }
        }
    }

    // Source 2 fallback: parse trigger-keywords from raw text if serde missed it
    if triggers.is_empty() {
        if let Some(csv) = extract_field_raw(&frontmatter_text, "trigger-keywords") {
            for part in csv.split(',') {
                let cleaned = strip_quotes(part.trim());
                if !cleaned.is_empty() {
                    triggers.push(cleaned);
                }
            }
        }
    }

    // Source 3: keywords embedded in description (only if sources 1+2 are empty)
    if triggers.is_empty() {
        // Try serde-parsed description first, then raw extraction
        let desc = fm.description.unwrap_or_default();
        let desc = if desc.is_empty() {
            extract_field_raw(&frontmatter_text, "description").unwrap_or_default()
        } else {
            desc
        };
        if !desc.is_empty() {
            let embedded = extract_description_keywords(&desc);
            triggers.extend(embedded);
        }
    }

    Ok(triggers)
}

/// Extract the raw frontmatter text between `---` markers
fn extract_frontmatter_raw(content: &str) -> Result<String> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        anyhow::bail!("No frontmatter found");
    }

    let mut end_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end_idx = Some(idx);
            break;
        }
    }

    let end_idx = end_idx.context("Frontmatter not closed")?;
    Ok(lines[1..end_idx].join("\n"))
}

/// Parse a `triggers:` YAML list from raw frontmatter text (line-by-line)
fn parse_triggers_list_raw(text: &str) -> Vec<String> {
    let mut triggers = Vec::new();
    let mut in_triggers = false;

    for line in text.lines() {
        if line.starts_with("triggers:") {
            in_triggers = true;
            continue;
        }
        if in_triggers {
            let trimmed = line.trim();
            if let Some(item) = trimmed.strip_prefix("- ") {
                let cleaned = strip_quotes(item.trim());
                if !cleaned.is_empty() {
                    triggers.push(cleaned);
                }
            } else if trimmed.starts_with('-') && trimmed.len() > 1 {
                let cleaned = strip_quotes(trimmed[1..].trim());
                if !cleaned.is_empty() {
                    triggers.push(cleaned);
                }
            } else if trimmed.is_empty() {
                continue;
            } else if !line.starts_with(' ') && !line.starts_with('\t') {
                break;
            }
        }
    }

    triggers
}

/// Extract a simple scalar field value from raw frontmatter text
fn extract_field_raw(text: &str, field: &str) -> Option<String> {
    let prefix = format!("{}:", field);
    for line in text.lines() {
        if line.starts_with(&prefix) {
            let value = line[prefix.len()..].trim().to_string();
            if !value.is_empty() {
                return Some(strip_quotes(&value));
            }
        }
    }
    None
}

/// Normalize a keyword: lowercase, trim whitespace
fn normalize_keyword(keyword: &str) -> String {
    keyword.trim().to_lowercase()
}

/// Strip surrounding quotes from a string
fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Check if a normalized keyword is a stopword.
/// Multi-word keywords (containing spaces) are never filtered.
fn is_stopword(normalized: &str, stopwords: &HashSet<&str>) -> bool {
    if normalized.contains(' ') {
        return false;
    }
    stopwords.contains(normalized)
}

/// Extract keywords embedded in the description field.
///
/// Looks for markers like "Triggers:", "Trigger keywords:", "Keywords:"
/// (case-insensitive) and extracts the comma-separated list after the marker
/// up to the next sentence boundary (". " followed by uppercase) or end of text.
fn extract_description_keywords(description: &str) -> Vec<String> {
    // Join all lines into a single string for multi-line descriptions
    let text = description.lines().collect::<Vec<_>>().join(" ");
    let lower = text.to_lowercase();

    // Try markers in order of specificity
    let markers = ["trigger keywords:", "triggers:", "keywords:"];

    for marker in &markers {
        if let Some(pos) = lower.find(marker) {
            let after = &text[pos + marker.len()..];
            // Find the end: ". " followed by uppercase letter, or end of string
            let end = find_sentence_boundary(after);
            let keywords_str = &after[..end];

            let keywords: Vec<String> = keywords_str
                .split(',')
                .map(|s| strip_quotes(s.trim()))
                .map(|s| {
                    // Strip trailing period if present
                    s.trim_end_matches('.').trim().to_string()
                })
                .filter(|s| !s.is_empty())
                .collect();

            if !keywords.is_empty() {
                return keywords;
            }
        }
    }

    Vec::new()
}

/// Find the boundary of the current "sentence" in text.
///
/// Returns the index just before ". [A-Z]" pattern, or the length of the
/// string if no such boundary is found.
fn find_sentence_boundary(text: &str) -> usize {
    let bytes = text.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b'.' && bytes[i + 1] == b' ' && (bytes[i + 2] as char).is_ascii_uppercase() {
            return i;
        }
    }
    text.len()
}
