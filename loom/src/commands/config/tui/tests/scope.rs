//! Coverage for the two-tier editor: which file a save lands in, which keys
//! have a project tier at all, and which tier the daemon ends up reading.
//!
//! The last of those is the one that keeps this screen honest. `entries.rs`
//! decides the same question for the dashboard against the same
//! `Workspace::shadows`; if these ever disagree, one of the two surfaces is
//! telling operators the wrong thing about their own config.

use super::super::state::{ConfigRow, ConfigState, InForce, Scope, Source};
use super::{focus_by_name, scratch, scratch_without_workspace};
use crate::user_config::ConfigValue;

/// The row for `name`, by key rather than by position.
fn row_of<'a>(state: &'a ConfigState, name: &str) -> &'a ConfigRow {
    state
        .rows()
        .iter()
        .find(|row| row.spec().name == name)
        .unwrap_or_else(|| panic!("no registry key named {name:?}"))
}

/// A project-scope edit writes the project file and leaves the user file alone.
#[test]
fn a_project_scope_save_writes_only_the_project_file() {
    let scratch = scratch();
    let mut state = scratch.state();
    state.set_scope(Scope::Project);
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);

    state.save();

    assert_eq!(state.status(), "1 key written · 1 project.");
    let project = std::fs::read_to_string(scratch.project_config()).expect("project config");
    assert!(project.contains("backend = \"tmux\""), "{project}");
    let user = std::fs::read_to_string(scratch.user_config()).unwrap_or_default();
    assert!(!user.contains("backend"), "{user}");
}

/// `update.check` has no project tier at all: the project tab can show it, but
/// nothing there can stage an edit against it.
#[test]
fn a_user_only_key_stages_nothing_at_project_scope() {
    let scratch = scratch();
    let mut state = scratch.state();
    state.set_scope(Scope::Project);
    focus_by_name(&mut state, "update.check");
    assert_eq!(
        row_of(&state, "update.check").source(Scope::Project),
        Source::UserOnly
    );

    state.cycle(1);
    assert!(!state.is_modified());
    assert!(
        state.status().contains("update.check"),
        "{}",
        state.status()
    );
    assert!(
        state.status().contains("no project scope"),
        "{}",
        state.status()
    );

    state.stage_clear();
    assert!(!state.is_modified());
    assert!(
        state.status().contains("update.check"),
        "{}",
        state.status()
    );

    state.save();
    assert_eq!(state.status(), "0 keys written; nothing pending.");
}

/// Edits staged on one tab survive a switch to the other, and one `s` writes
/// both files.
#[test]
fn edits_on_both_scopes_survive_a_toggle_and_save_together() {
    let scratch = scratch();
    let mut state = scratch.state();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);
    assert!(state.is_modified());

    state.toggle_scope();
    assert_eq!(state.scope(), Scope::Project);
    assert!(!state.is_modified(), "the project tab starts clean");
    state.cycle(1);

    state.toggle_scope();
    assert_eq!(state.scope(), Scope::User);
    assert!(state.is_modified(), "the user edit survived the round trip");
    state.toggle_scope();
    assert!(
        state.is_modified(),
        "the project edit survived the round trip"
    );

    state.save();

    assert_eq!(state.status(), "2 keys written · 1 user · 1 project.");
    assert!(std::fs::read_to_string(scratch.user_config())
        .unwrap()
        .contains("backend = \"tmux\""));
    assert!(std::fs::read_to_string(scratch.project_config())
        .unwrap()
        .contains("backend = \"tmux\""));
}

/// With no `.loom/work` the project tab still opens — an operator who cannot
/// see the tier cannot learn it exists — but nothing there is stageable.
#[test]
fn a_tree_without_a_workspace_shows_the_project_tier_as_missing() {
    let scratch = scratch_without_workspace();
    let mut state = scratch.state();

    state.set_scope(Scope::Project);

    assert_eq!(state.scope(), Scope::Project);
    assert_eq!(
        state.status(),
        "no .loom/work in this tree — run loom init to create a project config"
    );
    assert!(state.project_path().is_none());
    assert!(state.active_path().is_none());
    for row in state.rows() {
        let source = row.source(Scope::Project);
        assert!(
            matches!(source, Source::NoWorkspace | Source::UserOnly),
            "{} reported {source:?}",
            row.spec().name
        );
        assert!(row.displayed(Scope::Project).is_none());
    }

    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);
    assert!(!state.is_modified());
    state.stage_clear();
    assert!(!state.is_modified());

    state.save();
    assert_eq!(state.status(), "0 keys written; nothing pending.");
}

/// The OTHER column answers "what does the other FILE set", never "what does
/// the other tier resolve to".
///
/// The two differ on every project-backed key the project file omits, because
/// the project tier falls through to the user's value there — a resolving
/// reading would print `project <the very same value>` on nearly every row.
#[test]
fn the_other_column_reports_only_what_the_other_file_sets() {
    let scratch = scratch();
    let mut state = scratch.state();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);
    state.save();

    let row = row_of(&state, "terminal.backend");
    let tmux = ConfigValue::Text("tmux".to_owned());
    // Seen from the user tab, the other tier is the project file, which says
    // nothing about this key even though it resolves to a value.
    assert_eq!(row.set_value(Scope::Project), None);
    // Seen from the project tab, the other tier is the user file, which set it.
    assert_eq!(row.set_value(Scope::User), Some(&tmux));

    // The narrower question belongs to OTHER alone. The project tier still
    // RESOLVES to the inherited value, and the VALUE column, the inspector's
    // tier lines and `stage_clear` all depend on it continuing to.
    assert_eq!(row.displayed(Scope::Project), Some(tmux));
    assert_eq!(row.source(Scope::Project), Source::Inherited);
}

/// The in-force rule, end to end: the project tier wins when the project file
/// sets the key, the user tier when it does not, and clearing the project key
/// hands it back.
#[test]
fn the_project_tier_is_in_force_only_while_the_project_file_sets_the_key() {
    let scratch = scratch();

    let mut state = scratch.state();
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);
    state.save();
    assert_eq!(row_of(&state, "terminal.backend").in_force(), InForce::User);

    let mut state = scratch.state();
    state.set_scope(Scope::Project);
    focus_by_name(&mut state, "terminal.backend");
    state.cycle(1);
    state.save();
    let row = row_of(&state, "terminal.backend");
    assert_eq!(row.in_force(), InForce::Project);
    assert_eq!(row.source(Scope::Project), Source::Set);
    assert_eq!(row.source(Scope::User), Source::Set);

    let mut state = scratch.state();
    state.set_scope(Scope::Project);
    focus_by_name(&mut state, "terminal.backend");
    state.stage_clear();
    state.save();
    let row = row_of(&state, "terminal.backend");
    assert_eq!(row.in_force(), InForce::User);
    assert_eq!(row.source(Scope::Project), Source::Inherited);
}
