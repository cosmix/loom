use crate::context::fuse::*;
use crate::context::rank::*;
use crate::context::schema::*;

fn candidate(
    id: &str,
    channel: Channel,
    score: f32,
    reasons: Vec<SelectionReason>,
    token_count: usize,
) -> RankedCandidate {
    RankedCandidate {
        id: ChunkId::from(id),
        channel,
        score,
        reasons,
        token_count,
    }
}

#[test]
fn rule_20_rrf_uses_one_based_positions_within_each_list() {
    let fused = fuse(&[vec![
        candidate("first", Channel::Knowledge, 99.0, vec![], 1),
        candidate("second", Channel::Knowledge, 1.0, vec![], 1),
    ]]);
    let second = fused
        .iter()
        .find(|candidate| candidate.id.as_str() == "second")
        .unwrap();
    assert!(
        (second.score - 0.016_129).abs() < 1e-4,
        "got {}",
        second.score
    );
}

#[test]
fn rule_21_rrf_sums_contributions_across_lists() {
    let fused = fuse(&[
        vec![candidate("shared", Channel::Knowledge, 1.0, vec![], 1)],
        vec![
            candidate("other", Channel::Source, 100.0, vec![], 1),
            candidate("shared", Channel::Source, 0.0, vec![], 1),
        ],
    ]);
    assert!(
        (fused[0].score - 0.032_522).abs() < 1e-4,
        "got {}",
        fused[0].score
    );
}

#[test]
fn rule_22_fusion_keeps_best_channel_and_first_metadata() {
    let fused = fuse(&[
        vec![
            candidate("other", Channel::Knowledge, 2.0, vec![], 1),
            candidate(
                "shared",
                Channel::Knowledge,
                1.0,
                vec![SelectionReason::Lexical],
                5,
            ),
        ],
        vec![candidate(
            "shared",
            Channel::Source,
            1.0,
            vec![SelectionReason::ExplicitId, SelectionReason::Lexical],
            9,
        )],
    ]);
    let shared = fused
        .iter()
        .find(|candidate| candidate.id.as_str() == "shared")
        .unwrap();
    assert!(
        (shared.score - 0.032_522).abs() < 1e-4,
        "got {}",
        shared.score
    );
    assert_eq!(shared.channel, Channel::Source);
    assert_eq!(shared.token_count, 5);
    assert_eq!(
        shared.reasons,
        vec![SelectionReason::Lexical, SelectionReason::ExplicitId]
    );
}

#[test]
fn rule_23_output_uses_rrf_scores_and_sorts_ties_by_id() {
    let fused = fuse(&[
        vec![candidate("b", Channel::Knowledge, 999.0, vec![], 1)],
        vec![candidate("a", Channel::Source, -1.0, vec![], 1)],
    ]);
    assert_eq!(
        fused
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert!(
        (fused[0].score - 0.016_393).abs() < 1e-4,
        "got {}",
        fused[0].score
    );
}

#[test]
fn rule_24_empty_input_and_empty_lists_produce_no_candidates() {
    assert!(fuse(&[]).is_empty());
    assert!(fuse(&[Vec::new(), Vec::new()]).is_empty());
}
