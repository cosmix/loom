//! The pasted-content wrapper: every complete, well-formed expansion is removed wherever it sits.

use super::super::parse_unwrapped;
use std::borrow::Cow;

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
    assert_eq!(unwrap_pasted(&delivered), delivered.as_str());
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
        "<pasted_content id=f3a0>\nbody\n</pasted_content id=f3a0>".to_owned(),
    ];
    for message in &whole {
        assert!(
            matches!(unwrap_pasted(message), Cow::Borrowed(_)),
            "{message:?}"
        );
        assert_eq!(unwrap_pasted(message), message.as_str(), "{message:?}");
        assert_eq!(parse_message(message).0, *message, "{message:?}");
    }
}

#[test]
fn a_wrapper_with_an_empty_chunk_unwraps_to_nothing() {
    let empty = "<pasted_content id=\"f3a0\">\n</pasted_content id=\"f3a0\">";
    assert_eq!(unwrap_pasted(empty), "");
}

#[test]
fn a_paste_split_before_the_closing_tag_unwraps_to_the_sent_text() {
    let recorded = "\n\n<pasted_content id=\"5193\">\nBODY\nline two\n\n<loom-attachments>\n\
        [1] source doc/a.md\n> quote\n</pasted_content id=\"5193\">\n\n</loom-attachments>";
    let sent = "BODY\nline two\n\n<loom-attachments>\n[1] source doc/a.md\n> quote\n\
        </loom-attachments>";
    assert_eq!(unwrap_pasted(recorded), sent);
    let (text, attachments) = parse_message(recorded);
    assert_eq!(text, "BODY\nline two");
    assert_eq!(attachments.len(), 1);
    let short = "\n\n<pasted_content id=\"5193\">\nBODY\n</pasted_content id=\"5193\">\n\n</loom-attachments>";
    assert_eq!(unwrap_pasted(short), "BODY\n</loom-attachments>");
}

#[test]
fn a_wrapper_amid_typed_text_is_removed_with_its_expansion_newlines() {
    let recorded =
        "before\n\n<pasted_content id=\"a1\">\nmiddle\n</pasted_content id=\"a1\">\nafter";
    assert_eq!(unwrap_pasted(recorded), "beforemiddleafter");
    let spaced =
        "before \n\n<pasted_content id=\"a1\">\n middle \n</pasted_content id=\"a1\">\n after";
    assert_eq!(unwrap_pasted(spaced), "before  middle  after");
}

#[test]
fn every_complete_wrapper_is_removed() {
    let two = "\n\n<pasted_content id=\"a1\">\nA\n</pasted_content id=\"a1\">\n\
        \n\n<pasted_content id=\"b2\">\nB\n</pasted_content id=\"b2\">\n";
    assert_eq!(unwrap_pasted(two), "AB");
    let one_broken = "\n\n<pasted_content id=\"a1\">\nA\n</pasted_content id=\"a1\">\n\
        \n\n<pasted_content id=\"b2\">\nB";
    assert_eq!(
        unwrap_pasted(one_broken),
        "A\n\n<pasted_content id=\"b2\">\nB"
    );
}

#[test]
fn typed_text_that_quotes_a_tag_inside_a_line_is_unchanged() {
    let inner = wrapped("f3a0", "f3a0", "body");
    let typed = [
        "the tag <pasted_content id=\"f3a0\"> mid-line".to_owned(),
        format!("  {inner}"),
        format!("{inner} and then this?"),
    ];
    for message in &typed {
        assert_eq!(unwrap_pasted(message), message.as_str(), "{message:?}");
        assert_eq!(parse_message(message).0, *message, "{message:?}");
    }
}

#[test]
fn typed_text_that_quotes_a_whole_wrapper_loses_it() {
    let inner = wrapped("f3a0", "f3a0", "body");
    for message in [
        format!("Why does the transcript show\n{inner}"),
        format!("{inner}\nand then this?"),
        format!("before\n{inner}\nafter"),
    ] {
        assert!(unwrap_pasted(&message).contains("body"), "{message:?}");
        assert!(
            !unwrap_pasted(&message).contains("pasted_content"),
            "{message:?}"
        );
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
