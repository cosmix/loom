//! Fail-open test for `subagent-verify-guard.sh` when its install is broken.
//!
//! Split from the parent for size. Uses the parent's harness (process-tree
//! construction, env scrubbing) via `use super::*`.

use super::*;

#[test]
fn a_missing_matchers_helper_lets_the_call_through_with_a_warning() {
    if skip_unless_gate_visible(
        "helper::a_missing_matchers_helper_lets_the_call_through_with_a_warning",
    ) {
        return;
    }
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());
    fs::remove_file(dir.path().join("_verify_matchers.sh")).expect("remove matchers helper");

    let (code, stderr) = run_hook_in_tree(
        dir.path(),
        &hook_path,
        true,
        &bash_payload("cargo test"),
        &[],
    );

    // Exit 1 is non-blocking in Claude Code; exit 2 would deny the call.
    assert_eq!(
        code, 1,
        "a broken install must warn, not block; stderr={stderr}"
    );
    assert!(
        stderr.contains("loom install-assets"),
        "the warning must name the fix; stderr={stderr}"
    );
}
