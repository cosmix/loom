//! The feedback a contract-rulings verdict leaves the agent: each contract's
//! outcome and reasoning, what the daemon wrote (uncommitted: completion reads
//! the worktree, but the branch keeps the old file until the agent commits
//! it), what it did not write and why, and what was re-frozen.

use std::collections::BTreeMap;

use crate::models::dispute::{ContractRuling, ContractRulingKind};

use super::super::apply_contract::Refrozen;

/// What an applied verdict with at least one accept did.
pub(super) struct Outcome<'a> {
    pub(super) rulings: &'a [ContractRuling],
    pub(super) refrozen: &'a Refrozen,
    /// The replacement files written, in filing order.
    pub(super) written: &'a [String],
    /// Every replacement file the dispute carried, in filing order.
    pub(super) filed: &'a [String],
    pub(super) contract_files: &'a BTreeMap<String, String>,
}

/// The notice for a verdict that accepts at least one contract.
pub(super) fn accepted(outcome: &Outcome<'_>) -> String {
    let paths: Vec<&str> = outcome
        .refrozen
        .files
        .iter()
        .map(|(path, _)| path.as_str())
        .collect();
    let dropped = &outcome.refrozen.dropped;
    let dropped = if dropped.is_empty() {
        String::new()
    } else {
        format!(
            " Dropped from the freeze, as the worktree no longer holds them: {}.",
            dropped.join(", ")
        )
    };
    let action = if outcome.written.is_empty() {
        "implement against the contracts as they are frozen now."
    } else {
        "implement until every contract passes."
    };
    format!(
        "The adjudicator ruled on your contract dispute.\n\n{}\nThe stage's contract and \
         harness files are re-frozen at their current content ({}).{dropped}{}{}\n\n\
         Action: {action}\n",
        rulings_section(outcome.rulings),
        paths.join(", "),
        written_section(outcome.written),
        held_back_section(outcome),
    )
}

/// The notice for a verdict that rejects every disputed contract. Any
/// replacement was discarded and the worktree left alone.
pub(super) fn rejected(stage_id: &str, rulings: &[ContractRuling], replaced: bool) -> String {
    let action = if replaced {
        "Your replacements were discarded; the worktree was not changed. Implement against \
         the frozen contracts."
            .to_string()
    } else {
        let restores: Vec<String> = rulings
            .iter()
            .map(|ruling| {
                format!(
                    "`loom stage contracts restore {stage_id} --contract {}`",
                    ruling.contract
                )
            })
            .collect();
        format!(
            "restore each frozen contract ({}), then implement against it.",
            restores.join(", ")
        )
    };
    format!(
        "The adjudicator rejected your contract dispute: every disputed contract stands as \
         frozen.\n\n{}\nAction: {action}\n",
        rulings_section(rulings)
    )
}

fn rulings_section(rulings: &[ContractRuling]) -> String {
    let mut section = String::from("### Rulings\n\n");
    for ruling in rulings {
        let verb = match ruling.ruling {
            ContractRulingKind::Accept => "accepted",
            ContractRulingKind::Reject => "rejected",
        };
        section.push_str(&format!(
            "- `{}` {verb}: {}\n",
            ruling.contract,
            ruling.reasoning.trim().replace('\n', " ")
        ));
    }
    section
}

fn written_section(written: &[String]) -> String {
    match written {
        [] => String::new(),
        [file] => format!(
            "\n\nThe daemon wrote the accepted replacement to `{file}`. Stage it with \
             `git add {file}` and commit it with your other files through `loom stage commit`."
        ),
        files => {
            let quoted: Vec<String> = files.iter().map(|file| format!("`{file}`")).collect();
            format!(
                "\n\nThe daemon wrote the accepted replacements to {}. Stage them with \
                 `git add {}` and commit them with your other files through \
                 `loom stage commit`.",
                quoted.join(", "),
                files.join(" ")
            )
        }
    }
}

/// The replacements the verdict held back, each with why.
fn held_back_section(outcome: &Outcome<'_>) -> String {
    let held: Vec<String> = outcome
        .filed
        .iter()
        .filter(|file| !outcome.written.contains(file))
        .map(|file| format!("`{file}` ({})", held_back_reason(outcome, file)))
        .collect();
    if held.is_empty() {
        return String::new();
    }
    format!(
        "\n\nNot written, as filed: {}. Those files stay as frozen.",
        held.join("; ")
    )
}

fn held_back_reason(outcome: &Outcome<'_>, file: &str) -> String {
    let rejected: Vec<&str> = outcome
        .contract_files
        .iter()
        .filter(|(_, held)| held.as_str() == file)
        .map(|(id, _)| id.as_str())
        .filter(|id| {
            !outcome
                .rulings
                .iter()
                .any(|r| r.contract == *id && r.ruling == ContractRulingKind::Accept)
        })
        .collect();
    if rejected.is_empty() {
        "a harness file is written only when every ruling is an accept".to_string()
    } else {
        format!("it holds contract {}, not accepted", rejected.join(", "))
    }
}
