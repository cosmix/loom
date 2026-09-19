//! Evidence-rule, once-per-session, and header-pinning tests for the
//! skill-trigger hook. Split out of hooks_skill_trigger.rs purely for size
//! (CLAUDE.md rule 17's 400-line file cap), the same way that file already
//! splits out its --codex tests.

use super::*;
use std::collections::BTreeSet;

#[test]
fn header_is_pinned_and_never_directive() {
    if skip_unless_python3("hooks_skill_trigger::header_is_pinned_and_never_directive") {
        return;
    }
    let home = FakeHome::new();
    let (_hook_dir, hook) = install_hook();
    let out = run_hook(&hook, &home, "docker", None, false);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let ctx = additional_context(&out.stdout);
    assert!(
        ctx.starts_with(
            "SKILL MATCH: skills matching this request (keyword hits shown; \
repo: markers are context, not a reason to load)."
        ),
        "advisory header drifted from the pinned text: {ctx}"
    );
    assert!(
        !ctx.contains("Load EVERY"),
        "a directive skill-loading variant must never ship: {ctx}"
    );
}

#[test]
fn agent_message_prompt_is_silent() {
    if skip_unless_python3("hooks_skill_trigger::agent_message_prompt_is_silent") {
        return;
    }
    let home = FakeHome::new();
    let (_hook_dir, hook) = install_hook();
    let out = run_hook(
        &hook,
        &home,
        "Background agent stopped after finishing the docker setup",
        None,
        false,
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.trim().is_empty(),
        "a machine-generated prompt should never get a skill suggestion: {}",
        out.stdout
    );
}

#[test]
fn generic_keyword_alone_with_ci_cd_marker_stays_silent() {
    if skip_unless_python3(
        "hooks_skill_trigger::generic_keyword_alone_with_ci_cd_marker_stays_silent",
    ) {
        return;
    }
    let home = FakeHome::new();
    home.add_ci_cd_project();
    let (_hook_dir, hook) = install_hook();
    // "stage" alone is one generic keyword hit (score 1); the ci-cd repo
    // marker from .github/workflows used to add the second point needed to
    // clear MIN_SCORE by itself (report section 4.4: loom-ci-cd suggested
    // 934 times on the word "stage"). A repo marker is never evidence now,
    // so a lone generic word plus it must stay silent.
    let out = run_hook(&hook, &home, "stage", None, false);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.trim().is_empty(),
        "a lone generic keyword plus the ci-cd repo marker should not qualify: {}",
        out.stdout
    );
}

#[test]
fn phrase_trigger_still_qualifies_a_now_stopword_word() {
    if skip_unless_python3(
        "hooks_skill_trigger::phrase_trigger_still_qualifies_a_now_stopword_word",
    ) {
        return;
    }
    let home = FakeHome::new();
    let (_hook_dir, hook) = install_hook();
    // "model" alone is now in the loom-vocabulary stopwords and is not
    // indexed on its own in this fixture, but a phrase trigger always scores
    // 2 and qualifies through the phrase rule regardless of the bare word's
    // stopword status. Decision: phrase triggers stay; only bare generic
    // words are filtered (SKILL.md trigger lists themselves are the
    // doctrine stage's concern, not this hook's).
    let out = run_hook(&hook, &home, "model selection", None, false);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let ctx = additional_context(&out.stdout);
    assert!(
        ctx.contains("loom-model-evaluation"),
        "phrase trigger should still qualify: {ctx}"
    );
}

#[test]
fn bare_stopword_matching_a_skill_name_prefix_stays_silent() {
    if skip_unless_python3(
        "hooks_skill_trigger::bare_stopword_matching_a_skill_name_prefix_stays_silent",
    ) {
        return;
    }
    let home = FakeHome::new();
    // Simulate what a defective indexer used to produce: the bare stopword
    // "model" kept in the index for loom-model-evaluation purely because it
    // is a >=4-char prefix of the effective name "model-evaluation". With
    // the fix, a stopword only earns name-match weight on EXACT equality,
    // so this single word hit (weight 1) must not qualify on its own.
    home.add_index_entry("model", &["loom-model-evaluation"]);
    let (_hook_dir, hook) = install_hook();
    let out = run_hook(&hook, &home, "model of the situation today", None, false);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.trim().is_empty(),
        "a bare stopword prefix-matching a skill name must not solo-qualify it: {}",
        out.stdout
    );
}

#[test]
fn repeated_prompt_in_same_session_is_silent_second_time() {
    if skip_unless_python3(
        "hooks_skill_trigger::repeated_prompt_in_same_session_is_silent_second_time",
    ) {
        return;
    }
    let home = FakeHome::new();
    let (_hook_dir, hook) = install_hook();
    let first = run_hook(&hook, &home, "docker", None, false);
    assert_eq!(first.code, 0, "stderr={}", first.stderr);
    assert!(
        first.stdout.contains("loom-docker"),
        "expected a suggestion on the first prompt: {}",
        first.stdout
    );
    let second = run_hook(&hook, &home, "docker", None, false);
    assert_eq!(second.code, 0, "stderr={}", second.stderr);
    assert!(
        second.stdout.trim().is_empty(),
        "a skill already suggested this session should not repeat: {}",
        second.stdout
    );
}

#[test]
fn stopwords_match_between_shell_and_rust_index() {
    let start = HOOK_SKILL_TRIGGER
        .find("STOPWORDS = frozenset({")
        .expect("shell STOPWORDS block present");
    let after = &HOOK_SKILL_TRIGGER[start..];
    let end = after.find("})").expect("shell STOPWORDS block closed");
    let block = &after[..end];
    let shell_words: BTreeSet<&str> = block
        .split('"')
        .enumerate()
        .filter_map(|(i, s)| (i % 2 == 1).then_some(s))
        .collect();
    let rust_words: BTreeSet<&str> = loom::commands::skill_index::STOPWORDS
        .iter()
        .copied()
        .collect();
    assert_eq!(
        shell_words, rust_words,
        "skill-trigger.sh and skill_index.rs stopword lists drifted apart"
    );
}
