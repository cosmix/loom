//! A refusal quotes the path it names: the daemon logs the reason, a session
//! picks its paths, and a newline in one must not start a log line.

use super::*;

#[test]
fn a_state_path_with_a_newline_is_quoted_in_the_refusal() {
    let temp = repo();
    let root = temp.path();
    stage(root, ".loom/x\nforged: line", "state\n");
    let state = request(root, "feat(s1): x");

    let refusal = reason(commit_staged(root, &stage_scope(), &state));

    assert_eq!(refusal.lines().count(), 1, "{refusal:?}");
    assert!(
        refusal.starts_with(r#"".loom/x\nforged: line" is loom state"#),
        "{refusal:?}"
    );
}

#[test]
fn a_knowledge_path_with_a_newline_is_quoted_in_the_refusal() {
    let temp = repo();
    let root = temp.path();
    git_in(root, &["checkout", "main"]);
    stage(root, "docs\nforged: line.md", "x\n");
    let docs = request(root, "docs: k");

    let refusal = reason(commit_staged(root, &knowledge_scope(), &docs));

    assert_eq!(refusal.lines().count(), 1, "{refusal:?}");
    assert!(
        refusal.starts_with(r#""docs\nforged: line.md" is outside doc/loom/knowledge;"#),
        "{refusal:?}"
    );
}
