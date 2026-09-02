//! Tokenization and text-matching primitives used by [`crate::context::rank`].

mod evidence;

pub use evidence::{backtick_spans, TermEvidence};
pub(crate) use evidence::{occurs_backticked, ExactGate};

/// `admits_exact`, `is_shaped` and `is_plain_word` are `pub` because together
/// they are the exact-rung PREDICATE that A.1 rests on and the unit tests pin
/// them directly, one input at a time — a rule this consequential is not
/// allowed to be reachable only through the 8,500-candidate path that uses it.
/// Production calls them solely via [`ExactGate::admits`], which is why the
/// re-export is test-only: outside tests nothing should reach past the gate to
/// re-derive the decision itself.
#[cfg(test)]
pub use evidence::{admits_exact, is_plain_word, is_shaped};

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

/// The word parts of a symbol name, lowercased and in order:
/// `configure_loom_hooks` → `configure, loom, hooks`, `ResidentPoint` →
/// `resident, point`, `Foo::Bar` → `foo, bar`, `remaining` → `remaining`.
///
/// [`tokenize`]'s parts WITHOUT the compound token it also emits, which is the
/// whole reason this exists as its own function. `tokenize("configure_loom_hooks")`
/// leads with `configure_loom_hooks` itself, and a caller asking "did the prompt
/// name every word of this symbol?" must not be handed a term that only a prompt
/// spelling the identifier out could ever supply — that question already has an
/// answer, and it is the exact-symbol rung.
///
/// Splits on the same two boundaries `tokenize` does — any non-alphanumeric
/// run, and an interior lowercase→uppercase transition — so a part is always a
/// term a QUERY can produce. `_` is a separator here rather than part of a
/// token, which follows from dropping the compound.
///
/// It is not, however, always a term the DOCUMENT built from the same name
/// holds. `tokenize`'s camel-case scan runs over the whole raw token without
/// restarting at `_`, so `foo_barBaz` tokenizes to `foo_barbaz, foo, barbaz,
/// foo_bar, baz` and never emits a bare `bar`, while this returns `foo, bar,
/// baz`. A prompt saying "foo bar baz" therefore NAMES such a node under
/// [`crate::context::rank_source`]'s candidacy floor while BM25 can only score
/// it on `foo` and `baz`. That asymmetry is deliberate on this side: the writer
/// did name the symbol, and the missing term is a limitation of the tokenizer,
/// not evidence against the node. Closing it would mean changing `tokenize`,
/// which re-scores every corpus and forces an `INDEX_VERSION` bump.
pub(crate) fn name_parts(name: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut previous: Option<char> = None;

    for character in name.chars() {
        if !character.is_ascii_alphanumeric() {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
            previous = None;
            continue;
        }
        if character.is_ascii_uppercase() && previous.is_some_and(|last| last.is_ascii_lowercase())
        {
            parts.push(std::mem::take(&mut current));
        }
        current.push(character.to_ascii_lowercase());
        previous = Some(character);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
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

/// Byte ranges of every occurrence of `lower_term` in `lower_text` delimited by
/// non-identifier characters. BOTH arguments must already be ASCII-lowercased.
///
/// The exact-match rungs must not fire on a fragment buried inside a longer
/// word. Curated prose is full of one- and two-character backticked tokens
/// (`n`, `rg`, `pub`), and plain substring containment made a symbol like `n`
/// match every query containing the letter — awarding the exact-symbol boost,
/// and a `high` confidence label, essentially at random. Requiring identifier
/// boundaries keeps `Foo::Bar` and `src/context/pack.rs` matching while a bare
/// `n` no longer matches "signal generation".
///
/// Case folding is the CALLER's job precisely because the returned ranges are
/// byte offsets its caller compares against spans taken from the raw prompt
/// (`evidence::occurs_backticked`). `str::to_ascii_lowercase` preserves every
/// offset, so folding once at the top and threading the folded text down keeps
/// the two coordinate systems identical — and keeps a 500-character stage query
/// from being re-folded once per candidate name.
pub(crate) fn whole_term_ranges(lower_text: &str, lower_term: &str) -> Vec<(usize, usize)> {
    if lower_term.is_empty() {
        return Vec::new();
    }
    let is_identifier_char = |c: char| c.is_ascii_alphanumeric() || c == '_';

    let mut ranges = Vec::new();
    let mut offset = 0;
    while let Some(found) = lower_text[offset..].find(lower_term) {
        let start = offset + found;
        let end = start + lower_term.len();
        let before_ok = lower_text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_identifier_char(c));
        let after_ok = lower_text[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_identifier_char(c));
        if before_ok && after_ok {
            ranges.push((start, end));
        }
        offset = start + lower_term.chars().next().map_or(1, char::len_utf8);
    }
    ranges
}

/// Byte range of the first identifier-boundary-delimited occurrence of `term`
/// in `text`, case-insensitively.
///
/// The range indexes `text` directly: ASCII case folding does not move a single
/// byte, so an offset found in the folded copy is the same offset in the
/// original.
pub(crate) fn find_whole_term(text: &str, term: &str) -> Option<(usize, usize)> {
    whole_term_ranges(&text.to_ascii_lowercase(), &term.to_ascii_lowercase())
        .into_iter()
        .next()
}

/// True when `term` occurs in `text` delimited by non-identifier characters.
///
/// See [`whole_term_ranges`] for why the boundaries are required at all.
pub(crate) fn contains_whole_term(text: &str, term: &str) -> bool {
    find_whole_term(text, term).is_some()
}
