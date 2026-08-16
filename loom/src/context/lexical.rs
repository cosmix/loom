//! Tokenization and text-matching primitives used by [`crate::context::rank`].

use crate::context::schema::KnowledgeChunk;
use std::path::{Component, Path, PathBuf};

/// Weight for a chunk heading used as its title.
pub const WEIGHT_TITLE: f32 = 3.0;
/// Weight for aliases.
pub const WEIGHT_ALIASES: f32 = 2.5;
/// Weight for a chunk anchor used as a heading.
pub const WEIGHT_HEADINGS: f32 = 2.0;
/// Weight for source symbols.
pub const WEIGHT_SYMBOLS: f32 = 2.0;
/// Weight for referenced source paths.
pub const WEIGHT_PATHS: f32 = 2.0;
/// Weight for chunk body text.
pub const WEIGHT_BODY: f32 = 1.0;

/// Lowercase and split text into original, snake-case, and camel-case terms.
///
/// Repeated terms are deliberately retained because BM25 term frequency counts
/// them. Underscores remain in a raw token long enough to retain that token and
/// to emit its component parts.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut raw = String::new();

    let mut emit = |token: &str| {
        if token.is_empty() {
            return;
        }
        terms.push(token.to_ascii_lowercase());
        if token.contains('_') {
            terms.extend(
                token
                    .split('_')
                    .filter(|part| !part.is_empty())
                    .map(str::to_ascii_lowercase),
            );
        }
        let mut start = 0;
        let bytes = token.as_bytes();
        for index in 1..bytes.len() {
            if bytes[index - 1].is_ascii_lowercase() && bytes[index].is_ascii_uppercase() {
                terms.push(token[start..index].to_ascii_lowercase());
                start = index;
            }
        }
        if start != 0 {
            terms.push(token[start..].to_ascii_lowercase());
        }
    };

    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            raw.push(character);
        } else {
            emit(&raw);
            raw.clear();
        }
    }
    emit(&raw);
    terms
}

/// Tokenize every weighted field of a chunk (heading, aliases, anchor,
/// symbols, source paths, body) into `(term, weight)` pairs.
pub(crate) fn field_tokens(chunk: &KnowledgeChunk) -> Vec<(String, f32)> {
    let mut terms = Vec::new();
    add_field(&mut terms, &chunk.heading, WEIGHT_TITLE);
    for alias in &chunk.aliases {
        add_field(&mut terms, alias, WEIGHT_ALIASES);
    }
    add_field(&mut terms, &chunk.anchor, WEIGHT_HEADINGS);
    for symbol in &chunk.symbols {
        add_field(&mut terms, symbol, WEIGHT_SYMBOLS);
    }
    for path in &chunk.source_paths {
        add_field(&mut terms, path, WEIGHT_PATHS);
    }
    add_field(&mut terms, &chunk.body, WEIGHT_BODY);
    terms
}

fn add_field(terms: &mut Vec<(String, f32)>, text: &str, weight: f32) {
    terms.extend(tokenize(text).into_iter().map(|term| (term, weight)));
}

/// Resolve a markdown link `target` written inside `source_file` and test
/// whether it designates `candidate_file`. Both file paths are relative to the
/// knowledge root; `target` is relative to `source_file`'s directory.
///
/// A link is almost never written relative to the knowledge root: a chunk at
/// `mistakes/a.md` linking to `verification-harness.md` means
/// `mistakes/verification-harness.md`, not a root-level file of that name.
/// Resolution is purely lexical (no filesystem access) so this stays safe to
/// call on arbitrary, possibly-nonexistent targets.
pub(crate) fn link_target_matches(source_file: &Path, target: &str, candidate_file: &Path) -> bool {
    match resolve_relative_link(source_file, target) {
        Some(resolved) => resolved == normalize_slashes(candidate_file),
        None => false,
    }
}

/// Lexically join `target` onto `source_file`'s parent directory and resolve
/// `.`/`..` components, without touching the filesystem. Returns `None` when
/// the target climbs above the knowledge root — such a link can never
/// designate anything inside the tree.
fn resolve_relative_link(source_file: &Path, target: &str) -> Option<PathBuf> {
    let mut components: Vec<String> = source_file
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();

    for part in target.replace('\\', "/").split('/') {
        match part {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            other => components.push(other.to_string()),
        }
    }

    Some(PathBuf::from(components.join("/")))
}

/// Normalize a knowledge-relative path to forward slashes for comparison.
fn normalize_slashes(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}

/// True when `term` occurs in `text` delimited by non-identifier characters.
///
/// The exact-match rungs must not fire on a fragment buried inside a longer
/// word. Curated prose is full of one- and two-character backticked tokens
/// (`n`, `rg`, `pub`), and plain substring containment made a symbol like `n`
/// match every query containing the letter — awarding the exact-symbol boost,
/// and a `high` confidence label, essentially at random. Requiring identifier
/// boundaries keeps `Foo::Bar` and `src/context/pack.rs` matching while a bare
/// `n` no longer matches "signal generation".
pub(crate) fn contains_whole_term(text: &str, term: &str) -> bool {
    if term.is_empty() {
        return false;
    }
    let text_lower = text.to_ascii_lowercase();
    let term_lower = term.to_ascii_lowercase();
    let is_identifier_char = |c: char| c.is_ascii_alphanumeric() || c == '_';

    let mut offset = 0;
    while let Some(found) = text_lower[offset..].find(&term_lower) {
        let start = offset + found;
        let end = start + term_lower.len();
        let before_ok = text_lower[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_identifier_char(c));
        let after_ok = text_lower[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_identifier_char(c));
        if before_ok && after_ok {
            return true;
        }
        offset = start + term_lower.chars().next().map_or(1, char::len_utf8);
    }
    false
}
