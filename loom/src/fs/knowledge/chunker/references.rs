//! Typed evidence extracted from backticked markdown spans.

use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;

static SOURCE_PATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9_$~<>./-]+\.(rs|tsx|ts|py|go|sh|md|toml|yaml|yml)")
        .unwrap_or_else(|error| panic!("source path regex must be valid: {error}"))
});
static SYMBOL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*$")
        .unwrap_or_else(|error| panic!("symbol regex must be valid: {error}"))
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EvidenceKind {
    Live,
    Example,
    Runtime,
    External,
    Historical,
}

impl EvidenceKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Example => "example",
            Self::Runtime => "runtime",
            Self::External => "external",
            Self::Historical => "historical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EvidenceReference {
    pub(crate) source_path: String,
    pub(crate) kind: EvidenceKind,
}

/// Classify a backticked path using its sentence-local role.
pub(crate) fn classify_reference(span: &str, sentence: &str) -> EvidenceKind {
    let path = span.trim().to_lowercase();
    let sentence = sentence.to_lowercase();
    if is_runtime_path(&path) {
        return EvidenceKind::Runtime;
    }
    if contains_any(&path, EXAMPLE_MARKERS) || contains_any(&sentence, EXAMPLE_MARKERS) {
        return EvidenceKind::Example;
    }
    if is_there_is_no_this_path(&sentence, &path) || contains_any(&sentence, HISTORICAL_MARKERS) {
        return EvidenceKind::Historical;
    }
    if contains_any(&sentence, EXTERNAL_MARKERS) {
        return EvidenceKind::External;
    }
    EvidenceKind::Live
}

const EXAMPLE_MARKERS: &[&str] = &[
    "foo",
    "bar",
    "baz",
    "<",
    "...",
    "…",
    "path/to",
    "slug",
    "newcmd",
    "example",
    "e.g.",
    "placeholder",
    "nn-",
    "xxx",
];
const HISTORICAL_MARKERS: &[&str] = &[
    "does not exist",
    "do not exist",
    "no longer",
    "never existed",
    "invented",
    "earlier version",
    "was deleted",
    "was removed",
    "removed in",
    "deleted in",
    "renamed to",
    "used to",
];
const EXTERNAL_MARKERS: &[&str] = &[
    "external",
    "upstream",
    "another project",
    "plugin's",
    "openai",
    "codex-rs",
    "claude code's",
];

fn contains_any(value: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| value.contains(marker))
}

/// True only when "there is no" is immediately followed by this exact
/// backticked path, e.g. "there is no `gc.rs`" — not "there is no reason to
/// change `loom/src/x.rs`", where the phrase names no particular path at all.
/// Both `sentence` and `path` are already lowercased by the caller.
fn is_there_is_no_this_path(sentence: &str, path: &str) -> bool {
    sentence.contains(&format!("there is no `{path}`"))
}

fn is_runtime_path(path: &str) -> bool {
    path.starts_with(".loom/")
        || path.starts_with(".work/")
        || path.starts_with("target/")
        || path.starts_with("node_modules/")
        || path.starts_with('~')
        || path.starts_with("/tmp")
        || path.starts_with('$')
        || path.contains('<')
        || path.contains('>')
}

/// Extract typed source references and identifiers outside fenced code blocks.
pub(crate) fn references_in(body: &str) -> (Vec<EvidenceReference>, Vec<String>) {
    let mut references = Vec::new();
    let mut symbols = Vec::new();
    let mut scanner = super::super::scanner::Scanner::default();

    for raw_line in body.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\n', '\r']);
        if scanner.fenced(line) {
            continue;
        }
        references_in_line(line, &mut references, &mut symbols);
    }

    (deduplicate(references), deduplicate(symbols))
}

fn references_in_line(
    line: &str,
    references: &mut Vec<EvidenceReference>,
    symbols: &mut Vec<String>,
) {
    let sentence_ends = sentence_ends(line);
    let mut span_start = None;
    for (position, character) in line.char_indices() {
        if character != '`' {
            continue;
        }
        if let Some(start) = span_start.take() {
            let span = &line[start..position];
            let mut sentence = None;
            for matched in SOURCE_PATH_REGEX.find_iter(span) {
                if !continues_name(&span[matched.end()..]) {
                    let source_path = matched.as_str().to_string();
                    let window = sentence.get_or_insert_with(|| {
                        sentence_window(line, &sentence_ends, start.saturating_sub(1), position + 1)
                    });
                    references.push(EvidenceReference {
                        kind: classify_reference(&source_path, window),
                        source_path,
                    });
                }
            }
            let symbol = span.trim();
            if SYMBOL_REGEX.is_match(symbol) {
                symbols.push(symbol.to_string());
            }
        } else {
            span_start = Some(position + character.len_utf8());
        }
    }
}

/// True when `rest`, the text after a path match, continues the name: an
/// identifier character (`a.rsx`), or a `.` and then one (`AGENTS.md.template`
/// names a template, not `AGENTS.md`). A sentence-final `.` continues nothing.
fn continues_name(rest: &str) -> bool {
    let mut chars = rest.chars();
    let identifier = |character: char| character.is_ascii_alphanumeric();
    match chars.next() {
        Some('.') => chars.next().is_some_and(identifier),
        first => first.is_some_and(identifier),
    }
}

/// The sentence-local window around a reference: bounded by real sentence
/// ends (`sentence_ends`, ascending byte offsets of their `.`) on either side,
/// with every OTHER backtick span's contents blanked out so a marker word in a
/// neighbouring code span never leaks in. The current span, including its
/// backticks, stays verbatim so `is_there_is_no_this_path` still sees it.
fn sentence_window(
    line: &str,
    sentence_ends: &[usize],
    current_open: usize,
    after_closing: usize,
) -> String {
    let current_close = after_closing.saturating_sub(1);
    let before = sentence_ends.partition_point(|&dot| dot < current_open);
    let start = before
        .checked_sub(1)
        .map_or(0, |index| sentence_ends[index] + 1);
    let after = sentence_ends.partition_point(|&dot| dot < after_closing);
    let end = sentence_ends.get(after).map_or(line.len(), |&dot| dot + 1);
    mask_other_spans(line, start, end, current_open, current_close)
}

/// Blank out the contents of every backtick span in `line[start..end]` other
/// than the current reference's own (`current_open`/`current_close` are the
/// absolute indices of its opening and closing backticks).
fn mask_other_spans(
    line: &str,
    start: usize,
    end: usize,
    current_open: usize,
    current_close: usize,
) -> String {
    let mut window = String::with_capacity(end - start);
    let mut inside_other = false;
    for (offset, character) in line[start..end].char_indices() {
        let absolute = start + offset;
        if character == '`' {
            if absolute != current_open && absolute != current_close {
                inside_other = !inside_other;
            }
            window.push(character);
        } else if !inside_other {
            window.push(character);
        }
    }
    window
}

/// Byte offsets of every `.` that ends a sentence, in one pass over `line`. A
/// `.` ends a sentence only when it is followed by whitespace or the end of
/// the line, sits outside any backtick span (an odd number of backticks before
/// it means inside, so unbalanced backticks leave the rest of the line inside),
/// and does not close one of [`ABBREVIATIONS`] (checked case-insensitively):
/// `e.g.`, `i.e.`, `vs.`, and `cf.` never end a sentence in this corpus.
/// `etc.` is deliberately left out — it does end a sentence here.
fn sentence_ends(line: &str) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut inside_backticks = false;
    for (position, character) in line.char_indices() {
        match character {
            '`' => inside_backticks = !inside_backticks,
            '.' if !inside_backticks => {
                let next = position + 1;
                let followed_by_boundary =
                    line[next..].chars().next().is_none_or(char::is_whitespace);
                if followed_by_boundary && !ends_with_abbreviation(&line[..next]) {
                    ends.push(position);
                }
            }
            _ => {}
        }
    }
    ends
}

const ABBREVIATIONS: &[&str] = &["e.g.", "i.e.", "vs.", "cf."];

/// True when `prefix` ends with one of [`ABBREVIATIONS`], comparing only the
/// matching tail (not the whole prefix) so this stays cheap on long lines.
/// `get` avoids a char-boundary panic when `prefix` is shorter than the
/// abbreviation.
fn ends_with_abbreviation(prefix: &str) -> bool {
    ABBREVIATIONS.iter().any(|abbreviation| {
        prefix.len() >= abbreviation.len()
            && prefix
                .get(prefix.len() - abbreviation.len()..)
                .is_some_and(|tail| tail.eq_ignore_ascii_case(abbreviation))
    })
}

fn deduplicate<T>(values: Vec<T>) -> Vec<T>
where
    T: Clone + Ord,
{
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(body: &str) -> Vec<String> {
        references_in(body)
            .0
            .into_iter()
            .map(|reference| reference.source_path)
            .collect()
    }

    #[test]
    fn a_backticked_template_filename_is_not_a_reference_to_its_prefix() {
        assert!(paths("Copy `AGENTS.md.template` into the repo.").is_empty());
    }

    #[test]
    fn a_plain_source_path_is_still_a_reference() {
        assert_eq!(paths("See `src/a.rs` for details."), ["src/a.rs"]);
    }

    #[test]
    fn a_path_followed_by_sentence_punctuation_is_still_a_reference() {
        assert_eq!(paths("Edit `src/a.rs.` first."), ["src/a.rs"]);
    }

    /// The per-dot logic the single pass replaced, kept as the reference.
    fn old_is_sentence_end(line: &str, dot: usize) -> bool {
        let next = dot + 1;
        let followed_by_boundary =
            next >= line.len() || line[next..].chars().next().is_some_and(char::is_whitespace);
        if !followed_by_boundary || line[..dot].matches('`').count() % 2 == 1 {
            return false;
        }
        !ends_with_abbreviation(&line[..next])
    }

    fn old_sentence_ends(line: &str) -> Vec<usize> {
        line.char_indices()
            .filter(|&(index, character)| character == '.' && old_is_sentence_end(line, index))
            .map(|(index, _)| index)
            .collect()
    }

    #[test]
    fn sentence_ends_match_the_per_dot_logic_on_awkward_input() {
        let cases: &[(&str, &[usize])] = &[
            ("", &[]),
            (".", &[0]),
            ("One. Two.", &[3, 8]),
            ("a `b. c` d. e", &[10]),
            ("`unbalanced. still inside. here", &[]),
            ("x. `y` z. `w. v", &[1, 8]),
            ("`a`. `b`. `c`.", &[3, 8, 13]),
            ("`a``b`. next", &[6]),
            ("See e.g. this, vs. that, i.e. CF. done. ok", &[38]),
            ("héllo wörld. ünï. 日本語. `日.` x.", &[13, 20, 31, 41]),
            ("no.space and a.b", &[]),
        ];
        for (line, expected) in cases {
            assert_eq!(sentence_ends(line), *expected, "{line:?}");
            assert_eq!(old_sentence_ends(line), *expected, "{line:?}");
        }
    }

    #[test]
    fn a_window_stops_at_the_sentence_ends_around_a_reference() {
        let line = "Gone `a.rs` x. There is no `gc.rs` here. Then `b.rs`.";
        let kinds: Vec<_> = references_in(line)
            .0
            .into_iter()
            .map(|r| (r.source_path, r.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("a.rs".to_string(), EvidenceKind::Live),
                ("gc.rs".to_string(), EvidenceKind::Historical),
                ("b.rs".to_string(), EvidenceKind::Live),
            ]
        );
    }

    #[test]
    fn a_line_of_half_a_megabyte_of_spans_and_periods_is_scanned_in_linear_time() {
        let body = "`a.md` needle in a sentence. ".repeat(17_000);
        let start = std::time::Instant::now();
        let (references, _) = references_in(&body);
        assert_eq!(references.len(), 1);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "took {:?}",
            start.elapsed()
        );
    }
}
