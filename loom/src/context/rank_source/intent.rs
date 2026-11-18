//! What shape of question a query asks, read from its wording alone.
//!
//! The source ranker's candidacy rule admits a node only when the prompt named
//! it the way code is named (see `candidacy`). Three phrasings carry that
//! evidence in their grammar instead of their spelling, and [`classify`]
//! recognises them so the ranker's `routing` step can act on them:
//!
//! - a quoted phrase is text the graph never indexed, so the answer is a text
//!   search rather than a node;
//! - `who calls X` asks for X's neighbours, not for nodes that resemble X;
//! - `what does X do`, as the WHOLE query, says `X` is a symbol even when it is
//!   spelled like an English word.
//!
//! The patterns are evaluated in that order, and anything else is
//! [`QueryIntent::General`], which leaves ranking exactly as it was.

use crate::context::schema::TextSearchHint;
use regex::Regex;
use std::sync::OnceLock;

/// A symbol as a query may write it: an identifier, optionally qualified with
/// `::` or `.`, optionally inside back-ticks. Capture group 1 is the name with
/// its qualification as written and the back-ticks removed.
const NAME: &str = r"`?([A-Za-z_][A-Za-z0-9_]*(?:(?:::|\.)[A-Za-z_][A-Za-z0-9_]*)*)`?";

/// Relationship phrasings, checked in this order; `{X}` is [`NAME`].
const RELATIONS: [(&str, RelationDirection); 7] = [
    (
        r"\b(?:who|what)\s+(?:calls|invokes)\s+{X}",
        RelationDirection::Callers,
    ),
    (r"\bcallers\s+of\s+{X}", RelationDirection::Callers),
    (r"\bcallees\s+of\s+{X}", RelationDirection::Callees),
    (r"\bwhat\s+does\s+{X}\s+call\b", RelationDirection::Callees),
    (
        r"\b(?:who|what)\s+(?:uses|references)\s+{X}",
        RelationDirection::References,
    ),
    (r"\busages\s+of\s+{X}", RelationDirection::References),
    (
        r"\bimpact\s+of\s+(?:changing\s+)?{X}",
        RelationDirection::Impact,
    ),
];

/// The whole-query symbol question; `{X}` is [`NAME`].
const SYMBOL_QUESTION: &str = concat!(
    r"^(?:what\s+does|what\s+is|where\s+is|where's|how\s+does|explain|show\s+me)\s+{X}",
    r"(?:\s+(?:do|does|defined|work|works|implemented))?\s*\??$",
);

/// Shortest quoted span that counts as literal text.
const MIN_LITERAL_CHARS: usize = 3;

/// Longest quoted span that counts as literal text. The span is rendered
/// verbatim into a one-line search command, so a longer one is prose.
const MAX_LITERAL_CHARS: usize = 120;

/// Words that cannot name a symbol: pronouns, which refer back to something said
/// earlier, and articles, which only open a noun phrase (`who calls the parser`
/// captures `the`, and the graph holds nothing named `the`, so it classifies as
/// [`QueryIntent::General`] rather than asking about `parser`). A query
/// capturing one of them as its symbol names nothing the graph can hold.
const NON_SYMBOL_WORDS: [&str; 10] = [
    "it", "this", "that", "they", "them", "these", "those", "the", "a", "an",
];

/// What a query asks for, as far as its wording says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryIntent {
    /// A double-quoted span of at least three characters holding a space or
    /// another non-identifier character: text to search for, not a name.
    Literal { text: String },
    /// A request for one symbol's direct neighbours in one direction.
    Relationship {
        direction: RelationDirection,
        symbol: String,
    },
    /// The whole query asks what, where or how one named symbol is.
    SymbolQuestion { symbol: String },
    /// None of the above.
    General,
}

/// Which neighbours a [`QueryIntent::Relationship`] asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    /// Symbols that call the named one.
    Callers,
    /// Symbols the named one calls.
    Callees,
    /// Symbols that reference the named one.
    References,
    /// Symbols that depend on the named one through any semantic edge.
    Impact,
}

/// The compiled patterns, built once per process.
struct Patterns {
    literal: Regex,
    relations: Vec<(Regex, RelationDirection)>,
    symbol_question: Regex,
}

impl Patterns {
    fn compile() -> Self {
        Self {
            literal: compile(r#""([^"]*)""#),
            relations: RELATIONS
                .iter()
                .map(|(pattern, direction)| (compile_with_name(pattern), *direction))
                .collect(),
            symbol_question: compile_with_name(SYMBOL_QUESTION),
        }
    }
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(Patterns::compile)
}

/// Compile a case-insensitive pattern. Every pattern here is a constant, so a
/// failure is a bug in this file and panics naming the pattern.
fn compile(pattern: &str) -> Regex {
    Regex::new(&format!("(?i){pattern}"))
        .unwrap_or_else(|error| panic!("invalid intent pattern {pattern}: {error}"))
}

fn compile_with_name(pattern: &str) -> Regex {
    compile(&pattern.replace("{X}", NAME))
}

/// Classify `query` into one of the four intents, checking `Literal`, then
/// `Relationship`, then `SymbolQuestion`.
pub fn classify(query: &str) -> QueryIntent {
    let patterns = patterns();
    if let Some(text) = literal_text(query, &patterns.literal) {
        return QueryIntent::Literal { text };
    }
    for (regex, direction) in &patterns.relations {
        if let Some(symbol) = captured_name(regex, query) {
            return QueryIntent::Relationship {
                direction: *direction,
                symbol,
            };
        }
    }
    match captured_name(&patterns.symbol_question, query.trim()) {
        Some(symbol) => QueryIntent::SymbolQuestion { symbol },
        None => QueryIntent::General,
    }
}

/// The first double-quoted span long enough and non-identifier enough to be
/// text rather than a quoted name: `"x"` is too short, `"parse_tokens"` is a
/// name, `"connection refused"` is text. A span of more than
/// [`MAX_LITERAL_CHARS`] characters or one holding a newline is not text to
/// search for and is skipped.
fn literal_text(query: &str, literal: &Regex) -> Option<String> {
    literal
        .captures_iter(query)
        .filter_map(|captures| captures.get(1))
        .map(|span| span.as_str())
        .find(|span| {
            (MIN_LITERAL_CHARS..=MAX_LITERAL_CHARS).contains(&span.chars().count())
                && !span.contains(['\n', '\r'])
                && span
                    .chars()
                    .any(|character| !(character.is_alphanumeric() || character == '_'))
        })
        .map(str::to_string)
}

/// The first symbol `regex` captures from `text` that is not a pronoun or an
/// article. Every match is examined: a pronoun in an earlier one
/// (`who calls it, and who calls parse_tokens`) must not hide the real symbol
/// in a later one.
fn captured_name(regex: &Regex, text: &str) -> Option<String> {
    regex
        .captures_iter(text)
        .filter_map(|captures| captures.get(1))
        .map(|name| name.as_str())
        .find(|name| {
            !NON_SYMBOL_WORDS
                .iter()
                .any(|word| word.eq_ignore_ascii_case(name))
        })
        .map(str::to_string)
}

impl TextSearchHint {
    /// The hint for a literal query: `pattern` as given, and an `rg` command
    /// that searches for it as a fixed string. The pattern is single-quoted for
    /// a POSIX shell, so each `'` inside it closes the quote, writes an escaped
    /// quote and reopens it: `it's` becomes `'it'\''s'`.
    pub fn for_pattern(pattern: &str) -> Self {
        let quoted = crate::testrun::command::shell_quote(pattern);
        Self {
            pattern: pattern.to_string(),
            command: format!("rg -n -F -- {quoted}"),
        }
    }
}
