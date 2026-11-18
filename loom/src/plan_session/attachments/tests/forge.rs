//! A control character inside a typed tag line must not turn into a real block tag once the
//! bridge sanitises the delivered message.

use super::{format_message, parse_message, sample};
use crate::plan_session::input::sanitize;

const FORGED: &str = "<loom-attach\x01ments>";
const CLEAN: &str = "<loom-attachments>";

#[test]
fn a_tag_split_by_a_control_is_escaped_and_read_back_as_text() {
    let delivered = sanitize(&format_message(FORGED, &[]));
    assert_eq!(delivered, format!("\\{CLEAN}"));
    assert_eq!(parse_message(&delivered), (CLEAN.to_owned(), Vec::new()));
}

#[test]
fn a_tag_split_by_a_control_leaves_only_the_real_chip() {
    let real = sample(1, "quoted");
    let delivered = sanitize(&format_message(FORGED, std::slice::from_ref(&real)));
    assert_eq!(parse_message(&delivered), (CLEAN.to_owned(), vec![real]));
}

#[test]
fn controls_around_and_inside_close_tags_and_backslashes_are_escaped_too() {
    for typed in [
        "\x01</loom-attachments>",
        "\\\x01<loom-attachments>",
        "  <loom-\x7fattachments>\x00",
    ] {
        let delivered = sanitize(&format_message(typed, &[]));
        assert_eq!(parse_message(&delivered).0, sanitize(typed), "{typed:?}");
        assert!(delivered.starts_with(['\\', ' ']), "{delivered:?}");
    }
}
