//! A selection the sandbox keeps from running at the base is waived for the
//! full suite with a note naming it and how it failed in the worktree.

use super::*;

#[test]
fn each_selection_the_sandbox_refused_at_the_base_is_named_in_its_note() {
    let (repo, base) = stage_repo();
    let canned = Canned::new(sandbox_refused);
    let mut other = failed(1);
    other.id = "IT-cargo-test:other".to_string();
    other.failure.kind = FailureKind::DoesNotBuild;
    let probe = BaseProbe {
        worktree: repo.path(),
        commit: Ok(base),
        runner: &canned,
    };
    let mut notes = BTreeSet::new();

    let standing = compare(&probe, vec![failed(2), other], &mut notes);

    assert!(standing.is_empty(), "{standing:?}");
    assert_eq!(notes.len(), 2, "one note per waived selection: {notes:?}");
    let named = |id: &str, kind: &str| {
        notes
            .iter()
            .any(|note| note.contains(&format!("`{id}` {kind} is waived")))
    };
    assert!(named("IT-cargo-test:.", "fails (2 failed)"), "{notes:?}");
    assert!(named("IT-cargo-test:other", "does not build"), "{notes:?}");
    for note in &notes {
        assert!(note.starts_with("base not runnable: "), "{note}");
        assert!(note.ends_with(FULL_SUITE), "{note}");
    }
}
