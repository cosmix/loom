//! Integration tests for the skill-trigger UserPromptSubmit hook.

use loom::fs::permissions::constants::HOOK_SKILL_TRIGGER;

#[path = "hooks_skill_trigger_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn every_qualifying_skill_is_listed() {
    if skip_unless_python3("hooks_skill_trigger::every_qualifying_skill_is_listed") {
        return;
    }
    let home = FakeHome::new();
    let (_hook_dir, hook) = install_hook();
    let out = run_hook(
        &hook,
        &home,
        "build a react form in typescript and ship it in docker",
        None,
        false,
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let ctx = additional_context(&out.stdout);
    let ts_pos = ctx
        .find("- loom-typescript --")
        .unwrap_or_else(|| panic!("missing loom-typescript line: {ctx}"));
    let docker_pos = ctx
        .find("- loom-docker --")
        .unwrap_or_else(|| panic!("missing loom-docker line: {ctx}"));
    let react_pos = ctx
        .find("- loom-react --")
        .unwrap_or_else(|| panic!("missing loom-react line: {ctx}"));
    assert!(
        ts_pos < docker_pos && docker_pos < react_pos,
        "unexpected line order: {ctx}"
    );
    let combined = ctx
        .lines()
        .find(|l| l.contains("All catalogued matches at once"))
        .unwrap_or_else(|| panic!("missing combined line: {ctx}"));
    assert!(
        combined.contains("args=\"loom-typescript loom-docker loom-react\""),
        "combined line has wrong names/order: {combined}"
    );
}

#[test]
fn ranking_is_deterministic_across_hash_seeds() {
    if skip_unless_python3("hooks_skill_trigger::ranking_is_deterministic_across_hash_seeds") {
        return;
    }
    let (_hook_dir, hook) = install_hook();
    let prompt = "build a react form in typescript and ship it in docker";
    let mut outputs = Vec::new();
    for seed in ["0", "1", "2", "3"] {
        // A fresh FakeHome (and so a fresh once-per-session ledger) per seed:
        // this test is about sort-order determinism across hash seeds, not
        // the ledger, and re-running the identical prompt against one ledger
        // would suggest nothing from the second call on.
        let home = FakeHome::new();
        let out = run_hook(&hook, &home, prompt, Some(seed), false);
        assert_eq!(out.code, 0, "seed {seed}: stderr={}", out.stderr);
        outputs.push((seed, out.stdout));
    }
    let (first_seed, first_stdout) = &outputs[0];
    for (seed, stdout) in &outputs[1..] {
        assert_eq!(
            first_stdout, stdout,
            "seed {seed} output differs from seed {first_seed}"
        );
    }
}

#[test]
fn loader_line_dropped_when_domain_skills_qualify() {
    if skip_unless_python3("hooks_skill_trigger::loader_line_dropped_when_domain_skills_qualify") {
        return;
    }
    let home = FakeHome::new();
    let (_hook_dir, hook) = install_hook();
    let out = run_hook(&hook, &home, "skills for react", None, false);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let ctx = additional_context(&out.stdout);
    assert!(
        !ctx.contains("/loom-skills"),
        "loom-skills loader line should be dropped once react qualifies: {ctx}"
    );
    let out = run_hook(&hook, &home, "list the skills", None, false);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let ctx = additional_context(&out.stdout);
    assert!(
        ctx.contains("/loom-skills"),
        "loom-skills loader line should be kept as the only match: {ctx}"
    );
    assert!(
        !ctx.contains("All catalogued matches"),
        "a single core-skill match should have no combined line: {ctx}"
    );
}

#[test]
fn single_catalogued_match_has_no_combined_line() {
    if skip_unless_python3("hooks_skill_trigger::single_catalogued_match_has_no_combined_line") {
        return;
    }
    let home = FakeHome::new();
    let (_hook_dir, hook) = install_hook();
    let out = run_hook(&hook, &home, "docker", None, false);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let ctx = additional_context(&out.stdout);
    assert!(
        ctx.contains("loom-docker"),
        "expected loom-docker line: {ctx}"
    );
    assert!(
        !ctx.contains("loom-react") && !ctx.contains("loom-typescript"),
        "unexpected extra skill matched: {ctx}"
    );
    assert!(
        !ctx.contains("All catalogued matches"),
        "a single catalogued match should have no combined line: {ctx}"
    );
}

#[test]
fn core_skill_is_never_in_combined_args() {
    if skip_unless_python3("hooks_skill_trigger::core_skill_is_never_in_combined_args") {
        return;
    }
    let home = FakeHome::new();
    let (_hook_dir, hook) = install_hook();
    // "plan" is a stopword, no longer a name match for loom-plan-writer on
    // a mere prefix; "plan" and "writer" together are two distinct
    // single-word hits, real evidence under the new rule.
    let out = run_hook(
        &hook,
        &home,
        "plan a writer app for react in typescript",
        None,
        false,
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let ctx = additional_context(&out.stdout);
    assert!(
        ctx.contains("/loom-plan-writer"),
        "expected core skill line: {ctx}"
    );
    let combined = ctx
        .lines()
        .find(|l| l.contains("All catalogued matches at once"))
        .unwrap_or_else(|| panic!("missing combined line: {ctx}"));
    assert!(combined.contains("loom-react"), "combined line: {combined}");
    assert!(
        combined.contains("loom-typescript"),
        "combined line: {combined}"
    );
    assert!(
        !combined.contains("loom-plan-writer"),
        "core skill leaked into combined args: {combined}"
    );
}

#[test]
fn cap_limits_flood() {
    if skip_unless_python3("hooks_skill_trigger::cap_limits_flood") {
        return;
    }
    let home = FakeHome::new();
    let (_hook_dir, hook) = install_hook();
    let prompt = "alpha0 alpha1 alpha2 alpha3 alpha4 alpha5 alpha6 alpha7 alpha8 alpha9";
    let out = run_hook(&hook, &home, prompt, None, false);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let ctx = additional_context(&out.stdout);
    let skill_lines: Vec<&str> = ctx.lines().filter(|l| l.starts_with("  -")).collect();
    assert_eq!(skill_lines.len(), 5, "expected exactly 5 lines: {ctx}");
    let combined = ctx
        .lines()
        .find(|l| l.contains("All catalogued matches at once"))
        .unwrap_or_else(|| panic!("missing combined line: {ctx}"));
    let args = combined
        .split("args=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("missing args in combined line: {combined}"));
    assert_eq!(
        args.split_whitespace().count(),
        5,
        "expected 5 names in combined args: {combined}"
    );
}

#[path = "hooks_skill_trigger_codex.rs"]
mod codex;

#[path = "hooks_skill_trigger_evidence.rs"]
mod evidence;
