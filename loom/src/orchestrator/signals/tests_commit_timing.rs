//! Pins the commit-timing doctrine (`append_commit_timing_rules`, `helpers.rs`):
//! commits are the ORCHESTRATOR's, made ONLY as the final step of the stage,
//! after every subagent has returned and all verification is green — never
//! mid-stage. The doctrine text is shared across all four stable prefixes and
//! `CLAUDE.md.template`; these tests pin the sentinel phrases across every
//! surface so a reword in one place cannot silently drift from the others.

use std::fs;

use tempfile::TempDir;

use super::super::cache::{
    generate_integration_verify_stable_prefix, generate_knowledge_distill_stable_prefix,
    generate_knowledge_stable_prefix, generate_stable_prefix,
};
use super::super::generate::generate_signal_with_metrics;
use super::{create_test_session, create_test_stage, create_test_worktree};

const CLAUDE_MD_TEMPLATE: &str = include_str!("../../../../CLAUDE.md.template");

/// A stable-prefix generator, named for its failure message.
type PrefixGenerator = fn() -> String;

fn all_generators() -> [(&'static str, PrefixGenerator); 4] {
    [
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
    ]
}

#[test]
fn commit_timing_doctrine_present_in_all_stable_prefixes() {
    for (name, generator) in all_generators() {
        let prefix = generator();
        assert!(
            prefix.contains("When to Commit (ORCHESTRATOR ONLY"),
            "{name} must carry the When-to-Commit header"
        );
        assert!(
            prefix.contains("ONLY as the final step of the stage"),
            "{name} must frame commits as ONLY the final step of the stage"
        );
        assert!(
            prefix.contains("never mid-stage"),
            "{name} must forbid mid-stage commits"
        );
    }
}

#[test]
fn code_prefixes_gate_commits_on_the_adversarial_review() {
    let standard = generate_stable_prefix();
    let iv = generate_integration_verify_stable_prefix();
    let knowledge = generate_knowledge_stable_prefix();
    let distill = generate_knowledge_distill_stable_prefix();

    for (name, prefix) in [("standard", &standard), ("integration-verify", &iv)] {
        assert!(
            prefix.contains("The mini adversarial code review has RETURNED"),
            "{name} must gate commits on the adversarial review having returned"
        );
    }

    for (name, prefix) in [("knowledge", &knowledge), ("knowledge-distill", &distill)] {
        assert!(
            !prefix.contains("The mini adversarial code review has RETURNED"),
            "{name} emits no code, so it must not gate commits on a code review"
        );
        assert!(
            prefix.contains("re-read every knowledge file you wrote"),
            "{name} must gate commits on re-reading the knowledge files it wrote"
        );
    }
}

#[test]
fn commit_timing_sentinels_agree_with_claude_md_template() {
    assert!(
        CLAUDE_MD_TEMPLATE.contains("ONLY as the final step of the stage"),
        "CLAUDE.md.template must frame commits as ONLY the final step of the stage"
    );
    assert!(
        CLAUDE_MD_TEMPLATE.contains("never mid-stage"),
        "CLAUDE.md.template must forbid mid-stage commits"
    );
    assert!(
        CLAUDE_MD_TEMPLATE
            .contains("When to commit — at the END, after ALL verification, never before."),
        "CLAUDE.md.template's Rule 4 must carry the When-to-commit header"
    );

    let sentinel = "Commits happen ONLY as the final step of the stage";
    let occurrences = CLAUDE_MD_TEMPLATE.matches(sentinel).count();
    assert_eq!(
        occurrences, 2,
        "hard stop 3's commit-timing sentence must appear exactly twice in \
         CLAUDE.md.template (the rule at the top and its verbatim recap at the \
         bottom); found {occurrences}"
    );
}

#[test]
fn retired_commit_wording_is_gone() {
    let retired = concat!(
        "**Commit to your worktree branch** - it will be ",
        "merged after verification"
    );
    for (name, generator) in all_generators() {
        let prefix = generator();
        assert!(
            !prefix.contains(retired),
            "{name} must not carry the retired commit-timing wording"
        );
    }
}

#[test]
fn recitation_carries_the_stage_end_sequence() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    let (signal_path, _) =
        generate_signal_with_metrics(&session, &stage, &worktree, &[], None, None, &work_dir)
            .unwrap();
    let content = fs::read_to_string(&signal_path).unwrap();

    let tasks_pos = content
        .find("## Immediate Tasks")
        .expect("signal must carry an Immediate Tasks section");
    let sequence_pos = content
        .find("Stage end sequence (in this order, nothing skipped)")
        .expect("signal must carry the stage end sequence recap");
    assert!(
        sequence_pos > tasks_pos,
        "the stage end sequence recap must be recited AFTER the task list, at \
         the end of the recitation section for maximum attention"
    );
}

#[test]
fn sandbox_section_names_the_package_cache_grant() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let worktree = create_test_worktree();

    let mut enabled_stage = create_test_stage();
    enabled_stage.sandbox.enabled = Some(true);
    let (enabled_path, _) = generate_signal_with_metrics(
        &session,
        &enabled_stage,
        &worktree,
        &[],
        None,
        None,
        &work_dir,
    )
    .unwrap();
    let enabled_content = fs::read_to_string(&enabled_path).unwrap();
    assert!(
        enabled_content.contains("**Package-manager caches:**"),
        "an enabled sandbox must name the package-manager-cache carve-out"
    );

    let mut disabled_stage = create_test_stage();
    disabled_stage.sandbox.enabled = Some(false);
    let (disabled_path, _) = generate_signal_with_metrics(
        &session,
        &disabled_stage,
        &worktree,
        &[],
        None,
        None,
        &work_dir,
    )
    .unwrap();
    let disabled_content = fs::read_to_string(&disabled_path).unwrap();
    assert!(
        !disabled_content.contains("**Package-manager caches:**"),
        "a disabled sandbox has nothing to carve an exception out of"
    );
}
