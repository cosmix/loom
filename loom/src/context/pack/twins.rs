//! Which curated chunks restate each other, and what the packer walks first
//! because of it.
//!
//! Curated knowledge is two tiers: a tier-1 file (`mistakes.md`) keeps a two-
//! to eight-line summary per topic that ends in a link to a tier-2 file
//! (`mistakes/<slug>.md`), and `loom knowledge update` scaffolds that file with
//! the tier-1 heading verbatim. The two therefore carry the same anchor and
//! score within a rounding error of each other on the same terms, so both used
//! to be packed — the summary spending budget to tell the reader about text
//! sitting directly above it.
//!
//! [`tier1_twin`] names the relationship and [`details_before_summaries`]
//! orders the pair so the detail is always offered the budget first. Dropping
//! the summary is [`super::select`]'s decision, and only once the detail has
//! actually been taken: when the detail does not fit, the summary is the one
//! thing left that can tell the reader the topic exists.

use crate::context::rank::RankedCandidate;
use crate::context::schema::{Channel, SelectionReason};
use crate::fs::knowledge::catalog::prose::PROSE_ID_PREFIX;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The id of the tier-1 summary that duplicates `tier2_id`, when `tier2_id`
/// names a chunk in a tier-2 topic file.
///
/// `mistakes/sandbox-and-settings.md#sandbox-contradictory-path-rules#0` maps
/// to `mistakes.md#sandbox-contradictory-path-rules#0`: the spilled topic file
/// keeps its parent tier-1 file's stem as its directory and its heading
/// verbatim, which is the only relation `loom knowledge update` creates. The
/// occurrence index of the twin is always `0` — a tier-1 file states a topic
/// once.
///
/// Keying on parent-directory-equals-stem is what keeps the rule from
/// collapsing an unrelated pair that merely shares an anchor:
/// `architecture/overview.md#overview#0` is the twin of
/// `architecture.md#overview#0` and never of `conventions.md#overview#0`.
///
/// `None` for every other shape:
///
/// - a `prose:`-prefixed id, which is indexed project prose with no curated
///   tier-1 counterpart at all;
/// - a path nested deeper than one directory (`a/b/c.md`), which no tier-1
///   file can be the parent of;
/// - a tier-1 id itself (`mistakes.md#...`), which has no directory;
/// - an empty anchor, i.e. a file's own preamble chunk rather than a spilled
///   topic — `mistakes/sandbox-and-settings.md##0` is not a restatement of
///   anything in `mistakes.md`;
/// - a source-node id, which is `<path>#<kind>:<scope>` and so carries a single
///   `#` where a knowledge chunk id carries two.
pub(crate) fn tier1_twin(tier2_id: &str) -> Option<String> {
    if tier2_id.starts_with(PROSE_ID_PREFIX) {
        return None;
    }
    let (path, suffix) = tier2_id.split_once('#')?;
    let (anchor, occurrence) = suffix.split_once('#')?;
    if anchor.is_empty() || occurrence.parse::<usize>().is_err() {
        return None;
    }
    let path = Path::new(path);
    if path.extension()? != "md" {
        return None;
    }
    let parent = path.parent()?;
    let stem = parent.to_str()?;
    if stem.is_empty() || parent.parent() != Some(Path::new("")) {
        return None;
    }
    Some(format!("{stem}.md#{anchor}#0"))
}

/// The tier-1 twin of a candidate, for knowledge candidates only.
///
/// Gated on the channel for the reason [`super::build_item`] dispatches on it:
/// a channel is authoritative about what its own ids mean, so a source node is
/// never anybody's tier-2 detail however its id happens to be punctuated.
pub(super) fn knowledge_twin(candidate: &RankedCandidate) -> Option<String> {
    (candidate.channel == Channel::Knowledge)
        .then(|| tier1_twin(candidate.id.as_str()))
        .flatten()
}

/// True when the caller named this candidate outright — `--require-id`, or a
/// stage's `required_ids`, which is the only thing that awards
/// [`SelectionReason::ExplicitId`] (`rank/ladder.rs:38`).
///
/// The twin rule stands aside for such a summary, on both counts. Suppressing
/// it would answer a request for one id with a different one, and promoting
/// its detail would be worse still: the request boosts the summary by
/// `BOOST_EXPLICIT_ID` while leaving the detail on whatever weak lexical score
/// it earned, so the pair is no longer the near-tie the rule is written for and
/// the promotion would drag a barely-relevant chunk to the head of the pack.
pub(super) fn explicitly_required(candidate: &RankedCandidate) -> bool {
    candidate.reasons.contains(&SelectionReason::ExplicitId)
}

/// `ranked` reordered so a tier-2 detail is walked immediately before the
/// tier-1 summary it duplicates, when the summary ranked higher.
///
/// This is the look-ahead half of the twin rule. Order otherwise carries
/// everything the packer decides, so a summary that outranks its own detail
/// would be taken first and the detail skipped behind it as the duplicate —
/// spending the slot on the pointer instead of on the text it points at.
/// Moving the detail into the summary's position spends it on the detail, and
/// leaving the summary directly behind keeps it available as the fallback when
/// the detail turns out not to fit.
///
/// Deterministic in every input: at most one detail is promoted per summary
/// (`or_insert` keeps the highest-ranked when two tier-2 files share a
/// heading), a promoted detail is emitted once and skipped at its old
/// position, and every other candidate keeps its relative order.
pub(super) fn details_before_summaries(ranked: &[RankedCandidate]) -> Vec<&RankedCandidate> {
    let mut detail_of: BTreeMap<String, usize> = BTreeMap::new();
    for (position, candidate) in ranked.iter().enumerate() {
        if let Some(twin) = knowledge_twin(candidate) {
            detail_of.entry(twin).or_insert(position);
        }
    }

    let mut promoted: BTreeSet<usize> = BTreeSet::new();
    let mut ordered: Vec<&RankedCandidate> = Vec::with_capacity(ranked.len());
    for (position, candidate) in ranked.iter().enumerate() {
        if let Some(&detail) = detail_of.get(candidate.id.as_str()) {
            if detail > position && !explicitly_required(candidate) {
                ordered.push(&ranked[detail]);
                promoted.insert(detail);
            }
        }
        if !promoted.contains(&position) {
            ordered.push(candidate);
        }
    }
    ordered
}
