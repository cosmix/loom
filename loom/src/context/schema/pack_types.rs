//! The retrieval result types: one selected [`ContextItem`], a requirement the
//! packer could not honor, and the [`ContextPack`] that carries them.
//!
//! Split out of the parent module to keep it under the file-size cap; every name
//! here is re-exported from [`crate::context::schema`].

use serde::{Deserialize, Serialize};

use super::{
    Channel, ChunkId, Confidence, Freshness, ItemKind, LifecycleState, OmissionSummary,
    SelectionReason, SourcePointer,
};

/// One selected unit of context, with its full provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextItem {
    pub id: ChunkId,
    pub kind: ItemKind,
    pub pointer: SourcePointer,
    /// Short human-readable description, not the body.
    pub summary: String,
    /// Channel this item was retrieved from.
    pub source: Channel,
    /// Estimated tokens this item costs against the budget.
    pub token_count: usize,
    /// Fused relevance score.
    pub score: f32,
    /// Every reason that contributed to `score`.
    #[serde(default)]
    pub reasons: Vec<SelectionReason>,
    pub confidence: Confidence,
    pub state: LifecycleState,
    /// `sha256:<hex>` over the backing chunk body, copied from
    /// [`super::KnowledgeChunk::content_hash`]. Empty when the backing unit has no hash.
    ///
    /// Carried on the item so a delivery record can be written, and a repeat
    /// delivery suppressed, without a second lookup into the catalog.
    #[serde(default)]
    pub content_hash: String,
    /// Verbatim text of the backing unit, ready to quote.
    ///
    /// `None` when the packer had no body to copy. Under
    /// [`super::RequiredRepresentation::Compact`] this is bounded to
    /// [`super::EXCERPT_MAX_TOKENS`], and a truncated string ends with
    /// [`super::EXCERPT_TRUNCATION_MARKER`] on its own line. Under
    /// [`super::RequiredRepresentation::Full`] — the default representation for a
    /// `--require-id` reservation — it is copied whole with no bound at all
    /// and `truncated` stays `false`: `Full` exists so a caller can demand the
    /// entire unit regardless of `EXCERPT_MAX_TOKENS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    /// Whether the item's selected representation was truncated.
    #[serde(default)]
    pub truncated: bool,
    /// How many DISTINCT query terms this item matched lexically.
    ///
    /// The hook's emit floor needs a per-item strength signal that survives the
    /// trip from `RankedCandidate` into the pack; a score cannot serve, because
    /// scores are not comparable across fusion tiers.
    #[serde(default)]
    pub matched_term_count: usize,
    /// Why a graph neighbour was admitted, e.g. ``called by `seed` at path:L12``.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    /// A trust warning rendered with the item, e.g. `partial coverage`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caveat: Option<String>,
    /// A short verbatim source window attached to a strongly matched item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
}

/// A required id the pack could not honor under its budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnmetRequirement {
    pub id: String,
    /// Estimated tokens the requested representation needs, wrappers included.
    pub needed_tokens: usize,
    /// Tokens left for the item's own body once the markdown chrome its
    /// inclusion would add — a new section heading, its path's group prefix
    /// if that path starts a new run — is paid for. See
    /// `context::pack::required::reserve`, which derives this from the same
    /// chrome-aware total `needed_tokens` was measured against, so
    /// `needed_tokens > available_tokens` always agrees with why the item was
    /// turned away.
    pub available_tokens: usize,
    pub reason: String,
}

/// A literal-text query the graph cannot answer: the pattern to search for and
/// the command that searches for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSearchHint {
    /// The quoted text the query asked for.
    pub pattern: String,
    /// A shell command that searches the tree for `pattern`.
    pub command: String,
}

/// The result of one retrieval: what was selected, what was not, and how stale
/// the underlying derived data is.
///
/// `estimated_tokens` covers the Knowledge Brief frame
/// ([`super::BRIEF_FRAME_TOKENS`]), every packed item's rendered cost, and the
/// markdown chrome `format_knowledge_brief` wraps them in — section headings,
/// per-path source bullet prefixes, and unmet-requirement lines — see
/// [`ContextPack::recompute_estimate`]. The packer guarantees
/// `estimated_tokens <= budget_tokens` (see [`ContextPack::within_budget`])
/// whenever the frame plus the unmet-requirement lines alone fit the budget,
/// since `pack::required::reserve_within_budget` holds back exactly that
/// cost first. The one exception, a budget too small even for that floor:
/// every required id must still be reported, so `items` is empty and
/// `estimated_tokens` is pinned to the floor, over budget — pinned by
/// `tests::pack::assert_pack_never_overshoots_budget`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextPack {
    pub query: String,
    /// Channels this query covered.
    #[serde(default)]
    pub scope: Vec<Channel>,
    pub budget_tokens: usize,
    /// Estimated tokens charged for the pack's current representation.
    pub estimated_tokens: usize,
    pub structural_freshness: Freshness,
    pub semantic_freshness: Freshness,
    #[serde(default)]
    pub items: Vec<ContextItem>,
    #[serde(default)]
    pub unmet_required: Vec<UnmetRequirement>,
    pub omitted: OmissionSummary,
    /// Query terms dropped before scoring as corpus-ubiquitous or too short.
    ///
    /// Observability only: `--json` and `--explain` surface it, the hook brief
    /// never renders it. Empty until the stopwording pass exists.
    #[serde(default)]
    pub dropped_terms: Vec<String>,
    /// Set when this pack was served from a knowingly incomplete index.
    ///
    /// Carries a human-readable reason; `None` is healthy. A missing base
    /// layer ALONE is not a degradation: a base is published from committed
    /// `HEAD` content even on a dirty tree, so the overlay is the designed
    /// path. What a base alone cannot represent is the dirty working-tree
    /// state, which the overlay carries. See `retrieve::graph` for why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded: Option<String>,
    /// Set when the query asked for literal text, which the graph does not
    /// index: the search the caller should run instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_search: Option<TextSearchHint>,
}

impl ContextPack {
    /// Replace items with a retained subset and update its accounting.
    pub(crate) fn retain_items(&mut self, items: Vec<ContextItem>) {
        let dropped = self.items.len().saturating_sub(items.len());
        self.items = items;
        self.omitted.omitted += dropped;
        self.omitted.coverage.included = self.items.len();
        self.omitted.coverage.included_tokens =
            self.items.iter().map(|item| item.token_count).sum();
        if dropped > 0 {
            self.omitted.weakest_included_score = self
                .items
                .iter()
                .map(|item| item.score)
                .reduce(f32::min)
                .unwrap_or(0.0);
        }
        self.recompute_estimate();
    }

    /// The invariant the packer must never violate.
    pub fn within_budget(&self) -> bool {
        self.estimated_tokens <= self.budget_tokens
    }

    /// Recompute the estimated cost of the exact brief frame, its items, and
    /// the markdown chrome wrapped around them — delegates to
    /// `rendered_brief_tokens` in `context::render`, the single definition
    /// this and the packer's own selection accounting both call so a budget
    /// decision and this published estimate can never disagree.
    ///
    /// Also charges the `Literal text:` line when the pack carries a
    /// [`TextSearchHint`], which the brief renders outside its items.
    pub fn recompute_estimate(&mut self) {
        let hint = self
            .text_search
            .as_ref()
            .map_or(0, crate::context::render::literal_text_tokens);
        self.estimated_tokens =
            crate::context::render::rendered_brief_tokens(self.items.iter(), &self.unmet_required)
                + hint;
    }
}
