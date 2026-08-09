use super::*;

#[test]
fn hook_repair_propagates_skill_index_write_failure() {
    let root = tempfile::tempdir().unwrap();
    let error = fix_hooks_with(
        root.path(),
        || Ok(()),
        |_| Ok(()),
        || anyhow::bail!("simulated skill-index write failure"),
    )
    .expect_err("a failed skill-index rebuild must fail the repair action");

    assert!(error
        .to_string()
        .contains("simulated skill-index write failure"));
}

#[test]
fn loom_run_cmdline_matches_plain_and_pathed() {
    assert!(is_loom_run_cmdline("loom run"));
    assert!(is_loom_run_cmdline("loom run --watch --max-parallel 4"));
    assert!(is_loom_run_cmdline("/usr/local/bin/loom run"));
    assert!(is_loom_run_cmdline(
        "/home/u/.cargo/bin/loom run --no-merge"
    ));
}

#[test]
fn loom_run_cmdline_rejects_non_run_and_unrelated() {
    assert!(!is_loom_run_cmdline("loom status"));
    assert!(!is_loom_run_cmdline("loom stop"));
    assert!(!is_loom_run_cmdline("loom"));
    assert!(!is_loom_run_cmdline("vim loom/src/commands/run/mod.rs"));
    assert!(!is_loom_run_cmdline("cargo run -- loom"));
    assert!(!is_loom_run_cmdline("loomx run"));
    assert!(!is_loom_run_cmdline(""));
}
