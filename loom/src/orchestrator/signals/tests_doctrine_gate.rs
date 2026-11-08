//! A standard stage's gate is the orchestrator's single `loom check`; no
//! guidance surface sends it to a verifier subagent, and the surfaces that
//! describe the flow agree on `loom check` and `--criterion`.

use std::path::Path;

use super::cache::generate_stable_prefix;
use super::v2_section::append_v2_section;
use crate::models::stage::{Stage, StageType};

const CLAUDE_MD_TEMPLATE: &str = include_str!("../../../../CLAUDE.md.template");
const ORCHESTRATION_SKILL: &str = include_str!("../../../../skills/loom-orchestration/SKILL.md");
const SUBAGENT_PREAMBLE: &str = include_str!("../../../../loom-hooks/_subagent-preamble.txt");
const PLAN_TEMPLATE: &str =
    include_str!("../../../../skills/loom-plan-writer/references/plan-template.md");
const CODE_REVIEWER: &str = include_str!("../../../../agents/loom-code-reviewer.md");

/// Assembled so this file never contains the retired agent name as a literal.
const RETIRED_AGENT: &str = concat!("loom-", "verifier");

#[test]
fn the_retired_agent_definition_does_not_exist() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../agents/{RETIRED_AGENT}.md"));
    assert!(!path.exists(), "{} must stay deleted", path.display());
}

#[test]
fn no_gate_surface_names_the_retired_agent() {
    let prefix = generate_stable_prefix();
    for (label, text) in [
        ("standard stable prefix gate block", prefix.as_str()),
        ("skills/loom-orchestration/SKILL.md", ORCHESTRATION_SKILL),
        ("CLAUDE.md.template", CLAUDE_MD_TEMPLATE),
        ("loom-hooks/_subagent-preamble.txt", SUBAGENT_PREAMBLE),
        ("references/plan-template.md", PLAN_TEMPLATE),
    ] {
        assert!(
            !text.contains(RETIRED_AGENT),
            "{label} still names {RETIRED_AGENT}; the gate is the orchestrator's `loom check`"
        );
    }
}

#[test]
fn gate_block_and_orchestration_skill_state_the_check_flow() {
    let prefix = generate_stable_prefix();
    for (label, text) in [
        ("standard stable prefix gate block", prefix.as_str()),
        ("skills/loom-orchestration/SKILL.md", ORCHESTRATION_SKILL),
    ] {
        for needle in ["loom check", "--criterion"] {
            assert!(text.contains(needle), "{label} lost `{needle}`");
        }
    }
}

#[test]
fn gate_block_keeps_the_full_suite_behind_loom_check() {
    let prefix = generate_stable_prefix();
    assert!(
        prefix.contains("The full suite runs only through `loom check`"),
        "the standard prefix must send the full suite through `loom check`"
    );
    assert!(
        !prefix.contains("Never re-run what the check covers"),
        "the retired re-run ban must not return: a repeated check is cheap, not refused"
    );
}

#[test]
fn code_reviewer_judges_criteria_intent() {
    assert!(
        CODE_REVIEWER.contains("acceptance criterion") && CODE_REVIEWER.contains("intent"),
        "loom-code-reviewer must judge the diff against each acceptance criterion's intent"
    );
}

/// The reviewer is read-only and never sees the stage signal, so the criteria it
/// judges reach it only through the orchestrator's brief.
#[test]
fn orchestrator_pastes_the_acceptance_criteria_into_the_reviewer_brief() {
    let work_dir = tempfile::tempdir().expect("create a temporary work dir");
    let stage = Stage {
        id: "gate".to_string(),
        plan_version: 2,
        stage_type: StageType::Standard,
        ..Stage::default()
    };
    let mut gate = String::new();
    append_v2_section(&mut gate, &stage, work_dir.path());
    let needle =
        "paste the stage's acceptance criteria (with `exit_code` and `stdout_contains` where set)";

    for (label, text) in [
        ("the rendered review gate", gate.as_str()),
        ("skills/loom-orchestration/SKILL.md", ORCHESTRATION_SKILL),
    ] {
        let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            one_line.contains(needle),
            "{label} must tell the orchestrator to `{needle}` into every reviewer brief"
        );
    }
    assert!(
        CODE_REVIEWER.contains("acceptance criterion in your brief"),
        "loom-code-reviewer must take the criteria from its brief, not a signal it never sees"
    );
}

#[test]
fn codex_job_doctrine_pins_sizing_and_brief_handoff() {
    use super::format::format_codex_implementers_section;
    use crate::models::stage::{Implementer, Implementers};

    let text =
        format_codex_implementers_section(&Implementers::new(vec![Implementer::Codex]), true);
    for needle in [
        "ONE CONCERN",
        "250,000",
        "23,000",
        "gpt-6.1-sol",
        "gpt-6.1-terra",
        "50,000",
        "20,000",
        "1500000",
        "1560000",
        "twice the time at xhigh",
        "absolute path",
        "never retypes the brief",
        "LOOM_CODEX_FORWARD=1",
        "context-injecting hooks stay silent while guard hooks still run",
        "Never slice jobs into waves by the cap of 6",
    ] {
        assert!(text.contains(needle), "codex doctrine lost {needle:?}");
    }
}

#[test]
fn orchestration_review_loop_pins_followup_and_edit_exceptions() {
    for needle in [
        "models.code_reviewer_model",
        "models.code_reviewer_followup_model",
        "ONE fix worker before ONE re-review",
        "Comment-only lines, blank lines",
        "prose markdown outside skills/, agents/, commands/, loom-hooks/, tests and fixtures",
        "the base must be unchanged",
        "Block comments and every other edit",
    ] {
        assert!(
            ORCHESTRATION_SKILL.contains(needle),
            "review loop lost {needle:?}"
        );
    }
}
