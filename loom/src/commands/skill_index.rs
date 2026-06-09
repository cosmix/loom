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

use crate::parser::frontmatter::extract_frontmatter_raw as canonical_extract_frontmatter_raw;

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
    /// Shipped skills use mixed field names; also accept a top-level
    /// `keywords:` CSV so skills like loom-feature-flags aren't silently
    /// excluded from the index.
    #[serde(default)]
    keywords: Option<String>,
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
                    // A stopword is dropped unless it *names* this skill —
                    // e.g. "test" is normally too generic, but it is also
                    // how a user refers to loom-testing, so we keep it when
                    // it would boost to a name-match hit at lookup time.
                    if is_stopword(&normalized, &stopwords)
                        && !is_skill_name_match(&normalized, &skill_name)
                    {
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

    // Source 2b: plain `keywords:` CSV field (used by e.g. loom-feature-flags).
    // Falls back to raw-text extraction independently of earlier sources so a
    // skill that declares both `triggers:` and `keywords:` indexes both.
    let keywords_csv = fm
        .keywords
        .or_else(|| extract_field_raw(&frontmatter_text, "keywords"));
    if let Some(csv) = keywords_csv {
        for part in csv.split(',') {
            let cleaned = strip_quotes(part.trim());
            if !cleaned.is_empty() {
                triggers.push(cleaned);
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

/// Extract the raw frontmatter text between `---` markers.
///
/// Delegates to the canonical [`crate::parser::frontmatter::extract_frontmatter_raw`]
/// which correctly handles `---` inside YAML block scalars by matching the closing
/// delimiter at the same indentation as the opening one.
fn extract_frontmatter_raw(content: &str) -> Result<String> {
    Ok(canonical_extract_frontmatter_raw(content)?.to_string())
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

/// True when `keyword` strongly identifies `skill_name`.
///
/// Mirrors the logic in hooks/skill-trigger.sh so a stopword-exempt
/// indexed keyword will also pick up the name-match weight boost at
/// lookup time. Strips the `loom-` prefix every shipped skill uses.
fn is_skill_name_match(keyword: &str, skill_name: &str) -> bool {
    let effective = skill_name.strip_prefix("loom-").unwrap_or(skill_name);
    if keyword == effective {
        return true;
    }
    keyword.len() >= 4 && effective.starts_with(keyword)
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

    // Try markers in order of specificity. Several shipped skills use
    // phrasings like "Triggers for this skill -" instead of a bare
    // "Triggers:", so match both colon and dash-separated variants.
    let markers = [
        "trigger keywords:",
        "trigger keywords -",
        "triggers for this skill:",
        "triggers for this skill -",
        "triggers:",
        "triggers -",
        "keywords:",
        "keywords -",
    ];

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_skill(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn name_match_strips_loom_prefix() {
        assert!(is_skill_name_match("rust", "loom-rust"));
        assert!(is_skill_name_match("refactor", "loom-refactoring"));
        assert!(is_skill_name_match("testing", "loom-testing"));
        assert!(is_skill_name_match("test", "loom-testing"));
        assert!(is_skill_name_match("debug", "loom-debugging"));
        assert!(is_skill_name_match("plan", "loom-plan-writer"));
        assert!(is_skill_name_match("security", "loom-security-audit"));
        // Unrelated skill — no match
        assert!(!is_skill_name_match("rust", "loom-auth"));
        // Keyword shorter than 4 chars and not an exact match — no match
        assert!(!is_skill_name_match("cd", "loom-argocd"));
        assert!(!is_skill_name_match("re", "loom-react"));
    }

    #[test]
    fn parses_plain_keywords_field() {
        let skill = write_skill(concat!(
            "---\n",
            "name: loom-feature-flags\n",
            "description: Feature flag patterns.\n",
            "keywords: feature flag, feature toggle, LaunchDarkly, A/B test\n",
            "---\n",
            "\nBody.\n",
        ));
        let triggers = parse_skill_triggers(skill.path()).unwrap();
        assert!(triggers.iter().any(|t| t == "feature flag"));
        assert!(triggers.iter().any(|t| t == "feature toggle"));
        assert!(triggers.iter().any(|t| t == "LaunchDarkly"));
    }

    #[test]
    fn keywords_augment_existing_triggers_list() {
        let skill = write_skill(concat!(
            "---\n",
            "name: loom-example\n",
            "triggers:\n",
            "  - foo\n",
            "  - bar\n",
            "keywords: baz, qux\n",
            "---\n",
        ));
        let triggers = parse_skill_triggers(skill.path()).unwrap();
        for expected in ["foo", "bar", "baz", "qux"] {
            assert!(
                triggers.iter().any(|t| t == expected),
                "missing {expected}: {triggers:?}"
            );
        }
    }

    #[test]
    fn description_marker_variants_are_recognized() {
        // "Triggers for this skill -" is used by loom-logging-observability.
        let skill = write_skill(concat!(
            "---\n",
            "name: loom-logging-observability\n",
            "description: |\n",
            "  Comprehensive logging. Triggers for this skill - log, logging, OpenTelemetry, OTEL.\n",
            "---\n",
        ));
        let triggers = parse_skill_triggers(skill.path()).unwrap();
        for expected in ["log", "logging", "OpenTelemetry", "OTEL"] {
            assert!(
                triggers.iter().any(|t| t == expected),
                "missing {expected}: {triggers:?}"
            );
        }
    }

    #[test]
    fn is_stopword_respects_name_match_exemption() {
        let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();
        // "test" is a stopword generally ...
        assert!(is_stopword("test", &stopwords));
        // ... but it name-matches loom-testing, so callers should keep it.
        assert!(is_skill_name_match("test", "loom-testing"));
        // "debug" name-matches loom-debugging.
        assert!(is_stopword("debug", &stopwords));
        assert!(is_skill_name_match("debug", "loom-debugging"));
        // "build" is a stopword and doesn't identify any specific skill.
        assert!(is_stopword("build", &stopwords));
        assert!(!is_skill_name_match("build", "loom-auth"));
    }
}
