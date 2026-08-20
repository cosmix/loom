//! `loom knowledge eval` — score retrieval against a checked-in ground-truth
//! set (precision@5 / MRR), so a ranking change can be judged instead of eyeballed.
//!
//! See `doc/PROPOSAL-retrieval-precision.md` §5 ("Measure it") and Appendix
//! A.20 for the design this module implements. The cases file
//! (`loom/eval/retrieval-cases.yaml` by default) pairs a query with the ids
//! [`retrieve_for_stage`] must (`expect`) or must never (`forbid`) return.
//!
//! `mode: stage` differs from `mode: prompt` only in its default budget (3000
//! vs 1500 tokens) and in that `stage_fields` lines are newline-joined onto
//! `query` — the same shape `build_stage_query_text` assembles a real stage's
//! metadata into (`orchestrator/signals/retrieval.rs:128-148`), just supplied
//! by the case instead of a `Stage` record.
//!
//! **This is a CLI gate, not a `cargo test`.** It reads the live on-disk
//! index (whatever `loom knowledge sync` last built), which is not
//! reproducible in CI, and today it deliberately exits 1 (several seeded
//! cases pin failure shapes Appendix A.1-A.6 have not fixed yet — see the
//! comment at the top of the cases file). Wire it into a plan or run it by
//! hand; never into the crate's automated test suite.

use crate::context::retrieve::{retrieve_for_stage, StageQuery};
use crate::context::untrusted::inline_safe;
use crate::fs::work_dir::WorkDir;
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Default token budget for a `mode: prompt` case, absent an override.
const DEFAULT_PROMPT_BUDGET_TOKENS: usize = 1500;
/// Default token budget for a `mode: stage` case, absent an override.
const DEFAULT_STAGE_BUDGET_TOKENS: usize = 3000;
/// Default cases file, relative to the main project root.
const CASES_RELATIVE_PATH: &str = "loom/eval/retrieval-cases.yaml";

/// A case's query mode: which default budget applies and whether
/// `stage_fields` gets folded into the query text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum EvalMode {
    #[default]
    Prompt,
    Stage,
}

/// One ground-truth case, deserialized straight off the YAML cases file.
#[derive(Debug, Clone, Deserialize)]
struct EvalCase {
    name: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    mode: EvalMode,
    #[serde(default)]
    budget_tokens: Option<usize>,
    #[serde(default)]
    stage_fields: Vec<String>,
    #[serde(default)]
    require_ids: Vec<String>,
    #[serde(default)]
    expect: Vec<String>,
    #[serde(default)]
    forbid: Vec<String>,
}

fn default_pass_floor() -> f32 {
    0.5
}

/// The whole cases file: an aggregate gate plus the cases it gates on.
#[derive(Debug, Deserialize)]
struct CasesFile {
    #[serde(default = "default_pass_floor")]
    pass_floor: f32,
    #[serde(default)]
    cases: Vec<EvalCase>,
}

/// One case's outcome. `counts_toward_precision` is false for a `forbid`-only
/// case (empty `expect`): there is no id `hit_at_5`/`mrr` could ever describe,
/// so such a case would otherwise drag precision@5 down forever, including
/// after the regression it pins is fixed. It still contributes its
/// `forbid_violations` to the aggregate gate.
struct CaseResult {
    name: String,
    counts_toward_precision: bool,
    hit_at_5: bool,
    mrr: f32,
    forbid_violations: Vec<String>,
}

/// Aggregate scores over every case in a run.
struct Aggregates {
    precision_at_5: f32,
    precision_hits: usize,
    precision_applicable: usize,
    mean_mrr: f32,
    forbid_violations: usize,
}

/// Run every case in `cases` (or the default cases file) and report the gate.
pub fn eval(cases: Option<PathBuf>, budget_tokens: Option<usize>, json: bool) -> Result<()> {
    let cases_path = match cases {
        Some(path) => path,
        None => default_cases_path()?,
    };
    let cases_file = load_cases_file(&cases_path)?;

    let work_dir_hint = Path::new(".");
    let mut results = Vec::with_capacity(cases_file.cases.len());
    for case in &cases_file.cases {
        results.push(run_case(case, budget_tokens, work_dir_hint)?);
    }

    let aggregates = aggregate(&results);
    let reason = exit_reason(&aggregates, cases_file.pass_floor);

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

/// `<main project root>/loom/eval/retrieval-cases.yaml`, resolved the way
/// [`crate::context::store::ContextStore::open`] resolves the context cache:
/// through [`WorkDir::main_project_root`], not the plain project root, so a
/// stage running inside a worktree still reads the ONE checked-in cases file
/// off the main checkout rather than a worktree branch that may predate it.
fn default_cases_path() -> Result<PathBuf> {
    let work_dir = WorkDir::new(".")?;
    let main_root = work_dir.main_project_root().ok_or_else(|| {
        anyhow!("Could not resolve the main project root to locate {CASES_RELATIVE_PATH}")
    })?;
    Ok(main_root.join(CASES_RELATIVE_PATH))
}

fn load_cases_file(path: &Path) -> Result<CasesFile> {
    let raw = std::fs::read_to_string(path).with_context(|| {
        format!(
            "No eval cases file at {} - pass --cases or create it",
            path.display()
        )
    })?;
    let parsed: CasesFile = serde_yaml::from_str(&raw)
        .with_context(|| format!("Failed to parse eval cases file {}", path.display()))?;
    validate_cases(&parsed)?;
    Ok(parsed)
}

/// Reject a cases file that would silently score nothing: a duplicate name
/// makes one case's result overwrite another's in any keyed report, an empty
/// query retrieves nothing meaningful, and a case with neither `expect` nor
/// `forbid` can never fail no matter what retrieval returns.
fn validate_cases(file: &CasesFile) -> Result<()> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for case in &file.cases {
        if !seen.insert(case.name.as_str()) {
            bail!("eval case '{}' is defined more than once", case.name);
        }
        if case.query.trim().is_empty() {
            bail!("eval case '{}' has an empty query", case.name);
        }
        if case.expect.is_empty() && case.forbid.is_empty() {
            bail!(
                "eval case '{}' has neither `expect` nor `forbid` - it could never fail, \
                 which makes it worse than no case at all",
                case.name
            );
        }
    }
    Ok(())
}

/// Fold `stage_fields` onto `query`, newline-joined, the way
/// `build_stage_query_text` (`orchestrator/signals/retrieval.rs:128-148`)
/// joins a stage's id/name/description/files/etc. into one query string.
/// `mode: prompt` never consults `stage_fields`.
fn build_query_text(case: &EvalCase) -> String {
    if case.mode != EvalMode::Stage || case.stage_fields.is_empty() {
        return case.query.clone();
    }
    let mut parts = vec![case.query.clone()];
    parts.extend(case.stage_fields.iter().cloned());
    parts.retain(|part| !part.trim().is_empty());
    parts.join("\n")
}

fn resolve_budget(case: &EvalCase, cli_override: Option<usize>) -> usize {
    if let Some(budget) = cli_override {
        return budget;
    }
    if let Some(budget) = case.budget_tokens {
        return budget;
    }
    match case.mode {
        EvalMode::Prompt => DEFAULT_PROMPT_BUDGET_TOKENS,
        EvalMode::Stage => DEFAULT_STAGE_BUDGET_TOKENS,
    }
}

fn run_case(
    case: &EvalCase,
    cli_budget: Option<usize>,
    work_dir_hint: &Path,
) -> Result<CaseResult> {
    let budget = resolve_budget(case, cli_budget);
    let mut query = StageQuery::new(work_dir_hint, build_query_text(case));
    query.required_ids = case.require_ids.clone();

    let pack = retrieve_for_stage(&query, budget)
        .with_context(|| format!("eval case '{}' failed retrieval", case.name))?;
    let item_ids: Vec<String> = pack
        .items
        .iter()
        .map(|item| item.id.as_str().to_string())
        .collect();
    Ok(score_case(case, &item_ids))
}

/// Pure scoring: takes the ordered item ids a pack returned, not the pack
/// itself, so tests can construct `item_ids` directly against a synthetic
/// ranking without building a real [`crate::context::ContextPack`].
fn score_case(case: &EvalCase, item_ids: &[String]) -> CaseResult {
    CaseResult {
        name: case.name.clone(),
        counts_toward_precision: !case.expect.is_empty(),
        hit_at_5: hit_at_5(&case.expect, item_ids),
        mrr: mrr(&case.expect, item_ids),
        forbid_violations: forbid_violations(&case.forbid, item_ids),
    }
}

/// True when any `expect` id is among the first 5 `item_ids`.
fn hit_at_5(expect: &[String], item_ids: &[String]) -> bool {
    item_ids
        .iter()
        .take(5)
        .any(|id| expect.iter().any(|wanted| wanted == id))
}

/// `1 / (1-based rank of the first expect id anywhere in item_ids)`, or `0.0`
/// when none of `expect` appears at all.
fn mrr(expect: &[String], item_ids: &[String]) -> f32 {
    for (index, id) in item_ids.iter().enumerate() {
        if expect.iter().any(|wanted| wanted == id) {
            return 1.0 / (index + 1) as f32;
        }
    }
    0.0
}

/// Every `forbid` id present anywhere in `item_ids`, in `forbid`'s order.
fn forbid_violations(forbid: &[String], item_ids: &[String]) -> Vec<String> {
    forbid
        .iter()
        .filter(|wanted| item_ids.iter().any(|id| id == *wanted))
        .cloned()
        .collect()
}

fn aggregate(results: &[CaseResult]) -> Aggregates {
    let applicable: Vec<&CaseResult> = results
        .iter()
        .filter(|result| result.counts_toward_precision)
        .collect();
    let hits = applicable.iter().filter(|result| result.hit_at_5).count();
    let mrr_sum: f32 = applicable.iter().map(|result| result.mrr).sum();
    let denominator = applicable.len().max(1) as f32;

    Aggregates {
        precision_at_5: hits as f32 / denominator,
        precision_hits: hits,
        precision_applicable: applicable.len(),
        mean_mrr: mrr_sum / denominator,
        forbid_violations: results.iter().map(|r| r.forbid_violations.len()).sum(),
    }
}

/// The reason a run fails, or `None` when it passes. Failure is precision@5
/// below the floor OR any forbid violation at all: A.20 names only the
/// precision floor, but a `forbid` entry that can never fail the run is not a
/// regression suite for the collision fixes (A.1-A.6) — it is decoration.
fn exit_reason(aggregates: &Aggregates, pass_floor: f32) -> Option<String> {
    let mut reasons = Vec::new();
    if aggregates.precision_at_5 < pass_floor {
        reasons.push(format!(
            "precision@5 {:.2} < pass_floor {:.2}",
            aggregates.precision_at_5, pass_floor
        ));
    }
    if aggregates.forbid_violations > 0 {
        reasons.push(format!(
            "{} forbid violation(s)",
            aggregates.forbid_violations
        ));
    }
    (!reasons.is_empty()).then(|| reasons.join("; "))
}

fn print_human(
    results: &[CaseResult],
    cases_file: &CasesFile,
    aggregates: &Aggregates,
    reason: Option<&str>,
) {
    for result in results {
        println!("{}", format_case_line(result));
        for violation in &result.forbid_violations {
            println!("          forbidden id present: {}", inline_safe(violation));
        }
    }
    println!();
    println!(
        "Aggregate: precision@5 = {:.2} ({}/{})  mean_mrr = {:.2}  forbid_violations = {}  pass_floor = {:.2}",
        aggregates.precision_at_5,
        aggregates.precision_hits,
        aggregates.precision_applicable,
        aggregates.mean_mrr,
        aggregates.forbid_violations,
        cases_file.pass_floor,
    );
    match reason {
        Some(reason) => println!("Result: FAIL - {reason}"),
        None => println!("Result: PASS"),
    }
}

fn format_case_line(result: &CaseResult) -> String {
    let status = if case_passed(result) { "PASS" } else { "FAIL" };
    let hit_display = if result.counts_toward_precision {
        if result.hit_at_5 {
            "yes".to_string()
        } else {
            "no".to_string()
        }
    } else {
        "n/a".to_string()
    };
    let mrr_display = if result.counts_toward_precision {
        format!("{:.2}", result.mrr)
    } else {
        "n/a".to_string()
    };
    format!(
        "  {:<4}  {:<40}  hit@5={:<3}  mrr={:<4}  forbid={}",
        status,
        inline_safe(&result.name),
        hit_display,
        mrr_display,
        result.forbid_violations.len(),
    )
}

/// Per-case display verdict: informational only, distinct from the aggregate
/// gate `eval` exits on. A case not counted toward precision (no `expect`)
/// passes on `forbid` alone.
fn case_passed(result: &CaseResult) -> bool {
    (!result.counts_toward_precision || result.hit_at_5) && result.forbid_violations.is_empty()
}

fn print_json(
    results: &[CaseResult],
    cases_file: &CasesFile,
    aggregates: &Aggregates,
    reason: Option<&str>,
) -> Result<()> {
    let payload = serde_json::json!({
        "cases": results.iter().map(case_json).collect::<Vec<_>>(),
        "precision_at_5": aggregates.precision_at_5,
        "precision_hits": aggregates.precision_hits,
        "precision_applicable": aggregates.precision_applicable,
        "mean_mrr": aggregates.mean_mrr,
        "forbid_violations": aggregates.forbid_violations,
        "pass_floor": cases_file.pass_floor,
        "exit_reason": reason,
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

fn case_json(result: &CaseResult) -> serde_json::Value {
    serde_json::json!({
        "name": result.name,
        "hit_at_5": result.counts_toward_precision.then_some(result.hit_at_5),
        "mrr": result.counts_toward_precision.then_some(result.mrr),
        "forbid_violations": result.forbid_violations,
    })
}

#[cfg(test)]
#[path = "tests_eval.rs"]
mod tests;
