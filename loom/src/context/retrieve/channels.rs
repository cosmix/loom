//! Per-channel ranking dispatch for [`super::retrieve_for_stage`].
//!
//! One place decides which ranker a [`Channel`] goes to, what happens when the
//! corpus behind a channel is missing, and which channel's corpus diagnostics
//! the pack ends up reporting. Split out of `retrieve.rs` so that entry point
//! stays a readable sequence of steps rather than a file with a ranker
//! dispatcher wedged into the middle of it.

use crate::context::config::RetrievalConfig;
use crate::context::graph_store::ResolvedGraph;
use crate::context::rank::{rank_channel, RankQuery, RankedCandidate};
use crate::context::rank_source::rank_source_channel;
use crate::context::schema::Channel;
use crate::fs::knowledge::catalog::Catalog;

use super::StageQuery;

/// Every requested channel's candidate list, plus the one dropped-term set the
/// pack reports.
pub(super) struct RankedChannels {
    /// One candidate list per requested channel, in `scope` order.
    pub(super) lists: Vec<Vec<RankedCandidate>>,
    /// Query terms dropped before scoring, for `ContextPack::dropped_terms`.
    pub(super) dropped_terms: Vec<String>,
}

/// Rank the catalog once per requested channel, producing one candidate list
/// per channel.
///
/// Each channel is ranked over its own corpus: the knowledge channel over the
/// catalog's chunks via [`rank_channel`], the source channel over `graph`'s
/// nodes via [`rank_source_channel`]. `graph` is `None` when the source graph
/// was never built or could not be read for this query — that is a degraded
/// pack, not an error, so the source channel simply contributes no candidates
/// rather than failing the whole retrieval.
///
/// The pack carries ONE dropped-term set, not one per channel: both channels
/// tokenize the same query text, so the two sets differ only where their
/// corpora disagree on a term's document frequency. The knowledge channel's is
/// the one reported — it is the channel whose omissions a reader is asking
/// about — and the source channel's stands in only when the knowledge channel
/// was out of scope entirely.
pub(super) fn rank_channels(
    channels: &[Channel],
    rank_query: &RankQuery,
    catalog: &Catalog,
    graph: Option<&ResolvedGraph>,
    config: &RetrievalConfig,
) -> RankedChannels {
    let mut lists = Vec::with_capacity(channels.len());
    let mut knowledge_dropped = None;
    let mut source_dropped = None;
    for channel in channels {
        let ranking = match channel {
            Channel::Knowledge => {
                rank_channel(rank_query, &catalog.chunks, Channel::Knowledge, config)
            }
            Channel::Source => graph
                .map(|graph| rank_source_channel(rank_query, graph, config))
                .unwrap_or_default(),
        };
        let dropped = match channel {
            Channel::Knowledge => &mut knowledge_dropped,
            Channel::Source => &mut source_dropped,
        };
        *dropped = Some(ranking.dropped_terms);
        lists.push(ranking.candidates);
    }
    RankedChannels {
        lists,
        dropped_terms: knowledge_dropped.or(source_dropped).unwrap_or_default(),
    }
}

/// Build the ranker's view of `query`.
///
/// A [`StageQuery`] additionally names where to look; a [`RankQuery`] is only
/// what to look for, which is why the two types exist separately.
pub(super) fn build_rank_query(query: &StageQuery) -> RankQuery {
    RankQuery {
        text: query.text.clone(),
        required_ids: query.required_ids.clone(),
        stage_dependency_ids: query.stage_dependency_ids.clone(),
        dependency_paths: query.dependency_paths.clone(),
    }
}
