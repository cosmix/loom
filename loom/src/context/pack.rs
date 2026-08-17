//! Budget-constrained construction of context packs.

use crate::context::graph_store::ResolvedGraph;
use crate::context::rank::RankedCandidate;
use crate::context::schema::{
    estimate_tokens, Channel, ChunkId, Confidence, ContextItem, ContextPack, Coverage, Freshness,
    ItemKind, KnowledgeChunk, LifecycleState, OmissionSummary, SourceNode, SourcePointer,
    BYTES_PER_TOKEN_ESTIMATE, EXCERPT_MAX_TOKENS, EXCERPT_TRUNCATION_MARKER,
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

/// Copy `body` verbatim, cut to at most [`EXCERPT_MAX_TOKENS`].
///
/// This bound is independent of the retrieval budget — the item's `token_count`
/// still describes the whole chunk — and exists only so one very long section
/// cannot dominate a rendered brief.
///
/// The cut walks back twice: first to a character boundary, because slicing a
/// `&str` mid-scalar panics, and then to the preceding newline, so a quoted
/// excerpt never ends mid-line and misrepresents the source. A body with no
/// newline inside the limit keeps the character-boundary cut.
fn bounded_excerpt(body: &str) -> String {
    if estimate_tokens(body) <= EXCERPT_MAX_TOKENS {
        return body.to_string();
    }

    let mut end = (EXCERPT_MAX_TOKENS * BYTES_PER_TOKEN_ESTIMATE).min(body.len());
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }

    let head = &body[..end];
    let head = match head.rfind('\n') {
        Some(newline) => &head[..newline],
        None => head,
    };
    format!("{head}\n{EXCERPT_TRUNCATION_MARKER}")
}

/// Build one `ContextItem` from a ranked candidate and its backing chunk.
fn build_chunk_item(candidate: &RankedCandidate, chunk: &KnowledgeChunk) -> ContextItem {
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
        content_hash: chunk.content_hash.clone(),
        excerpt: Some(bounded_excerpt(&chunk.body)),
    }
}

/// Build one `ContextItem` from a ranked candidate and its backing source node.
///
/// `state` is always [`LifecycleState::Active`]: a source node has no curation
/// lifecycle (draft/deprecated/superseded) the way a hand-written knowledge
/// chunk does — it is simply whatever the code on disk currently says. Do not
/// try to derive one from `node.coverage`; that describes extraction quality,
/// not trustworthiness.
///
/// `content_hash` is `node.body_hash`, already `sha256:<hex>` over this node's
/// exact source bytes — strictly more precise than the owning file's hash for
/// the delivery-record suppression `ContextItem::content_hash` feeds, since it
/// changes only when this node's own bytes do.
///
/// `excerpt` goes through [`bounded_excerpt`], never
/// `crate::utils::truncate_for_display`: `bounded_excerpt` is what enforces the
/// documented contract on `ContextItem::excerpt` (bounded by
/// [`EXCERPT_MAX_TOKENS`], truncated text ends with
/// [`EXCERPT_TRUNCATION_MARKER`] on its own line). A signature is short, so
/// this is nearly always a no-op, but using the other helper would silently
/// make source items the only ones in the corpus violating that contract.
///
/// No file reads here or anywhere else in the packer: retrieval is a pure
/// function of bytes already loaded into the `SourceNode`, not of the working
/// tree at query time.
fn build_source_item(candidate: &RankedCandidate, node: &SourceNode) -> ContextItem {
    ContextItem {
        id: ChunkId::from(node.id.as_str()),
        kind: ItemKind::SourceNode,
        pointer: SourcePointer {
            path: node.path.clone(),
            anchor: String::new(),
            line_start: Some(node.span.line_start),
            line_end: Some(node.span.line_end),
        },
        summary: format!(
            "{} {} - {}:{}-{}",
            node.kind.as_str(),
            node.scope.join("::"),
            node.path.display(),
            node.span.line_start,
            node.span.line_end
        ),
        source: Channel::Source,
        token_count: candidate.token_count,
        score: candidate.score,
        reasons: candidate.reasons.clone(),
        confidence: Confidence::from_reasons(&candidate.reasons),
        state: LifecycleState::Active,
        content_hash: node.body_hash.clone(),
        excerpt: Some(bounded_excerpt(&node.signature)),
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

/// Build the `ContextItem` for one candidate, dispatching on its channel.
///
/// Dispatch is on `candidate.channel`, never on which lookup map hits first —
/// a channel is authoritative about what its own ids mean. Knowledge chunk
/// ids have the form `<path>#<heading>#<occurrence>`; source node ids have
/// the form `<path>#<kind>:<scope>`. Those are disjoint id spaces today, so
/// trying both maps and taking whichever hits would not currently misfire.
/// But `fuse` keys its accumulator by `ChunkId` across channels, so if a
/// future change ever let the two channels produce a colliding id, a
/// both-maps dispatch would silently consult whichever map happened to hit
/// first instead of the one `candidate.channel` actually names, and hide the
/// bug behind a plausible-looking item. Keying off `channel` makes that class
/// of mistake unreachable rather than merely untested.
fn build_item(
    candidate: &RankedCandidate,
    chunks: &BTreeMap<&str, &KnowledgeChunk>,
    nodes: &BTreeMap<&str, &SourceNode>,
) -> Option<ContextItem> {
    match candidate.channel {
        Channel::Knowledge => chunks
            .get(candidate.id.as_str())
            .map(|chunk| build_chunk_item(candidate, chunk)),
        Channel::Source => nodes
            .get(candidate.id.as_str())
            .map(|node| build_source_item(candidate, node)),
    }
}

/// Walk the fused list in order, taking whole items while they fit the budget.
///
/// Every ranked candidate not included is counted as an omission, whether
/// because it fell outside `chunks`/`graph` or because it did not fit the
/// remaining budget.
pub fn pack(
    request: &PackRequest,
    ranked: &[RankedCandidate],
    chunks: &[KnowledgeChunk],
    graph: Option<&ResolvedGraph>,
) -> ContextPack {
    let chunk_lookup: BTreeMap<&str, &KnowledgeChunk> = chunks
        .iter()
        .map(|chunk| (chunk.id.as_str(), chunk))
        .collect();
    let node_lookup: BTreeMap<&str, &SourceNode> = graph
        .into_iter()
        .flat_map(|graph| graph.nodes())
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut items = Vec::new();
    let mut estimated_tokens = 0;
    let mut omitted = 0;

    for candidate in ranked {
        let Some(item) = build_item(candidate, &chunk_lookup, &node_lookup) else {
            omitted += 1;
            continue;
        };
        let remaining = request.budget_tokens - estimated_tokens;
        if request.budget_tokens == 0 || candidate.token_count > remaining {
            omitted += 1;
            continue;
        }
        estimated_tokens += candidate.token_count;
        items.push(item);
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
