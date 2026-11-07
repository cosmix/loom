# gate-noise-and-review C1: assertion-statement extraction (codex unit)

Lane: codex terra (`--model gpt-6.1-terra --effort xhigh`), foreground, Bash timeout 1560000 ms.
Wave 1, in the same message as C2. Read `common.md` in the directory that holds this brief first.
Source paths below are relative to the worktree's `loom/` directory (the package root). Do not run
git or touch `.loom/`. Compile-only check: `cargo build --lib` (warnings about unused items are
expected until the detector is wired).

## Files

Own: `src/verify/integrity/statements.rs` (the orchestrator created it with one `//!` line and
declared `mod statements;` in `src/verify/integrity/mod.rs`) and
`src/verify/integrity/statements_tests.rs`. Never edit `mod.rs`, `edits.rs` or any other file.
Read: `src/verify/integrity/edits.rs` (`normalize`, now `pub(super)`: call it as
`super::edits::normalize(text, profile)`), `src/verify/integrity/count.rs` (`Matchers.assertion`
regex per profile), `src/testrun/languages.rs` (the 14 profile names and assertion regexes).

## Why

`edits.rs::lost_assertions` compares removed and added diff LINES that match the assertion regex. A
reflow, a multi-line assert and a rewrap beside a real edit raise false events, and a changed
continuation line of a multi-line assert (`3,` to `4,`) raises none. Statements fix both.

## Interface (pinned; W1 and the contracts call it)

```rust
use regex::Regex;

/// One assertion statement of a test file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Statement {
    /// The statement's code, comments removed, every run of whitespace outside string literals
    /// that holds a line break replaced by one space, trimmed; a line break inside a string
    /// literal is kept as written. It includes the terminating `;` when that ended it.
    pub text: String,
    /// `super::edits::normalize(&code, profile)` over the same code: the comparison key.
    pub key: String,
    /// Byte offset in `content` where the statement starts (rule 1).
    pub start: usize,
    /// Byte offset in `content` one past the last byte the statement covers (after the `;`, or at
    /// the line break / closing bracket that ended it).
    pub end: usize,
}

/// Every assertion statement of `content`, in order. `assertion` is the profile's compiled
/// assertion regex, `profile` its name (`rust`, `go`, `python`, ...).
pub(super) fn assertion_statements(content: &str, assertion: &Regex, profile: &str) -> Vec<Statement>;
```

## Rules (apply exactly; they are the decided values)

1. Cursor starts at 0. Find the next regex match at or after the cursor (`Regex::find_at`). The
   statement starts at the MATCH START, except for `kotlin` and `scala`: their infix matchers
   (`\bshould\w+\b`, `\bshould(?:Be|Equal|EqualTo)\b`) match mid-line, so a statement starts at
   the latest of the first non-blank character of the match's line, the cursor, and one past the
   last `;` outside strings between that line start and the match (an earlier statement on the
   line); `x shouldBe 1` is then one statement with its subject. Scan forward from the start. After
   the statement ends at `end`, the cursor is `end`; a match nested inside the statement is part of
   it and yields no second statement.
2. Scanning tracks bracket `depth` (`(`, `[`, `{` add one, `)`, `]`, `}` subtract one) outside
   strings and comments, and an optional open string.
3. The statement ENDS at the first of:
   - a `;` at depth 0 outside a string: the `;` is included;
   - a `\n` at depth 0 outside a string, unless the last non-whitespace character before it is
     `\` (a continuation: keep going; the backslash is dropped from `text`): the break is excluded;
   - a closing bracket that takes `depth` below 0 (the assertion sat inside a closure or block
     such as `|| assert!(x) }`): the bracket is excluded;
   - the end of `content`.
4. Strings, per profile. Delimiters: `"` and the backtick for every profile; `'` too except for
   `rust` (lifetimes, char literals). A backslash escapes the next character inside a string. A
   `"` string and a backtick string may span line breaks. A `'` string ends at its closing quote OR
   at the next line break, whichever comes first (so an apostrophe in a trailing comment cannot
   swallow following lines). `python` also treats `"""` and `'''` as triple-quoted strings that
   span lines and close on the same triple. Bracket, `;` and comment characters inside a string
   are text.
5. Line comments outside strings: `//` for `rust`, `go`, `javascript`, `java`, `kotlin`, `scala`,
   `csharp`, `swift`, `cpp`, `dart`, `php`; `#` for `python`, `ruby`, `elixir`, `php`. The comment
   runs to the next line break and is left out of `text` and `key`; the line break after it still
   counts as a break. Block comments are not handled (residual risk, documented in a comment).
6. `text` is the code with comments removed; `key` is `normalize(code_before_whitespace_collapse,
   profile)` computed over the same comment-free code (so a comment edit next to an assertion
   changes nothing). Whitespace collapse for `text` applies only to runs outside string literals
   that contain a line break; a run of spaces inside one line stays as written, and a string
   literal keeps its line breaks (the reviewer-brief hook flattens them with `single_line`).
7. A match inside a string or a comment still starts a statement (the regex is lexical, as the
   count code is). Do not try to be smarter.

## Steps

1. Write `assertion_statements` and small helpers, each function under 50 lines, the file under 300
   lines. A private `struct Scanner` holding `(chars, depth, string state)` is the clean shape.
2. Write `statements_tests.rs`, declared `#[cfg(test)] #[path = "statements_tests.rs"] mod tests;`
   at the bottom of `statements.rs`. One test per case, building regexes from
   `crate::testrun::languages::all()` by profile name (`Regex::new(profile.assertion)`):
   - rust single line: `    assert_eq!(a, b);` yields text `assert_eq!(a, b);`, start at the `a` of `assert`;
   - rust multi-line with trailing comma equals the single-line form by `key`, differs in `text`;
   - `;` inside a string literal does not end the statement;
   - two asserts on one line yield two statements;
   - closure: `let f = || assert!(x) }` ends before `}`;
   - rust `fn f<'a>(x: &'a str) { assert!(x.is_empty()); }` keeps `'a` out of string state;
   - comment: `assert!(a); // note's` yields `assert!(a);`; `assert!(a, // why\n b);` yields
     `assert!(a, b);` and a changed comment leaves `key` unchanged;
   - javascript: `expect(a)\n  .toBe(1);` ends at the first line break (documented cut) and the two
     halves are separate lines of text; `expect(\n a,\n).toBe(1);` is one statement;
   - javascript apostrophe: `expect(a); // don't` followed by `expect(b);` yields two statements;
   - python: `assert x == (1,\n 2)` is one statement; `assert x == 1\nassert y == 2` is two;
     backslash continuation joins two lines; a trailing comma in `(1,)` is kept in `key`;
   - python triple-quoted string spanning lines inside an assert stays one statement;
   - rust `assert_eq!(s, "a\nb");` written with a real line break inside the literal keeps that
     line break in `text`;
   - kotlin: `    x shouldBe 1` yields text `x shouldBe 1`; `x shouldBe 1` and `y shouldBe 1` have
     different keys; `val r = f(); r shouldBe 1` yields `r shouldBe 1`; scala `x shouldEqual 2`
     yields `x shouldEqual 2`;
   - unterminated statement at end of content ends at the end;
   - empty content and content with no match yield an empty vector.
3. `cargo build --lib` (the only check you may run besides your tests; if the sandbox allows it,
   `cargo test --lib verify::integrity::statements::` once).

## Traps

- `start` and `end` are byte offsets into `content`, not char indexes: advance with
  `char_indices` and keep `str` slicing on char boundaries.
- The regex match for `go` is `\bt\.(?:Error|Fatal|Fail)...\(`: its statement starts at `t.`, not
  at the line start.
- `normalize` drops all whitespace outside strings, so `key` equality survives reflow; do not
  pre-normalize `text`.
- Never panic on any input: this runs on every changed test file of every stage.
