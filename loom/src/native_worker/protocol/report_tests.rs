use super::{AgentMessage, ReportAccumulator};

fn record(acc: &mut ReportAccumulator, turn: &str, id: &str, text: &str, phase: Option<&str>) {
    acc.record(
        turn,
        &AgentMessage {
            id: id.into(),
            text: text.into(),
            phase: phase.map(str::to_owned),
        },
    );
}

#[test]
fn final_answer_items_are_joined_with_blank_line_and_complete() {
    let mut acc = ReportAccumulator::new();
    record(&mut acc, "t", "a", "first", Some("final_answer"));
    record(&mut acc, "t", "b", "second", Some("final_answer"));
    acc.mark_completed("t");
    assert_eq!(acc.report(), Some(("first\n\nsecond".into(), true)));
}

#[test]
fn commentary_never_enters_the_report() {
    let mut acc = ReportAccumulator::new();
    record(&mut acc, "t", "a", "hidden", Some("commentary"));
    acc.mark_completed("t");
    assert_eq!(acc.report(), None);
    record(&mut acc, "t", "b", "shown", Some("final_answer"));
    assert_eq!(acc.report(), Some(("shown".into(), true)));
}

#[test]
fn partial_answer_never_enters_the_report() {
    let mut acc = ReportAccumulator::new();
    record(&mut acc, "t", "a", "hidden", Some("partial_answer"));
    record(&mut acc, "t", "b", "unknown", Some("other"));
    acc.mark_completed("t");
    assert_eq!(acc.report(), None);
}

#[test]
fn null_phase_fallback_uses_last_item_and_is_incomplete() {
    let mut acc = ReportAccumulator::new();
    record(&mut acc, "t", "a", "first", None);
    record(&mut acc, "t", "b", "last", None);
    record(&mut acc, "t", "c", "hidden", Some("commentary"));
    acc.mark_completed("t");
    assert_eq!(acc.report(), Some(("last".into(), false)));
}

#[test]
fn final_answer_beats_null_phase_items() {
    let mut acc = ReportAccumulator::new();
    record(&mut acc, "t", "a", "final", Some("final_answer"));
    record(&mut acc, "t", "b", "fallback", None);
    acc.mark_completed("t");
    assert_eq!(acc.report(), Some(("final".into(), true)));
}

#[test]
fn repeated_item_id_replaces_text_in_place() {
    let mut acc = ReportAccumulator::new();
    record(&mut acc, "t", "a", "A", Some("commentary"));
    record(&mut acc, "t", "b", "B", Some("final_answer"));
    record(&mut acc, "t", "c", "C", Some("final_answer"));
    record(&mut acc, "t", "b", "B2", Some("final_answer"));
    acc.mark_completed("t");
    assert_eq!(acc.report(), Some(("B2\n\nC".into(), true)));
    record(&mut acc, "t", "c", "hidden", Some("commentary"));
    assert_eq!(acc.report(), Some(("B2".into(), true)));
}

#[test]
fn only_the_marked_turn_counts() {
    let mut acc = ReportAccumulator::new();
    record(&mut acc, "first", "same", "chosen", None);
    record(&mut acc, "second", "same", "ignored", Some("final_answer"));
    acc.mark_completed("first");
    assert_eq!(acc.report(), Some(("chosen".into(), false)));
}

#[test]
fn no_marked_turn_gives_none() {
    let mut acc = ReportAccumulator::new();
    record(&mut acc, "t", "a", "answer", Some("final_answer"));
    assert_eq!(acc.report(), None);
}

#[test]
fn marked_turn_without_agent_messages_gives_none() {
    let mut acc = ReportAccumulator::new();
    acc.mark_completed("missing");
    assert_eq!(acc.report(), None);
}

#[test]
fn a_later_marked_turn_replaces_an_earlier_one() {
    let mut acc = ReportAccumulator::new();
    record(&mut acc, "first", "a", "first", Some("final_answer"));
    record(&mut acc, "second", "b", "second", None);
    acc.mark_completed("first");
    assert_eq!(acc.report(), Some(("first".into(), true)));
    acc.mark_completed("second");
    assert_eq!(acc.report(), Some(("second".into(), false)));
    acc.mark_completed("missing");
    assert_eq!(acc.report(), None);
}
