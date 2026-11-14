//! Contracts for the model-allocation doctrine: opus runs at effort medium and
//! implements escalated work itself, the orchestration skill teaches that tier,
//! both review surfaces carry the frontend rules, and the signal's codex
//! doctrine names the sol tier beside terra and luna.

use super::format::format_codex_implementers_section;
use crate::codex::{CODEX_IMPLEMENTER_EFFORT, CODEX_IMPLEMENTER_MODEL_SOL};
use crate::models::stage::{Implementer, Implementers};

const SENIOR_ENGINEER_AGENT: &str =
    include_str!("../../../../agents/loom-senior-software-engineer.md");
const ORCHESTRATION_SKILL: &str = include_str!("../../../../skills/loom-orchestration/SKILL.md");
const CODE_REVIEWER_AGENT: &str = include_str!("../../../../agents/loom-code-reviewer.md");
const CODE_REVIEW_SKILL: &str = include_str!("../../../../skills/loom-code-review/SKILL.md");

/// The lines between the first two `---` lines of a markdown file.
fn frontmatter_lines(text: &str) -> Vec<&str> {
    let mut lines = text.lines().skip_while(|line| line.trim() != "---");
    assert!(lines.next().is_some(), "file has no opening --- line");
    lines.take_while(|line| line.trim() != "---").collect()
}

/// Every run of whitespace collapsed to one space, so hard wraps do not matter.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text from the `## <heading>` line up to the next `## ` heading.
fn section<'a>(text: &'a str, heading: &str, label: &str) -> Vec<&'a str> {
    let wanted = format!("## {heading}");
    let mut lines = text.lines().skip_while(|line| line.trim_end() != wanted);
    assert!(
        lines.next().is_some(),
        "{label} has no `{wanted}` heading line"
    );
    lines.take_while(|line| !line.starts_with("## ")).collect()
}

#[test]
fn senior_engineer_runs_at_medium_effort_and_implements() {
    let frontmatter = frontmatter_lines(SENIOR_ENGINEER_AGENT);
    assert!(
        frontmatter.iter().any(|line| line.trim() == "effort: medium"),
        "agents/loom-senior-software-engineer.md frontmatter must set `effort: medium`; got {frontmatter:?}"
    );
    for retired in ["not the implementer", "hand it to `loom-software-engineer`"] {
        assert!(
            !SENIOR_ENGINEER_AGENT.contains(retired),
            "agents/loom-senior-software-engineer.md still contains {retired:?}: an escalated \
             opus implementer must implement the work itself"
        );
    }
}

#[test]
fn orchestration_skill_names_opus_at_medium_effort() {
    let skill = collapse_whitespace(ORCHESTRATION_SKILL);
    assert!(
        skill.contains("OPUS (loom-senior-software-engineer, effort medium)"),
        "skills/loom-orchestration/SKILL.md must name `OPUS (loom-senior-software-engineer, \
         effort medium)` in Rule 7"
    );
    assert!(
        !skill.contains("`effort: xhigh`"),
        "skills/loom-orchestration/SKILL.md still teaches `effort: xhigh` for opus spawns"
    );
}

#[test]
fn both_review_surfaces_carry_the_frontend_rules() {
    let surfaces = [
        ("agents/loom-code-reviewer.md", CODE_REVIEWER_AGENT),
        ("skills/loom-code-review/SKILL.md", CODE_REVIEW_SKILL),
    ];
    for (label, text) in surfaces {
        let body = section(text, "Frontend changes", label).join("\n");
        for word in ["duplicate", "convention"] {
            assert!(
                body.contains(word),
                "{label}'s `## Frontend changes` section must contain {word:?}; got:\n{body}"
            );
        }
    }
}

#[test]
fn codex_section_names_the_sol_spawn_form() {
    assert_eq!(CODEX_IMPLEMENTER_MODEL_SOL, "gpt-6.1-sol");
    let implementers = Implementers::new(vec![Implementer::Codex, Implementer::Claude]);
    let section = format_codex_implementers_section(&implementers, true);
    let sol_form =
        format!("--model {CODEX_IMPLEMENTER_MODEL_SOL} --effort {CODEX_IMPLEMENTER_EFFORT}");
    assert!(
        section.contains(&sol_form),
        "the available-case `## Codex Implementers` section must give the sol spawn form \
         {sol_form:?}; got:\n{section}"
    );
}
