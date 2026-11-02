//! Turn one retrieved [`ContextPack`] into the hook's single stdout line, or
//! into nothing.
//!
//! Items must earn admission, survive per-epoch dedupe, and fit the byte ceiling.

use crate::context::config::RetrievalConfig;
use crate::context::schema::{ContextItem, ContextPack, SelectionReason};
use std::collections::BTreeSet;

/// What the shared renderer reports on its `Selected from:` line. A prompt hook
/// retrieves against the one question that was just typed, not against the
/// stage's whole query surface.
const QUERY_INPUTS: &str = "this prompt";

pub(super) enum ComposeOutcome {
    Emitted(String, Box<ContextPack>),
    Abstained(&'static str),
}

/// Compose the hook's single stdout line, together with the pack that line
/// actually delivers.
///
/// The brief itself is rendered by
/// [`crate::orchestrator::signals::format_knowledge_brief`] — the same renderer
/// the signal path uses. Its fencing rule is what keeps an untrusted excerpt
/// from escaping its own quoted block, and a containment rule with two copies
/// is a containment rule that drifts, so this path never forks it. Only the
/// emit floor, the per-epoch dedupe, and the serialized ceiling are the
/// hook's own.
///
/// An over-budget pack DEGRADES rather than being discarded: its weakest units
/// are dropped until the object fits. Discarding instead would recompose and
/// throw away the same pack on every prompt for the rest of the epoch, and would
/// file no record — so the strongest matches, which do fit, would never arrive.
///
/// `None` means there is no honest payload to send. [`compose_with_reason`]
/// retains the reason for the telemetry-producing caller.
#[cfg(test)]
pub(super) fn compose(
    pull_stage: Option<&str>,
    pack: &ContextPack,
    delivered: &BTreeSet<(String, String)>,
    config: &RetrievalConfig,
) -> Option<(String, ContextPack)> {
    match compose_with_reason(pull_stage, pack, delivered, config) {
        ComposeOutcome::Emitted(payload, handed_over) => Some((payload, *handed_over)),
        ComposeOutcome::Abstained(_) => None,
    }
}

pub(super) fn compose_with_reason(
    pull_stage: Option<&str>,
    pack: &ContextPack,
    delivered: &BTreeSet<(String, String)>,
    config: &RetrievalConfig,
) -> ComposeOutcome {
    let Some(admitted) = admitted(pack, config) else {
        return ComposeOutcome::Abstained("floor");
    };
    let Some(mut handed_over) = undelivered(&admitted, delivered) else {
        return ComposeOutcome::Abstained("all-delivered");
    };
    // `undelivered` already dropped every repeat; what survives here is fresh,
    // just too weak on its own — that is the emit floor's call, not a repeat,
    // so it gets the same reason a first-time pull that never cleared the
    // floor would get.
    if !clears_emit_floor(&handed_over, config) {
        return ComposeOutcome::Abstained("floor");
    }

    loop {
        let Some(line) = render_payload(pull_stage, &handed_over) else {
            return ComposeOutcome::Abstained("over-ceiling");
        };
        if line.len() <= config.max_payload_bytes {
            return ComposeOutcome::Emitted(line, Box::new(handed_over));
        }
        let Some(narrowed) = without_weakest(&handed_over) else {
            return ComposeOutcome::Abstained("over-ceiling");
        };
        handed_over = narrowed;
        if !clears_emit_floor(&handed_over, config) {
            return ComposeOutcome::Abstained("over-ceiling");
        }
    }
}

/// The pack a fresh session would actually receive for `pack` — after the
/// emit floor, the (empty, for a fresh session) per-epoch dedupe, and the
/// byte ceiling — or `None` when the hook would stay silent for it.
///
/// Used both by the real hook path (indirectly, via [`compose_with_reason`])
/// and by `loom knowledge eval`, which judges `mode: prompt` cases against
/// what this returns rather than against the raw retrieved pack: a case
/// scored on units the hook would never hand a session measures retrieval,
/// not what a prompt actually delivers.
pub(crate) fn delivered(pack: &ContextPack, config: &RetrievalConfig) -> Option<ContextPack> {
    match compose_with_reason(None, pack, &BTreeSet::new(), config) {
        ComposeOutcome::Emitted(_, handed_over) => Some(*handed_over),
        ComposeOutcome::Abstained(_) => None,
    }
}

/// True when `item` earns admission on its own, or is a graph neighbour of
/// an exact-rung item that remains in this retrieved pack.
fn admits(item: &ContextItem, pack: &ContextPack, config: &RetrievalConfig) -> bool {
    clears_item_floor(item, config)
        || (item.reasons.contains(&SelectionReason::GraphNeighbor)
            && pack
                .items
                .iter()
                .any(|item| item.reasons.iter().any(is_exact_rung)))
}

/// Narrow `pack` to individually admitted items and account for every drop.
fn admitted(pack: &ContextPack, config: &RetrievalConfig) -> Option<ContextPack> {
    let kept: Vec<ContextItem> = pack
        .items
        .iter()
        .filter(|item| admits(item, pack, config))
        .cloned()
        .collect();
    carrying_after_drop(pack, kept)
}

/// Emit when any item has an exact or symbol-question reason, or enough
/// distinct matched terms. This floor applies only to unsolicited prompt hooks;
/// explicit knowledge queries and stage/worker briefs use their own policies.
fn clears_emit_floor(pack: &ContextPack, config: &RetrievalConfig) -> bool {
    pack.items
        .iter()
        .any(|item| clears_item_floor(item, config))
}

/// A symbol question (`what does X do`) names its one term outright, so the
/// item it admitted passes even though a single term is below
/// `config.min_knowledge_terms`.
fn clears_item_floor(item: &ContextItem, config: &RetrievalConfig) -> bool {
    item.reasons
        .iter()
        .any(|reason| is_exact_rung(reason) || *reason == SelectionReason::SymbolQuestion)
        || item.matched_term_count >= config.min_knowledge_terms
}

/// A [`SelectionReason`] strong enough to justify emitting on its own — every
/// rung above plain lexical overlap.
fn is_exact_rung(reason: &SelectionReason) -> bool {
    matches!(
        reason,
        SelectionReason::ExplicitId
            | SelectionReason::ExactPath
            | SelectionReason::ExactSymbol
            | SelectionReason::LinkedFrom
            | SelectionReason::StageDependency
    )
}

/// The single stdout line for `handed_over`: the shared brief, less its
/// omission count, wrapped in the hook's JSON envelope.
fn render_payload(pull_stage: Option<&str>, handed_over: &ContextPack) -> Option<String> {
    let rendered =
        crate::orchestrator::signals::format_knowledge_brief(handed_over, pull_stage, QUERY_INPUTS);
    let brief = without_omitted_line(rendered, handed_over.omitted.omitted);
    let payload = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": brief,
        }
    });
    serde_json::to_string(&payload).ok()
}

/// `brief` without the shared renderer's `Omitted: N weaker matches.` line.
///
/// An unsolicited brief is read by a session that did not ask for it, and a
/// count of what it was NOT given tells that session nothing the footer's pull
/// command does not already offer. `loom knowledge context` and the stage
/// briefs keep the line — the renderer is shared, so it comes off here. The
/// LAST occurrence is the footer's: every excerpt is rendered before it, so an
/// excerpt quoting the same words is left alone.
fn without_omitted_line(mut brief: String, omitted: usize) -> String {
    let line = format!("Omitted: {omitted} weaker matches.\n\n");
    if let Some(start) = brief.rfind(&line) {
        brief.replace_range(start..start + line.len(), "");
    }
    brief
}

/// `pack` minus every unit already delivered to this recipient this epoch, or
/// `None` when nothing survives.
///
/// A unit dropped here for dedupe is folded into `omitted` the same way
/// [`without_weakest`] folds its own per-unit drops in: `pack.omitted` as
/// retrieval built it describes only what did not fit the BUDGET, so left
/// alone the `PromptBrief` telemetry would record that the session was handed
/// everything retrieval found, when some of it was simply repeated from an
/// earlier prompt this epoch.
fn undelivered(pack: &ContextPack, delivered: &BTreeSet<(String, String)>) -> Option<ContextPack> {
    let kept: Vec<ContextItem> = pack
        .items
        .iter()
        .filter(|item| {
            let key = (item.id.as_str().to_string(), item.content_hash.clone());
            !delivered.contains(&key)
        })
        .cloned()
        .collect();
    carrying_after_drop(pack, kept)
}

fn carrying_after_drop(pack: &ContextPack, kept: Vec<ContextItem>) -> Option<ContextPack> {
    if kept.is_empty() {
        return None;
    }
    Some(carrying(pack, kept))
}

/// `pack` without its lowest-scoring unit, or `None` when a single unit is all
/// that is left — one unit that does not fit cannot be trimmed into fitting.
fn without_weakest(pack: &ContextPack) -> Option<ContextPack> {
    if pack.items.len() <= 1 {
        return None;
    }
    let weakest = (0..pack.items.len()).min_by(|&left, &right| {
        pack.items[left]
            .score
            .total_cmp(&pack.items[right].score)
            // Ties drop the later unit: pack order is strongest first.
            .then(right.cmp(&left))
    })?;

    let mut items = pack.items.clone();
    items.remove(weakest);
    Some(carrying(pack, items))
}

/// Clone a pack with accounting for the retained items.
fn carrying(pack: &ContextPack, items: Vec<ContextItem>) -> ContextPack {
    let mut narrowed = pack.clone();
    narrowed.retain_items(items);
    narrowed
}

#[cfg(test)]
mod floor_tests {
    use super::*;
    use crate::context::schema::{
        Channel, ChunkId, Confidence, Freshness, ItemKind, LifecycleState, OmissionSummary,
        SourcePointer,
    };
    use std::path::PathBuf;

    fn symbol_question_item() -> ContextItem {
        ContextItem {
            id: ChunkId::from("src/a.rs#function:plan_key"),
            kind: ItemKind::SourceNode,
            pointer: SourcePointer {
                path: PathBuf::from("src/a.rs"),
                anchor: String::new(),
                line_start: Some(1),
                line_end: Some(3),
            },
            summary: "function plan_key".to_string(),
            source: Channel::Source,
            token_count: 10,
            score: 1.0,
            reasons: vec![SelectionReason::SymbolQuestion],
            confidence: Confidence::Low,
            state: LifecycleState::Active,
            content_hash: "sha256:a".to_string(),
            excerpt: None,
            truncated: false,
            matched_term_count: 1,
            explanation: None,
            caveat: None,
            window: None,
        }
    }

    #[test]
    fn symbol_question_item_passes_the_emit_floor() {
        let config = RetrievalConfig::default();
        let mut item = symbol_question_item();
        assert!(
            item.matched_term_count < config.min_knowledge_terms,
            "the fixture must sit below the term floor for this test to mean anything"
        );
        assert!(clears_item_floor(&item, &config));

        item.reasons = vec![SelectionReason::Lexical];
        assert!(
            !clears_item_floor(&item, &config),
            "the same item without the symbol-question reason stays below the floor"
        );
    }

    #[test]
    fn a_pack_of_one_symbol_question_item_is_emitted() {
        let config = RetrievalConfig::default();
        let pack = ContextPack {
            query: "what does plan_key do".to_string(),
            scope: Channel::all().to_vec(),
            budget_tokens: config.prompt_budget_tokens,
            estimated_tokens: 10,
            structural_freshness: Freshness::default(),
            semantic_freshness: Freshness::default(),
            items: vec![symbol_question_item()],
            unmet_required: Vec::new(),
            omitted: OmissionSummary::default(),
            dropped_terms: Vec::new(),
            degraded: None,
            text_search: None,
        };

        assert!(compose(Some("stage-a"), &pack, &BTreeSet::new(), &config).is_some());
    }
}
