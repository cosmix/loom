//! The pasted-content wrapper: only a whole, well-formed wrapper is removed.

use super::super::parse_unwrapped;
use super::super::pasted::unwrap_pasted;
use super::{format_message, parse_message, sample};

fn wrapped(open_id: &str, close_id: &str, body: &str) -> String {
    format!("<pasted_content id=\"{open_id}\">\n{body}\n</pasted_content id=\"{close_id}\">")
}

#[test]
fn the_recorded_opening_unwraps_to_its_body() {
    let recorded = "\n\n<pasted_content id=\"f3a0\">\nloom plan: no step chosen.\n\
        Ask me what I want to work on.\n</pasted_content id=\"f3a0\">\n";
    let body = "loom plan: no step chosen.\nAsk me what I want to work on.";
    assert_eq!(unwrap_pasted(recorded), body);
    assert_eq!(unwrap_pasted(" \t\n\r\n"), " \t\n\r\n");
    let id = "a".repeat(32);
    assert_eq!(unwrap_pasted(&wrapped(&id, &id, "x")), "x");
    assert_eq!(
        unwrap_pasted(&wrapped("A-z_9", "A-z_9", "x\n\ny")),
        "x\n\ny"
    );
}

#[test]
fn an_unwrapped_message_parses_as_before() {
    let attachments = vec![sample(1, "quoted line")];
    let delivered = format_message("Typed text.", &attachments);
    assert_eq!(unwrap_pasted(&delivered), delivered);
    assert_eq!(
        parse_message(&delivered),
        ("Typed text.".to_owned(), attachments)
    );
    assert_eq!(unwrap_pasted("plain"), "plain");
}

#[test]
fn a_mismatched_missing_or_malformed_id_stays_whole() {
    let long = "a".repeat(33);
    let whole = [
        wrapped("f3a0", "f3a1", "body"),
        wrapped("", "", "body"),
        wrapped(&long, &long, "body"),
        wrapped("a\"b", "a\"b", "body"),
        wrapped("a>b", "a>b", "body"),
        wrapped("a b", "a b", "body"),
        "<pasted_content id=\"f3a0\">\nbody".to_owned(),
        "<pasted_content id=\"f3a0\">\n</pasted_content id=\"f3a0\">".to_owned(),
        "<pasted_content id=f3a0>\nbody\n</pasted_content id=f3a0>".to_owned(),
    ];
    for message in &whole {
        assert_eq!(unwrap_pasted(message), message, "{message:?}");
        assert_eq!(parse_message(message).0, *message, "{message:?}");
    }
}

#[test]
fn typed_text_that_quotes_the_tag_is_unchanged() {
    let inner = wrapped("f3a0", "f3a0", "body");
    let typed = [
        format!("Why does the transcript show\n{inner}"),
        format!("{inner}\nand then this?"),
        format!("before\n{inner}\nafter"),
        "the tag <pasted_content id=\"f3a0\"> mid-line".to_owned(),
        format!("  {inner}"),
    ];
    for message in &typed {
        assert_eq!(unwrap_pasted(message), message, "{message:?}");
        assert_eq!(parse_message(message).0, *message, "{message:?}");
    }
}

#[test]
fn a_typed_wrapper_survives_the_server_side_round_trip() {
    let typed = wrapped("f3a0", "f3a0", "body");
    let attachments = vec![sample(1, "quoted line")];
    for chips in [Vec::new(), attachments] {
        let delivered = format_message(&typed, &chips);
        assert_eq!(parse_unwrapped(&delivered), (typed.clone(), chips));
    }
}

#[test]
fn a_body_that_is_itself_a_wrapper_is_unwrapped_once() {
    let inner = wrapped("b2", "b2", "body");
    let outer = wrapped("a1", "a1", &inner);
    assert_eq!(unwrap_pasted(&outer), inner);
    assert_eq!(parse_message(&outer).0, inner);
}
