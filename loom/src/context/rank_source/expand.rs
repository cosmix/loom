//! Bounded source-graph expansion from exact-rung ranking seeds.

use super::{estimate_node_tokens, paths::apply_test_path_factor};
use crate::context::config::RetrievalConfig;
use crate::context::graph_store::ResolvedGraph;
use crate::context::rank::{
    neighbor_explanation, EdgeDirection, NeighborVia, RankedCandidate, BOOST_EXACT_SYMBOL,
};
use crate::context::schema::{
    estimate_tokens, Channel, ChunkId, Confidence, FileCoverage, SelectionReason, SourceNode,
    SourceNodeKind,
};
use crate::context::source_graph::{SourceEdge, SourceEdgeKind};
use std::collections::{BTreeMap, BTreeSet};

/// Ceiling on the estimated rendered tokens of the neighbours one query admits,
/// routed (`routing`) and expanded together.
pub const MAX_EXPANDED_TOKENS: usize = 300;
pub(super) const MAX_EXPANSION_SEEDS: usize = 5;
pub(super) const MAX_NEIGHBORS_PER_SEED: usize = 4;
pub(super) const MAX_EXPANDED: usize = 12;
pub(super) const NEIGHBOR_SCORE_FACTOR: f32 = 0.2;
pub(super) const MIN_NEIGHBOR_EDGE_CONFIDENCE: f32 = 0.5;

/// Neighbours examined per seed before the loop moves on, independent of how
/// many of them are actually accepted.
///
/// `MAX_NEIGHBORS_PER_SEED` bounds acceptances, but a seed's neighbour list can
/// be mostly rejections — already a candidate, a `File` node, partial coverage
/// — and none of those rejections used to stop the scan. This bounds the scan
/// itself, on the sorted-by-confidence list `neighbours_for_seed` already
/// produces, so truncating it still keeps the strongest neighbours.
pub(super) const MAX_EXAMINED_NEIGHBORS_PER_SEED: usize = 32;

/// Ceiling on a neighbour's score: at most the weakest a genuine exact rung
/// can ever score, so a node that merely neighbours a seed can never outrank
/// a node the query matched directly.
///
/// [`BOOST_EXACT_SYMBOL`] (80.0) is not itself that floor. `score_node`
/// (`rank_source.rs:281`) applies `config.test_path_factor` to a node's WHOLE
/// score as its last step, so a genuine `ExactSymbol` hit in a test file
/// scores as low as `BOOST_EXACT_SYMBOL * config.test_path_factor` (32.0 at
/// the 0.4 default) — well below 80. A flat cap near 80 let a neighbour of an
/// `ExplicitId` or `ExactPath` seed (`BOOST_EXPLICIT_ID` = 1000,
/// `BOOST_EXACT_PATH` = 100) outrank exactly that kind of direct hit.
/// Deriving the cap from the same factor keeps the invariant true regardless
/// of which file the direct match lives in.
pub(super) fn max_neighbor_score(config: &RetrievalConfig) -> f32 {
    BOOST_EXACT_SYMBOL * config.test_path_factor
}

/// Estimated rendered tokens of `node` as a neighbour reached over `via`: the
/// node itself and the explanation rendered beside it.
pub(super) fn neighbour_tokens(node: &SourceNode, via: &NeighborVia) -> usize {
    estimate_node_tokens(node) + estimate_tokens(&neighbor_explanation(via))
}

/// Whether a node may be admitted as a graph neighbour: a file node is the
/// container of the symbols a reader wants, and a partly extracted node cannot
/// back a claim about what it is. Shared by expansion and relationship routing
/// so the two admit the same kinds of node.
pub(super) fn is_expandable(node: &SourceNode) -> bool {
    !matches!(node.kind, SourceNodeKind::File) && matches!(node.coverage, FileCoverage::Full)
}

/// What intent routing spent of the expansion budget: the neighbours it
/// admitted and their estimated rendered tokens. Expansion continues from both,
/// so [`MAX_EXPANDED`] and [`MAX_EXPANDED_TOKENS`] each cover the whole query,
/// routed and expanded neighbours together.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ExpansionSpend {
    pub(super) neighbours: usize,
    pub(super) tokens: usize,
}

type Adjacency<'a> = BTreeMap<&'a str, Vec<&'a SourceEdge>>;

/// One neighbour a seed reached: its id, the confidence of the edge that
/// reached it, that edge, and the edge's direction relative to the neighbour.
type Neighbour<'a> = (&'a str, f32, &'a SourceEdge, EdgeDirection);

/// One seed's id and score with the neighbours it reached, in the order the
/// accept walk examines them.
struct SeedReach<'a> {
    id: String,
    score: f32,
    neighbours: Vec<Neighbour<'a>>,
}

/// Mutable state threaded through one expansion pass: the growing candidate
/// list, the id set it already carries, how many neighbours have been
/// accepted overall (routed ones included), the estimated rendered tokens of
/// those neighbours, and an id index over the graph so accepting a neighbour
/// costs a map probe rather than a scan of every node.
struct Expansion<'a> {
    ranked: Vec<RankedCandidate>,
    existing: BTreeSet<String>,
    expanded: usize,
    tokens: usize,
    token_capped: bool,
    nodes_by_id: BTreeMap<&'a str, &'a SourceNode>,
}

impl Expansion<'_> {
    /// True once no further neighbour may be admitted: the count cap is met or
    /// the next neighbour would pass [`MAX_EXPANDED_TOKENS`].
    fn is_full(&self) -> bool {
        self.expanded == MAX_EXPANDED || self.token_capped
    }
}

/// Add graph neighbours of the strongest exact-rung candidates as tier-2 candidates.
/// `ranked` is the channel's scored list BEFORE truncation to `MAX_SOURCE_CANDIDATES`,
/// sorted strongest first. `routed` is what intent routing already spent of
/// [`MAX_EXPANDED`] and [`MAX_EXPANDED_TOKENS`]. Returns the list with
/// neighbours appended (unsorted).
pub(super) fn expand_from_seeds(
    ranked: Vec<RankedCandidate>,
    graph: &ResolvedGraph,
    config: &RetrievalConfig,
    routed: ExpansionSpend,
) -> Vec<RankedCandidate> {
    let seeds: Vec<(String, f32)> = ranked
        .iter()
        .filter(|candidate| has_seed_reason(&candidate.reasons))
        .take(MAX_EXPANSION_SEEDS)
        .map(|candidate| (candidate.id.as_str().to_string(), candidate.score))
        .collect();
    if seeds.is_empty() {
        return ranked;
    }

    let existing: BTreeSet<String> = ranked
        .iter()
        .map(|candidate| candidate.id.as_str().to_string())
        .collect();

    let seed_ids: BTreeSet<&str> = seeds.iter().map(|(id, _)| id.as_str()).collect();
    let (forward, reverse) = build_seed_adjacencies(graph, &seed_ids);
    let (per_seed, wanted_ids) = resolve_seed_neighbours(&seeds, &forward, &reverse);
    let nodes_by_id = index_wanted_nodes(graph, &wanted_ids);

    let mut state = Expansion {
        ranked,
        existing,
        expanded: routed.neighbours,
        tokens: routed.tokens,
        token_capped: false,
        nodes_by_id,
    };
    for reach in per_seed {
        if state.is_full() {
            break;
        }
        append_neighbours(reach, config, &mut state);
    }
    state.ranked
}

/// Resolve every seed's neighbour ids before touching `graph.nodes()`, so the
/// id index `index_wanted_nodes` builds only has to cover ids the traversal
/// can actually reach — at most `MAX_EXPANSION_SEEDS *
/// MAX_EXAMINED_NEIGHBORS_PER_SEED` — instead of every node in the graph.
fn resolve_seed_neighbours<'a>(
    seeds: &[(String, f32)],
    forward: &Adjacency<'a>,
    reverse: &Adjacency<'a>,
) -> (Vec<SeedReach<'a>>, BTreeSet<&'a str>) {
    let mut per_seed = Vec::with_capacity(seeds.len());
    let mut wanted_ids: BTreeSet<&str> = BTreeSet::new();
    for (seed_id, seed_score) in seeds {
        let neighbours: Vec<Neighbour<'a>> = neighbours_for_seed(seed_id, forward, reverse)
            .into_iter()
            .take(MAX_EXAMINED_NEIGHBORS_PER_SEED)
            .collect();
        wanted_ids.extend(neighbours.iter().map(|(id, ..)| *id));
        per_seed.push(SeedReach {
            id: seed_id.clone(),
            score: *seed_score,
            neighbours,
        });
    }
    (per_seed, wanted_ids)
}

/// Index just the nodes `resolve_seed_neighbours` found reachable, rather
/// than every node the graph holds.
fn index_wanted_nodes<'a>(
    graph: &'a ResolvedGraph,
    wanted_ids: &BTreeSet<&str>,
) -> BTreeMap<&'a str, &'a SourceNode> {
    graph
        .nodes()
        .filter(|node| wanted_ids.contains(node.id.as_str()))
        .map(|node| (node.id.as_str(), node))
        .collect()
}

fn has_seed_reason(reasons: &[SelectionReason]) -> bool {
    reasons.iter().any(|reason| {
        matches!(
            reason,
            SelectionReason::ExplicitId
                | SelectionReason::ExactPath
                | SelectionReason::ExactSymbol
                | SelectionReason::StageDependency
        )
    })
}

/// Forward/reverse adjacency restricted to edges that touch a seed.
///
/// A retrieval pass has at most [`MAX_EXPANSION_SEEDS`] seeds, so scanning
/// every node and every edge in the graph to answer at most 5 lookups wastes
/// work on the prompt-hook path the lexical cache exists to keep cheap. One
/// pass over `graph.edges()`, keeping only the edges whose `from` or `to` is a
/// seed, does the same job without a `Vec` entry for every node the graph
/// actually has.
fn build_seed_adjacencies<'a>(
    graph: &'a ResolvedGraph,
    seed_ids: &BTreeSet<&str>,
) -> (Adjacency<'a>, Adjacency<'a>) {
    let mut forward: Adjacency<'a> = BTreeMap::new();
    let mut reverse: Adjacency<'a> = BTreeMap::new();
    for edge in graph.edges() {
        if seed_ids.contains(edge.from.as_str()) {
            forward.entry(edge.from.as_str()).or_default().push(edge);
        }
        if seed_ids.contains(edge.to.as_str()) {
            reverse.entry(edge.to.as_str()).or_default().push(edge);
        }
    }
    (forward, reverse)
}

/// The neighbours of one seed, strongest edge first. A forward edge (the seed
/// is the caller) reaches a neighbour the seed calls, so the neighbour sits at
/// the `Incoming` end; a reverse edge reaches a neighbour that calls the seed,
/// which is `Outgoing` from the neighbour's side.
fn neighbours_for_seed<'a>(
    seed_id: &str,
    forward: &Adjacency<'a>,
    reverse: &Adjacency<'a>,
) -> Vec<Neighbour<'a>> {
    let mut neighbours = Vec::new();
    for edge in forward.get(seed_id).into_iter().flatten() {
        if eligible_edge(edge) {
            neighbours.push((
                edge.to.as_str(),
                edge.confidence,
                *edge,
                EdgeDirection::Incoming,
            ));
        }
    }
    for edge in reverse.get(seed_id).into_iter().flatten() {
        if eligible_edge(edge) {
            neighbours.push((
                edge.from.as_str(),
                edge.confidence,
                *edge,
                EdgeDirection::Outgoing,
            ));
        }
    }
    neighbours.sort_by(|(a_id, a_confidence, ..), (b_id, b_confidence, ..)| {
        b_confidence
            .total_cmp(a_confidence)
            .then_with(|| a_id.cmp(b_id))
    });
    neighbours
}

fn eligible_edge(edge: &SourceEdge) -> bool {
    matches!(
        edge.kind,
        SourceEdgeKind::Calls
            | SourceEdgeKind::Implements
            | SourceEdgeKind::Extends
            | SourceEdgeKind::References
    ) && !edge.is_unresolved()
        && edge.confidence >= MIN_NEIGHBOR_EDGE_CONFIDENCE
}

/// The edge that introduced a neighbour, as the explanation rendered beside it
/// will name it. A zero line is a span the extractor never positioned.
fn neighbor_via(seed_id: &str, edge: &SourceEdge, direction: EdgeDirection) -> NeighborVia {
    NeighborVia {
        seed: seed_id.to_string(),
        edge_kind: edge.kind,
        direction,
        provenance: edge.provenance,
        site_line: edge
            .sites
            .first()
            .map(|site| site.line_start)
            .filter(|line| *line > 0),
    }
}

fn append_neighbours(reach: SeedReach<'_>, config: &RetrievalConfig, state: &mut Expansion<'_>) {
    let mut added_for_seed = 0;
    for (id, _, edge, direction) in reach.neighbours {
        if added_for_seed == MAX_NEIGHBORS_PER_SEED || state.is_full() {
            break;
        }
        // Cheapest rejection first: a set probe, before the node lookup.
        if state.existing.contains(id) {
            continue;
        }
        let Some(node) = state.nodes_by_id.get(id).copied() else {
            continue;
        };
        if !is_expandable(node) {
            continue;
        }
        let via = neighbor_via(&reach.id, edge, direction);
        let tokens = neighbour_tokens(node, &via);
        if state.tokens + tokens > MAX_EXPANDED_TOKENS {
            state.token_capped = true;
            break;
        }
        let score = apply_test_path_factor(
            node,
            (reach.score * NEIGHBOR_SCORE_FACTOR).min(max_neighbor_score(config)),
            config,
        );
        state.ranked.push(RankedCandidate {
            id: ChunkId::from(id),
            channel: Channel::Source,
            score,
            reasons: vec![SelectionReason::GraphNeighbor],
            token_count: estimate_node_tokens(node),
            matched_term_count: 0,
            confidence_ceiling: Some(Confidence::Medium),
            via: Some(via),
        });
        state.existing.insert(id.to_string());
        state.tokens += tokens;
        added_for_seed += 1;
        state.expanded += 1;
    }
}
