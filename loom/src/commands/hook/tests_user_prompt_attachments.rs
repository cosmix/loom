//! Tests for attachment stripping, at both levels it matters: the token rule
//! itself, and what `parse_prompt` hands retrieval once it has run.

use super::super::parse_prompt;
use super::*;

/// The prompt `parse_prompt` extracts from a payload carrying `prompt`.
fn parsed(prompt: &str) -> Option<String> {
    let payload = serde_json::json!({ "session_id": "abc", "prompt": prompt }).to_string();
    parse_prompt(&payload)
}

#[test]
fn an_attached_path_is_dropped_and_the_question_around_it_survives() {
    let parsed = parsed("read @doc/loom/knowledge/INDEX.md and the remaining knowledge files")
        .expect("a question remains once the attachment is gone");

    assert!(
        !parsed.contains("INDEX.md"),
        "the attached path must not reach retrieval: {parsed}"
    );
    assert!(parsed.contains("read"), "{parsed}");
    assert!(parsed.contains("the remaining knowledge files"), "{parsed}");
}

/// The whole point of the rule: a path the user typed as part of the question
/// is the question, and must still fire the `ExactPath` rung.
#[test]
fn a_path_the_user_typed_without_an_at_sign_is_kept() {
    let parsed =
        parsed("where is loom/src/context/rank.rs defined in this tree?").expect("a real question");

    assert!(parsed.contains("loom/src/context/rank.rs"), "{parsed}");
}

#[test]
fn a_bare_at_handle_is_not_an_attachment() {
    let parsed =
        parsed("ask @dimos whether the merge queue design still holds").expect("a real question");

    assert!(parsed.contains("@dimos"), "{parsed}");
}

/// An attachment on its own asked nothing. What is left after stripping is
/// empty, so the length floor — not a special case — drops it.
#[test]
fn a_prompt_that_is_only_an_attachment_earns_no_retrieval() {
    assert!(parsed("@doc/loom/knowledge/INDEX.md").is_none());
    assert!(parsed("  @doc/loom/knowledge/architecture.md  ").is_none());
}

#[test]
fn an_extension_only_attachment_is_dropped_with_its_trailing_punctuation() {
    assert!(is_attachment("@INDEX.md"));
    assert!(is_attachment("@INDEX.md."));
    assert!(is_attachment("@Cargo.toml,"));
    assert!(is_attachment("@doc/plans/PLAN-thing.md"));
    assert!(is_attachment("@src/"));
}

#[test]
fn a_token_that_is_not_a_path_is_not_an_attachment() {
    assert!(!is_attachment("@dimos"));
    assert!(!is_attachment("@"));
    assert!(!is_attachment("doc/loom/knowledge/INDEX.md"));
    assert!(!is_attachment("@.md"), "no stem to name a file");
    assert!(
        !is_attachment("@thanks."),
        "a word with sentence punctuation is not a filename"
    );
}

/// Stripping is the only edit this function may make: a prompt carrying no
/// attachment has to come out byte-identical, newlines included.
#[test]
fn a_prompt_without_attachments_is_returned_unchanged() {
    let prompt = "why does the daemon\nrestart after a merge?\n  indented line";
    assert_eq!(strip_attachments(prompt), prompt);
}

#[test]
fn stripping_leaves_the_surrounding_whitespace_where_it_was() {
    assert_eq!(
        strip_attachments("first line\n@notes/todo.md\nsecond line"),
        "first line\n\nsecond line"
    );
}
