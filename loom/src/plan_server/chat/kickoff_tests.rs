use super::{parse_shape, KickoffTracker};
use crate::plan_server::wire::kickoff::{KickoffShape, KickoffView, ShapeSection, ShapeState};
use crate::plan_session::scope::KickoffSize;
use serde_json::json;

fn block(body: &str) -> String {
    format!("```loom-kickoff\n{body}\n```\n")
}

#[test]
fn last_block_wins() {
    let markdown = format!(
        "{}Some discussion.\n{}",
        block(r#"{"problem":"first"}"#),
        block(r#"{"problem":"last"}"#)
    );

    assert_eq!(parse_shape(&markdown).unwrap().problem, "last");
}

#[test]
fn invalid_last_block_does_not_fall_back() {
    let markdown = format!("{}{}", block("{}"), block("invalid json"));

    assert_eq!(parse_shape(&markdown), None);
}

#[test]
fn tilde_fence_is_accepted() {
    let markdown = "~~~loom-kickoff\n{\"size\":\"plan\"}\n~~~";

    assert_eq!(parse_shape(markdown).unwrap().size, Some(KickoffSize::Plan));
}

#[test]
fn longer_closing_fence_is_accepted() {
    let markdown = "````loom-kickoff\n{}\n``````";

    assert_eq!(parse_shape(markdown), Some(KickoffShape::default()));
}

#[test]
fn unclosed_final_block_does_not_fall_back() {
    let markdown = format!("{}```loom-kickoff\n{{}}", block("{}"));

    assert_eq!(parse_shape(&markdown), None);
}

#[test]
fn mismatched_or_short_closing_fences_do_not_close() {
    assert_eq!(parse_shape("````loom-kickoff\n{}\n```"), None);
    assert_eq!(parse_shape("```loom-kickoff\n{}\n~~~"), None);
    assert_eq!(parse_shape("```loom-kickoff\n{}\n```text"), None);
}

#[test]
fn wrong_info_strings_are_ignored() {
    let ignored = "```json\n{}\n```\n```loom-kickoff-x\n{}\n```\n";

    assert_eq!(parse_shape(ignored), None);
    assert_eq!(
        parse_shape(&format!("{}{ignored}", block("{}"))),
        Some(KickoffShape::default())
    );
}

#[test]
fn kickoff_example_inside_another_fence_is_ignored() {
    let markdown = "````markdown\n```loom-kickoff\n{}\n```\n````";

    assert_eq!(parse_shape(markdown), None);
}

#[test]
fn body_limit_counts_bytes_including_whitespace() {
    let at_limit = format!("{}{{}}", " ".repeat(16 * 1024 - 3));
    let over_limit = format!(" {at_limit}");

    assert_eq!(
        parse_shape(&block(&at_limit)),
        Some(KickoffShape::default())
    );
    assert_eq!(parse_shape(&block(&over_limit)), None);
    assert_eq!(
        parse_shape(&format!("{}{}", block("{}"), block(&over_limit))),
        None
    );
}

#[test]
fn multibyte_body_is_bounded_in_bytes() {
    let body = json!({"problem": "界".repeat(5500)}).to_string();

    assert_eq!(parse_shape(&block(&body)), None);
}

#[test]
fn unknown_fields_are_rejected() {
    assert_eq!(parse_shape(&block(r#"{"unknown":true}"#)), None);
    assert_eq!(
        parse_shape(&block(
            r#"{"sections":[{"title":"Scope","state":"open","unknown":true}]}"#
        )),
        None
    );
}

#[test]
fn strings_are_trimmed_and_empty_entries_are_dropped() {
    let body = json!({
        "size": "feature",
        "name": "  new-work \n",
        "proposal": true,
        "problem": "  A problem \n",
        "decisions": ["  A decision  ", "\n"],
        "questions": ["", " A question "],
        "sections": [
            {"title": "  Scope  ", "state": "wording"},
            {"title": "\t", "state": "open"}
        ]
    });

    assert_eq!(
        parse_shape(&block(&body.to_string())),
        Some(KickoffShape {
            size: Some(KickoffSize::Feature),
            name: Some("new-work".into()),
            proposal: true,
            problem: "A problem".into(),
            decisions: vec!["A decision".into()],
            questions: vec!["A question".into()],
            sections: vec![ShapeSection {
                title: "Scope".into(),
                state: ShapeState::Wording,
            }],
        })
    );
}

#[test]
fn every_string_is_capped_in_characters() {
    let long = format!("  {}  ", "界".repeat(501));
    let body = json!({
        "name": "a".repeat(501),
        "problem": long,
        "decisions": [long],
        "questions": [long],
        "sections": [{"title": long, "state": "settled"}]
    });
    let shape = parse_shape(&block(&body.to_string())).unwrap();

    assert_eq!(shape.name, Some("a".repeat(500)));
    assert_eq!(shape.problem, "界".repeat(500));
    assert_eq!(shape.decisions, vec!["界".repeat(500)]);
    assert_eq!(shape.questions, vec!["界".repeat(500)]);
    assert_eq!(shape.sections[0].title, "界".repeat(500));
}

#[test]
fn every_list_is_capped_at_forty_nonempty_entries() {
    let mut strings = vec![" "; 2];
    strings.extend(vec![" item "; 41]);
    let mut sections = vec![json!({"title": " ", "state": "open"})];
    sections.extend(vec![json!({"title": " Section ", "state": "settled"}); 41]);
    let body = json!({"decisions": strings, "questions": strings, "sections": sections});
    let shape = parse_shape(&block(&body.to_string())).unwrap();

    assert_eq!(shape.decisions, vec!["item"; 40]);
    assert_eq!(shape.questions, vec!["item"; 40]);
    assert_eq!(
        shape.sections,
        vec![
            ShapeSection {
                title: "Section".into(),
                state: ShapeState::Settled
            };
            40
        ]
    );
}

#[test]
fn unusable_names_are_removed() {
    for name in [
        "", " ", "archive", "briefs", "plans", "overview", "../bad", "Upper", "bad_name",
    ] {
        let body = json!({"name": name});

        assert_eq!(parse_shape(&block(&body.to_string())).unwrap().name, None);
    }
}

#[test]
fn tracker_resets_shape_and_target_for_a_new_session() {
    let mut tracker = KickoffTracker::default();
    tracker.observe("first", &block("{}"));
    tracker.confirm("first", "doc/plans/PLAN-first.md");

    let view = tracker.observe("second", &block("{}"));

    assert_eq!(
        view,
        Some(KickoffView {
            session: "second".into(),
            shape: KickoffShape::default(),
            target: None,
        })
    );
}

#[test]
fn new_session_resets_even_when_its_text_has_no_shape() {
    let mut tracker = KickoffTracker::default();
    tracker.observe("first", &block("{}"));
    tracker.confirm("first", "doc/plans/PLAN-first.md");

    assert_eq!(tracker.observe("second", "Discussion"), None);
    assert_eq!(tracker.view(), None);
    assert_eq!(tracker.confirm("first", "doc/plans/PLAN-first.md"), None);
}

#[test]
fn confirmed_target_survives_later_shape_updates() {
    let mut tracker = KickoffTracker::default();
    tracker.observe("session", &block("{}"));
    let target = "doc/product/new-work/index.md";
    let confirmed = tracker.confirm("session", target).unwrap();

    let updated = tracker
        .observe("session", &block(r#"{"problem":"Updated"}"#))
        .unwrap();

    assert_eq!(confirmed.target.as_deref(), Some(target));
    assert_eq!(updated.target.as_deref(), Some(target));
    assert_eq!(updated.shape.problem, "Updated");
    assert_eq!(tracker.view(), Some(updated));
}

#[test]
fn identical_sanitized_shape_does_not_publish_again() {
    let mut tracker = KickoffTracker::default();
    let first = tracker.observe("session", &block(r#"{"problem":"A problem"}"#));
    assert!(first.is_some());
    tracker.confirm("session", "doc/plans/PLAN-new-work.md");
    let before = tracker.view();

    assert_eq!(
        tracker.observe("session", &block(r#"{"problem":" A problem "}"#)),
        None
    );
    assert_eq!(tracker.view(), before);
}

#[test]
fn invalid_update_preserves_the_current_view() {
    let mut tracker = KickoffTracker::default();
    let before = tracker.observe("session", &block("{}"));

    assert_eq!(tracker.observe("session", &block("invalid")), None);
    assert_eq!(tracker.view(), before);
}

#[test]
fn confirmation_requires_a_view_for_the_same_session() {
    let mut tracker = KickoffTracker::default();
    assert_eq!(tracker.confirm("session", "doc/plans/PLAN-new.md"), None);
    let before = tracker.observe("session", &block("{}"));

    assert_eq!(tracker.confirm("other", "doc/plans/PLAN-other.md"), None);
    assert_eq!(tracker.view(), before);
}

#[test]
fn clear_only_removes_the_matching_session() {
    let mut tracker = KickoffTracker::default();
    let before = tracker.observe("session", &block("{}"));

    tracker.clear("other");
    assert_eq!(tracker.view(), before);
    tracker.clear("session");
    assert_eq!(tracker.view(), None);
    assert!(tracker.observe("session", &block("{}")).is_some());
}
