//! Bounded reverse traversal: who reaches a node, and how far to trust the claim.
//!
//! Edges point caller -> callee, so "what breaks if I change this?" is a walk
//! *backwards*: from a node to the `from` of every edge whose `to` is that node.
//! The walk is breadth-first and depth-bounded, and a node is emitted once.
//!
//! ## Trust is the minimum, never a product
//!
//! Each hit reports the confidence of the **weakest** edge on the path taken,
//! along with that edge's provenance and kind. A path is worth exactly its worst
//! step: one guess anywhere in a chain caps everything beyond it, and a caller
//! that sees `0.5 / syntax` knows which link to distrust. Multiplying
//! confidences would instead punish long chains for their length — five fully
//! bound hops would decay below a single guess, which is precisely backwards.
//!
//! ## Candidate sets
//!
//! An ambiguous call keeps the ids it could mean. With `follow_candidates`, each
//! member is walked as a reverse edge at [`AMBIGUOUS_CANDIDATE_CONFIDENCE`], and a
//! hit whose path used one is flagged `via_candidates`. A `min_confidence` above
//! that trust requires bound edges throughout.
//!
//! [`AMBIGUOUS_CANDIDATE_CONFIDENCE`]: crate::context::source_graph::AMBIGUOUS_CANDIDATE_CONFIDENCE
//!
//! ## What a traversal may not claim
//!
//! Reachability here is reachability *in the derived graph*, which is not the
//! program. Edges the extractors could not resolve, and could not narrow to a
//! candidate set, are dropped before the walk rather than treated as edges to an
//! "unresolved" hub, so an absent path means "not found", never "does not exist".

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{EdgeProvenance, SourceEdgeKind, SourceNode, SourceNodeKind};

#[path = "impact_trust.rs"]
mod trust;
use trust::{Step, Trust};

/// One node reached by a reverse traversal, with the trust of the weakest step.
#[derive(Debug, Clone, PartialEq)]
pub struct ImpactHit {
    pub id: String,
    pub kind: SourceNodeKind,
    pub path: PathBuf,
    /// Hops from the start node; 1 is a direct dependent.
    pub depth: usize,
    /// MINIMUM confidence along the path taken to reach this node.
    pub min_confidence: f32,
    /// Provenance of the weakest edge on that path.
    pub weakest_provenance: EdgeProvenance,
    /// Kind of the weakest edge on that path.
    pub weakest_kind: SourceEdgeKind,
    /// True when the path taken passed through a member of a candidate set.
    pub via_candidates: bool,
}

/// Filters and bounds for one reverse-impact traversal.
#[derive(Debug, Clone, PartialEq)]
pub struct ImpactOptions {
    pub max_depth: usize,
    /// An empty list walks every semantic dependency kind, excluding file
    /// containment and imports. A non-empty list is an explicit allow-list.
    pub kinds: Vec<SourceEdgeKind>,
    /// Zero is unlimited; otherwise only the first `limit` sorted hits remain.
    pub limit: usize,
    pub path_prefix: Option<String>,
    pub min_confidence: f32,
    /// Walk only edges of these provenance classes; empty walks every class.
    pub provenances: Vec<EdgeProvenance>,
    /// Walk each member of an ambiguous edge's candidate set at
    /// [`AMBIGUOUS_CANDIDATE_CONFIDENCE`].
    ///
    /// [`AMBIGUOUS_CANDIDATE_CONFIDENCE`]: crate::context::source_graph::AMBIGUOUS_CANDIDATE_CONFIDENCE
    pub follow_candidates: bool,
}

impl Default for ImpactOptions {
    fn default() -> Self {
        Self {
            max_depth: 3,
            kinds: Vec::new(),
            limit: 0,
            path_prefix: None,
            min_confidence: 0.0,
            provenances: Vec::new(),
            follow_candidates: true,
        }
    }
}

impl ImpactOptions {
    /// Whether a kind is included in this traversal's edge allow-list.
    pub fn accepts_kind(&self, kind: SourceEdgeKind) -> bool {
        if self.kinds.is_empty() {
            !matches!(kind, SourceEdgeKind::Contains | SourceEdgeKind::Imports)
        } else {
            self.kinds.contains(&kind)
        }
    }
}

/// Reverse-impact hits plus the numbers removed by [`ImpactOptions::limit`] and
/// [`ImpactOptions::path_prefix`].
#[derive(Debug, Clone, PartialEq)]
pub struct ImpactResult {
    pub hits: Vec<ImpactHit>,
    pub suppressed: usize,
    /// Hits `path_prefix` removed.
    pub filtered_out: usize,
}

type Reverse<'a> = BTreeMap<&'a str, Vec<Step<'a>>>;
type Nodes<'a> = BTreeMap<&'a str, &'a SourceNode>;
type Frontier<'a> = Vec<(&'a str, Option<Trust>)>;

#[cfg(test)]
thread_local! {
    /// Graph indexes built on this thread, so a test can prove a walk builds one.
    static INDEX_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl ImpactOptions {
    /// Graph indexes [`ImpactOptions::impact_from`] has built on this thread.
    pub(crate) fn index_builds() -> usize {
        INDEX_BUILDS.with(std::cell::Cell::get)
    }
}

/// Node lookup plus incoming steps keyed by target, so a level step does not
/// rescan the graph. An unresolved edge names no target worth walking back
/// from, so it is indexed only under the candidates it lists, and only when
/// they are followed. Building this dominates a walk on a large graph, which
/// is why one walk serves every start node.
fn index<'a>(graph: &'a ResolvedGraph, options: &ImpactOptions) -> (Nodes<'a>, Reverse<'a>) {
    #[cfg(test)]
    INDEX_BUILDS.with(|builds| builds.set(builds.get() + 1));
    let nodes = graph.nodes().map(|node| (node.id.as_str(), node)).collect();
    let mut reverse: Reverse<'a> = BTreeMap::new();
    for edge in graph.edges() {
        if !edge.is_unresolved() {
            let step = Step {
                edge,
                candidate: false,
            };
            reverse.entry(edge.to.as_str()).or_default().push(step);
        }
        if options.follow_candidates {
            for id in &edge.candidates {
                let step = Step {
                    edge,
                    candidate: true,
                };
                reverse.entry(id.as_str()).or_default().push(step);
            }
        }
    }
    (nodes, reverse)
}

/// Breadth-first state for one [`impact`] query.
struct Walk<'a> {
    starts: BTreeSet<&'a str>,
    hits: Vec<ImpactHit>,
    seen: BTreeMap<&'a str, usize>,
}

impl<'a> Walk<'a> {
    /// Step one level outward, returning the next frontier with each newly
    /// reached node's best-known trust.
    fn expand(
        &mut self,
        frontier: &Frontier<'a>,
        depth: usize,
        reverse: &Reverse<'a>,
        nodes: &Nodes<'a>,
        options: &ImpactOptions,
    ) -> Frontier<'a> {
        let mut discovered: Vec<&'a str> = Vec::new();
        for (id, carried) in frontier {
            for &step in reverse.get(*id).into_iter().flatten() {
                if !step_is_allowed(step, options) {
                    continue;
                }
                let from = step.edge.from.as_str();
                // A start node is the query's subject, never a result; skipping
                // it is also what makes a cycle back to it terminate.
                if self.starts.contains(from) {
                    continue;
                }
                let Some(&node) = nodes.get(from) else {
                    continue;
                };
                if self.record(node, depth, Trust::extend(*carried, step)) {
                    discovered.push(from);
                }
            }
        }
        // Read after the level is complete, so each node propagates its settled
        // best trust rather than whichever path happened to reach it first.
        let mut next: Frontier<'a> = Vec::new();
        for id in discovered {
            if let Some(hit) = self.hit(id) {
                next.push((id, Some(Trust::from(hit))));
            }
        }
        next
    }

    fn hit(&self, id: &str) -> Option<&ImpactHit> {
        self.seen.get(id).and_then(|index| self.hits.get(*index))
    }

    /// Emit `node` at `depth`, or improve the trust already recorded for it.
    /// Returns true only the first time the walk reaches it.
    fn record(&mut self, node: &'a SourceNode, depth: usize, trust: Trust) -> bool {
        if let Some(&index) = self.seen.get(node.id.as_str()) {
            if let Some(hit) = self.hits.get_mut(index) {
                // Reaching a node again at the same depth by a more trusted path
                // improves what we report but never re-expands it — that is what
                // bounds a cyclic graph.
                if hit.depth == depth && trust.min_confidence > hit.min_confidence {
                    hit.min_confidence = trust.min_confidence;
                    hit.weakest_provenance = trust.provenance;
                    hit.weakest_kind = trust.kind;
                    hit.via_candidates = trust.via_candidates;
                }
            }
            return false;
        }
        self.seen.insert(node.id.as_str(), self.hits.len());
        self.hits.push(ImpactHit {
            id: node.id.clone(),
            kind: node.kind,
            path: node.path.clone(),
            depth,
            min_confidence: trust.min_confidence,
            weakest_provenance: trust.provenance,
            weakest_kind: trust.kind,
            via_candidates: trust.via_candidates,
        });
        true
    }

    /// Nearest first, then most trusted, then by id, so output is stable.
    fn finish(mut self) -> Vec<ImpactHit> {
        self.hits.sort_by(|a, b| {
            a.depth
                .cmp(&b.depth)
                .then_with(|| b.min_confidence.total_cmp(&a.min_confidence))
                .then_with(|| a.id.cmp(&b.id))
        });
        self.hits
    }
}

/// Apply edge filters while expanding the graph. Filtering here, rather than
/// retaining hits afterward, prevents a rejected edge from becoming a bridge
/// to otherwise-matching nodes farther away.
fn step_is_allowed(step: Step<'_>, options: &ImpactOptions) -> bool {
    let edge = step.edge;
    let kind_allowed = options.accepts_kind(edge.kind);
    let provenance_allowed =
        options.provenances.is_empty() || options.provenances.contains(&edge.provenance);
    kind_allowed && provenance_allowed && step.confidence() >= options.min_confidence
}

impl ImpactOptions {
    /// Breadth-first reverse traversal from every one of `start_ids` at once,
    /// over one graph index. A hit's depth counts from its nearest start, no
    /// start is ever a hit, and the hit set is the union of the single-start
    /// walks' hits less the starts.
    pub fn impact_from(&self, graph: &ResolvedGraph, start_ids: &[&str]) -> ImpactResult {
        if self.max_depth == 0 {
            return bound(Vec::new(), self);
        }
        let (nodes, reverse) = index(graph, self);
        let mut walk = Walk {
            starts: start_ids.iter().copied().collect(),
            hits: Vec::new(),
            seen: BTreeMap::new(),
        };

        let mut frontier: Frontier<'_> = walk.starts.iter().map(|id| (*id, None)).collect();
        for depth in 1..=self.max_depth {
            let next = walk.expand(&frontier, depth, &reverse, &nodes, self);
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        bound(walk.finish(), self)
    }
}

/// Breadth-first reverse traversal with edge, path, confidence, and result bounds.
pub fn impact_with(graph: &ResolvedGraph, start_id: &str, options: &ImpactOptions) -> ImpactResult {
    options.impact_from(graph, &[start_id])
}

/// Apply [`ImpactOptions::path_prefix`] and then [`ImpactOptions::limit`] to
/// sorted hits, counting what each removed.
fn bound(mut hits: Vec<ImpactHit>, options: &ImpactOptions) -> ImpactResult {
    let before_filter = hits.len();
    if let Some(prefix) = &options.path_prefix {
        hits.retain(|hit| hit.path.to_string_lossy().starts_with(prefix));
    }
    let filtered_out = before_filter - hits.len();
    let suppressed = if options.limit > 0 && hits.len() > options.limit {
        let suppressed = hits.len() - options.limit;
        hits.truncate(options.limit);
        suppressed
    } else {
        0
    };
    ImpactResult {
        hits,
        suppressed,
        filtered_out,
    }
}

/// Compatibility traversal retaining the original all-edge-kinds semantics.
pub fn impact(graph: &ResolvedGraph, start_id: &str, max_depth: usize) -> Vec<ImpactHit> {
    let options = ImpactOptions {
        max_depth,
        kinds: vec![
            SourceEdgeKind::Contains,
            SourceEdgeKind::Imports,
            SourceEdgeKind::Calls,
            SourceEdgeKind::References,
            SourceEdgeKind::Implements,
            SourceEdgeKind::Extends,
        ],
        ..ImpactOptions::default()
    };
    impact_with(graph, start_id, &options).hits
}

#[cfg(test)]
#[path = "tests_impact.rs"]
mod tests;

#[cfg(test)]
#[path = "tests_impact_from.rs"]
mod tests_from;
