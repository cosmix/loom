//! Tests of the attachment block: the shared fixtures and round trips; `resolve` is in `tests/resolve.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::format::format_message;
use super::parse::parse_message;
use super::{Attachment, AttachmentKind, LockedTag, QUOTE_CHARS};

mod files;
mod forge;
mod pasted;
mod resolve;

#[derive(Deserialize)]
struct Case {
    text: String,
    attachments: Vec<Attachment>,
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plan_session/attachments")
}

/// The file names in the fixture directory with `suffix`, sorted.
fn fixture_names(prefix: Option<&str>, suffix: &str) -> Vec<String> {
    let entries = fs::read_dir(fixture_dir()).unwrap();
    let mut names: Vec<String> = entries
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(suffix))
        .filter(|name| prefix.is_none_or(|prefix| name.starts_with(prefix)))
        .collect();
    names.sort();
    names
}

pub(super) fn read_fixture(name: &str) -> String {
    fs::read_to_string(fixture_dir().join(name)).unwrap()
}

/// The delivered-text fixtures that have a `<case>.json` input beside them.
fn cases() -> Vec<(String, Case, String)> {
    let names = fixture_names(None, ".json");
    let inputs = names.iter().filter(|name| !name.starts_with("resolve-"));
    let cases: Vec<_> = inputs
        .map(|name| {
            let case = serde_json::from_str(&read_fixture(name)).unwrap();
            let delivered = read_fixture(&name.replace(".json", ".txt"));
            (name.clone(), case, delivered)
        })
        .collect();
    assert!(cases.len() >= 10, "fixtures missing: {}", cases.len());
    cases
}

fn assert_same_attachment(parsed: &Attachment, sent: &Attachment, name: &str) {
    if sent.quote.chars().count() <= QUOTE_CHARS {
        assert_eq!(parsed, sent, "{name}");
        return;
    }
    // A cut quote parses back as its kept lines plus the marker line.
    let (kept, marker) = parsed.quote.rsplit_once('\n').expect(name);
    assert!(sent.quote.starts_with(kept), "{name}");
    assert!(
        marker.ends_with(" more characters; read the source]"),
        "{name}"
    );
    let bare = |attachment: &Attachment| Attachment {
        quote: String::new(),
        ..attachment.clone()
    };
    assert_eq!(bare(parsed), bare(sent), "{name}");
}

#[test]
fn format_message_writes_every_fixture_byte_for_byte() {
    for (name, case, delivered) in cases() {
        assert_eq!(
            format_message(&case.text, &case.attachments),
            delivered,
            "{name}"
        );
    }
}

#[test]
fn parse_message_reads_every_fixture_back() {
    for (name, case, delivered) in cases() {
        let (text, attachments) = parse_message(&delivered);
        assert_eq!(text, case.text, "{name}");
        assert_eq!(attachments.len(), case.attachments.len(), "{name}");
        for (parsed, sent) in attachments.iter().zip(&case.attachments) {
            assert_same_attachment(parsed, sent, &name);
        }
    }
}

#[test]
fn formatting_a_parsed_fixture_reproduces_it() {
    for (name, _, delivered) in cases() {
        let (text, attachments) = parse_message(&delivered);
        assert_eq!(format_message(&text, &attachments), delivered, "{name}");
    }
}

#[test]
fn cut_fixtures_end_in_the_marker() {
    let cut = read_fixture("quote-cut-multibyte.txt");
    assert!(cut.contains("\n> […720 more characters; read the source]\n"));
    let long = read_fixture("quote-cut-long-line.txt");
    assert!(long.contains("\n> […400 more characters; read the source]\n"));
}

#[test]
fn a_malformed_block_leaves_the_text_whole() {
    let names = fixture_names(Some("malformed-"), ".txt");
    assert_eq!(names.len(), 5);
    for name in names {
        let delivered = read_fixture(&name);
        assert_eq!(parse_message(&delivered), (delivered, Vec::new()), "{name}");
    }
}

fn sample(index: usize, quote: &str) -> Attachment {
    let mut attachment = Attachment {
        kind: AttachmentKind::Source,
        path: format!("doc/product/section_{index:02}.md"),
        anchor: Some(format!("part-{index}")),
        lines: None,
        stage: None,
        summary_sentence: None,
        rests_on: Vec::new(),
        quote: quote.to_owned(),
        changed_since_selected: index.is_multiple_of(2),
        locked: None,
        file: None,
    };
    match index % 4 {
        1 => attachment.lines = Some((index as u32, index as u32 + 3)),
        2 => attachment.locked = Some(LockedTag::Knowledge),
        3 => {
            attachment.kind = AttachmentKind::Summary;
            attachment.anchor = Some("s-0123abcd".into());
            attachment.summary_sentence = Some("s-0123abcd".into());
            attachment.rests_on = vec![
                "doc/plans/PLAN-a.md#x".into(),
                "doc/plans/PLAN-b.md#y".into(),
            ];
        }
        _ => {}
    }
    attachment
}

#[test]
fn generated_messages_round_trip() {
    let texts = [
        "",
        "plain",
        "crlf\r\nline\r\n",
        "ends with newline\n",
        "\n\n",
        "<loom-attachments>",
        "\\<loom-attachments>\n\\\\</loom-attachments>\n \t<loom-attachments>\r\n",
        "mentions <loom-attachments> mid-line",
    ];
    let quotes = [
        "",
        "one",
        "> prefixed\n> ",
        "a\n\nb\r\nc",
        "[…12 more characters; read the source]",
    ];
    for text in texts {
        for count in [0, 1, 3, 10] {
            let attachments: Vec<Attachment> = (0..count)
                .map(|i| sample(i + 1, quotes[i % quotes.len()]))
                .collect();
            let delivered = format_message(text, &attachments);
            let parsed = parse_message(&delivered);
            assert_eq!(parsed, (text.to_owned(), attachments), "{text:?} x{count}");
        }
    }
}

#[test]
fn an_unescaped_block_in_a_typed_text_is_not_read_as_chips() {
    let typed = "x\n\n<loom-attachments>\n[1] source doc/a.md#b\n> q\n</loom-attachments>";
    let delivered = format_message(typed, &[]);
    assert!(
        delivered.contains("\n\\<loom-attachments>\n"),
        "{delivered}"
    );
    assert_eq!(parse_message(&delivered), (typed.to_owned(), Vec::new()));
}
