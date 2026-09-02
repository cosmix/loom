//! Removing Claude Code's `@` file attachments from a prompt before the hook
//! retrieves against it.
//!
//! An attachment's CONTENT is already in the model's context by the time this
//! hook runs; what stays in the prompt text is its path, and a full relative
//! path fires the `ExactPath` rung outright. `read
//! @doc/loom/knowledge/INDEX.md and the remaining knowledge files` therefore
//! filled an entire 1500-token brief with chunks that merely mention
//! `INDEX.md`, and left out the documents the question was actually about.
//! Retrieving against a path whose file the session already holds is pure
//! noise, so the token goes before retrieval ever sees it.
//!
//! Its own file rather than more of `user_prompt.rs`, which was already near
//! the maintainability line limit — wired in the same `#[path]` idiom that
//! file already uses for `user_prompt_compose.rs`.

/// `prompt` with its `@`-prefixed file attachments removed, every other byte
/// and every whitespace run left exactly where it was.
///
/// Whitespace is preserved rather than normalised because stripping
/// attachments is the only change this function is entitled to make: a prompt
/// carrying none must come out identical to the one that went in, newlines
/// included. Removing a token does leave the space that stood beside it, which
/// costs nothing — every consumer downstream tokenises on whitespace.
///
/// Only the `@` form is dropped. A path the user typed deliberately —
/// `where is loom/src/context/rank.rs defined` — must still fire the rung: it
/// is the question, not an attachment riding alongside it.
pub(super) fn strip_attachments(prompt: &str) -> String {
    let mut kept = String::with_capacity(prompt.len());
    let mut token_start = None;
    for (index, character) in prompt.char_indices() {
        if character.is_whitespace() {
            if let Some(start) = token_start.take() {
                push_unless_attachment(&mut kept, &prompt[start..index]);
            }
            kept.push(character);
        } else if token_start.is_none() {
            token_start = Some(index);
        }
    }
    if let Some(start) = token_start {
        push_unless_attachment(&mut kept, &prompt[start..]);
    }
    kept
}

fn push_unless_attachment(kept: &mut String, token: &str) {
    if !is_attachment(token) {
        kept.push_str(token);
    }
}

/// True for one whitespace-delimited token that is a file attachment: a
/// leading `@` over something path-shaped — a remainder holding a `/`, or one
/// ending in a short alphanumeric extension (`@INDEX.md`) — with sentence
/// punctuation trimmed off the end first, so `@INDEX.md.` reads the same as
/// `@INDEX.md`.
///
/// A bare `@name` is left alone: `@` is also how a session names an agent or a
/// person, and neither is a path. The cost of drawing the line there is that a
/// handle shaped exactly like a filename (`@first.last`) reads as an
/// attachment — accepted deliberately, since dropping one rare mention costs a
/// query term while keeping one attachment costs the whole brief.
fn is_attachment(token: &str) -> bool {
    let Some(path) = token.strip_prefix('@') else {
        return false;
    };
    let path = path.trim_end_matches(['.', ',', ';', ':', ')', '!', '?']);
    path.contains('/') || has_file_extension(path)
}

/// True when `path` ends in `.<ext>` with a non-empty stem and a short,
/// alphanumeric extension — the shape a filename has, and an ordinary
/// sentence word does not.
fn has_file_extension(path: &str) -> bool {
    let Some((stem, extension)) = path.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && (1..=8).contains(&extension.chars().count())
        && extension
            .chars()
            .all(|character| character.is_alphanumeric())
}

#[cfg(test)]
#[path = "tests_user_prompt_attachments.rs"]
mod tests;
