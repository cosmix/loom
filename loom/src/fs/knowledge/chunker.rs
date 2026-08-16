//! Split curated knowledge markdown files into retrievable H2 sections.

use crate::context::schema::LifecycleState;
use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

/// Re-export the canonical knowledge chunk type for knowledge callers.
pub use crate::context::schema::KnowledgeChunk;

static SOURCE_PATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    match Regex::new(r"[A-Za-z0-9_./-]+\.(rs|tsx|ts|py|go|sh|md|toml|yaml|yml)") {
        Ok(regex) => regex,
        Err(error) => panic!("source path regex must be valid: {error}"),
    }
});
static SYMBOL_REGEX: LazyLock<Regex> =
    LazyLock::new(
        || match Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*$") {
            Ok(regex) => regex,
            Err(error) => panic!("symbol regex must be valid: {error}"),
        },
    );
static LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| match Regex::new(r"\[([^\]]+)\]\(([^)]+\.md)\)") {
        Ok(regex) => regex,
        Err(error) => panic!("link regex must be valid: {error}"),
    });

#[derive(Debug, Default, Deserialize)]
struct Frontmatter {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    state: Option<LifecycleState>,
    #[serde(default)]
    sources: Vec<String>,
}

/// Build one `KnowledgeChunk` from a section at `index`, tracking heading
/// occurrence counts in `occurrences` (shared across a file's sections) so
/// each chunk's anchor stays unique even under repeated headings.
#[allow(clippy::too_many_arguments)]
fn build_chunk(
    index: usize,
    section: Section<'_>,
    relative_path: &str,
    category: &Option<String>,
    state: LifecycleState,
    frontmatter: &Frontmatter,
    occurrences: &mut std::collections::BTreeMap<String, usize>,
) -> KnowledgeChunk {
    let body = trim_trailing_blank_lines(section.body);
    let normalized_heading = normalize_heading(section.heading);
    let occurrence = occurrences.entry(normalized_heading.clone()).or_insert(0);
    let derived_id = format!("{relative_path}#{normalized_heading}#{occurrence}");
    *occurrence += 1;

    let (mut source_paths, symbols) = references_in(&body);
    if index == 0 {
        source_paths = deduplicate(frontmatter.sources.iter().cloned().chain(source_paths));
    }
    let content_hash = format!("sha256:{}", hex::encode(Sha256::digest(body.as_bytes())));
    // This is an estimate, not a tokenizer count.
    let estimated_tokens = crate::context::schema::estimate_tokens(&body);
    let links = links_in(&body);

    KnowledgeChunk {
        id: if index == 0 {
            frontmatter.id.clone().unwrap_or(derived_id)
        } else {
            derived_id
        },
        file: PathBuf::from(relative_path),
        anchor: normalized_heading,
        heading: section.heading.trim().to_string(),
        body,
        content_hash,
        estimated_tokens,
        aliases: if index == 0 {
            frontmatter.aliases.clone()
        } else {
            Vec::new()
        },
        category: category.clone(),
        source_paths,
        symbols,
        links,
        state,
    }
}

/// Split a knowledge markdown file into a preamble and its H2 sections.
pub fn chunk_file(path: &Path, bytes: &[u8]) -> anyhow::Result<Vec<KnowledgeChunk>> {
    let text = String::from_utf8_lossy(bytes);
    let (frontmatter, content) = split_frontmatter(&text);
    let sections = split_sections(content);
    let relative_path = display_path(path);
    let category = category_for(path);
    let state = frontmatter.state.unwrap_or_default();
    let mut occurrences: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();

    let chunks = sections
        .into_iter()
        .enumerate()
        .map(|(index, section)| {
            build_chunk(
                index,
                section,
                &relative_path,
                &category,
                state,
                &frontmatter,
                &mut occurrences,
            )
        })
        .collect();

    Ok(chunks)
}

struct Section<'a> {
    body: &'a str,
    heading: &'a str,
}

fn split_frontmatter(text: &str) -> (Frontmatter, &str) {
    let Some((first_end, first_line)) = line_at(text, 0) else {
        return (Frontmatter::default(), text);
    };
    if first_line != "---" {
        return (Frontmatter::default(), text);
    }

    let mut offset = first_end;
    while let Some((line_end, line)) = line_at(text, offset) {
        if line == "---" {
            // A malformed frontmatter block is not fatal: fall back to defaults
            // and keep chunking the file.
            let frontmatter = serde_yaml::from_str(&text[first_end..offset]).unwrap_or_default();
            return (frontmatter, &text[line_end..]);
        }
        offset = line_end;
    }
    (Frontmatter::default(), text)
}

fn split_sections(content: &str) -> Vec<Section<'_>> {
    let mut split_points = Vec::new();
    let mut fence = None;
    let mut offset = 0;

    while let Some((line_end, line)) = line_at(content, offset) {
        let fence_marker = fence_marker(line);
        if let Some(open_fence) = fence {
            if fence_marker == Some(open_fence) {
                fence = None;
            }
        } else if let Some(marker) = fence_marker {
            fence = Some(marker);
        } else if line.starts_with("## ") {
            split_points.push(offset);
        }
        offset = line_end;
    }

    let mut sections = Vec::new();
    if let Some(&first_split) = split_points.first() {
        if !content[..first_split].trim().is_empty() {
            sections.push(Section {
                body: &content[..first_split],
                heading: "",
            });
        }
    } else if !content.trim().is_empty() {
        sections.push(Section {
            body: content,
            heading: "",
        });
    }

    for (position, start) in split_points.iter().enumerate() {
        let end = split_points
            .get(position + 1)
            .copied()
            .unwrap_or(content.len());
        let (_, line) = line_at(content, *start).unwrap_or((end, ""));
        sections.push(Section {
            body: &content[*start..end],
            heading: line.strip_prefix("## ").unwrap_or_default(),
        });
    }
    sections
}

fn line_at(text: &str, start: usize) -> Option<(usize, &str)> {
    if start >= text.len() {
        return None;
    }
    let end = text[start..]
        .find('\n')
        .map(|position| start + position + 1)
        .unwrap_or(text.len());
    let line = text[start..end].trim_end_matches(['\n', '\r']);
    Some((end, line))
}

fn fence_marker(line: &str) -> Option<char> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") {
        Some('`')
    } else if trimmed.starts_with("~~~") {
        Some('~')
    } else {
        None
    }
}

fn trim_trailing_blank_lines(body: &str) -> String {
    let mut offset = 0;
    let mut last_non_blank_end = 0;
    while let Some((line_end, line)) = line_at(body, offset) {
        if !line.trim().is_empty() {
            last_non_blank_end = line_end;
        }
        offset = line_end;
    }
    body[..last_non_blank_end].to_string()
}

fn normalize_heading(heading: &str) -> String {
    let mut normalized = String::new();
    let mut needs_separator = false;
    for character in heading.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if needs_separator && !normalized.is_empty() {
                normalized.push('-');
            }
            normalized.push(character);
            needs_separator = false;
        } else if !normalized.is_empty() {
            needs_separator = true;
        }
    }
    normalized
}

fn references_in(body: &str) -> (Vec<String>, Vec<String>) {
    let mut source_paths = Vec::new();
    let mut symbols = Vec::new();
    let mut span_start = None;

    for (position, character) in body.char_indices() {
        if character != '`' {
            continue;
        }
        if let Some(start) = span_start.take() {
            let span = &body[start..position];
            // The regex crate has no lookahead, so a match like "foo.rs" inside
            // "foo.rsx" is only ruled out here: reject it when the next
            // character is still part of an identifier (an extension must end
            // at a boundary). `matched.end()` is always a valid char boundary,
            // so slicing from it is safe even next to multi-byte characters.
            source_paths.extend(SOURCE_PATH_REGEX.find_iter(span).filter_map(|matched| {
                let followed_by_identifier_char = span[matched.end()..]
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphanumeric());
                (!followed_by_identifier_char).then(|| matched.as_str().to_string())
            }));
            let symbol = span.trim();
            if SYMBOL_REGEX.is_match(symbol) {
                symbols.push(symbol.to_string());
            }
        } else {
            span_start = Some(position + character.len_utf8());
        }
    }

    (deduplicate(source_paths), deduplicate(symbols))
}

fn links_in(body: &str) -> Vec<(String, String)> {
    LINK_REGEX
        .captures_iter(body)
        .filter_map(|captures| {
            Some((
                captures.get(1)?.as_str().to_string(),
                captures.get(2)?.as_str().to_string(),
            ))
        })
        .collect()
}

fn deduplicate(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn category_for(path: &Path) -> Option<String> {
    let components: Vec<_> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    components.get(components.len().checked_sub(2)?).cloned()
}
