use crate::context::schema::*;

#[test]
fn estimate_tokens_is_four_bytes_per_token() {
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("abc"), 0);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcdefgh"), 2);
}

#[test]
fn confidence_ranks_identity_above_structure_above_lexical() {
    assert_eq!(
        Confidence::from_reasons(&[SelectionReason::Lexical, SelectionReason::ExactPath]),
        Confidence::High
    );
    assert_eq!(
        Confidence::from_reasons(&[SelectionReason::Lexical, SelectionReason::LinkedFrom]),
        Confidence::Medium
    );
    assert_eq!(
        Confidence::from_reasons(&[SelectionReason::Lexical]),
        Confidence::Low
    );
    assert_eq!(Confidence::from_reasons(&[]), Confidence::Low);
}

#[test]
fn coverage_ratio_treats_empty_candidate_set_as_complete() {
    assert_eq!(Coverage::default().token_ratio(), 1.0);
    let partial = Coverage {
        candidates: 4,
        included: 1,
        candidate_tokens: 400,
        included_tokens: 100,
    };
    assert!((partial.token_ratio() - 0.25).abs() < f32::EPSILON);
}

#[test]
fn never_built_freshness_reports_stale() {
    let freshness = Freshness::never_built("catalog has never been built");
    assert!(freshness.stale);
    assert!(freshness.revision.is_empty());
    assert!(freshness.detail.is_some());
}

#[test]
fn lifecycle_state_defaults_to_active_and_round_trips() {
    assert_eq!(LifecycleState::default(), LifecycleState::Active);
    let json = serde_json::to_string(&LifecycleState::Superseded).unwrap();
    assert_eq!(json, "\"superseded\"");
    let back: LifecycleState = serde_json::from_str(&json).unwrap();
    assert_eq!(back, LifecycleState::Superseded);
}

#[test]
fn chunk_id_is_transparent_in_json() {
    let id = ChunkId::new("architecture/hook-system.md#hooks#0");
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"architecture/hook-system.md#hooks#0\"");
    assert_eq!(id.as_str(), "architecture/hook-system.md#hooks#0");
}
