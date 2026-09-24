//! Regex lints over wiring patterns and `rg`/`grep` command lines: a pattern
//! the verifier cannot compile, a `[[` the author almost certainly meant
//! literally, a pattern the tool reads as a flag, and an `rg`/`grep` pattern
//! that is not valid Rust `regex` syntax.

use regex::{Regex, RegexBuilder};

use crate::plan::schema::StageDefinition;
use crate::verify::goal_backward::wiring::PATTERN_SIZE_LIMIT;

use super::super::search_args::SearchArgs;
use super::super::shell_lex::Word;
use super::{visit_stage_argvs, LintContext, LintFinding, StageCommand};

const LITERAL_BRACKETS: &str = "contains `[[`, which opens a character class matching one \
     character; escape it (`\\[\\[`) to match the brackets literally";

pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    for stage in &ctx.metadata.loom.stages {
        check_wiring(stage, out);
        visit_stage_argvs(stage, &mut |command, argv| {
            check_search(stage, command, argv, out);
        });
    }
}

fn check_wiring(stage: &StageDefinition, out: &mut Vec<LintFinding>) {
    for (idx, wiring) in stage.wiring.iter().enumerate() {
        if wiring.literal {
            continue;
        }
        let pattern = &wiring.pattern;
        let label = format!("Wiring #{} pattern `{pattern}`", idx + 1);
        let limited = RegexBuilder::new(pattern)
            .size_limit(PATTERN_SIZE_LIMIT)
            .build();
        // `validate()` already rejects a pattern `Regex::new` cannot parse;
        // this reports one it accepts that the verifier's size limit rejects.
        if let Err(error) = limited {
            if Regex::new(pattern).is_ok() {
                let message = format!(
                    "{label} does not compile under the verifier's {PATTERN_SIZE_LIMIT}-byte \
                     size limit ({error}), so the wiring check can never pass"
                );
                out.push(LintFinding::in_stage(stage, message, true));
            }
        }
        if has_literal_bracket_pair(pattern) {
            let message = format!("{label} {LITERAL_BRACKETS}");
            out.push(LintFinding::in_stage(stage, message, false));
        }
    }
}

fn check_search(
    stage: &StageDefinition,
    command: &StageCommand<'_>,
    argv: &[&Word],
    out: &mut Vec<LintFinding>,
) {
    let Some(args) = SearchArgs::parse(argv) else {
        return;
    };
    // `parse` returned `Some`, so argv holds the tool's name.
    let tool = argv[0].command_name();
    if let Some(word) = pattern_read_as_flag(argv, &args) {
        let problem = format!(
            "passes the pattern `{}` where {tool} reads it as a flag; put `-e` (or `--`) \
             before a pattern that starts with `-`",
            word.value
        );
        out.push(LintFinding::in_stage(
            stage,
            command.describe(&problem),
            true,
        ));
    }
    if has_flag(&args, 'F', "--fixed-strings") {
        return;
    }
    let rust_syntax = uses_rust_syntax(&args);
    for &(idx, offset) in &args.patterns {
        if let Some(pattern) = argv.get(idx).and_then(|word| word.value.get(offset..)) {
            check_pattern(stage, command, tool, pattern, rust_syntax, out);
        }
    }
}

/// One pattern an `rg`/`grep` command passes: a compile error when the tool
/// reads Rust `regex` syntax, and a `[[` meant literally.
fn check_pattern(
    stage: &StageDefinition,
    command: &StageCommand<'_>,
    tool: &str,
    pattern: &str,
    rust_syntax: bool,
    out: &mut Vec<LintFinding>,
) {
    let quoted = format!("passes {tool} the pattern `{pattern}`, which");
    if rust_syntax {
        if let Err(error) = Regex::new(pattern) {
            let problem = format!("{quoted} does not compile: {error}");
            out.push(LintFinding::in_stage(
                stage,
                command.describe(&problem),
                true,
            ));
        }
    }
    if has_literal_bracket_pair(pattern) {
        let problem = format!("{quoted} {LITERAL_BRACKETS}");
        out.push(LintFinding::in_stage(
            stage,
            command.describe(&problem),
            false,
        ));
    }
}

/// The first word the author quoted as a pattern that starts with `-` and
/// sits where the tool takes its positional pattern: unprotected by `-e`,
/// `--regexp`, a pattern file or `--`, the tool parses it as an option.
///
/// Only a word whose leading `-` is itself quoted or escaped counts: an
/// unquoted `--foo` is indistinguishable from an option the tool accepts.
fn pattern_read_as_flag<'w>(argv: &[&'w Word], args: &SearchArgs) -> Option<&'w Word> {
    if has_flag(args, 'e', "--regexp") || has_flag(args, 'f', "--file") {
        return None;
    }
    argv.iter().enumerate().skip(1).find_map(|(idx, word)| {
        let value = word.value.as_str();
        let dash_data = value.starts_with('-') && value != "-" && !word.raw.starts_with('-');
        let positional = args.is_pattern(idx) || args.operands.contains(&idx);
        (dash_data && !positional && is_positional_pattern_slot(argv, idx)).then_some(*word)
    })
}

/// Whether a plain word at `idx` would be parsed as the positional pattern:
/// re-parse with the word replaced, so option values consumed by the word
/// before it are accounted for exactly as `SearchArgs` accounts for them.
fn is_positional_pattern_slot(argv: &[&Word], idx: usize) -> bool {
    let plain = Word {
        raw: "pattern".to_string(),
        value: "pattern".to_string(),
        ..Word::default()
    };
    let mut probe: Vec<&Word> = argv.to_vec();
    probe[idx] = &plain;
    SearchArgs::parse(&probe).is_some_and(|parsed| parsed.patterns == [(idx, 0)])
}

/// Whether the pattern is Rust `regex` syntax: `rg`'s default engine, or
/// `grep -E`. Basic and Perl-compatible regexes follow other rules (`foo(` is
/// valid BRE), so compiling them here would report patterns the tool accepts.
fn uses_rust_syntax(args: &SearchArgs) -> bool {
    if args.is_rg {
        let other_engine = ["--pcre2", "--engine", "--auto-hybrid-regex"]
            .into_iter()
            .any(|name| has_long(args, name));
        return !other_engine && !args.short_flags.contains('P');
    }
    let other = has_flag(args, 'P', "--perl-regexp") || has_flag(args, 'G', "--basic-regexp");
    has_flag(args, 'E', "--extended-regexp") && !other
}

fn has_flag(args: &SearchArgs, short: char, long: &str) -> bool {
    args.short_flags.contains(short) || has_long(args, long)
}

fn has_long(args: &SearchArgs, name: &str) -> bool {
    args.long_options.iter().any(|option| option == name)
}

/// An unescaped `[[` that does not open a POSIX class (`[[:alpha:]]`), an
/// equivalence class (`[[=a=]]`) or a collating symbol (`[[.-.]]`).
fn has_literal_bracket_pair(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut idx = 0;
    while idx + 1 < bytes.len() {
        match (bytes[idx], bytes[idx + 1]) {
            (b'\\', _) => idx += 2,
            (b'[', b'[') => {
                if !matches!(bytes.get(idx + 2), Some(b':' | b'=' | b'.')) {
                    return true;
                }
                idx += 2;
            }
            _ => idx += 1,
        }
    }
    false
}
