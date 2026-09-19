//! `mode: prompt` judges `precision_at_5` and `relevant_token_fraction` over
//! the pack the hook would actually deliver, not the raw retrieved pack -
//! see `eval::metrics::score_case` and `judged_pack`. These two cases pin
//! that behavior at both ends: an exact-rung target that clears the emit
//! floor, and an all-lexical pack the hook abstains on entirely.

use super::cases::{EvalCase, EvalMode};
use super::metrics::score_case;
use crate::context::config::RetrievalConfig;
use crate::context::schema::{
    Channel, ChunkId, Confidence, ContextItem, ContextPack, Freshness, ItemKind, LifecycleState,
    OmissionSummary, SelectionReason, SourcePointer,
};
use std::path::PathBuf;

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn case(name: &str, expect: &[&str]) -> EvalCase {
    EvalCase {
        name: name.to_string(),
        query: "q".to_string(),
        mode: EvalMode::Prompt,
        budget_tokens: None,
        stage_fields: Vec::new(),
        require_ids: Vec::new(),
        expect: ids(expect),
        relevant: Vec::new(),
        forbid: Vec::new(),
        abstain: false,
        max_rendered_tokens: None,
    }
}

fn lexical_item(id: &str) -> ContextItem {
    ContextItem {
        id: ChunkId::from(id),
        kind: ItemKind::KnowledgeChunk,
        pointer: SourcePointer {
            path: PathBuf::from("doc/loom/knowledge/test.md"),
            anchor: "test".to_string(),
            line_start: None,
            line_end: None,
        },
        summary: id.to_string(),
        source: Channel::Knowledge,
        token_count: 10,
        score: 0.5,
        reasons: vec![SelectionReason::Lexical],
        confidence: Confidence::Low,
        state: LifecycleState::Active,
        content_hash: format!("sha256:{id}"),
        excerpt: Some(format!("## {id}\n\nfixture")),
        truncated: false,
        matched_term_count: 0,
    }
}

fn pack(items: Vec<ContextItem>) -> ContextPack {
    let estimated_tokens = items.iter().map(|item| item.token_count).sum();
    ContextPack {
        query: "query".to_string(),
        scope: vec![Channel::Knowledge],
        budget_tokens: 1600,
        estimated_tokens,
        structural_freshness: Freshness::default(),
        semantic_freshness: Freshness::default(),
        items,
        unmet_required: Vec::new(),
        omitted: OmissionSummary::default(),
        dropped_terms: Vec::new(),
        degraded: None,
    }
}

#[test]
fn precision_is_judged_over_the_hook_delivered_pack_when_it_clears_the_floor() {
    let mut target = lexical_item("target");
    target.reasons = vec![SelectionReason::ExactSymbol];
    target.score = 5.0;
    let noise: Vec<ContextItem> = ["a", "b", "c", "d"].into_iter().map(lexical_item).collect();
    let mut items = vec![target];
    items.extend(noise);

    let eval_case = case("prompt-clears-floor", &["target"]);
    let result = score_case(&eval_case, &pack(items), &RetrievalConfig::default());

    assert!(result.would_emit);
    assert_eq!(result.precision_at_5, Some(1.0));
}

#[test]
fn precision_is_zero_over_a_pack_the_hook_would_abstain_on() {
    let items: Vec<ContextItem> = ["target", "a", "b", "c"]
        .into_iter()
        .map(lexical_item)
        .collect();

    let eval_case = case("prompt-abstains", &["target"]);
    let result = score_case(&eval_case, &pack(items), &RetrievalConfig::default());

    assert!(!result.would_emit);
    assert!(result.hit_at_5, "raw-pack metrics stay on the raw pack");
    assert_eq!(result.precision_at_5, Some(0.0));
    assert_eq!(result.relevant_token_fraction, Some(0.0));
}
