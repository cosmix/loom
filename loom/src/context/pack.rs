//! Budget-constrained construction of context packs.

use crate::context::rank::RankedCandidate;
use crate::context::schema::{
    Channel, Confidence, ContextItem, ContextPack, Coverage, Freshness, ItemKind, KnowledgeChunk,
    OmissionSummary, SourcePointer,
};
use std::collections::BTreeMap;

/// Everything the packer needs besides the ranked list.
#[derive(Debug, Clone)]
pub struct PackRequest {
    /// Original query text.
    pub query: String,
    /// Retrieval channels included in this request.
    pub scope: Vec<Channel>,
    /// Maximum estimated token cost of returned items.
    pub budget_tokens: usize,
    /// Freshness of the structural retrieval layer.
    pub structural_freshness: Freshness,
    /// Freshness of the semantic retrieval layer.
    pub semantic_freshness: Freshness,
}

fn summary(chunk: &KnowledgeChunk) -> String {
    if !chunk.heading.is_empty() {
        return chunk.heading.clone();
    }
    let line = chunk
        .body
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .unwrap_or("");
    line.chars().take(120).collect()
}

/// Build one `ContextItem` from a ranked candidate and its backing chunk.
fn build_item(candidate: &RankedCandidate, chunk: &KnowledgeChunk) -> ContextItem {
    ContextItem {
        id: candidate.id.clone(),
        kind: ItemKind::KnowledgeChunk,
        pointer: SourcePointer {
            path: chunk.file.clone(),
            anchor: chunk.anchor.clone(),
            line_start: None,
            line_end: None,
        },
        summary: summary(chunk),
        source: candidate.channel,
        token_count: candidate.token_count,
        score: candidate.score,
        reasons: candidate.reasons.clone(),
        confidence: Confidence::from_reasons(&candidate.reasons),
        state: chunk.state,
    }
}

/// Summarize coverage and omissions for a completed pack: how many of the
/// ranked candidates (and their tokens) made it into `items`.
fn build_omission_summary(
    ranked: &[RankedCandidate],
    items: &[ContextItem],
    omitted: usize,
) -> OmissionSummary {
    let weakest_included_score = items
        .iter()
        .map(|item| item.score)
        .reduce(f32::min)
        .unwrap_or(0.0);
    let candidate_tokens = ranked.iter().map(|candidate| candidate.token_count).sum();
    let included_tokens = items.iter().map(|item| item.token_count).sum();
    OmissionSummary {
        omitted,
        weakest_included_score,
        coverage: Coverage {
            candidates: ranked.len(),
            included: ranked.len() - omitted,
            candidate_tokens,
            included_tokens,
        },
    }
}

/// Walk the fused list in order, taking whole chunks while they fit the budget.
///
/// Every ranked candidate not included is counted as an omission.
pub fn pack(
    request: &PackRequest,
    ranked: &[RankedCandidate],
    chunks: &[KnowledgeChunk],
) -> ContextPack {
    let lookup: BTreeMap<&str, &KnowledgeChunk> = chunks
        .iter()
        .map(|chunk| (chunk.id.as_str(), chunk))
        .collect();
    let mut items = Vec::new();
    let mut estimated_tokens = 0;
    let mut omitted = 0;

    for candidate in ranked {
        let Some(chunk) = lookup.get(candidate.id.as_str()) else {
            omitted += 1;
            continue;
        };
        let remaining = request.budget_tokens - estimated_tokens;
        if request.budget_tokens == 0 || candidate.token_count > remaining {
            omitted += 1;
            continue;
        }
        estimated_tokens += candidate.token_count;
        items.push(build_item(candidate, chunk));
    }

    let omitted_summary = build_omission_summary(ranked, &items, omitted);
    ContextPack {
        query: request.query.clone(),
        scope: request.scope.clone(),
        budget_tokens: request.budget_tokens,
        estimated_tokens,
        structural_freshness: request.structural_freshness.clone(),
        semantic_freshness: request.semantic_freshness.clone(),
        items,
        omitted: omitted_summary,
    }
}
