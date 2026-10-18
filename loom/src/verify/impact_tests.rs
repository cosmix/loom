//! Impact-selected tests at stage completion (DESIGN D14).
//!
//! A v2 `standard` stage runs the tests that reach what it changed, so a
//! regression outside its contracts shows before integration-verify runs the
//! full suite. Every node in a changed file is walked backwards through the
//! worktree graph, starting from every node in a file the stage changed since
//! its merge base with the target branch (the graph itself is still layered on
//! the nearest published base); the nodes met in test files through bound
//! edges, and the changed test nodes themselves, become runner targets grouped
//! by the runner detected for their package. A test file met only through an
//! ambiguous call's candidate set is a note. `runs` runs each runner's
//! selection through the acceptance-criteria runner, so an identical earlier
//! run is reused from the certified cache. A runner that cannot select, a
//! package missing the installed environment its adapter declares
//! (`TestRunnerAdapter::required_environment`), a runner that exits 127, a
//! timeout, or no reached test leaves a note: the full suite still runs in
//! integration-verify.
//!
//! A selection that fails is judged further (`report`): one a dispute of the
//! stage accepted (`accepted`) is a note while it fails exactly as accepted,
//! and one whose tests fail at the stage's merge base (`base`) with at least
//! as many failures counted is a pre-existing failure and a note. The rest fail the
//! gate with [`ImpactTestsFailed`], each named by the id
//! `loom stage dispute-criteria --field impact-tests --test <id>` disputes;
//! the completion records them (`observed`) for that dispute to name.

mod accepted;
mod base;
mod failure;
mod libtest;
mod observed;
mod reach;
mod report;
mod runs;

use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{file_node_id, SourceNode, SourceNodeKind};
use crate::context::worktree_graph::{build_for_worktree, WorktreeGraph};
use crate::git::runner::run_git_checked;
use crate::models::stage::{CommandConfinement, Stage};
use crate::skills::project::ProjectProfile;
use crate::testrun::{registry, SelectionNames, TestRunnerAdapter, TestTarget};
use crate::verify::contracts::{normalize, owning_package};
use crate::verify::criteria::{CriteriaConfig, CriteriaProbe, ProbeRunner};
use crate::verify::review::fingerprint::{changes_since_merge_base, local_git};

pub(in crate::verify) use accepted::ACCEPTED_FILE;
pub use accepted::{load_accepted, AcceptedFailure, AcceptedImpact};
pub use failure::{check_failure_id, FailureKind, ImpactFailure};
pub use observed::{load_observed, record_observed, ObservedImpact};
pub use report::ImpactTestsFailed;

use base::{BaseCriteria, BaseProbe};
use libtest::LibtestPaths;
use reach::{reached_test_nodes, TestNodes};
use report::Report;
use runs::Selection;

/// How long one selection may run (DESIGN D14).
const IMPACT_TIMEOUT: Duration = Duration::from_secs(300);

const FULL_SUITE: &str = "the full suite runs in integration-verify";

/// The impact-selected tests of a completing stage, when none failed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImpactOutcome {
    /// Selection commands that ran and passed.
    pub ran: Vec<String>,
    /// Reached tests left unrun and runs that proved nothing, each with why.
    pub notes: Vec<String>,
}

/// The criteria configuration selections run under: the plan's confinement,
/// the pass cache in the state directory `work_dir`, and all of the stage
/// worktree `tree_root` visible, as acceptance has it (a package uses its
/// siblings and root stores).
pub fn criteria_config(
    work_dir: &Path,
    plan_confinement: Option<CommandConfinement>,
    tree_root: Option<&Path>,
) -> CriteriaConfig {
    CriteriaConfig::default()
        .with_plan_confinement(plan_confinement)
        .with_cache_dir(work_dir)
        .with_tree_root(tree_root)
}

/// Run the tests that reach what the stage changed since its merge base with
/// `target_branch`, in the worktree holding `working_dir`, judging failures
/// against the disputes the state directory `work_dir` records. Fails with
/// [`ImpactTestsFailed`] naming each failure left; the caller gates this on a
/// v2 `standard` stage. Each selection always runs with its own 300 s
/// timeout, whatever `criteria_config` sets.
pub fn run(
    stage: &Stage,
    work_dir: &Path,
    working_dir: &Path,
    criteria_config: &CriteriaConfig,
    target_branch: &str,
) -> Result<ImpactOutcome> {
    check(stage, work_dir, working_dir, criteria_config, target_branch)?.into_outcome(&stage.id)
}

fn check(
    stage: &Stage,
    work_dir: &Path,
    working_dir: &Path,
    criteria_config: &CriteriaConfig,
    target_branch: &str,
) -> Result<Report> {
    let graph =
        build_for_worktree(working_dir).context("failed to build the worktree source graph")?;
    let root = PathBuf::from(
        run_git_checked(&["rev-parse", "--show-toplevel"], working_dir)
            .context("failed to find the worktree root")?,
    );
    let changes = Changes::list(&root, &graph, target_branch);
    let config = CriteriaConfig {
        command_timeout: IMPACT_TIMEOUT,
        ..criteria_config.clone()
    };
    let runner = CriteriaProbe {
        stage,
        config: config.clone(),
    };
    let base_runner = BaseCriteria { stage, config };
    let check = Check {
        stage,
        working_dir,
        runner: &runner,
        base: BaseProbe {
            worktree: &root,
            commit: changes.base,
            runner: &base_runner,
        },
        accepted: load_accepted(work_dir, &stage.id)?,
    };
    let mut report = run_with(&check, &graph, &changes.paths)?;
    if let Some(note) = changes.note {
        report.outcome.notes.push(note);
        report.outcome.notes.sort();
    }
    Ok(report)
}

/// Worktree-relative paths the stage changed since `git merge-base HEAD <target_branch>`,
/// sorted: committed, staged, unstaged and untracked-not-ignored.
pub fn stage_changes(worktree: &Path, target_branch: &str) -> Result<Vec<PathBuf>> {
    Ok(merge_base_changes(worktree, target_branch)?.1)
}

/// The merge base [`stage_changes`] measures from, and those changes.
fn merge_base_changes(worktree: &Path, target_branch: &str) -> Result<(String, Vec<PathBuf>)> {
    let (base, paths) = changes_since_merge_base(&local_git(worktree)?, target_branch)?;
    Ok((base, paths.into_iter().map(PathBuf::from).collect()))
}

/// What the stage changed since its merge base with the target branch.
struct Changes {
    /// The merge base, or why it could not be found.
    base: Result<String, String>,
    paths: Vec<PathBuf>,
    /// Why the listing fell back to the graph's own changes, if it did.
    note: Option<String>,
}

impl Changes {
    /// The [`stage_changes`] of the worktree at `root`. When they cannot be
    /// listed, the files changed since the graph's nearest published base,
    /// with a note.
    fn list(root: &Path, graph: &WorktreeGraph, target_branch: &str) -> Self {
        match merge_base_changes(root, target_branch) {
            Ok((base, paths)) => Self {
                base: Ok(base),
                paths,
                note: None,
            },
            Err(error) => Self {
                base: Err(format!("{error:#}")),
                paths: graph.changed.clone(),
                note: Some(format!(
                    "the stage's changes since its merge base with `{target_branch}` could not \
                     be listed ({error:#}); selection uses the files changed since the nearest \
                     published base"
                )),
            },
        }
    }
}

/// One impact check: the stage, where its selections run, and what judges a
/// failure beyond its own run.
struct Check<'a> {
    stage: &'a Stage,
    working_dir: &'a Path,
    runner: &'a dyn ProbeRunner,
    base: BaseProbe<'a>,
    accepted: AcceptedImpact,
}

fn run_with(check: &Check<'_>, graph: &WorktreeGraph, changed: &[PathBuf]) -> Result<Report> {
    let profile = ProjectProfile::discover(check.working_dir);
    let contracts = contract_files(check.stage, check.working_dir, &profile.root)?;
    let reached = reached_test_nodes(&graph.graph, changed, &contracts);
    let mut notes: BTreeSet<String> = graph
        .degraded
        .iter()
        .map(|reason| format!("the source graph is degraded: {reason}"))
        .collect();
    if reached.selected.is_empty() {
        notes.insert(format!(
            "no test reaches the files this stage changed through bound edges; {FULL_SUITE}"
        ));
    }
    notes.extend(reached.namesakes.iter().map(|file| {
        format!(
            "tests in {file} are not selected: they reach this stage's changes only through an \
             ambiguous call matched by name; {FULL_SUITE}"
        )
    }));
    let groups = group_targets(&graph.graph, &profile, &reached.selected, &mut notes);
    let mut selection = Selection::new(&profile.root, check.runner, notes);
    for ((package, _), (adapter, targets)) in &groups {
        selection.run_group(package, *adapter, targets);
    }
    Ok(Report::judge(selection, &check.accepted, &check.base))
}

/// The stage's contract files, checkout-relative: the contract check ran them.
fn contract_files(stage: &Stage, working_dir: &Path, root: &Path) -> Result<BTreeSet<String>> {
    let working = working_dir
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", working_dir.display()))?;
    let inside = working
        .strip_prefix(root)
        .context("the working directory is outside its checkout")?;
    Ok(stage
        .contracts
        .iter()
        .map(|contract| file_node_id(&inside.join(normalize(&contract.file))))
        .collect())
}

/// Targets keyed by package directory and runner name, with that runner.
type Groups = BTreeMap<(PathBuf, &'static str), (&'static dyn TestRunnerAdapter, Vec<TestTarget>)>;

/// Reached nodes as runner targets, grouped by the runner of their package.
fn group_targets(
    graph: &ResolvedGraph,
    profile: &ProjectProfile,
    reached: &TestNodes<'_>,
    notes: &mut BTreeSet<String>,
) -> Groups {
    let packages = profile.package_details();
    let mut paths = LibtestPaths::new(graph, &profile.root);
    let mut groups = Groups::new();
    for node in reached.values() {
        let file = file_node_id(&node.path);
        let owner = owning_package(&packages, Path::new(&file))
            .and_then(|package| Some((package, registry::by_name(package.runner?)?)));
        let Some((package, adapter)) = owner else {
            notes.insert(format!(
                "no test runner is detected for the package of {file}; {FULL_SUITE}"
            ));
            continue;
        };
        let Some(target) = target_of(&mut paths, &package.path, adapter, node, notes) else {
            continue;
        };
        let (_, targets) = groups
            .entry((package.path.clone(), adapter.name()))
            .or_insert_with(|| (adapter, Vec::new()));
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    groups
}

/// What `node` selects: its file for a runner selecting by file, a function's
/// libtest path for one selecting by libtest path
/// (`TestRunnerAdapter::selection_names`). `None` when such a runner cannot
/// name it, with a note when it is a function.
fn target_of(
    paths: &mut LibtestPaths<'_>,
    package: &Path,
    adapter: &dyn TestRunnerAdapter,
    node: &SourceNode,
    notes: &mut BTreeSet<String>,
) -> Option<TestTarget> {
    let file = file_node_id(node.path.strip_prefix(package).ok()?);
    let SelectionNames::LibtestPaths { is_crate_root } = adapter.selection_names() else {
        return Some(TestTarget { file, name: None });
    };
    if node.kind != SourceNodeKind::Function {
        return None;
    }
    let Some(name) = paths.of(node, package, is_crate_root) else {
        notes.insert(format!(
            "`{}` is not selected: its libtest path cannot be derived from the module \
             declarations in the source graph",
            node.id
        ));
        return None;
    };
    Some(TestTarget {
        file,
        name: Some(name),
    })
}

#[cfg(test)]
#[path = "impact_tests_tests.rs"]
mod tests;
