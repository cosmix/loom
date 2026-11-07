//! Extract assertion statements from test-file text.

use regex::Regex;

/// One assertion statement of a test file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Statement {
    /// Comment-free code, with multiline whitespace outside strings collapsed.
    pub text: String,
    /// Comparison key for the comment-free code.
    pub key: String,
    /// Byte offset where the statement starts.
    pub start: usize,
    /// Byte offset one past the code covered by the statement.
    pub end: usize,
}

#[derive(Clone, Copy)]
struct Quote {
    delimiter: char,
    triple: bool,
    escaped: bool,
    /// `#` count of a Rust raw string (`r#"..."#`); such a string has no escapes.
    raw_hashes: Option<usize>,
}

/// `#` count of a Rust raw string opening (`r"`, `r#"`, `br##"`) at the start of
/// `rest`, whose opening is the `r`, the hashes and the quote.
fn raw_string_hashes(rest: &str) -> Option<usize> {
    let hashes = rest.strip_prefix('r')?.chars().take_while(|&ch| ch == '#');
    let count = hashes.count();
    rest[1 + count..].starts_with('"').then_some(count)
}

/// Whether `rest` starts a Rust char literal (`'x'`, `'\''`, `'\u{1F600}'`)
/// rather than a lifetime (`'a`) or loop label.
fn rust_char_literal(rest: &str) -> bool {
    let mut chars = rest.chars().skip(1);
    match chars.next() {
        Some('\\') => chars
            .skip(1)
            .take(10)
            .take_while(|&ch| ch != '\n')
            .any(|ch| ch == '\''),
        Some('\'' | '\n') | None => false,
        Some(_) => chars.next() == Some('\''),
    }
}

impl Quote {
    fn opening(rest: &str, profile: &str) -> Option<Self> {
        let delimiter = rest.chars().next()?;
        let rust = profile == "rust";
        if rust && delimiter == 'r' {
            let hashes = raw_string_hashes(rest)?;
            return Some(Self {
                delimiter: '"',
                triple: false,
                escaped: false,
                raw_hashes: Some(hashes),
            });
        }
        let char_literal = rust && delimiter == '\'' && rust_char_literal(rest);
        if !matches!(delimiter, '"' | '`') && !(delimiter == '\'' && (!rust || char_literal)) {
            return None;
        }
        let triple = profile == "python"
            && delimiter != '`'
            && rest.starts_with(&delimiter.to_string().repeat(3));
        Some(Self {
            delimiter,
            triple,
            escaped: false,
            raw_hashes: None,
        })
    }

    // Returns the number of bytes consumed and whether the string closed.
    fn consume(&mut self, rest: &str) -> (usize, bool) {
        let ch = rest.chars().next().unwrap_or('\0');
        if let Some(hashes) = self.raw_hashes {
            let closes = ch == '"'
                && rest[1..]
                    .bytes()
                    .take(hashes)
                    .filter(|&b| b == b'#')
                    .count()
                    == hashes;
            return if closes {
                (1 + hashes, true)
            } else {
                (ch.len_utf8(), false)
            };
        }
        if self.delimiter == '\'' && !self.triple && ch == '\n' {
            return (0, true);
        }
        if self.escaped {
            self.escaped = false;
        } else if ch == '\\' {
            self.escaped = true;
        } else if ch == self.delimiter {
            if !self.triple {
                return (ch.len_utf8(), true);
            }
            if rest.starts_with(&self.delimiter.to_string().repeat(3)) {
                return (3, true);
            }
        }
        (ch.len_utf8(), false)
    }

    fn opening_len(self) -> usize {
        if let Some(hashes) = self.raw_hashes {
            2 + hashes
        } else if self.triple {
            3
        } else {
            self.delimiter.len_utf8()
        }
    }
}

fn line_comment(rest: &str, profile: &str) -> bool {
    (rest.starts_with("//")
        && matches!(
            profile,
            "rust"
                | "go"
                | "javascript"
                | "java"
                | "kotlin"
                | "scala"
                | "csharp"
                | "swift"
                | "cpp"
                | "dart"
                | "php"
        ))
        || (rest.starts_with('#') && matches!(profile, "python" | "ruby" | "elixir" | "php"))
}

struct Scanner<'a> {
    content: &'a str,
    profile: &'a str,
    pos: usize,
    depth: usize,
    quote: Option<Quote>,
    code: String,
}

impl<'a> Scanner<'a> {
    fn new(content: &'a str, profile: &'a str, start: usize) -> Self {
        Self {
            content,
            profile,
            pos: start,
            depth: 0,
            quote: None,
            code: String::new(),
        }
    }

    fn scan(mut self) -> (usize, String) {
        while self.pos < self.content.len() {
            if self.quote.is_some() {
                self.string_char();
                continue;
            }
            let rest = &self.content[self.pos..];
            // Block comments deliberately remain code; only line comments are recognized.
            if line_comment(rest, self.profile) {
                self.pos += rest.find('\n').unwrap_or(rest.len());
                continue;
            }
            if let Some(quote) = Quote::opening(rest, self.profile) {
                let len = quote.opening_len();
                self.code.push_str(&rest[..len]);
                self.pos += len;
                self.quote = Some(quote);
                continue;
            }
            if !self.code_char() {
                break;
            }
        }
        (self.pos, self.code)
    }

    fn string_char(&mut self) {
        let rest = &self.content[self.pos..];
        if let Some(quote) = &mut self.quote {
            let (len, closed) = quote.consume(rest);
            self.code.push_str(&rest[..len]);
            self.pos += len;
            if closed {
                self.quote = None;
            }
        }
    }

    fn code_char(&mut self) -> bool {
        let ch = self.content[self.pos..].chars().next().unwrap_or('\0');
        match ch {
            '(' | '[' | '{' => self.depth += 1,
            ')' | ']' | '}' if self.depth == 0 => return false,
            ')' | ']' | '}' => self.depth -= 1,
            ';' if self.depth == 0 => {
                self.code.push(ch);
                self.pos += 1;
                return false;
            }
            '\n' if self.depth == 0 => {
                if !self.drop_continuation() {
                    return false;
                }
            }
            '\n' => {
                self.drop_continuation();
            }
            _ => {}
        }
        self.code.push(ch);
        self.pos += ch.len_utf8();
        true
    }

    fn drop_continuation(&mut self) -> bool {
        if let Some((index, '\\')) = self
            .code
            .char_indices()
            .rev()
            .find(|(_, ch)| !ch.is_whitespace())
        {
            self.code.remove(index);
            true
        } else {
            false
        }
    }
}

// Collapse only whitespace outside strings, leaving string contents verbatim.
fn display_text(code: &str, profile: &str) -> String {
    let mut out = String::new();
    let mut whitespace = String::new();
    let mut quote: Option<Quote> = None;
    let mut pos = 0;
    while pos < code.len() {
        let rest = &code[pos..];
        if let Some(open) = &mut quote {
            let (len, closed) = open.consume(rest);
            out.push_str(&rest[..len]);
            pos += len;
            if closed {
                quote = None;
            }
        } else if let Some(open) = Quote::opening(rest, profile) {
            flush_whitespace(&mut out, &mut whitespace);
            let len = open.opening_len();
            out.push_str(&rest[..len]);
            pos += len;
            quote = Some(open);
        } else {
            let ch = rest.chars().next().unwrap_or('\0');
            if ch.is_whitespace() {
                whitespace.push(ch);
            } else {
                flush_whitespace(&mut out, &mut whitespace);
                out.push(ch);
            }
            pos += ch.len_utf8();
        }
    }
    flush_whitespace(&mut out, &mut whitespace);
    out.trim().to_owned()
}

fn flush_whitespace(out: &mut String, whitespace: &mut String) {
    if whitespace.contains('\n') {
        out.push(' ');
    } else {
        out.push_str(whitespace);
    }
    whitespace.clear();
}

/// Start of the statement around an infix match for kotlin and scala: the
/// first non-blank character of the match's line, moved past the end of an
/// earlier statement on that line (`;`) and past any closer of a bracket an
/// earlier line opened (`} shouldBe x` after a multi-line `f { ... }`).
fn statement_start(content: &str, found: usize, cursor: usize, profile: &str) -> usize {
    if !matches!(profile, "kotlin" | "scala") {
        return found;
    }
    let line_start = content[..found].rfind('\n').map_or(0, |pos| pos + 1);
    let prefix = &content[line_start..found];
    let first = prefix
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map_or(found, |(pos, _)| line_start + pos);
    first
        .max(cursor)
        .max(line_start + prefix_boundary(prefix, profile))
}

/// Offset in `prefix` just past its last `;` or last closer that has no
/// opener inside `prefix`, outside strings; `0` when there is none.
fn prefix_boundary(prefix: &str, profile: &str) -> usize {
    let mut pos = 0;
    let mut last = 0;
    let mut depth = 0usize;
    let mut quote: Option<Quote> = None;
    while pos < prefix.len() {
        let rest = &prefix[pos..];
        if let Some(open) = &mut quote {
            let (len, closed) = open.consume(rest);
            pos += len;
            if closed {
                quote = None;
            }
        } else if line_comment(rest, profile) {
            break;
        } else if let Some(open) = Quote::opening(rest, profile) {
            pos += open.opening_len();
            quote = Some(open);
        } else {
            let ch = rest.chars().next().unwrap_or('\0');
            pos += ch.len_utf8();
            match ch {
                ';' => last = pos,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth > 0 => depth -= 1,
                ')' | ']' | '}' => last = pos,
                _ => {}
            }
        }
    }
    last
}

/// Extract assertions in source order, including matches in strings and comments.
pub(super) fn assertion_statements(
    content: &str,
    assertion: &Regex,
    profile: &str,
) -> Vec<Statement> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while let Some(found) = assertion.find_at(content, cursor) {
        let start = statement_start(content, found.start(), cursor, profile);
        let (end, code) = Scanner::new(content, profile, start).scan();
        result.push(Statement {
            text: display_text(&code, profile),
            key: super::edits::normalize(&code, profile),
            start,
            end,
        });
        if end > found.start() {
            cursor = end;
        } else if let Some(ch) = content[found.start()..].chars().next() {
            cursor = found.start() + ch.len_utf8();
        } else {
            break;
        }
    }
    result
}

#[cfg(test)]
#[path = "statements_tests.rs"]
mod tests;
