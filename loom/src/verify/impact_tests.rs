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
//! ambiguous call's candidate set is a note. `runs` runs each runner's selection through the
//! acceptance-criteria runner, so an identical earlier run is reused from the
//! certified cache. A runner that cannot select, a JS package without
//! `node_modules` or a uv package without `.venv`, a runner that exits 127, a timeout, or no reached test
//! leaves a note: the full suite still runs in integration-verify.

mod libtest;
mod runs;

use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::context::graph_store::ResolvedGraph;
use crate::context::resolve::{impact_with, ImpactOptions};
use crate::context::source_graph::{
    file_node_id, SourceNode, SourceNodeKind, UNIQUE_NAME_CONFIDENCE,
};
use crate::context::worktree_graph::{build_for_worktree, WorktreeGraph};
use crate::git::runner::run_git_checked;
use crate::models::stage::Stage;
use crate::skills::project::ProjectProfile;
use crate::testrun::{languages, registry, TestRunnerAdapter, TestTarget};
use crate::verify::contracts::{normalize, owning_package};
use crate::verify::criteria::{CriteriaConfig, CriteriaProbe, ProbeRunner};
use crate::verify::goal_backward::reachable::REACHABLE_KINDS;
use crate::verify::review::fingerprint::{changes_since_merge_base, local_git};

use libtest::LibtestPaths;
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

/// Worktree-relative paths the stage changed since `git merge-base HEAD <target_branch>`,
/// sorted: committed, staged, unstaged and untracked-not-ignored.
pub fn stage_changes(worktree: &Path, target_branch: &str) -> Result<Vec<PathBuf>> {
    let (_, paths) = changes_since_merge_base(&local_git(worktree)?, target_branch)?;
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

/// Run the tests that reach what the stage changed since its merge base with
/// `target_branch`, in the worktree holding `working_dir`. Fails naming each
/// selection that fails or does not build; the caller gates this on a v2
/// `standard` stage.
/// Each selection always runs with its own 300 s timeout, whatever `criteria_config` sets.
pub fn run(
    stage: &Stage,
    working_dir: &Path,
    criteria_config: &CriteriaConfig,
    target_branch: &str,
) -> Result<ImpactOutcome> {
    let graph =
        build_for_worktree(working_dir).context("failed to build the worktree source graph")?;
    let root = PathBuf::from(
        run_git_checked(&["rev-parse", "--show-toplevel"], working_dir)
            .context("failed to find the worktree root")?,
    );
    let (changed, listing_note) = match stage_changes(&root, target_branch) {
        Ok(changed) => (changed, None),
        Err(error) => (
            graph.changed.clone(),
            Some(format!(
                "the stage's changes since its merge base with `{target_branch}` could not be \
                 listed ({error:#}); selection uses the files changed since the nearest \
                 published base"
            )),
        ),
    };
    let runner = CriteriaProbe {
        stage,
        config: CriteriaConfig {
            command_timeout: IMPACT_TIMEOUT,
            ..criteria_config.clone()
        },
    };
    let mut outcome = run_with(stage, working_dir, &graph, &changed, &runner)?;
    if let Some(note) = listing_note {
        outcome.notes.push(note);
        outcome.notes.sort();
    }
    Ok(outcome)
}

fn run_with(
    stage: &Stage,
    working_dir: &Path,
    graph: &WorktreeGraph,
    changed: &[PathBuf],
    runner: &dyn ProbeRunner,
) -> Result<ImpactOutcome> {
    let profile = ProjectProfile::discover(working_dir);
    let contracts = contract_files(stage, working_dir, &profile.root)?;
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
    let mut selection = Selection::new(&profile.root, runner, notes);
    for ((package, _), (adapter, targets)) in &groups {
        selection.run_group(package, *adapter, targets);
    }
    selection.finish(&stage.id)
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

/// Test nodes keyed by id.
type TestNodes<'g> = BTreeMap<&'g str, &'g SourceNode>;

/// The test nodes a stage's change reaches, split by the evidence of the path.
struct Reached<'g> {
    /// Reached through bound edges only, or changed themselves: these run.
    selected: TestNodes<'g>,
    /// Test files with a node reached only through a step into an ambiguous
    /// call's candidate set: noted, never run.
    namesakes: BTreeSet<String>,
}

/// Every node in a test file that reaches a node of a changed file, or is
/// one, contract files excluded. A test selects only when every step of its
/// path is a bound edge; one reached only by guessing which namesake an
/// ambiguous call means is a namesake note.
fn reached_test_nodes<'g>(
    graph: &'g ResolvedGraph,
    changed: &[PathBuf],
    contracts: &BTreeSet<String>,
) -> Reached<'g> {
    let walk = |options: ImpactOptions| {
        let options = ImpactOptions {
            max_depth: usize::MAX,
            kinds: REACHABLE_KINDS.to_vec(),
            ..options
        };
        walk_tests(graph, changed, contracts, &options)
    };
    // A candidate step trusts AMBIGUOUS_CANDIDATE_CONFIDENCE (0.2) and an
    // unbound syntax edge has no target to walk, so the weakest step left is
    // a unique-name bind. A test failing in a package the stage never touched,
    // reached through `validate` matching some other `validate`, would block
    // the stage on a guess; the full suite in integration-verify still runs it.
    let selected = walk(ImpactOptions {
        min_confidence: UNIQUE_NAME_CONFIDENCE,
        follow_candidates: false,
        ..ImpactOptions::default()
    });
    let namesakes = walk(ImpactOptions::default())
        .into_iter()
        .filter(|(id, _)| !selected.contains_key(id))
        .map(|(_, node)| file_node_id(&node.path))
        .collect();
    Reached {
        selected,
        namesakes,
    }
}

/// The test nodes of a changed file, and those reaching a node of a changed
/// file through `options`' edges.
fn walk_tests<'g>(
    graph: &'g ResolvedGraph,
    changed: &[PathBuf],
    contracts: &BTreeSet<String>,
    options: &ImpactOptions,
) -> TestNodes<'g> {
    let is_test = |file: &str| languages::for_path(file).is_some() && !contracts.contains(file);
    // A node an earlier walk reached is not walked again: with no depth limit
    // and a per-step filter, everything reaching it was reached by that walk too.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut reached = TestNodes::new();
    for file in changed.iter().map(|path| file_node_id(path)) {
        let Some(entry) = graph.files.get(&file) else {
            continue;
        };
        for node in &entry.nodes {
            if is_test(&file) {
                reached.insert(node.id.as_str(), node);
            }
            if !seen.insert(node.id.clone()) {
                continue;
            }
            for hit in impact_with(graph, &node.id, options).hits {
                let path = file_node_id(&hit.path);
                if seen.insert(hit.id.clone()) && is_test(&path) {
                    let found = node_in(graph, &path, &hit.id);
                    reached.extend(found.map(|found| (found.id.as_str(), found)));
                }
            }
        }
    }
    reached
}

/// Node `id` of `file`.
fn node_in<'g>(graph: &'g ResolvedGraph, file: &str, id: &str) -> Option<&'g SourceNode> {
    graph
        .files
        .get(file)?
        .nodes
        .iter()
        .find(|node| node.id == id)
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
/// libtest path for cargo. `None` when cargo cannot name it, with a note when
/// it is a function.
fn target_of(
    paths: &mut LibtestPaths<'_>,
    package: &Path,
    adapter: &dyn TestRunnerAdapter,
    node: &SourceNode,
    notes: &mut BTreeSet<String>,
) -> Option<TestTarget> {
    let file = file_node_id(node.path.strip_prefix(package).ok()?);
    if adapter.language() != "rust" {
        return Some(TestTarget { file, name: None });
    }
    if node.kind != SourceNodeKind::Function {
        return None;
    }
    let Some(name) = paths.of(node, package) else {
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
