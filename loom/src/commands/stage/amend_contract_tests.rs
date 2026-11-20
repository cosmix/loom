//! Contract `amend-refuses-inside-a-stage-session`: `loom stage amend` refuses
//! to run when `LOOM_WORKTREE_PATH` names a stage worktree.
use crate::commands::stage::amend::refuse_stage_session;

#[test]
fn amend_refuses_inside_a_stage_session() {
    assert!(
        refuse_stage_session(None).is_ok(),
        "no stage worktree: amend runs"
    );
    assert!(
        refuse_stage_session(Some("")).is_ok(),
        "an empty value names no worktree"
    );
    assert!(
        refuse_stage_session(Some("  \t")).is_ok(),
        "a blank value names no worktree"
    );

    let worktree = "/home/user/repo/.worktrees/some-stage";
    let error = refuse_stage_session(Some(worktree))
        .expect_err("a stage session must not amend its own plan");
    let text = format!("{error:#}");
    assert!(text.contains("cannot amend"), "refusal text: {text}");
    assert!(text.contains(worktree), "refusal names the path: {text}");
}
