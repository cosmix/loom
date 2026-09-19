//! Detects a stage command whose final top-level statement is guaranteed to
//! exit zero, so its result as an acceptance criterion is meaningless.
//!
//! Only the LAST statement's exit status becomes the script's own: `sh -c`
//! and `bash -c` return whatever their last statement returns, and a `;`
//! chain does too. A `|| true` earlier in the script — guarding cleanup,
//! say — never reaches the caller, so it is not this hazard.

use super::super::shell_lex::{Token, Word};

/// Whether the final top-level statement of `tokens` can never fail. Ending
/// in `|| true`, `|| :` or `|| exit 0` always counts: that guards a real
/// statement earlier in the same chain. Being itself bare `true`, `:` or
/// `exit 0` only counts when a non-empty statement precedes it — a lone
/// `true`/`:`/`exit 0` is the whole command, not a no-op masking one, so it
/// masks nothing.
pub(super) fn masks_exit_status(tokens: &[Token]) -> bool {
    let Some((statement, has_earlier_statement)) = final_statement(tokens) else {
        return false;
    };
    let statement = strip_parens(statement);
    ends_in_forced_success(statement) || (has_earlier_statement && is_forced_success(statement))
}

/// The last non-empty top-level statement in `tokens`, split at `;`, `;;`
/// and `&` outside parens, plus whether a non-empty statement precedes it. A
/// trailing separator (`cmd || true;`) leaves an empty final segment, so the
/// statement before it is used instead.
fn final_statement(tokens: &[Token]) -> Option<(&[Token], bool)> {
    let mut non_empty = statements(tokens).into_iter().filter(|s| !s.is_empty());
    let last = non_empty.next_back()?;
    let has_earlier_statement = non_empty.next().is_some();
    Some((last, has_earlier_statement))
}

/// Split `tokens` at top-level `;`, `;;` and `&`, tracking `(`/`)` depth so a
/// separator inside a subshell group does not end the outer statement.
fn statements(tokens: &[Token]) -> Vec<&[Token]> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut parts = Vec::new();
    for (idx, token) in tokens.iter().enumerate() {
        match token {
            Token::Control("(") => depth += 1,
            Token::Control(")") => depth = (depth - 1).max(0),
            Token::Control(";" | ";;" | "&") if depth == 0 => {
                parts.push(&tokens[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&tokens[start..]);
    parts
}

/// Drop a statement's own wrapping `( ... )`, e.g. `(cargo test || true)`, so
/// the group is inspected the same as its unwrapped body.
fn strip_parens(mut statement: &[Token]) -> &[Token] {
    while let [Token::Control("("), rest @ ..] = statement {
        statement = rest;
    }
    while let [rest @ .., Token::Control(")")] = statement {
        statement = rest;
    }
    statement
}

/// Whether `statement` ends with `|| true`, `|| :` or `|| exit 0`.
fn ends_in_forced_success(statement: &[Token]) -> bool {
    match statement {
        [.., Token::Control("||"), Token::Word(word)] => is_true_or_colon(word),
        [.., Token::Control("||"), Token::Word(exit), Token::Word(zero)] => {
            is_exit_zero(exit, zero)
        }
        _ => false,
    }
}

/// Whether `statement` is nothing but `true`, `:` or `exit 0`.
fn is_forced_success(statement: &[Token]) -> bool {
    match statement {
        [Token::Word(word)] => is_true_or_colon(word),
        [Token::Word(exit), Token::Word(zero)] => is_exit_zero(exit, zero),
        _ => false,
    }
}

fn is_true_or_colon(word: &Word) -> bool {
    !word.quoted && matches!(word.command_name(), "true" | ":")
}

fn is_exit_zero(exit: &Word, zero: &Word) -> bool {
    !exit.quoted && !zero.quoted && exit.command_name() == "exit" && zero.value == "0"
}
