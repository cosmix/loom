//! Shell command lexing shared by the criterion hazard lints and the
//! base-tree evaluator.
//!
//! A lexer, not a shell: it splits a command into words and operators the way
//! `sh` does, removes quotes, and records what each word would expand or
//! substitute without doing either. Quoted text stays data, so an operator
//! inside a quoted `rg` pattern (`rg -q "a || true" src`) is part of a word.
//! Comments and heredoc bodies are dropped, as the hook tokenizer drops them.

/// A word after quote removal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Word {
    /// The word as written, quotes included.
    pub raw: String,
    /// The word with quotes and escapes removed. Parameter expansions stay
    /// verbatim (`${TMPDIR:-/tmp}`); command substitutions are left out and
    /// kept in `substitutions` instead.
    pub value: String,
    /// Whether any part of the word was quoted or escaped.
    pub quoted: bool,
    /// Whether the word holds a `$` or backtick outside single quotes.
    pub expands: bool,
    /// Bodies of the `$(...)` and backtick command substitutions in the word.
    pub substitutions: Vec<String>,
}

impl Word {
    /// The variable name when this word is a `NAME=value` assignment.
    pub(crate) fn assignment_name(&self) -> Option<&str> {
        let (name, _) = self.raw.split_once('=')?;
        let mut chars = name.chars();
        let starts_well = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        let valid = starts_well && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        valid.then_some(name)
    }

    /// The command name this word invokes: its value without a directory.
    pub(super) fn command_name(&self) -> &str {
        self.value.rsplit('/').next().unwrap_or(&self.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token {
    Word(Word),
    /// `|`, `||`, `&&`, `;`, `;;`, `&`, `|&`, `(` or `)`; a newline lexes as `;`.
    Control(&'static str),
    /// A redirection operator; a leading file-descriptor number is dropped.
    Redirect(&'static str),
}

/// Operators, longest first so a prefix never shadows a longer match; the
/// flag marks control operators as opposed to redirections.
const OPERATORS: [(&str, bool); 21] = [
    ("&>>", false),
    ("<<<", false),
    ("<<-", false),
    ("||", true),
    ("|&", true),
    ("&&", true),
    (";;", true),
    (">>", false),
    ("<<", false),
    (">&", false),
    ("<&", false),
    ("&>", false),
    ("<>", false),
    (">|", false),
    ("|", true),
    ("&", true),
    (";", true),
    ("(", true),
    (")", true),
    ("<", false),
    (">", false),
];

/// One simple command: its words in order, and its redirection targets.
#[derive(Debug, Default)]
pub(crate) struct SimpleCommand<'a> {
    pub words: Vec<&'a Word>,
    pub redirect_targets: Vec<&'a Word>,
}

/// Lex `command` into words and operators.
pub(crate) fn lex(command: &str) -> Vec<Token> {
    let mut lexer = Lexer {
        chars: command.chars().collect(),
        ..Lexer::default()
    };
    lexer.run();
    lexer.tokens
}

/// Split a token stream at its control operators into simple commands.
pub(crate) fn simple_commands(tokens: &[Token]) -> Vec<SimpleCommand<'_>> {
    let mut commands = vec![SimpleCommand::default()];
    let mut expect_target = false;
    for token in tokens {
        match token {
            Token::Control(_) => {
                commands.push(SimpleCommand::default());
                expect_target = false;
            }
            Token::Redirect(_) => expect_target = true,
            Token::Word(word) => {
                if let Some(current) = commands.last_mut() {
                    if std::mem::take(&mut expect_target) {
                        current.redirect_targets.push(word);
                    } else {
                        current.words.push(word);
                    }
                }
            }
        }
    }
    commands.retain(|c| !c.words.is_empty() || !c.redirect_targets.is_empty());
    commands
}

#[derive(Default)]
struct Lexer {
    chars: Vec<char>,
    pos: usize,
    tokens: Vec<Token>,
    /// The word being built and the position it started at.
    word: Option<(usize, Word)>,
    /// Heredoc delimiters awaiting the next newline, with their `<<-` flag.
    heredocs: Vec<(String, bool)>,
}

impl Lexer {
    fn run(&mut self) {
        while let Some(&c) = self.chars.get(self.pos) {
            match c {
                ' ' | '\t' | '\r' => {
                    self.finish_word();
                    self.pos += 1;
                }
                '\n' => {
                    self.finish_word();
                    self.tokens.push(Token::Control(";"));
                    self.pos += 1;
                    self.skip_heredoc_bodies();
                }
                '#' if self.word.is_none() => self.skip_comment(),
                '\'' => self.single_quoted(),
                '"' => self.double_quoted(),
                '\\' => self.escaped(),
                '$' => self.dollar(),
                '`' => self.backtick(),
                '|' | '&' | ';' | '(' | ')' | '<' | '>' => self.operator(),
                _ => {
                    self.word().value.push(c);
                    self.pos += 1;
                }
            }
        }
        self.finish_word();
    }

    /// The word in progress, started at the current position if there is none.
    fn word(&mut self) -> &mut Word {
        let pos = self.pos;
        &mut self.word.get_or_insert_with(|| (pos, Word::default())).1
    }

    fn slice(&self, start: usize, end: usize) -> String {
        let len = self.chars.len();
        self.chars[start.min(len)..end.min(len)].iter().collect()
    }

    fn finish_word(&mut self) {
        let Some((start, mut word)) = self.word.take() else {
            return;
        };
        word.raw = self.slice(start, self.pos);
        if let Some(Token::Redirect(op @ ("<<" | "<<-"))) = self.tokens.last() {
            self.heredocs.push((word.value.clone(), *op == "<<-"));
        }
        self.tokens.push(Token::Word(word));
    }

    fn single_quoted(&mut self) {
        self.word().quoted = true;
        self.pos += 1;
        while let Some(&c) = self.chars.get(self.pos) {
            self.pos += 1;
            if c == '\'' {
                return;
            }
            self.word().value.push(c);
        }
    }

    fn double_quoted(&mut self) {
        self.word().quoted = true;
        self.pos += 1;
        while let Some(&c) = self.chars.get(self.pos) {
            match c {
                '"' => {
                    self.pos += 1;
                    return;
                }
                '\\' => match self.chars.get(self.pos + 1).copied() {
                    Some(next @ ('$' | '`' | '"' | '\\')) => {
                        self.word().value.push(next);
                        self.pos += 2;
                    }
                    Some('\n') => self.pos += 2,
                    _ => {
                        self.word().value.push('\\');
                        self.pos += 1;
                    }
                },
                '$' => self.dollar(),
                '`' => self.backtick(),
                _ => {
                    self.word().value.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn escaped(&mut self) {
        match self.chars.get(self.pos + 1).copied() {
            Some('\n') => {}
            Some(next) => {
                let word = self.word();
                word.quoted = true;
                word.value.push(next);
            }
            None => self.word().value.push('\\'),
        }
        self.pos += 2;
    }

    fn dollar(&mut self) {
        self.word().expands = true;
        let start = self.pos;
        let next = self.chars.get(self.pos + 1).copied();
        let after = self.chars.get(self.pos + 2).copied();
        match (next, after) {
            (Some('('), Some('(')) => {
                self.pos += 3;
                self.take_balanced('(', ')', 2);
            }
            (Some('('), _) => {
                self.pos += 2;
                let body = self.take_balanced('(', ')', 1);
                self.word().substitutions.push(body);
                return;
            }
            (Some('{'), _) => {
                self.pos += 2;
                self.take_balanced('{', '}', 1);
            }
            _ => self.pos += 1,
        }
        let text = self.slice(start, self.pos);
        self.word().value.push_str(&text);
    }

    fn backtick(&mut self) {
        self.word().expands = true;
        self.pos += 1;
        let body_start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            match c {
                '\\' => self.pos += 2,
                '`' => break,
                _ => self.pos += 1,
            }
        }
        let body = self.slice(body_start, self.pos);
        self.pos = (self.pos + 1).min(self.chars.len());
        self.word().substitutions.push(body);
    }

    /// Consume up to the delimiter that brings `depth` to zero, stepping over
    /// quoted text and escapes, and return what came before it.
    fn take_balanced(&mut self, open: char, close: char, mut depth: usize) -> String {
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            self.pos += 1;
            match c {
                '\\' => self.pos += 1,
                '\'' | '"' => self.skip_quoted(c),
                c if c == open => depth += 1,
                c if c == close => {
                    depth -= 1;
                    if depth == 0 {
                        return self.slice(start, self.pos - 1);
                    }
                }
                _ => {}
            }
        }
        self.pos = self.pos.min(self.chars.len());
        self.slice(start, self.pos)
    }

    fn skip_quoted(&mut self, quote: char) {
        while let Some(&c) = self.chars.get(self.pos) {
            self.pos += 1;
            if c == '\\' && quote == '"' {
                self.pos += 1;
            } else if c == quote {
                return;
            }
        }
    }

    fn operator(&mut self) {
        let ahead = self.slice(self.pos, self.pos + 3);
        let (text, control) = OPERATORS
            .iter()
            .copied()
            .find(|(op, _)| ahead.starts_with(op))
            .unwrap_or((";", true));
        let fd_number = match &self.word {
            Some((_, word)) => !control && is_fd_number(word),
            None => false,
        };
        if fd_number {
            self.word = None;
        } else {
            self.finish_word();
        }
        self.pos += text.len();
        self.tokens.push(if control {
            Token::Control(text)
        } else {
            Token::Redirect(text)
        });
    }

    fn skip_comment(&mut self) {
        while self.chars.get(self.pos).is_some_and(|&c| c != '\n') {
            self.pos += 1;
        }
    }

    fn skip_heredoc_bodies(&mut self) {
        for (delimiter, strip_tabs) in std::mem::take(&mut self.heredocs) {
            while self.pos < self.chars.len() {
                let end = self.chars[self.pos..]
                    .iter()
                    .position(|&c| c == '\n')
                    .map_or(self.chars.len(), |offset| self.pos + offset);
                let line = self.slice(self.pos, end);
                self.pos = (end + 1).min(self.chars.len());
                let line = if strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    line.as_str()
                };
                if line == delimiter {
                    break;
                }
            }
        }
    }
}

/// Whether `word` is the file-descriptor number of a redirection, as in `2>`.
fn is_fd_number(word: &Word) -> bool {
    !word.quoted && !word.expands && word.value.bytes().all(|b| b.is_ascii_digit())
}
