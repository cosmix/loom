use super::*;

#[test]
fn codex_keeps_generic_project_tiebreaker_conditional() {
    if skip_unless_python3(
        "hooks_skill_trigger::codex_keeps_generic_project_tiebreaker_conditional",
    ) {
        return;
    }
    let home = FakeHome::new();
    home.add_go_project();
    let (_hook_dir, hook) = install_hook();

    // `context` is the only matching generic keyword (score 1); project
    // discovery contributes the second point needed to render loom-golang.
    let generic = run_hook(&hook, &home, "context", None, true);
    assert_eq!(generic.code, 0, "stderr={}", generic.stderr);
    let generic_ctx = additional_context(&generic.stdout);
    let generic_line = generic_ctx
        .lines()
        .find(|line| line.contains("loom-golang"))
        .unwrap_or_else(|| panic!("missing Go skill: {generic_ctx}"));
    assert!(
        generic_line.contains("repo:golang"),
        "missing project match: {generic_line}"
    );
    assert!(
        generic_line.contains("if the task touches golang"),
        "should be conditional"
    );
    assert!(
        !generic_line.contains("in full"),
        "unexpected directive: {generic_line}"
    );
}

#[test]
fn codex_keeps_prefix_only_refactor_match_conditional() {
    if skip_unless_python3(
        "hooks_skill_trigger::codex_keeps_prefix_only_refactor_match_conditional",
    ) {
        return;
    }
    let home = FakeHome::new();
    home.add_go_project();
    let (_hook_dir, hook) = install_hook();

    let prefix = run_hook(&hook, &home, "refactor", None, true);
    assert_eq!(prefix.code, 0, "stderr={}", prefix.stderr);
    let prefix_ctx = additional_context(&prefix.stdout);
    let prefix_line = prefix_ctx
        .lines()
        .find(|line| line.contains("loom-refactoring"))
        .unwrap_or_else(|| panic!("missing prefix-ranked skill: {prefix_ctx}"));
    assert!(
        prefix_line.contains("if the task touches this"),
        "should be conditional"
    );
    assert!(
        !prefix_line.contains("in full"),
        "unexpected directive: {prefix_line}"
    );
}

#[test]
fn codex_exact_skill_name_reads_in_full() {
    if skip_unless_python3("hooks_skill_trigger::codex_exact_skill_name_reads_in_full") {
        return;
    }
    let home = FakeHome::new();
    home.add_go_project();
    let (_hook_dir, hook) = install_hook();

    let exact = run_hook(&hook, &home, "golang", None, true);
    assert_eq!(exact.code, 0, "stderr={}", exact.stderr);
    let exact_ctx = additional_context(&exact.stdout);
    assert!(
        exact_ctx.contains("SKILL.md\" in full"),
        "expected directive: {exact_ctx}"
    );
}

#[test]
fn codex_ignores_requests_without_keywords() {
    if skip_unless_python3("hooks_skill_trigger::codex_ignores_requests_without_keywords") {
        return;
    }
    let home = FakeHome::new();
    home.add_go_project();
    let (_hook_dir, hook) = install_hook();

    let no_keyword = run_hook(&hook, &home, "make changes", None, true);
    assert_eq!(no_keyword.code, 0, "stderr={}", no_keyword.stderr);
    assert!(
        no_keyword.stdout.trim().is_empty(),
        "unexpected: {}",
        no_keyword.stdout
    );
}

#[test]
fn codex_keeps_generic_keyword_collisions_conditional() {
    if skip_unless_python3(
        "hooks_skill_trigger::codex_keeps_generic_keyword_collisions_conditional",
    ) {
        return;
    }
    let home = FakeHome::new();
    home.add_go_project();
    let (_hook_dir, hook) = install_hook();
    // Each generic keyword scores one point, so this reaches MIN_SCORE without
    // relying on project discovery.
    let collision = run_hook(&hook, &home, "context pointer", None, true);
    assert_eq!(collision.code, 0, "stderr={}", collision.stderr);
    let collision_ctx = additional_context(&collision.stdout);
    let collision_line = collision_ctx
        .lines()
        .find(|line| line.contains("loom-golang"))
        .unwrap_or_else(|| panic!("missing Go skill: {collision_ctx}"));
    assert!(
        collision_line.contains("if the task touches golang"),
        "should be conditional: {collision_line}"
    );
    assert!(
        !collision_line.contains("in full"),
        "unexpected directive: {collision_line}"
    );
}
