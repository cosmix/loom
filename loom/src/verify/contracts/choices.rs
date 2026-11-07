//! The contract writer's choices: the `--test`, `--runner` and `--cwd` it
//! names for a contract with `loom stage contracts freeze`.
//!
//! A choice fills what the planner left open: the writer's test replaces the
//! plan's hint, and its runner or directory applies only where the plan sets
//! none (`ContractSpec::with_choice`). The daemon validates every choice
//! against the stage's base commit (`validate_choices_in`) when it admits a
//! freeze request and keeps them in `.loom/work/contracts/<stage>/choices.json`,
//! a file only the daemon writes. The freeze job and its application read the
//! stage through [`load_effective_stage`], so the contracts they run and
//! record are the plan's specs overlaid with the saved choices.
//!
//! Every reader prunes the saved choices first ([`prune_choices`]): a choice
//! for a contract the plan no longer has, or a runner or directory the plan
//! now sets, is dropped, so an amended plan never leaves a stale choice that
//! refuses every later freeze.
//!
//! Choices are agent text: they are quoted with `quoted` wherever they enter
//! a message, and never become part of a shell string here.

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::store::{canonical_work_dir, stage_dir};
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::Stage;
use crate::plan::schema::{ContractChoice, ContractChoices, ContractSpec};
use crate::verify::review::report::single_line;
use crate::verify::transitions::load_stage;

#[path = "choices_validate.rs"]
mod validate;
pub use validate::validate_choices;
pub(crate) use validate::validate_choices_in;

/// Most distinct contract ids one freeze may name.
pub const MAX_CHOICE_IDS: usize = 64;
/// Longest test identifier a writer may name.
pub const MAX_TEST_BYTES: usize = 512;
/// Longest runner name a writer may name.
pub const MAX_RUNNER_BYTES: usize = 64;
/// Longest run directory a writer may name.
pub const MAX_CWD_BYTES: usize = 512;
/// Most bytes of values one freeze may carry in all, well inside the 64 KiB
/// a freeze request may take on the wire.
pub const MAX_CHOICE_VALUE_BYTES: usize = 16 * 1024;

const CHOICES_FILE: &str = "choices.json";
const MAX_CHOICES_BYTES: usize = 64 * 1024;

/// One of the three choice flags: its name and the field it fills.
struct Flag {
    name: &'static str,
    limit: usize,
    field: fn(&mut ContractChoice) -> &mut Option<String>,
}

fn test_field(choice: &mut ContractChoice) -> &mut Option<String> {
    &mut choice.test
}

fn runner_field(choice: &mut ContractChoice) -> &mut Option<String> {
    &mut choice.runner
}

fn cwd_field(choice: &mut ContractChoice) -> &mut Option<String> {
    &mut choice.cwd
}

const TEST: Flag = Flag {
    name: "test",
    limit: MAX_TEST_BYTES,
    field: test_field,
};
const RUNNER: Flag = Flag {
    name: "runner",
    limit: MAX_RUNNER_BYTES,
    field: runner_field,
};
const CWD: Flag = Flag {
    name: "cwd",
    limit: MAX_CWD_BYTES,
    field: cwd_field,
};

/// The choices `--test`, `--runner` and `--cwd` name, each entry
/// `<contract-id>=<value>` split at its first `=` (a test name may hold one).
/// The values are kept trimmed. Refused: an entry with no `=`, an invalid id,
/// an empty value or one with a control character, a value over its flag's
/// limit, the same id twice for one flag, more than [`MAX_CHOICE_IDS`] ids,
/// and more than [`MAX_CHOICE_VALUE_BYTES`] of values in all.
pub fn parse_choices(
    tests: &[String],
    runners: &[String],
    cwds: &[String],
) -> Result<ContractChoices> {
    let mut choices = ContractChoices::new();
    let mut total = 0usize;
    for (flag, entries) in [(&TEST, tests), (&RUNNER, runners), (&CWD, cwds)] {
        for entry in entries {
            let Some((id, value)) = entry.split_once('=') else {
                bail!(
                    "--{} {} is not <contract-id>=<value>",
                    flag.name,
                    quoted(entry)
                );
            };
            crate::validation::validate_id(id).with_context(|| {
                format!("--{} {}: invalid contract id", flag.name, quoted(entry))
            })?;
            let value = check_value(flag.name, id, value, flag.limit)?;
            total = total.saturating_add(value.len());
            let slot = (flag.field)(choices.entry(id.to_string()).or_default());
            if slot.is_some() {
                bail!("--{} names contract '{id}' twice; give it once", flag.name);
            }
            *slot = Some(value.to_string());
        }
    }
    if choices.len() > MAX_CHOICE_IDS {
        bail!(
            "the freeze names {} contracts; at most {MAX_CHOICE_IDS} may be named",
            choices.len()
        );
    }
    if total > MAX_CHOICE_VALUE_BYTES {
        bail!("the choices carry {total} bytes of values; at most {MAX_CHOICE_VALUE_BYTES} in all");
    }
    Ok(choices)
}

/// `value` trimmed, refused when that is empty, holds a control character or
/// is longer than `limit` bytes. The message names the flag and the contract.
pub(super) fn check_value<'a>(
    flag: &str,
    id: &str,
    value: &'a str,
    limit: usize,
) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("--{flag} {id}= is empty: give a value after the '='");
    }
    if trimmed.chars().any(char::is_control) {
        bail!(
            "--{flag} {id}={} holds a control character; give the value on one line",
            quoted(trimmed)
        );
    }
    if trimmed.len() > limit {
        bail!(
            "--{flag} {id}= is {} bytes; at most {limit} are allowed",
            trimmed.len()
        );
    }
    Ok(trimmed)
}

/// `text` as it may appear in a message: on one line, at most 80 bytes,
/// in single quotes.
pub(super) fn quoted(text: &str) -> String {
    format!("'{}'", single_line(&truncate_to_byte_limit(text, 80)))
}

/// `current` with `update` laid over it: every id of either, and per field
/// `update`'s value where it has one, else `current`'s.
pub fn merge_choices(current: &ContractChoices, update: &ContractChoices) -> ContractChoices {
    let mut merged = current.clone();
    for (id, choice) in update {
        let entry = merged.entry(id.clone()).or_default();
        let ContractChoice { test, runner, cwd } = choice.clone();
        entry.test = test.or(entry.test.take());
        entry.runner = runner.or(entry.runner.take());
        entry.cwd = cwd.or(entry.cwd.take());
    }
    merged
}

/// `saved` without the ids that name no contract of `contracts` and without a
/// `runner` or `cwd` the contract's plan spec now sets; a choice left with no
/// field is dropped.
pub fn prune_choices(contracts: &[ContractSpec], saved: &ContractChoices) -> ContractChoices {
    saved
        .iter()
        .filter_map(|(id, choice)| {
            let spec = contracts.iter().find(|spec| &spec.id == id)?;
            let kept = ContractChoice {
                test: choice.test.clone(),
                runner: choice.runner.clone().filter(|_| spec.runner.is_none()),
                cwd: choice.cwd.clone().filter(|_| spec.cwd.is_none()),
            };
            (kept != ContractChoice::default()).then(|| (id.clone(), kept))
        })
        .collect()
}

/// The choices saved for `stage_id`; empty when none are.
pub fn load_choices(work_dir: &Path, stage_id: &str) -> Result<ContractChoices> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    let relative = stage_dir(Path::new(""), stage_id).join(CHOICES_FILE);
    let bytes = match read_bounded(&root, &relative, MAX_CHOICES_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Ok(ContractChoices::new()),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid contract choices for stage '{stage_id}'"))
}

/// Replace the choices saved for `stage_id`. Only the daemon calls it.
pub fn save_choices(work_dir: &Path, stage_id: &str, choices: &ContractChoices) -> Result<()> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    let dir = stage_dir(&root, stage_id);
    let json = serde_json::to_string_pretty(choices)?;
    locked_dir_update(&dir, || atomic_write_locked(&dir.join(CHOICES_FILE), &json))
        .with_context(|| format!("failed to save the contract choices of stage '{stage_id}'"))
}

/// `contracts` overlaid with the saved choices of `stage_id`, pruned and
/// merged with `update`. Reads only; a contract left without a test keeps
/// `test: None`.
pub fn overlay_with_saved(
    work_dir: &Path,
    stage_id: &str,
    contracts: &[ContractSpec],
    update: &ContractChoices,
) -> Result<Vec<ContractSpec>> {
    let saved = prune_choices(contracts, &load_choices(work_dir, stage_id)?);
    Ok(overlay(contracts, &merge_choices(&saved, update)))
}

/// Each spec with its choice from `choices` applied (none: unchanged).
fn overlay(contracts: &[ContractSpec], choices: &ContractChoices) -> Vec<ContractSpec> {
    let none = ContractChoice::default();
    contracts
        .iter()
        .map(|spec| spec.with_choice(choices.get(&spec.id).unwrap_or(&none)))
        .collect()
}

/// `stage`'s contracts as the freeze runs them: the plan's specs overlaid
/// with the saved choices. The first contract left without a test is the
/// error.
pub fn effective_contracts(work_dir: &Path, stage: &Stage) -> Result<Vec<ContractSpec>> {
    let specs = overlay_with_saved(
        work_dir,
        &stage.id,
        &stage.contracts,
        &ContractChoices::new(),
    )?;
    for spec in &specs {
        spec.required_test()?;
    }
    Ok(specs)
}

/// The stage `stage_id` with its contracts replaced by
/// [`effective_contracts`].
pub fn load_effective_stage(stage_id: &str, work_dir: &Path) -> Result<Stage> {
    let mut stage = load_stage(stage_id, work_dir)?;
    stage.contracts = effective_contracts(work_dir, &stage)?;
    Ok(stage)
}

/// The overlay of [`effective_contracts`] for display and prompts: it never
/// fails. Choices that cannot be read overlay nothing, and a contract
/// without a test keeps `test: None`.
pub fn lenient_contracts(work_dir: &Path, stage: &Stage) -> Vec<ContractSpec> {
    let none = ContractChoices::new();
    overlay_with_saved(work_dir, &stage.id, &stage.contracts, &none).unwrap_or_else(|error| {
        tracing::warn!(
            stage_id = %stage.id,
            error = %format!("{error:#}"),
            "Cannot read the contract writer's choices; showing the plan's contracts"
        );
        stage.contracts.clone()
    })
}

#[cfg(test)]
#[path = "choices_tests.rs"]
mod tests;
