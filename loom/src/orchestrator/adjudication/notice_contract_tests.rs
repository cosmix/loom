//! Contract: verdict notices for one stage accumulate in its feedback, so the notice of the
//! first of two disputes judged back to back still reaches the next session.

use super::feedback::{read_feedback, write_notice};

#[test]
fn a_second_verdict_notice_keeps_the_first() {
    let dir = tempfile::tempdir().unwrap();
    write_notice(dir.path(), "s1", "first notice").unwrap();
    write_notice(dir.path(), "s1", "second notice").unwrap();

    let text = read_feedback(dir.path(), "s1")
        .unwrap()
        .expect("the stage has feedback after two notices");
    let first = text
        .find("first notice")
        .unwrap_or_else(|| panic!("the first notice is gone: {text}"));
    let second = text
        .find("second notice")
        .unwrap_or_else(|| panic!("the second notice is missing: {text}"));
    assert!(
        first < second,
        "the first notice does not come before the second: {text}"
    );
}
