use super::super::{append_rejection, read_feedback, write_notice};
use super::SECTION_SEPARATOR;

const DROPPED: &str = "Earlier notices of this round were dropped: the file passed 64 KiB.\n";

#[test]
fn appending_the_same_rejection_twice_leaves_it_once() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    append_rejection(work, "s", "the check stands", &[]).unwrap();
    let once = read_feedback(work, "s").unwrap().unwrap();
    append_rejection(work, "s", "the check stands", &[]).unwrap();
    assert_eq!(read_feedback(work, "s").unwrap().unwrap(), once);
}

#[test]
fn a_repeated_notice_after_another_leaves_both_once() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_notice(work, "s", "first notice").unwrap();
    write_notice(work, "s", "second notice").unwrap();
    write_notice(work, "s", "first notice").unwrap();
    write_notice(work, "s", "second notice").unwrap();
    assert_eq!(
        read_feedback(work, "s").unwrap().unwrap(),
        format!("first notice{SECTION_SEPARATOR}second notice")
    );
}

#[test]
fn a_notice_after_the_drop_notice_is_not_appended_again() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_notice(work, "s", &"x".repeat(65_536)).unwrap();
    write_notice(work, "s", "new notice").unwrap();
    write_notice(work, "s", "new notice").unwrap();
    assert_eq!(
        read_feedback(work, "s").unwrap().unwrap(),
        format!("{DROPPED}new notice")
    );
}

#[test]
fn two_different_notices_both_remain_in_order() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_notice(work, "s", "alpha").unwrap();
    write_notice(work, "s", "beta").unwrap();
    assert_eq!(
        read_feedback(work, "s").unwrap().unwrap(),
        format!("alpha{SECTION_SEPARATOR}beta")
    );
}

#[test]
fn distinct_notices_past_the_bound_still_drop_the_earlier_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_notice(work, "s", &"a".repeat(40_000)).unwrap();
    write_notice(work, "s", &"b".repeat(40_000)).unwrap();
    let content = read_feedback(work, "s").unwrap().unwrap();
    assert_eq!(content, format!("{DROPPED}{}", "b".repeat(40_000)));
}

#[test]
fn a_notice_over_the_bound_in_an_empty_file_carries_no_drop_notice() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    let big = "z".repeat(70_000);
    write_notice(work, "s", &big).unwrap();
    assert_eq!(read_feedback(work, "s").unwrap().unwrap(), big);
}
