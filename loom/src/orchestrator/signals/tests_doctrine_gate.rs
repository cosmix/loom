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
