use std::path::Path;

use super::*;

const A: &str = "0123456789ab";
const B: &str = "ba9876543210";
const C: &str = "abcdefabcdef";

fn line(body: &str, stub: Option<&str>, upstream: Option<&str>) -> SettledLine {
    SettledLine {
        body: body.to_owned(),
        stub: stub.map(str::to_owned),
        upstream: upstream.map(str::to_owned),
    }
}

#[test]
fn crlf_text_keeps_its_endings() {
    let text = "# Alpha\r\n\r\nImplements: section_01\r\n\r\nBody.\r\n";
    let settled = line(A, Some(B), None);
    let marked = mark(text, &settled);
    assert_eq!(
        marked,
        format!(
            "# Alpha\r\n\r\nImplements: section_01\r\n\r\nSettled: {A} stub {B}\r\n\r\nBody.\r\n"
        )
    );
    assert_eq!(settled_line(&marked), Some(settled));
    assert_eq!(clear(&marked), text);
    assert_eq!(body_hash(&marked, &[]), body_hash(text, &[]));
}

#[test]
fn marking_again_is_a_no_op_and_a_new_line_replaces_the_old_in_place() {
    let text = "# Alpha\n\nBody.\n";
    let first = line(A, None, None);
    let marked = mark(text, &first);
    assert_eq!(marked, format!("# Alpha\n\nSettled: {A}\n\nBody.\n"));
    assert_eq!(mark(&marked, &first), marked);

    let second = line(B, Some(C), Some(A));
    let remarked = mark(&marked, &second);
    assert_eq!(
        remarked,
        format!("# Alpha\n\nSettled: {B} stub {C} upstream {A}\n\nBody.\n")
    );
    assert_eq!(clear(&remarked), text);
}

#[test]
fn a_malformed_line_is_replaced_in_place() {
    let text = "# Alpha\n\nIntro.\n\nSettled: nonsense\n\nBody.\n";
    let marked = mark(text, &line(A, None, None));
    assert_eq!(
        marked,
        format!("# Alpha\n\nIntro.\n\nSettled: {A}\n\nBody.\n")
    );
}

#[test]
fn clear_restores_every_text_mark_wrote_into() {
    let texts = [
        "# Alpha\n\nImplements: section_01\n\nBody.\n",
        "# Alpha\n\nBody.\n",
        "# Alpha\n",
        "# Alpha",
        "# Alpha\n\nImplements: section_01",
        "No title.\n",
        "",
        "## Sub\n\nText.\n\n# Title\n\nMore.\n",
        "# Alpha\r\n\r\nBody.",
    ];
    let settled = line(A, Some(B), Some(C));
    for text in texts {
        let marked = mark(text, &settled);
        assert_ne!(marked, text, "mark wrote nothing into {text:?}");
        assert_eq!(settled_line(&marked), Some(settled.clone()), "{marked:?}");
        assert_eq!(clear(&marked), text, "marked: {marked:?}");
        assert_eq!(body_hash(&marked, &[]), body_hash(text, &[]), "{text:?}");
    }
}

#[test]
fn the_mark_goes_after_the_title_or_the_implements_line_under_it() {
    let settled = line(A, None, None);
    let cases = [
        ("# T\n\nText.\n", format!("# T\n\nSettled: {A}\n\nText.\n")),
        (
            "# T\n\nImplements: none\n\nText.\n",
            format!("# T\n\nImplements: none\n\nSettled: {A}\n\nText.\n"),
        ),
        (
            "# T\n\nText.\n\nImplements: none\n",
            format!("# T\n\nSettled: {A}\n\nText.\n\nImplements: none\n"),
        ),
        ("Text.\n", format!("Settled: {A}\n\nText.\n")),
        ("# T", format!("# T\n\nSettled: {A}")),
    ];
    for (text, expected) in cases {
        assert_eq!(mark(text, &settled), expected, "{text:?}");
    }
}

#[test]
fn render_and_parse_round_trip_every_form() {
    let lines = [
        line(A, None, None),
        line(A, Some(B), None),
        line(A, None, Some(C)),
        line(A, Some(B), Some(C)),
    ];
    for settled in lines {
        let text = format!("# T\n\n{}\n", render(&settled));
        assert_eq!(settled_line(&text), Some(settled));
    }
}

#[test]
fn malformed_lines_are_marks_that_do_not_parse() {
    let bad = [
        "Settled:".to_owned(),
        "Settled: 0123".to_owned(),
        "Settled: 0123456789AB".to_owned(),
        format!("Settled:{A}"),
        format!("Settled: {A} upstream {B} stub {C}"),
        format!("Settled: {A} stub"),
        format!("Settled: {A} stub {B} extra"),
        format!("Settled: {A} later {B}"),
    ];
    for value in bad {
        let text = format!("# T\n\n{value}\n");
        let marks = settled_marks(&text);
        assert_eq!(marks.len(), 1, "{value}");
        assert_eq!(marks[0].line, 2);
        assert_eq!(marks[0].parsed, None, "{value}");
        assert_eq!(settled_line(&text), None, "{value}");
    }
}

#[test]
fn only_column_zero_lines_outside_fences_and_metadata_are_marks() {
    let fence = "`".repeat(3);
    let text = format!(
        "# T\n\nStatus: agreed vision.\n\n Settled: {A}\n\n{fence}\nSettled: {A}\n{fence}\n\n\
         <!-- loom METADATA -->\nSettled: {A}\n"
    );
    assert!(settled_marks(&text).is_empty());
    assert_eq!(settled_line(&text), None);
}

#[test]
fn the_first_valid_line_wins_and_every_line_is_a_mark() {
    let text = format!("# T\n\nSettled: bad\n\nSettled: {A}\n\nSettled: {B}\n");
    let marks = settled_marks(&text);
    assert_eq!(marks.iter().map(|m| m.line).collect::<Vec<_>>(), [2, 4, 6]);
    assert_eq!(settled_line(&text), Some(line(A, None, None)));
    assert_eq!(clear(&text), "# T\n");
}

#[test]
fn states_follow_body_then_stub_then_upstream() {
    let current = line(A, Some(B), Some(C));
    let cases = [
        (None, SettleState::Draft),
        (Some(line(B, Some(C), None)), SettleState::Revised),
        (Some(line(A, Some(C), Some(A))), SettleState::StubChanged),
        (Some(line(A, None, Some(C))), SettleState::StubChanged),
        (Some(line(A, Some(B), None)), SettleState::UpstreamChanged),
        (
            Some(line(A, Some(B), Some(A))),
            SettleState::UpstreamChanged,
        ),
        (Some(current.clone()), SettleState::Settled),
    ];
    for (recorded, expected) in cases {
        assert_eq!(state(recorded.as_ref(), &current), expected, "{recorded:?}");
    }
    let stub_gone = line(A, None, Some(C));
    assert_eq!(state(Some(&current), &stub_gone), SettleState::StubChanged);
}

#[test]
fn the_body_hash_covers_the_bundle_in_path_order() {
    let text = "# Plan\n\nBody.\n";
    let (one, two) = (
        Path::new("doc/plans/briefs/p/a.md"),
        Path::new("doc/plans/briefs/p/b.md"),
    );
    let plain = body_hash(text, &[]);
    let bundled = body_hash(text, &[(one, "A\n"), (two, "B\n")]);
    assert_ne!(plain, bundled);
    assert_eq!(bundled, body_hash(text, &[(two, "B\n"), (one, "A\n")]));
    assert_ne!(
        bundled,
        body_hash(text, &[(one, "A\n"), (two, "B, changed\n")])
    );
    assert_ne!(bundled, body_hash(text, &[(two, "A\n"), (one, "B\n")]));
    assert_eq!(bundled, body_hash(text, &[(one, "A  \r\n"), (two, "B\n")]));
    assert_eq!(bundled.len(), 12);
}

#[test]
fn short_hash_takes_the_first_twelve_hex_digits_of_a_normalized_hash() {
    assert_eq!(short_hash("sha256:abcdef0123456789"), "abcdef012345");
    assert_eq!(short_hash(&normalized_hash("x")).len(), 12);
}
