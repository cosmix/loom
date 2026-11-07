//! The rules a writer's choices pass before the daemon saves them.
//!
//! A chosen test, runner or directory moves where and how a host job runs a
//! command, so each is checked against the plan first. Not every check reads
//! the same tree:
//!
//! - The stage's base commit, through git: the run-root marker a `--cwd`
//!   must hold (`ls-tree` at the base). A marker the writer planted since the
//!   base never counts.
//! - The live worktree: adapter detection for a `--test` (`resolve_adapter`),
//!   the canonical working directory (`Place::new`), and the live `--cwd`
//!   directory, which must be a real directory and no symlink. A file the
//!   writer planted can satisfy the adapter check here, so the freeze job
//!   checks again on its own tree (`revalidate_saved`), and again on the read
//!   that decides how the contract runs (`host_verify::freeze_prepare`);
//!   `resolve_run` then refuses a run directory that resolves outside the
//!   worktree (`inside`).

use anyhow::{bail, Context, Result};
use std::path::{Component, Path, PathBuf};

use super::{check_value, quoted, MAX_CWD_BYTES, MAX_RUNNER_BYTES, MAX_TEST_BYTES};
use crate::git::worktree::WorktreeGit;
use crate::plan::schema::{ContractChoice, ContractChoices, ContractSpec};
use crate::testrun::{registry, TestRunnerAdapter};
use crate::verify::contracts::run_dir::canonical;
use crate::verify::contracts::{normalize, resolve_adapter, store};

/// `validate_choices_in` with git as `worktree_root`'s own `.git` directs
/// it: the writer's CLI, inside its sandbox, calls it.
pub fn validate_choices(
    contracts: &[ContractSpec],
    choices: &ContractChoices,
    worktree_root: &Path,
    working_dir: &Path,
    base: &str,
) -> Result<()> {
    let git = WorktreeGit::discovered(worktree_root);
    validate_choices_in(&git, contracts, choices, worktree_root, working_dir, base)
}

/// Refuse `choices` unless every key names a contract of `contracts`, every
/// contract ends with a test, and each writer value passes the rules of the
/// flag that set it. `choices` is the saved choices, pruned, merged with the
/// request's. `base` is the stage's base commit, read through `git`.
pub(crate) fn validate_choices_in(
    git: &WorktreeGit,
    contracts: &[ContractSpec],
    choices: &ContractChoices,
    worktree_root: &Path,
    working_dir: &Path,
    base: &str,
) -> Result<()> {
    if let Some(stray) = choices
        .keys()
        .find(|id| !contracts.iter().any(|spec| &spec.id == *id))
    {
        let known: Vec<&str> = contracts.iter().map(|spec| spec.id.as_str()).collect();
        bail!(
            "the freeze names contract {}, which the stage does not have; name one of: {}",
            quoted(stray),
            known.join(", ")
        );
    }
    let site = Place::new(git, worktree_root, working_dir, base)?;
    let none = ContractChoice::default();
    let problems: Vec<String> = contracts
        .iter()
        .filter_map(|spec| {
            let choice = choices.get(&spec.id).unwrap_or(&none);
            check_contract(&site, spec, choice)
                .err()
                .map(|error| format!("{error:#}"))
        })
        .collect();
    if !problems.is_empty() {
        bail!(
            "the contract choices are refused:\n  - {}",
            problems.join("\n  - ")
        );
    }
    Ok(())
}

/// The stage's tree as the rules read it.
struct Place<'a> {
    git: &'a WorktreeGit,
    /// The worktree root, canonical.
    root: PathBuf,
    working_dir: &'a Path,
    /// The working directory's components beneath the worktree root.
    working: Vec<String>,
    base: &'a str,
}

impl<'a> Place<'a> {
    fn new(
        git: &'a WorktreeGit,
        worktree_root: &Path,
        working_dir: &'a Path,
        base: &'a str,
    ) -> Result<Self> {
        let root = canonical(worktree_root)?;
        let working = canonical(working_dir)?;
        let inside = working
            .strip_prefix(&root)
            .context("the stage's working directory is outside its worktree")?;
        let working = parts(&inside.to_string_lossy());
        Ok(Self {
            git,
            root,
            working_dir,
            working,
            base,
        })
    }
}

/// The rules for one contract and its merged choice.
fn check_contract(place: &Place<'_>, spec: &ContractSpec, choice: &ContractChoice) -> Result<()> {
    let id = &spec.id;
    planner_set(spec, choice)?;
    if let Some(runner) = &choice.runner {
        check_value("runner", id, runner, MAX_RUNNER_BYTES)?;
    }
    let effective = spec.with_choice(choice);
    let test = effective.required_test()?;
    if let Some(runner) = effective.runner.as_deref() {
        if registry::by_name(runner).is_none() {
            let names: Vec<&str> = registry::all().iter().map(|a| a.name()).collect();
            bail!(
                "contract '{id}': runner {} names no test-runner adapter; \
                 use --runner {id}=<one of: {}>",
                quoted(runner),
                names.join(", ")
            );
        }
    }
    let adapter = resolve_adapter(&effective, place.working_dir);
    if choice.test.is_some() {
        check_test(id, test, adapter)?;
    }
    if choice.test.is_some() && adapter.is_none() {
        bail!(
            "contract '{id}': --test needs a test-runner adapter, and none answers for {}: \
             without one the test would run as a shell command, which only the plan may give; \
             name the adapter with --runner {id}=<adapter>",
            quoted(&spec.file)
        );
    }
    match &choice.cwd {
        Some(cwd) => check_cwd(place, spec, cwd, adapter),
        None => Ok(()),
    }
}

/// A writer `--runner` or `--cwd` for a field the plan sets is refused: the
/// plan's value is used, and the writer is told so.
fn planner_set(spec: &ContractSpec, choice: &ContractChoice) -> Result<()> {
    let id = &spec.id;
    if let (Some(_), Some(planned)) = (&choice.runner, &spec.runner) {
        bail!(
            "contract '{id}': the plan sets runner '{planned}', which is used; drop --runner {id}="
        );
    }
    if let (Some(_), Some(planned)) = (&choice.cwd, &spec.cwd) {
        bail!("contract '{id}': the plan sets cwd '{planned}', which is used; drop --cwd {id}=");
    }
    Ok(())
}

/// A writer's test is one identifier: one line, at most [`MAX_TEST_BYTES`],
/// never an option (the adapters place it where a leading `-` is read as
/// one, as `cargo test <test> -- --exact` does), and never more than one
/// word or command to the runner that reads it. sbt parses the quoted
/// `testOnly <test>` itself: a `;` there starts a second sbt command and a
/// space a second argument. A `;` is refused for every adapter, so a test
/// checked here under one adapter cannot become a second sbt command when
/// the freeze job detects sbt; a space stays valid elsewhere, where a full
/// test name (vitest, jest) holds spaces.
fn check_test(id: &str, test: &str, adapter: Option<&dyn TestRunnerAdapter>) -> Result<()> {
    let test = check_value("test", id, test, MAX_TEST_BYTES)?;
    if test.starts_with('-') {
        bail!(
            "contract '{id}': --test {id}={} starts with '-', which the runner reads as an option; \
             give the test's name",
            quoted(test)
        );
    }
    if test.contains(';') {
        bail!(
            "contract '{id}': --test {id}={} holds ';', which a runner such as sbt reads as the \
             start of another command; name the test without ';' (rename the test if it has one)",
            quoted(test)
        );
    }
    if adapter.is_some_and(|adapter| adapter.name() == "sbt") && test.contains(char::is_whitespace)
    {
        bail!(
            "contract '{id}': --test {id}={} holds whitespace, which sbt reads as a second test or \
             option to testOnly; give the one fully qualified suite name",
            quoted(test)
        );
    }
    Ok(())
}

/// A writer's `--cwd`: a plain path relative to the worktree root, at or
/// beneath the stage's working directory, above the contract's file, and
/// holding one of the adapter's run-root markers as a regular file committed
/// at `base`.
fn check_cwd(
    place: &Place<'_>,
    spec: &ContractSpec,
    cwd: &str,
    adapter: Option<&dyn TestRunnerAdapter>,
) -> Result<()> {
    let id = &spec.id;
    let cwd = check_value("cwd", id, cwd, MAX_CWD_BYTES)?;
    if store::validate_cwd(cwd).is_err() {
        bail!(
            "contract '{id}': --cwd {id}={} is not a plain relative path; name a directory \
             relative to the worktree root, without '..'",
            quoted(cwd)
        );
    }
    let file_path = normalize(&spec.file);
    store::validate_relative(&file_path).with_context(|| format!("contract '{id}' file"))?;
    let dir = parts(cwd);
    let mut file = place.working.clone();
    file.extend(parts(&file_path));
    if !dir.starts_with(&place.working) {
        bail!(
            "contract '{id}': --cwd {id}={} lies outside the stage's working directory '{}'; \
             name it or a directory beneath it",
            quoted(cwd),
            shown(&place.working)
        );
    }
    if dir.len() >= file.len() || !file.starts_with(&dir) {
        bail!(
            "contract '{id}': --cwd {id}={} does not hold the contract's file '{}'; \
             name a directory above it",
            quoted(cwd),
            shown(&file)
        );
    }
    committed_marker(place, id, &dir, adapter)?;
    live_directory(place, id, &dir)
}

/// Refused unless every component of `dir` beneath the worktree root is a
/// real directory in the live worktree: a symlink there would move the run
/// to wherever it points, whatever the base commit holds.
fn live_directory(place: &Place<'_>, id: &str, dir: &[String]) -> Result<()> {
    let mut path = place.root.clone();
    for part in dir {
        path.push(part);
        let real_dir = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_dir());
        if !real_dir {
            bail!(
                "contract '{id}': --cwd {id}='{}' is not a directory in the worktree ('{}' is a \
                 symlink, another file, or missing); name a real directory",
                shown(dir),
                part
            );
        }
    }
    Ok(())
}

/// Refused unless `adapter` has run-root markers and one of them is a
/// regular file tracked at `base` directly inside `dir`.
fn committed_marker(
    place: &Place<'_>,
    id: &str,
    dir: &[String],
    adapter: Option<&dyn TestRunnerAdapter>,
) -> Result<()> {
    let markers = adapter.map_or(&[][..], |adapter| adapter.run_root_markers());
    if markers.is_empty() {
        bail!(
            "contract '{id}': --cwd needs a runner whose run directory has a marker file; \
             {} has none, so the run directory is decided without --cwd",
            adapter.map_or("no adapter", |adapter| adapter.name())
        );
    }
    let path = format!("{}/", dir.join("/"));
    let mut args = vec!["ls-tree", "-z", place.base];
    if !dir.is_empty() {
        args.extend(["--", path.as_str()]);
    }
    let listing = place.git.run_checked(&args)?;
    let found = listing
        .split('\0')
        .filter_map(regular_entry_name)
        .any(|name| matches_marker(name, markers));
    if !found {
        bail!(
            "contract '{id}': --cwd {id}='{}' holds none of {} committed at the stage base; \
             a marker added since the base, untracked or a symlink does not count",
            shown(dir),
            markers.join(", ")
        );
    }
    Ok(())
}

/// The file name of an `ls-tree -z` entry (`<mode> <type> <oid>\t<path>`)
/// that is a regular file (mode `100644` or `100755`; a symlink is `120000`).
fn regular_entry_name(entry: &str) -> Option<&str> {
    let (meta, path) = entry.split_once('\t')?;
    if !meta.starts_with("100") {
        return None;
    }
    path.rsplit('/').next()
}

/// Whether `name` is one of `markers`; a `*.ext` marker matches any name with
/// that extension, as the run-directory search does.
pub(super) fn matches_marker(name: &str, markers: &[&str]) -> bool {
    markers
        .iter()
        .any(|marker| match marker.strip_prefix("*.") {
            Some(extension) => Path::new(name)
                .extension()
                .is_some_and(|ext| ext == extension),
            None => name == *marker,
        })
}

/// `path`'s plain components; `.` and empty ones dropped.
fn parts(path: &str) -> Vec<String> {
    Path::new(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

/// `parts` as a worktree-relative path, `.` for the root.
fn shown(parts: &[String]) -> String {
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

#[cfg(test)]
#[path = "choices_validate_tests.rs"]
mod tests;
