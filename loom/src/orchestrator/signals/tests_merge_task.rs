//! What the merge signal's task says about the landing gates and the
//! knowledge index.

use super::{create_test_session, create_test_stage, format_merge_signal_content};

fn signal() -> String {
    format_merge_signal_content(
        &create_test_session(),
        &create_test_stage(),
        "loom/stage-1",
        "main",
        &[],
        "",
    )
}

#[test]
fn signal_says_a_failed_gate_may_hold_the_stage_and_resolved_cancels_a_check() {
    let content = signal();
    assert!(
        content.contains("hands the stage to a stage session, or holds it for the operator"),
        "{content}"
    );
    assert!(
        content.contains("Loom cancels a `loom check` still running"),
        "{content}"
    );
    assert!(content.contains("so step 4 must pass"), "{content}");
}

/// The landing gates compare `INDEX.md` by the byte, git merges it as text, and
/// the sandbox denies the cache `sync` publishes after writing it.
#[test]
fn signal_resyncs_the_knowledge_index_before_the_commit() {
    let content = signal();
    for pinned in [
        "If the merge touched anything under `doc/loom/knowledge/`",
        "run `loom knowledge sync --structural-only` here",
        "`rebuilding the derived context catalog failed`",
        "it must not report `INDEX.md is stale`",
        "`git add doc/loom/knowledge/INDEX.md`",
        "keep that protection rule as it is and use `loom stage block`",
    ] {
        assert!(content.contains(pinned), "{pinned}: {content}");
    }
    let sync = content
        .find("run `loom knowledge sync --structural-only`")
        .unwrap();
    let check = content.find("Rerun the stage's").unwrap();
    let commit = content.find("Finish the merge").unwrap();
    assert!(sync < check && check < commit, "{content}");
}
