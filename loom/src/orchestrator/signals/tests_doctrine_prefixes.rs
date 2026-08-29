//! Cross-surface consistency tests for the emitted STABLE PREFIX / signal
//! text - as opposed to `tests_doctrine.rs`'s BLOCK-A/BLOCK-B pinning, which
//! covers the static guidance surfaces (CLAUDE.md.template, agent
//! definitions, the plan-writer skill). Split out of `tests_doctrine.rs` (see
//! that file's module doc) purely to keep both files under the line-count
//! ceiling - this is the same doctrine-pinning mechanism, just scoped to what
//! `cache.rs` and `generate.rs` actually emit at runtime.

use std::fs;

use tempfile::TempDir;

use super::cache::{
    generate_integration_verify_stable_prefix, generate_knowledge_distill_stable_prefix,
    generate_knowledge_stable_prefix, generate_stable_prefix, KNOWLEDGE_CONSUMPTION_CONTRACT,
};
use super::generate::generate_signal_with_metrics;
use super::tests::{create_test_session, create_test_stage, create_test_worktree};

const CLAUDE_MD_TEMPLATE: &str = include_str!("../../../../CLAUDE.md.template");

/// A stable-prefix generator, named for its failure message.
type PrefixGenerator = fn() -> String;

/// The "settled stage" completion doctrine (`is the LAST act of your
/// session` / `post-completion work is LOST WORK`) must reach every stable
/// prefix, not just the two surfaces `cache.rs`'s own unit tests happen to
/// pin - a fourth generator added later gets no coverage from those two.
#[test]
fn settled_completion_doctrine_present_in_all_stable_prefixes() {
    let generators: [(&str, PrefixGenerator); 4] = [
        ("generate_stable_prefix", generate_stable_prefix),
        (
            "generate_integration_verify_stable_prefix",
            generate_integration_verify_stable_prefix,
        ),
        (
            "generate_knowledge_distill_stable_prefix",
            generate_knowledge_distill_stable_prefix,
        ),
        (
            "generate_knowledge_stable_prefix",
            generate_knowledge_stable_prefix,
        ),
    ];

    for (name, generator) in generators {
        let prefix = generator();
        assert!(
            prefix.contains("is the LAST act of your session"),
            "{name} must frame `loom stage complete` as the session's LAST act"
        );
        assert!(
            prefix.contains("post-completion work is LOST WORK"),
            "{name} must warn that post-completion work is LOST WORK"
        );
    }
}

/// The subagent-response-budget block must frame the timeout as a check-in
/// cadence, not a deadline that licenses taking over a live subagent's work.
#[test]
fn subagent_budget_is_cadence_not_deadline_in_emitted_signal() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let worktree = create_test_worktree();
    let mut budgeted_stage = create_test_stage();
    budgeted_stage.subagent_timeout_secs = Some(900);

    let (signal_path, _) = generate_signal_with_metrics(
        &session,
        &budgeted_stage,
        &worktree,
        &[],
        None,
        None,
        &work_dir,
    )
    .unwrap();
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(
        content.contains("never a deadline on the subagent's own work"),
        "the block must say the budget is the idle threshold death is judged \
         against, never a deadline on the subagent's own work"
    );
    assert!(
        content.contains("positive evidence of death"),
        "the block must require positive evidence before a takeover, not elapsed \
         time alone"
    );
    assert!(
        content.contains("loom subagents watch"),
        "the signal must point the session at the sanctioned checking command, \
         not a hand-rolled poll loop"
    );
    assert!(
        !content.contains("report back within"),
        "the retired takeover doctrine must not resurface in the emitted block"
    );
    assert!(
        !content.contains("take the work over"),
        "the retired takeover doctrine must not resurface in the emitted block"
    );
}

/// A grep proves presence, never agreement - pin `KNOWLEDGE_CONSUMPTION_CONTRACT`
/// byte-for-byte against `CLAUDE.md.template`'s `## KNOWLEDGE-FIRST` section.
#[test]
fn knowledge_consumption_contract_agrees_with_claude_md_template() {
    let heading = "## KNOWLEDGE-FIRST";
    let start = CLAUDE_MD_TEMPLATE
        .find(heading)
        .expect("template needs a ## KNOWLEDGE-FIRST section")
        + heading.len();
    let rest = &CLAUDE_MD_TEMPLATE[start..];
    let end = rest
        .find("\n---")
        .expect("## KNOWLEDGE-FIRST must be followed by ---");
    let block = rest[..end].trim();
    assert_eq!(block, KNOWLEDGE_CONSUMPTION_CONTRACT.trim(), "CLAUDE.md.template's ## KNOWLEDGE-FIRST body must match cache::KNOWLEDGE_CONSUMPTION_CONTRACT byte-for-byte (see doc/loom/knowledge/mistakes/doctrine-and-acceptance.md)");
}
