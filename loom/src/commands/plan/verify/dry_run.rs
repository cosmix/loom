//! `loom plan verify --dry-run`: run every acceptance, `after_stage` and
//! `wiring_tests` command of the plan in one checkout of the committed HEAD,
//! under the confinement the daemon resolves for each, and report what each
//! result says about the criterion.
//!
//! The checkout lives under `<repository>/.loom/dry-run/`, on the repository's
//! disk: a cold build of every target is tens of gigabytes, too much for a
//! RAM-backed temporary directory. It holds tracked files only, so the working
//! tree, staged changes and untracked files are never part of the run. The
//! plan's `provision` entries run in it first, as `loom run` runs them in a
//! stage worktree. A command that times out runs once more in the same
//! checkout, whose build cache the first run warmed; a second timeout is an
//! error.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::dry_run_classify::classify_with;
pub use super::dry_run_classify::{
    classify, settle_timeout, severity, stage_cannot_tell, DryOutcome, DrySeverity,
};
use super::dry_run_exec;
use super::report::{parse_metadata, plan_repo_root, VerifyError, VerifyReport};
use crate::git::runner::run_git_checked;
use crate::models::stage::{
    AcceptanceCriterion, CommandConfinement, PlanIdentity, Stage, StageType,
};
use crate::orchestrator::provision::run_provision;
use crate::plan::schema::{detect_stage_type, StageDefinition};
use crate::verify::criteria::{
    extended_criterion_timeout, has_test_summary, resolve_confinement, CriterionResult,
    DEFAULT_CHECK_TIMEOUT, DEFAULT_COMMAND_TIMEOUT,
};
use crate::verify::goal_backward::wiring_tests::WIRING_TEST_TIMEOUT;
use crate::verify::impact_tests::base::BaseCheckout;

/// Longest command text a finding quotes.
const MAX_QUOTED: usize = 120;

/// One command of the plan, ready to run in the checkout.
#[derive(Debug, Clone)]
pub struct DryCommand {
    pub stage_id: String,
    /// `acceptance`, `after_stage` or `wiring_tests`.
    pub field: &'static str,
    /// 0-based position in the field's list.
    pub index: usize,
    pub command: String,
    /// The stage's working directory inside the checkout.
    pub dir: PathBuf,
    pub timeout: std::time::Duration,
    /// The checkout's top directory.
    pub tree_root: PathBuf,
    /// The stage's override, else the plan's `sandbox.command_confinement`.
    pub confinement: CommandConfinement,
}

/// Runs one command and returns its result with the failures the runner found.
pub type DryExecutor<'a> = dyn FnMut(&DryCommand) -> (CriterionResult, Vec<String>) + 'a;

/// Run the plan at `path` with the real, confined executor.
pub fn run_dry_run(path: &Path, report: &mut VerifyReport) -> Result<()> {
    dry_run_exec::run(path, report)
}

/// Everything `--dry-run` does except executing a command, which `execute` does.
pub fn run_dry_run_with(
    path: &Path,
    report: &mut VerifyReport,
    execute: &mut DryExecutor<'_>,
) -> Result<()> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let Some(metadata) = parse_metadata(&text) else {
        return skip(report, "the plan metadata does not parse");
    };
    let (root, head) = match committed_head(path) {
        Ok(found) => found,
        Err(reason) => return skip(report, &reason),
    };
    let stages = stages_of(path, &metadata.loom);
    let parent = root.join(".loom").join("dry-run");
    std::fs::create_dir_all(&parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;
    let checkout = BaseCheckout::create_in(&parent, &root, &head)
        .context("dry run: failed to check out the committed HEAD")?;
    if let Err(reason) = run_provision(&metadata.loom.provision, checkout.root()) {
        let reason = cut(&reason, 300);
        let message = format!("dry run: provision failed in the base checkout: {reason}");
        report.errors.push(VerifyError {
            stage_id: None,
            message,
        });
        report.valid = false;
        return Ok(());
    }
    let plan_confinement = metadata.loom.sandbox.command_confinement;
    for (definition, stage) in metadata.loom.stages.iter().zip(&stages) {
        let confinement =
            resolve_confinement(stage.sandbox.command_confinement, Some(plan_confinement));
        let commands = stage_commands(definition, confinement, checkout.root());
        run_stage(definition, &commands, execute, report);
    }
    report.valid = report.errors.is_empty();
    Ok(())
}

/// The runtime stages of the plan at `path`, built as `loom run` builds them.
pub(super) fn stages_of(path: &Path, loom: &crate::plan::schema::LoomConfig) -> Vec<Stage> {
    let id = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let identity = PlanIdentity {
        id: &id,
        version: loom.version,
        ratchet_files: &loom.ratchet_files,
    };
    loom.stages
        .iter()
        .map(|definition| Stage::from_definition(definition, &identity))
        .collect()
}

pub(super) fn skip(report: &mut VerifyReport, reason: &str) -> Result<()> {
    report.notes.push(format!("dry run skipped: {reason}"));
    Ok(())
}

/// The repository root of the plan and the commit its first stages branch from.
fn committed_head(path: &Path) -> Result<(PathBuf, String), String> {
    let root = plan_repo_root(path)
        .map_err(|failure| failure.message)?
        .ok_or("the plan is not inside a git repository")?;
    let head = run_git_checked(&["rev-parse", "HEAD"], &root)
        .map_err(|_| "the repository has no commit".to_string())?;
    Ok((root, head.trim().to_string()))
}

/// The commands of `definition` in run order: acceptance, `after_stage`, `wiring_tests`.
fn stage_commands(
    definition: &StageDefinition,
    confinement: CommandConfinement,
    root: &Path,
) -> Vec<DryCommand> {
    let dir = root.join(&definition.working_dir);
    let make = |field, index, command: &str, timeout| DryCommand {
        stage_id: definition.id.clone(),
        field,
        index,
        command: command.to_string(),
        dir: dir.clone(),
        timeout,
        tree_root: root.to_path_buf(),
        confinement,
    };
    let acceptance = definition
        .acceptance
        .iter()
        .enumerate()
        .map(|(i, criterion)| {
            let timeout = match criterion {
                AcceptanceCriterion::Simple(_) => DEFAULT_COMMAND_TIMEOUT,
                AcceptanceCriterion::Extended(check) => extended_criterion_timeout(check),
            };
            make("acceptance", i, criterion.command(), timeout)
        });
    let after_stage = definition.after_stage.iter().enumerate().map(|(i, check)| {
        make(
            "after_stage",
            i,
            &check.command,
            check.timeout(DEFAULT_CHECK_TIMEOUT),
        )
    });
    let wiring_tests = definition
        .wiring_tests
        .iter()
        .enumerate()
        .map(|(i, test)| make("wiring_tests", i, &test.command, WIRING_TEST_TIMEOUT));
    acceptance.chain(after_stage).chain(wiring_tests).collect()
}

fn run_stage(
    definition: &StageDefinition,
    commands: &[DryCommand],
    execute: &mut DryExecutor<'_>,
    report: &mut VerifyReport,
) {
    let has_dependencies = !definition.dependencies.is_empty();
    let mut acceptance = Vec::new();
    for command in commands {
        let outcome = settled(execute, command);
        if command.field == "acceptance" {
            acceptance.push(outcome.clone());
        }
        record(report, definition, command, &outcome);
    }
    // A knowledge stage's checks pass at base by design.
    let standard = detect_stage_type(definition) == StageType::Standard;
    if standard && stage_cannot_tell(&acceptance, has_dependencies) {
        report.warnings.baseline.push(format!(
            "dry run: stage `{}` passes every acceptance criterion at the base commit, so no \
             criterion can tell a stage that did its work from one that did nothing",
            definition.id
        ));
    }
}

/// The outcome of `command`, run a second time when the first run timed out.
fn settled(execute: &mut DryExecutor<'_>, command: &DryCommand) -> DryOutcome {
    let label = format!(
        "dry run: {} {} {}",
        command.stage_id, command.field, command.index
    );
    eprintln!("{label} `{}`", cut(&command.command, MAX_QUOTED));
    let first = outcome_of(execute, command);
    settle_timeout(first, &mut || {
        eprintln!("{label} timed out; running it once more");
        outcome_of(execute, command)
    })
}

fn outcome_of(execute: &mut DryExecutor<'_>, command: &DryCommand) -> DryOutcome {
    let (result, failures) = execute(command);
    let has_summary = has_test_summary(&command.command, &command.dir, &result);
    classify_with(&result, &failures, has_summary)
}

fn record(
    report: &mut VerifyReport,
    definition: &StageDefinition,
    command: &DryCommand,
    outcome: &DryOutcome,
) {
    let has_dependencies = !definition.dependencies.is_empty();
    // A dependency-free stage may create the program or script itself.
    let covered = match outcome {
        DryOutcome::Missing(_) if !has_dependencies => covering_token(definition, &command.command),
        _ => None,
    };
    let level = match covered {
        Some(_) => Some(DrySeverity::Note),
        None => severity(outcome, has_dependencies),
    };
    let what = describe(outcome, command, has_dependencies, covered.as_deref());
    let message = format!(
        "dry run: {} {} {} `{}` {what}",
        command.stage_id,
        command.field,
        command.index,
        cut(&command.command, MAX_QUOTED)
    );
    match level {
        Some(DrySeverity::Error) => report.errors.push(VerifyError {
            stage_id: Some(command.stage_id.clone()),
            message,
        }),
        Some(DrySeverity::Note) => report.notes.push(message),
        Some(DrySeverity::Warning) | None => {}
    }
}

fn describe(
    outcome: &DryOutcome,
    command: &DryCommand,
    has_dependencies: bool,
    covered: Option<&str>,
) -> String {
    match outcome {
        DryOutcome::Environment(marker) => {
            format!("hit an environment failure ({marker}) at the base commit")
        }
        DryOutcome::Missing(marker) => {
            let start = format!("cannot start at the base commit ({marker})");
            match covered {
                Some(token) => {
                    format!("{start}; the stage's own files or artifacts cover `{token}`")
                }
                None if has_dependencies => format!("{start}; an earlier stage may provide it"),
                None => format!("{start} and the stage has no dependency that could provide it"),
            }
        }
        DryOutcome::TimedOut => format!(
            "timed out at the base commit after {}s, also on a second run in the same checkout",
            command.timeout.as_secs()
        ),
        DryOutcome::ZeroTests => {
            "selects zero tests at the base commit (expected when the plan adds them)".to_string()
        }
        DryOutcome::Passed => "already passes at the base commit".to_string(),
        DryOutcome::Failed => {
            "fails at the base commit (expected before the stage's work)".to_string()
        }
    }
}

/// The word of `command` (a leading `./` stripped) that one of the stage's
/// `files:` globs or `artifacts:` paths names.
fn covering_token(definition: &StageDefinition, command: &str) -> Option<String> {
    command
        .split_whitespace()
        .map(|word| word.strip_prefix("./").unwrap_or(word))
        .find(|token| {
            let artifact = definition.artifacts.iter().any(|path| path == token);
            artifact
                || definition
                    .files
                    .iter()
                    .any(|glob| glob::Pattern::new(glob).is_ok_and(|p| p.matches(token)))
        })
        .map(str::to_string)
}

/// `text` on one line, cut to `limit` characters.
pub(super) fn cut(text: &str, limit: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= limit {
        return line;
    }
    let head: String = line.chars().take(limit).collect();
    format!("{head}...")
}

#[cfg(test)]
#[path = "dry_run_real_tests.rs"]
mod dry_run_real_tests;
#[cfg(test)]
#[path = "dry_run_tests.rs"]
mod dry_run_tests;
