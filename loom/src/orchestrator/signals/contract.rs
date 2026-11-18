//! Contract signal generation.
//!
//! A v2 standard stage with contracts runs a `Contract` session before its
//! `Stage` session (DESIGN D8): it writes the stage's contract tests, proves
//! each one fails, and freezes them with `loom stage contracts freeze`. Its
//! signal carries the contracts (the plan's specs overlaid with the choices
//! earlier freezes of the stage saved) with the adapter and command that run
//! each one once its test is named, the harness globs, the stage description as context only, the
//! knowledge brief, the language skills, and the rules of the phase. It
//! carries no acceptance criteria and no completion step: the stage belongs to
//! the implementation session that follows.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::git::worktree::WorktreeGit;
use crate::language::DetectedLanguage;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::plan::schema::ContractSpec;
use crate::skills::{skill_invocation, SkillIndex};
use crate::testrun::{languages, TestRunnerAdapter};
use crate::verify::contracts::changes::stage_base;
use crate::verify::contracts::choices::lenient_contracts;
use crate::verify::contracts::collection::{stage_scaffold, ScaffoldEdit, ScaffoldModule};
use crate::verify::contracts::resolve_adapter;
use crate::verify::contracts::run_dir::{resolve_run, ContractRun};

use super::format::{format_skill_recommendations, format_stage_brief};
use super::generate::declared_and_recommended_skills;
use super::retrieval::STAGE_QUERY_INPUTS;
use super::types::EmbeddedContext;

mod choose;
mod rules;
use choose::{append_choices, test_cell};
use rules::append_rules;

/// One contract with the adapter and the run loom runs it with, or why it
/// cannot be run: the same `loom stage contracts freeze` and the completion
/// check resolve. `run` is `None` while no test is named for the contract: the
/// writer names it with the freeze, and loom builds the command from it.
struct ResolvedContract<'a> {
    spec: &'a ContractSpec,
    adapter: Option<&'static dyn TestRunnerAdapter>,
    run: Option<Result<ContractRun, String>>,
    /// The test came from a choice an earlier freeze saved, not from the plan.
    test_saved: bool,
}

impl ResolvedContract<'_> {
    /// The skill for the contract's language: its adapter's, else its file's.
    fn language_skill(&self) -> Option<String> {
        let language = match self.adapter {
            Some(adapter) => adapter.language(),
            None => languages::for_path(&self.spec.file)?.name,
        };
        Some(languages::skill_for(language))
    }

    /// Why no adapter runs this contract, when none does.
    fn unsupported_reason(&self) -> Option<String> {
        if self.adapter.is_some() {
            return None;
        }
        Some(match &self.spec.runner {
            Some(name) => format!("`{name}` is not a known runner"),
            None => "no test runner is detected for the package that owns this file".to_string(),
        })
    }
}

/// Generate the signal file for a contract session, at the path every
/// signal is written to: `<work_dir>/signals/<session-id>.md`.
pub fn generate_contract_signal(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    work_dir: &Path,
    skill_index: Option<&SkillIndex>,
    detected_languages: &[DetectedLanguage],
) -> Result<PathBuf> {
    let skill_recommendations = skill_index
        .map(|index| declared_and_recommended_skills(index, stage, worktree, detected_languages))
        .unwrap_or_default();
    let embedded_context = EmbeddedContext {
        context_pack: super::helpers::retrieve_stage_pack(work_dir, stage),
        skill_recommendations,
        ..EmbeddedContext::default()
    };
    let stage_root = stage_root(stage, &worktree.path);
    let overlaid = lenient_contracts(work_dir, stage);
    let contracts: Vec<ResolvedContract<'_>> = overlaid
        .iter()
        .map(|spec| resolve_contract(stage, spec, &worktree.path, &stage_root))
        .collect();
    let scaffold = scaffold_for(work_dir, &worktree.path, &stage_root, &overlaid, &contracts);
    // No `has_disputes` gate (unlike the stage signal): the notice here comes
    // from a refused freeze, which the old dispute tallies do not count, and
    // only the adjudication apply clears it.
    let feedback = crate::orchestrator::adjudication::feedback::read_feedback(work_dir, &stage.id)
        .ok()
        .flatten();
    let content = format_contract_signal(
        session,
        stage,
        &stage_root,
        &contracts,
        &scaffold,
        &embedded_context,
        feedback.as_deref(),
    );

    super::helpers::publish_signal(&session.id, &content, work_dir, stage, &embedded_context)
}

/// `spec` (the plan's, overlaid with the saved choices) with the adapter and
/// run loom resolves for it. A spec with no test is not resolved: its run
/// needs the test the writer will name.
fn resolve_contract<'a>(
    stage: &Stage,
    spec: &'a ContractSpec,
    worktree: &Path,
    stage_root: &Path,
) -> ResolvedContract<'a> {
    let adapter = resolve_adapter(spec, stage_root);
    let run = spec.test.as_ref().map(|_| {
        resolve_run(spec, adapter, worktree, stage_root, None).map_err(|error| format!("{error:#}"))
    });
    let planned = stage.contracts.iter().find(|plan| plan.id == spec.id);
    let test_saved = spec.test.is_some() && planned.is_none_or(|plan| plan.test != spec.test);
    ResolvedContract {
        spec,
        adapter,
        run,
        test_saved,
    }
}

/// `value` as the text of one markdown table cell: a `|` would end the cell
/// and a line break would end the row.
pub(super) fn table_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\n', '\r'], " ")
}

/// The directory contract files are relative to and their commands run
/// from: the stage's `working_dir` inside its worktree.
fn stage_root(stage: &Stage, worktree: &Path) -> PathBuf {
    match stage.working_dir.as_deref() {
        Some(dir) if !dir.is_empty() && dir != "." => worktree.join(dir),
        _ => worktree.to_path_buf(),
    }
}

fn format_contract_signal(
    session: &Session,
    stage: &Stage,
    stage_root: &Path,
    contracts: &[ResolvedContract<'_>],
    scaffold: &Result<Vec<ScaffoldEdit>, String>,
    embedded_context: &EmbeddedContext,
    feedback: Option<&str>,
) -> String {
    let mut content = format!("# Contract Signal: {}\n\n", session.id);
    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {}\n", session.id));
    content.push_str(&format!("- **Stage**: {}\n", stage.id));
    if let Some(plan_id) = &stage.plan_id {
        content.push_str(&format!("- **Plan**: {plan_id}\n"));
    }
    content.push_str("- **Type**: Contract (write and freeze this stage's contract tests)\n");
    content.push_str(&format!(
        "- **Working directory**: {} (contract files and harness globs are relative to it)\n\n",
        stage_root.display()
    ));
    if let Some(pack) = &embedded_context.context_pack {
        content.push_str(&format_stage_brief(pack, &stage.id, STAGE_QUERY_INPUTS));
        content.push('\n');
    }
    append_stage_context(&mut content, stage);
    append_contracts(&mut content, contracts);
    append_choices(&mut content, &stage.contracts);
    append_harness(&mut content, &stage.harness);
    append_scaffold(&mut content, scaffold);
    append_skills(
        &mut content,
        contracts,
        &embedded_context.skill_recommendations,
    );
    if let Some(text) = feedback {
        content.push_str("## Adjudicator Feedback (from the refused freeze)\n\n");
        content.push_str(text);
        super::helpers::ensure_trailing_newline(&mut content);
        content.push('\n');
    }
    append_rules(&mut content, &stage.id);
    content
}

fn append_stage_context(content: &mut String, stage: &Stage) {
    content.push_str("## Stage Context\n\n");
    content.push_str(
        "The implementation session after you builds this. It is context for your \
         contracts; build none of it.\n\n",
    );
    content.push_str(&format!("**{}**\n\n", stage.name));
    match &stage.description {
        Some(description) => content.push_str(description),
        None => content.push_str("(no description provided)"),
    }
    content.push_str("\n\n");
}

fn append_contracts(content: &mut String, contracts: &[ResolvedContract<'_>]) {
    content.push_str("## Contracts\n\n");
    content.push_str("| Id | File | Test | Adapter | Scenario | Rejects |\n");
    content.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for contract in contracts {
        let spec = contract.spec;
        let adapter = contract
            .adapter
            .map_or("unsupported", |adapter| adapter.name());
        content.push_str(&format!(
            "| `{}` | `{}` | {} | {adapter} | {} | {} |\n",
            table_cell(&spec.id),
            table_cell(&spec.file),
            test_cell(contract),
            table_cell(&spec.scenario),
            table_cell(&spec.rejects),
        ));
    }
    content.push_str(
        "\nloom runs each contract from its run directory (relative to the worktree root: \
         the contract's `cwd`, else the nearest directory above its file holding the \
         runner's config, else the working directory) as:\n\n",
    );
    for contract in contracts {
        let id = &contract.spec.id;
        match &contract.run {
            Some(Ok(run)) => {
                content.push_str(&format!("- `{id}` in `{}`: `{}`\n", run.cwd, run.command))
            }
            Some(Err(error)) => content.push_str(&format!("- `{id}`: cannot be run: {error}\n")),
            None => content.push_str(&format!(
                "- `{id}`: name its test with `--test {id}=<identifier>`; loom builds the \
                 command from it\n"
            )),
        }
        if let Some(reason) = contract.unsupported_reason() {
            content.push_str(&format!(
                "  Unsupported runner ({reason}): its `test` runs as a shell command, and the \
                 freeze accepts it only on a non-zero exit, with a warning.\n"
            ));
        }
    }
    content.push('\n');
}

fn append_harness(content: &mut String, harness: &[String]) {
    content.push_str("## Harness\n\n");
    if harness.is_empty() {
        content.push_str("No harness globs: write only the contract files above.\n\n");
        return;
    }
    content.push_str("Besides the contract files, you may create or edit files matching:\n\n");
    for glob in harness {
        content.push_str(&format!("- `{glob}`\n"));
    }
    content.push('\n');
}

/// The scaffold edits the contracts' collection needs, derived over the
/// stage base and never over the worktree, which already holds the writer's
/// edits. `Err` is the sentence the signal prints in the section's place.
fn scaffold_for(
    work_dir: &Path,
    worktree_root: &Path,
    stage_root: &Path,
    specs: &[ContractSpec],
    contracts: &[ResolvedContract<'_>],
) -> Result<Vec<ScaffoldEdit>, String> {
    let has_rust = contracts
        .iter()
        .any(|contract| contract.adapter.is_some_and(|a| a.language() == "rust"));
    if !has_rust {
        return Ok(Vec::new());
    }
    let unknown = |error: String| {
        format!("Scaffold unknown: {error}; the freeze refuses until the stage base resolves.")
    };
    let repo = WorktreeGit::pinned_in_project_of(work_dir, worktree_root)
        .map_err(|error| unknown(format!("{error:#}")))?;
    let base = stage_base(&repo, work_dir).map_err(|error| unknown(format!("{error:#}")))?;
    stage_scaffold(&repo, worktree_root, stage_root, &base, specs).map_err(|error| {
        format!("Scaffold derivation failed: {error}\n\nThe freeze will refuse this stage's contracts until that is fixed.")
    })
}

fn append_scaffold(content: &mut String, scaffold: &Result<Vec<ScaffoldEdit>, String>) {
    content.push_str("## Scaffold\n\n");
    let edits = match scaffold {
        Ok(edits) if !edits.is_empty() => edits,
        Ok(_) => {
            content.push_str("No scaffold edits: every contract file is already collected.\n\n");
            return;
        }
        Err(message) => {
            content.push_str(message);
            content.push_str("\n\n");
            return;
        }
    };
    content.push_str(
        "A contract file is compiled only when a chain of `mod` items reaches it. Make these \
         edits:\n\n",
    );
    for edit in edits {
        content.push_str(&format!("- {}\n", scaffold_line(edit)));
    }
    content.push_str(
        "\nThese edits are not frozen; the implementer extends them. Add nothing else to \
         these files, and no `#![cfg(...)]` or `#[path]`.\n\n",
    );
}

/// One scaffold edit as a phrase: the one wording the contract signal, the
/// adjudication briefing and the next writer's notice all show.
pub(crate) fn scaffold_line(edit: &ScaffoldEdit) -> String {
    match edit {
        ScaffoldEdit::Declare { path, module } => {
            format!("in `{path}`, {}", declaration(module))
        }
        ScaffoldEdit::CreateRoot { paths, modules } => {
            let paths: Vec<String> = paths.iter().map(|path| format!("`{path}`")).collect();
            let held: Vec<String> = modules.iter().map(declaration).collect();
            format!(
                "a new {} holding only {}",
                paths.join(" or "),
                held.join(", ")
            )
        }
    }
}

/// One module declaration as the scaffold asks for it.
fn declaration(module: &ScaffoldModule) -> String {
    if module.test_only {
        format!("`#[cfg(test)] mod {};`", module.name)
    } else {
        format!("`mod {};`", module.name)
    }
}

fn append_skills(
    content: &mut String,
    contracts: &[ResolvedContract<'_>],
    recommended: &[crate::skills::SkillMatch],
) {
    let languages: BTreeSet<String> = contracts
        .iter()
        .filter_map(ResolvedContract::language_skill)
        .collect();
    if !languages.is_empty() {
        content.push_str("## Language Skills\n\nLoad before writing a contract test:\n\n");
        for skill in languages {
            content.push_str(&format!("- `{}`\n", skill_invocation(&skill)));
        }
        content.push('\n');
    }
    if !recommended.is_empty() {
        content.push_str(&format_skill_recommendations(recommended));
    }
}

#[cfg(test)]
#[path = "contract_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "contract/choices_tests.rs"]
mod choices_tests;
