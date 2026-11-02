//! Read-only CLI views over an already-resolved source graph.
//!
//! Every view is a pure `render_*` / `*_json` pair; the command layer owns
//! reading windows, printing and exit codes, and `compose` assembles one
//! request's views and the invocation-wide footer.
//!
//! None of these views fail on an empty result — a view that finds nothing
//! still renders a plain sentence saying so, because "the graph has no
//! answer" is not an error condition for a CLI query.
//!
//! Confidence and provenance are never flattened away: every row that comes
//! from `crate::context::impact` renders its own confidence, provenance, and
//! edge kind rather than collapsing them into a single generic line. And
//! reachability is never overclaimed: an empty reverse-impact result names
//! what was left untraversed (unresolved edges, symbol-less files) instead of
//! reading as "provably nothing depends on this".

use std::path::{Path, PathBuf};

use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{file_node_id, EdgeProvenance, SourceEdgeKind};
use crate::context::{CoverageReport, ResolutionStats};

mod compose;
pub mod eval_edges;
mod filters;
mod find_all;
mod impact;
pub mod json;
mod matching;
mod neighbors;
mod outline;
pub mod snapshot;
pub mod timings;
mod window;

pub use compose::{human_text, human_views, json_payload, json_views, ViewContext, ViewQuery};
pub use filters::{parse_language, parse_provenance, ViewFilters};
pub use find_all::{find_all_json, render_find_all};
pub use impact::{impact_json, render_impact};
pub use json::{footer_json, MAP_SCHEMA};
pub use matching::MatchMode;
pub use neighbors::{
    callees_json, callers_json, references_json, render_callees, render_callers, render_references,
};
pub use outline::{outline_json, render_outline};
pub use window::{render_window, window_json};

/// Maximum number of ambiguous start definitions a view will expand.
const IMPACT_MAX_STARTS: usize = 5;

/// What every view reads besides its argument: the traversal bounds and the
/// filters. `limit` zero means unlimited.
#[derive(Debug, Clone)]
pub struct ViewOptions {
    pub depth: usize,
    pub kinds: Vec<SourceEdgeKind>,
    pub limit: usize,
    pub min_confidence: f32,
    pub provenances: Vec<EdgeProvenance>,
    pub filters: ViewFilters,
}

/// Turn a user-supplied path into the forward-slashed, project-root-relative
/// form the graph keys on. Resolves against the CURRENT DIRECTORY first, so a
/// path the user can `cat` is a path `--outline` accepts.
pub(crate) fn project_relative(project_root: &Path, arg: &str) -> Option<String> {
    let candidate = if Path::new(arg).is_absolute() {
        Some(PathBuf::from(arg))
    } else {
        std::env::current_dir().ok().map(|dir| dir.join(arg))
    };

    if let Some(candidate) = candidate {
        if let (Ok(candidate), Ok(root)) = (candidate.canonicalize(), project_root.canonicalize()) {
            if let Ok(relative) = candidate.strip_prefix(&root) {
                return Some(file_node_id(relative));
            }
        }
    }

    // The candidate could not be resolved and canonicalized (most commonly:
    // the path does not exist yet, or the cwd could not be read) - fall back
    // to normalizing the argument itself; it may already be graph-relative.
    let normalized = arg.replace('\\', "/");
    let normalized = normalized.strip_prefix("./").unwrap_or(&normalized);
    Some(normalized.to_string())
}

/// Render the invocation-wide coverage and resolution footer.
pub fn render_footer(graph: &ResolvedGraph, stats: &ResolutionStats) -> String {
    format!(
        "{}\nresolution: {} retargeted, {} ambiguous (left unresolved), {} unresolved",
        CoverageReport::of(graph),
        stats.retargeted,
        stats.ambiguous,
        stats.unresolved
    )
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_identity_impact;
#[cfg(test)]
mod tests_rows;
