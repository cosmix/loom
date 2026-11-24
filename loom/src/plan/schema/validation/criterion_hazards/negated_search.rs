//! Detects a final negated search over the output of another command.

use super::super::shell_lex::{simple_commands, Token};
use super::command_start;
use super::masked_exit::{statements, strip_parens};

pub(super) fn negates_search(tokens: &[Token]) -> bool {
    let mut statements = statements(tokens)
        .into_iter()
        .rev()
        .filter(|s| !s.is_empty());
    let Some(statement) = statements.next() else {
        return false;
    };
    let last = judged_part(statement);
    is_negated_search(last)
        || and_operand_negates_search(statement)
        || (checks_search_failure(last)
            && statements
                .next()
                .is_some_and(|previous| pipes_to_matcher(judged_part(previous))))
}

/// `! <command> | <matcher>`: the negation turns the matcher's failure into success.
fn is_negated_search(part: &[Token]) -> bool {
    match part {
        [Token::Word(negation), pipeline @ ..] => {
            !negation.quoted
                && !negation.expands
                && negation.value == "!"
                && pipes_to_matcher(pipeline)
        }
        _ => false,
    }
}

/// In `a && b` a failing `a` decides the exit status, so a negated search that is not the last
/// operand of an all-`&&` chain is as fail-open as one in the last position.
fn and_operand_negates_search(statement: &[Token]) -> bool {
    let joins: Vec<(usize, &str)> = top_level_controls(statement)
        .filter(|(_, op)| matches!(*op, "&&" | "||"))
        .collect();
    if joins.iter().any(|(_, op)| *op == "||") {
        return false;
    }
    let mut start = 0;
    for (index, _) in joins {
        if is_negated_search(&statement[start..index]) {
            return true;
        }
        start = index + 1;
    }
    false
}

fn judged_part(mut statement: &[Token]) -> &[Token] {
    loop {
        let mut controls = top_level_controls(statement).peekable();
        if let [Token::Control("("), body @ .., Token::Control(")")] = statement {
            if controls.peek() == Some(&(statement.len() - 1, ")")) {
                // An adjacent paren can belong to an inner command rather than the whole body.
                let stripped = strip_parens(statement);
                statement = if stripped.len() == body.len() {
                    stripped
                } else {
                    body
                };
                continue;
            }
        }
        let Some((index, _)) = controls.filter(|(_, op)| matches!(*op, "&&" | "||")).last() else {
            return statement;
        };
        statement = &statement[index + 1..];
    }
}

fn top_level_controls(tokens: &[Token]) -> impl Iterator<Item = (usize, &'static str)> + '_ {
    let mut depth = 0usize;
    tokens.iter().enumerate().filter_map(move |(index, token)| {
        let Token::Control(op) = *token else {
            return None;
        };
        match op {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            _ => {}
        }
        (depth == 0).then_some((index, op))
    })
}

fn pipes_to_matcher(pipeline: &[Token]) -> bool {
    let mut last_pipe = None;
    for (index, op) in top_level_controls(pipeline) {
        match op {
            "|" | "|&" => last_pipe = Some(index),
            ")" => {}
            _ => return false,
        }
    }
    let Some(index) = last_pipe else {
        return false;
    };
    let tail = &pipeline[index + 1..];
    if tail.iter().any(|token| matches!(token, Token::Control(_))) {
        return false;
    }
    simple_commands(tail).first().is_some_and(|command| {
        let words = &command.words;
        words.get(command_start(words)).is_some_and(|word| {
            matches!(
                word.command_name(),
                "rg" | "grep" | "egrep" | "fgrep" | "ag"
            )
        })
    })
}

fn checks_search_failure(statement: &[Token]) -> bool {
    let args = match statement {
        [Token::Word(test), args @ ..] if test.value == "test" => args,
        [Token::Word(open), args @ .., Token::Word(close)]
            if open.value == "[" && close.value == "]" =>
        {
            args
        }
        _ => return false,
    };
    let [Token::Word(status), Token::Word(op), Token::Word(value)] = args else {
        return false;
    };
    status.expands
        && status.substitutions.is_empty()
        && status.value == "$?"
        && matches!(
            (op.value.as_str(), value.value.as_str()),
            ("-eq", "1") | ("-ne", "0")
        )
}
