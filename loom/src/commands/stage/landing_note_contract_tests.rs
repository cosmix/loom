//! Contract `landing-note-names-the-route`: the note printed after a forced
//! completion that left the stage unmerged names the route that merges it,
//! conditional on auto-merge, with the manual route and its directory.
use crate::commands::stage::complete::complete_force::landing_note;

#[test]
fn landing_note_names_the_route() {
    assert_eq!(
        landing_note("s1", true),
        "  → Not merged yet: with auto-merge on (the default) the running daemon lands it and \
         triggers dependents; otherwise run `loom stage merge s1` from .worktrees/s1.",
        "note with a running daemon"
    );
    assert_eq!(
        landing_note("s1", false),
        "  → Not merged yet, and no daemon is running to merge it: with auto-merge on (the \
         default) `loom run` merges it and triggers dependents; otherwise run \
         `loom stage merge s1` from .worktrees/s1.",
        "note with no daemon running"
    );
}
