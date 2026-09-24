//! Contract signal generation.
//!
//! A v2 standard stage with contracts runs a `Contract` session before its
//! `Stage` session (DESIGN D8): it writes the stage's contract tests, proves
//! each one fails, and freezes them with `loom stage contracts freeze`. Its
//! signal carries the contracts with the adapter and command that run each
//! one, the harness globs, the stage description as context only, the
//! knowledge brief, the language skills, and the rules of the phase. It
//! carries no acceptance criteria and no completion step: the stage belongs to
//! the implementation session that follows.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::language::DetectedLanguage;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::plan::schema::ContractSpec;
use crate::skills::{skill_invocation, SkillIndex};
use crate::testrun::{languages, TestRunnerAdapter};
use crate::verify::contracts::{contract_command, resolve_adapter};

use super::format::{format_skill_recommendations, format_stage_brief};
use super::generate::declared_and_recommended_skills;
use super::helpers::CONTEXT_CEILING_HANDOFF;
use super::retrieval::STAGE_QUERY_INPUTS;
use super::types::EmbeddedContext;

/// One contract with the adapter and the command loom runs it with: the same
/// pair `loom stage contracts freeze` and the completion check resolve.
struct ResolvedContract<'a> {
    spec: &'a ContractSpec,
    adapter: Option<&'static dyn TestRunnerAdapter>,
    command: String,
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
    let contracts: Vec<ResolvedContract<'_>> = stage
        .contracts
        .iter()
        .map(|spec| {
            let adapter = resolve_adapter(spec, &stage_root);
            let command = contract_command(spec, adapter, &stage_root);
            ResolvedContract {
                spec,
                adapter,
                command,
            }
        })
        .collect();
    let content =
        format_contract_signal(session, stage, &stage_root, &contracts, &embedded_context);

    // Record before writing, as every stage signal path does: a session that
    // was briefed must never read back as having received nothing.
    super::helpers::persist_delivery(work_dir, stage, &session.id, &embedded_context);
    super::helpers::write_signal_file(&session.id, &content, work_dir)
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
    embedded_context: &EmbeddedContext,
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
    append_harness(&mut content, &stage.harness);
    append_skills(
        &mut content,
        contracts,
        &embedded_context.skill_recommendations,
    );
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
            "| `{}` | `{}` | `{}` | {adapter} | {} | {} |\n",
            table_cell(&spec.id),
            table_cell(&spec.file),
            table_cell(&spec.test),
            table_cell(&spec.scenario),
            table_cell(&spec.rejects),
        ));
    }
    content.push_str("\nloom runs each contract from the working directory as:\n\n");
    for contract in contracts {
        let (id, command) = (&contract.spec.id, &contract.command);
        content.push_str(&format!("- `{id}`: `{command}`\n"));
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

fn append_rules(content: &mut String, stage_id: &str) {
    content.push_str("## Your Job\n\n");
    content.push_str(
        "1. Write each contract test above in its file. You may also write files matching \
         the harness globs. Implement nothing: no production code, no fixes, no refactors.\n\
         2. Each test sets up its scenario and must fail on the wrong implementation its \
         Rejects column names.\n\
         3. Every contract test must fail now, before the implementation exists. A compile \
         or collection failure counts as failing.\n\
         4. Do not commit and do not run `loom stage complete`: the stage belongs to the \
         implementation session after you.\n\
         5. Record mistakes, decisions and surprises with `loom memory note` and \
         `loom memory decision`, as any stage does.\n",
    );
    content.push_str(&format!(
        "6. Finish with `loom stage contracts freeze {stage_id}`. It checks that you changed \
         only contract and harness files, runs every contract, and records them. If it \
         refuses, fix what it names and run it again.\n\
         7. After a successful freeze, stop. Loom ends this session and starts the \
         implementation session.\n\n"
    ));
    content.push_str("- Stay inside this worktree.\n");
    content.push_str(CONTEXT_CEILING_HANDOFF);
}

#[cfg(test)]
#[path = "contract_tests.rs"]
mod tests;
