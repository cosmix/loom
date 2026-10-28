//! `loom knowledge eval` — score retrieval against a checked-in ground-truth
//! set using hit rate, ranking quality, delivery eligibility, and rendered cost.
//!
//! See `doc/PROPOSAL-retrieval-precision.md` §5 ("Measure it") and Appendix
//! A.20 for the design this module implements. The cases file
//! (`loom/eval/retrieval-cases.yaml` by default) pairs a query with relevant,
//! required, forbidden, or deliberately abstaining outcomes.
//!
//! `mode: stage` differs from `mode: prompt` in its default budget (3000 vs
//! 1500 tokens), in that `stage_fields` lines are newline-joined onto `query`
//! — the same shape `build_stage_query_text` assembles a real stage's
//! metadata into, just supplied by the case instead of a `Stage` record —
//! and in which pack `precision_at_5`/`relevant_token_fraction` judge:
//! `mode: stage` scores them over the raw retrieved pack, since an autonomous
//! stage spawn sees the best available retrieval regardless of the hook's
//! emit floor; `mode: prompt` scores them over the pack the hook would
//! actually hand a fresh session (0.0 when it would abstain), since that is
//! what a prompt-triggered brief actually delivers. hit@5, mrr, forbid
//! checks, mandatory recall, and rendered-token cost are always judged over
//! the raw pack — see `eval::metrics::score_case`.
//!
//! **This is a CLI gate, not a `cargo test`.** It reads the live on-disk
//! index (whatever `loom knowledge sync` last built), which is not
//! reproducible in CI. The unit tests exercise parsing, metrics, and gates
//! with synthetic packs; run this command by hand against a real index.

use crate::context::config::RetrievalConfig;
use crate::context::graph_store::GraphStore;
use crate::context::refresh::{ensure_snapshot, SnapshotAction, SnapshotOutcome, SnapshotPolicy};
use crate::context::retrieve::{retrieve_for_stage, StageQuery};
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

mod cases;
mod metrics;
mod report;

use cases::{build_query_text, load_cases_file, resolve_budget, EvalCase, CASES_RELATIVE_PATH};
use metrics::{score_case, CaseResult};
use report::{aggregate, exit_reason, print_human, print_json};

/// Run every case in `cases` (or the default cases file) and report the gate.
pub fn eval(cases: Option<PathBuf>, budget_tokens: Option<usize>, json: bool) -> Result<()> {
    let work_dir_hint = Path::new(".");
    let main_root = main_project_root(work_dir_hint)?;
    let cases_path = cases.unwrap_or_else(|| main_root.join(CASES_RELATIVE_PATH));
    let cases_file = load_cases_file(&cases_path)?;
    let config = RetrievalConfig::load(&main_root);

    refresh_source_graph(work_dir_hint);
    let results = cases_file
        .cases
        .iter()
        .map(|case| run_case(case, budget_tokens, work_dir_hint, &config))
        .collect::<Result<Vec<_>>>()?;
    let aggregates = aggregate(&results);
    let reason = exit_reason(
        &aggregates,
        cases_file.pass_floor,
        cases_file.precision_floor,
    );

    if json {
        print_json(&results, &cases_file, &aggregates, reason.as_deref())?;
    } else {
        print_human(&results, &cases_file, &aggregates, reason.as_deref());
    }

    if let Some(reason) = &reason {
        eprintln!("loom knowledge eval: FAIL - {reason}");
        std::process::exit(1);
    }
    Ok(())
}

fn main_project_root(work_dir_hint: &Path) -> Result<PathBuf> {
    let work_dir = WorkDir::new(work_dir_hint)?;
    work_dir.main_project_root().ok_or_else(|| {
        anyhow!("Could not resolve the main project root to locate {CASES_RELATIVE_PATH}")
    })
}

/// Bring the local source-graph snapshot up to date before any case runs, as
/// `loom map` does (`commands/map.rs::load_graph`). Retrieval never refreshes
/// the graph itself, so after a commit the local overlay is stale and the
/// source channel answers nothing. Advisory: a snapshot that cannot be built is
/// reported on stderr and the eval scores whatever retrieval returns.
///
/// Under a read-only cache the refresh's layers stay in memory, in the
/// `GraphStore` built here, while retrieval builds its own and reads the disk
/// (`retrieve/graph.rs`, `load_resolved_graph`). So the refresh helps only
/// where the cache is writable.
fn refresh_source_graph(work_dir_hint: &Path) {
    match ensure_local_snapshot(work_dir_hint) {
        Ok(outcome) if outcome.action != SnapshotAction::Reused => {
            eprintln!("{}", outcome.describe());
        }
        Ok(_) => {}
        Err(error) => eprintln!("source graph: unavailable ({error:#})"),
    }
}

fn ensure_local_snapshot(work_dir_hint: &Path) -> Result<SnapshotOutcome> {
    let work_dir = WorkDir::new(work_dir_hint)?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let store = ContextStore::open(&work_dir)?;
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    Ok(ensure_snapshot(
        &store,
        &graph_store,
        project_root,
        SnapshotPolicy::LocalCurrent,
    ))
}

fn run_case(
    case: &EvalCase,
    cli_budget: Option<usize>,
    work_dir_hint: &Path,
    config: &RetrievalConfig,
) -> Result<CaseResult> {
    let budget = resolve_budget(case, cli_budget);
    let mut query = StageQuery::new(work_dir_hint, build_query_text(case));
    query.required_ids = case.require_ids.clone();

    let pack = retrieve_for_stage(&query, budget)
        .with_context(|| format!("eval case '{}' failed retrieval", case.name))?;
    Ok(score_case(case, &pack, config))
}

#[cfg(test)]
#[path = "tests_eval.rs"]
mod tests;

#[cfg(test)]
#[path = "eval/tests_abstention.rs"]
mod tests_abstention;

#[cfg(test)]
#[path = "eval/tests_prompt_mode.rs"]
mod tests_prompt_mode;

#[cfg(test)]
#[path = "eval/tests_refresh.rs"]
mod tests_refresh;
