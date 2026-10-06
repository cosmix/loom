//! The hook script's own text: the blocking patterns and the operator message
//! it must carry.

use loom::fs::permissions::constants::HOOK_COMMIT_FILTER;

#[test]
fn hook_contains_blocking_logic() {
    // Check for the regex patterns used in the hook
    assert!(HOOK_COMMIT_FILTER.contains("Co-Authored-By:"));
    assert!(HOOK_COMMIT_FILTER.contains("(claude|anthropic|noreply@anthropic)"));
    assert!(HOOK_COMMIT_FILTER.contains("exit 2"));
    // Check for bypass vector coverage
    assert!(HOOK_COMMIT_FILTER.contains("--trailer"));
    assert!(HOOK_COMMIT_FILTER.contains("--author"));
    assert!(HOOK_COMMIT_FILTER.contains("GIT_AUTHOR_"));
    assert!(HOOK_COMMIT_FILTER.contains("GIT_COMMITTER_EMAIL"));
    assert!(HOOK_COMMIT_FILTER.contains("Signed-off-by:"));
    assert!(HOOK_COMMIT_FILTER.contains("Generated with"));
    assert!(HOOK_COMMIT_FILTER.contains("trailer."));
}

#[test]
fn hook_has_user_friendly_message() {
    assert!(HOOK_COMMIT_FILTER.contains("BLOCKED"));
    // Attribution is rule 9; rule 8 is native tools/formatting.
    assert!(HOOK_COMMIT_FILTER.contains("CLAUDE.md rule 9"));
    // The block kills the whole Bash call, so a chained `git add` is lost too.
    // Without this, the agent retries the commit alone and hits "no changes
    // added to commit" - the recurring second failure this guidance prevents.
    assert!(HOOK_COMMIT_FILTER.contains("NOTHING RAN"));
}
